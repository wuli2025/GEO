//! 自媒体「投递引擎」板块 B —— 生成 → 排版 → 上传草稿 的后端命令层。
//!
//! 板块 A（mediaops.rs）管的是**运营数据面**（题库 / 队列 / 平台设置 / 度量）；
//! 本模块管的是**执行面**：把一条稿件从「选题」跑成「平台后台里的一篇草稿」。
//!
//! 一条 job 由若干阶段组合而成（可裁剪）：
//!   - `generate`：调 Claude 引擎（自 spawn claude CLI，学 headless.rs 的只读纪律）按
//!     「自媒体主笔基础画像 + 平台补丁」写出 1200+ 字结构化 Markdown 正文，落
//!     `~/PolarisGEO/articles/{platform}/{date}-{slug}.md`。
//!   - `typeset`：公众号专属 —— 把 Markdown 转成「干净语义 HTML」正文，供 wechat_yiban
//!     的排版流程吃（`--body-file`）。其余平台 draft_uploader 直接吃 .md，本阶段空转。
//!   - `upload`：spawn python 跑对应平台脚本（wechat→wechat_yiban.py，其余→draft_uploader.py），
//!     脚本路径优先取运行期物化位置 `~/PolarisGEO/skills/...`，缺失回退仓库 templates 并记日志。
//!
//! 全程更新 mediaops 队列状态（running→draft_uploaded / failed）与度量事件（run/draft/fail），
//! job 日志滚 `~/PolarisGEO/logs/jobs/{job_id}.log`。子进程统一登记进 runtime::procs::CHILDREN，
//! `media_job_cancel` 借它一键杀进程树（复用 pipeline.rs 同款 kill_tree）。
//!
//! server flavor 也要能编译：命令用 `#[cfg_attr(feature="desktop", tauri::command)]`，不硬依赖 tauri。

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use polaris_runtime::procs::CHILDREN;

// ───────────────────────── 常量 ─────────────────────────

/// 合法阶段名，顺序即执行顺序。
const STAGE_GENERATE: &str = "generate";
const STAGE_TYPESET: &str = "typeset";
const STAGE_UPLOAD: &str = "upload";
const ALL_STAGES: &[&str] = &[STAGE_GENERATE, STAGE_TYPESET, STAGE_UPLOAD];

/// 生成阶段的墙钟超时（秒）——卡住必须能放手，别永久钉死后台线程。
const GENERATE_TIMEOUT_SECS: u64 = 420;

// ───────────────────────── 数据类型 ─────────────────────────

/// 一条投递流水线 job 的运行态快照（前端轮询用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaJob {
    pub id: String,
    /// 关联的规划队列项 id（有则跟进它的状态机）。
    #[serde(default)]
    pub queue_id: Option<String>,
    pub platform: String,
    pub title: String,
    #[serde(default)]
    pub topic: String,
    /// 本 job 要跑的阶段（generate/typeset/upload 的子集，按此顺序执行）。
    pub stages: Vec<String>,
    /// "pending" | "running" | "done" | "failed" | "canceled"
    pub status: String,
    /// 当前/最后所处阶段（空=未开跑）。
    #[serde(default)]
    pub stage: String,
    /// 生成/排版产物路径（.md 或公众号 .html body）。
    #[serde(default)]
    pub article_path: Option<String>,
    /// job 日志文件绝对路径。
    pub log_path: String,
    /// 失败原因（status=="failed"/"canceled" 时给可读说明）。
    #[serde(default)]
    pub error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

// ───────────────────────── 进程内 job 注册表 ─────────────────────────

static JOBS: Lazy<Mutex<HashMap<String, MediaJob>>> = Lazy::new(|| Mutex::new(HashMap::new()));
static SEQ: AtomicU64 = AtomicU64::new(0);

fn home() -> PathBuf {
    directories::UserDirs::new()
        .map(|u| u.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// job id：毫秒时间戳 + 进程内自增序号（十六进制），同毫秒多发也不撞。
fn gen_id() -> String {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    format!("{ms:x}{:04x}", seq & 0xffff)
}

/// 子进程池里的 key（cancel 按此杀进程树）。
fn child_key(job_id: &str) -> String {
    format!("mediajob-{job_id}")
}

fn update_job<F: FnOnce(&mut MediaJob)>(job_id: &str, f: F) {
    let mut jobs = JOBS.lock();
    if let Some(j) = jobs.get_mut(job_id) {
        f(j);
        j.updated_at = now_secs();
    }
}

fn get_job(job_id: &str) -> Option<MediaJob> {
    JOBS.lock().get(job_id).cloned()
}

fn job_is_canceled(job_id: &str) -> bool {
    JOBS.lock()
        .get(job_id)
        .map(|j| j.status == "canceled")
        .unwrap_or(true) // 找不到当已取消，别继续跑
}

// ───────────────────────── 路径 / slug ─────────────────────────

/// 平台中文名（进 prompt / 日志更可读）。
fn platform_cn(platform: &str) -> &'static str {
    match platform {
        "wechat" => "微信公众号",
        "xhs" => "小红书",
        "zhihu" => "知乎",
        "toutiao" => "今日头条",
        "baijia" => "百家号",
        "bilibili" => "哔哩哔哩",
        "douyin" => "抖音",
        _ => "自媒体平台",
    }
}

/// 从标题生成文件名 slug：保留中英文数字，其余转连字符，掐到 40 字符。
fn slugify(title: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for c in title.chars() {
        if c.is_alphanumeric() {
            out.push(c);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
        if out.chars().count() >= 40 {
            break;
        }
    }
    let s = out.trim_matches('-').to_string();
    if s.is_empty() {
        "untitled".to_string()
    } else {
        s
    }
}

fn today_stamp() -> String {
    chrono::Local::now().format("%Y%m%d").to_string()
}

/// 文章产物路径：`~/PolarisGEO/articles/{platform}/{date}-{slug}.md`
fn article_path_for(platform: &str, title: &str) -> PathBuf {
    home()
        .join("PolarisGEO")
        .join("articles")
        .join(platform)
        .join(format!("{}-{}.md", today_stamp(), slugify(title)))
}

/// job 日志路径：`~/PolarisGEO/logs/jobs/{job_id}.log`
fn log_path_for(job_id: &str) -> PathBuf {
    home()
        .join("PolarisGEO")
        .join("logs")
        .join("jobs")
        .join(format!("{job_id}.log"))
}

/// 追加一行带时间戳的日志（best-effort，逐次开文件——投递 job 日志量小，够用）。
fn log_line(log_path: &Path, msg: &str) {
    if let Some(parent) = log_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
    {
        let ts = chrono::Local::now().format("%H:%M:%S");
        let _ = writeln!(f, "[{ts}] {msg}");
    }
}

// ───────────────────────── 脚本路径解析（物化优先，回退仓库） ─────────────────────────

/// 解析某技能脚本的可执行路径：优先运行期物化 `~/PolarisGEO/skills/{skill}/scripts/{script}`，
/// 缺失回退仓库源码 `{CARGO_MANIFEST_DIR}/src/templates/skills/{skill}/scripts/{script}`。
/// 返回 (路径, 来源说明)；两处都没有返回 Err。
fn resolve_skill_script(skill: &str, script: &str) -> Result<(PathBuf, String), String> {
    let materialized = home()
        .join("PolarisGEO")
        .join("skills")
        .join(skill)
        .join("scripts")
        .join(script);
    if materialized.is_file() {
        return Ok((materialized, "运行期物化".to_string()));
    }
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("templates")
        .join("skills")
        .join(skill)
        .join("scripts")
        .join(script);
    if repo.is_file() {
        return Ok((repo, "仓库 templates 回退".to_string()));
    }
    Err(format!(
        "脚本 {skill}/scripts/{script} 未找到（既不在 {} 也不在仓库 templates）——请重启应用触发技能物化",
        materialized.display()
    ))
}

/// 探测一个可用的 python 解释器（防呆：没装 python 时给可读错误）。
fn resolve_python() -> Result<String, String> {
    let candidates: &[&str] = if cfg!(windows) {
        &["python", "python3", "py"]
    } else {
        &["python3", "python"]
    };
    for cand in candidates {
        let mut cmd = Command::new(cand);
        cmd.arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        no_window(&mut cmd);
        if let Ok(status) = cmd.status() {
            if status.success() {
                return Ok(cand.to_string());
            }
        }
    }
    Err("未找到 python 解释器（尝试过 python/python3/py 均不可用）——请先安装 Python 并确保在 PATH".to_string())
}

#[cfg_attr(not(windows), allow(unused_variables))]
fn no_window(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
}

// ───────────────────────── Markdown → 语义 HTML（公众号 typeset 前置） ─────────────────────────

/// 极简 Markdown → 语义 HTML：覆盖 # 标题 / 列表 / 引用 / 段落 / **粗体** / `代码`。
/// 目的是给 wechat_yiban 喂「干净语义正文」，由它再套主题样式——不追求完整 md 规范。
fn markdown_to_semantic_html(md: &str) -> String {
    let mut out = String::new();
    let mut in_ul = false;
    let mut in_ol = false;
    let close_lists = |out: &mut String, in_ul: &mut bool, in_ol: &mut bool| {
        if *in_ul {
            out.push_str("</ul>\n");
            *in_ul = false;
        }
        if *in_ol {
            out.push_str("</ol>\n");
            *in_ol = false;
        }
    };
    for raw in md.lines() {
        let line = raw.trim_end();
        let t = line.trim_start();
        if t.is_empty() {
            close_lists(&mut out, &mut in_ul, &mut in_ol);
            continue;
        }
        if let Some(rest) = t.strip_prefix("### ") {
            close_lists(&mut out, &mut in_ul, &mut in_ol);
            out.push_str(&format!("<h3>{}</h3>\n", inline_md(rest)));
        } else if let Some(rest) = t.strip_prefix("## ") {
            close_lists(&mut out, &mut in_ul, &mut in_ol);
            out.push_str(&format!("<h2>{}</h2>\n", inline_md(rest)));
        } else if let Some(rest) = t.strip_prefix("# ") {
            close_lists(&mut out, &mut in_ul, &mut in_ol);
            out.push_str(&format!("<h1>{}</h1>\n", inline_md(rest)));
        } else if let Some(rest) = t.strip_prefix("> ") {
            close_lists(&mut out, &mut in_ul, &mut in_ol);
            out.push_str(&format!("<blockquote>{}</blockquote>\n", inline_md(rest)));
        } else if let Some(rest) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
            if in_ol {
                out.push_str("</ol>\n");
                in_ol = false;
            }
            if !in_ul {
                out.push_str("<ul>\n");
                in_ul = true;
            }
            out.push_str(&format!("<li>{}</li>\n", inline_md(rest)));
        } else if let Some((num, rest)) = split_ordered(t) {
            let _ = num;
            if in_ul {
                out.push_str("</ul>\n");
                in_ul = false;
            }
            if !in_ol {
                out.push_str("<ol>\n");
                in_ol = true;
            }
            out.push_str(&format!("<li>{}</li>\n", inline_md(rest)));
        } else {
            close_lists(&mut out, &mut in_ul, &mut in_ol);
            out.push_str(&format!("<p>{}</p>\n", inline_md(t)));
        }
    }
    close_lists(&mut out, &mut in_ul, &mut in_ol);
    out
}

/// 识别有序列表行 "12. 正文"，返回 (序号串, 剩余)。
fn split_ordered(t: &str) -> Option<(&str, &str)> {
    let bytes = t.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i > 0 && i + 1 < bytes.len() && bytes[i] == b'.' && bytes[i + 1] == b' ' {
        Some((&t[..i], &t[i + 2..]))
    } else {
        None
    }
}

/// 行内：先 HTML 转义，再还原 **粗体** 与 `代码`（在转义后的文本上做，安全）。
fn inline_md(s: &str) -> String {
    let escaped = s
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    // **bold**
    let bolded = replace_pair(&escaped, "**", "<strong>", "</strong>");
    // `code`
    replace_pair(&bolded, "`", "<code>", "</code>")
}

/// 把成对的定界符替换成开合标签（落单的定界符原样保留）。
fn replace_pair(s: &str, delim: &str, open: &str, close: &str) -> String {
    let parts: Vec<&str> = s.split(delim).collect();
    if parts.len() < 3 {
        return s.to_string();
    }
    let mut out = String::new();
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            // 奇数分隔点=开，偶数=合；若最后落单（无配对合），补回定界符原文。
            let opening = i % 2 == 1;
            if opening && i == parts.len() - 1 {
                out.push_str(delim);
            } else {
                out.push_str(if opening { open } else { close });
            }
        }
        out.push_str(part);
    }
    out
}

// ───────────────────────── Claude 生成（自 spawn，参数纪律学 headless.rs） ─────────────────────────

/// 起一个只读 headless claude（allowedTools 仅 Read/Glob/Grep，物理上无法写文件），把 prompt
/// 经 stdin 喂进去，收集全部 assistant 文本返回。child 登记进 CHILDREN(key) 供 cancel 杀树。
fn run_claude_collect(job_id: &str, prompt: &str, log_path: &Path) -> Result<String, String> {
    let claude_bin: std::ffi::OsString = polaris_kernel::doctor::resolve_claude_exe()
        .map(|p| p.into_os_string())
        .ok_or_else(|| {
            "未找到 claude 可执行文件——请在环境医生里安装 Claude Code CLI".to_string()
        })?;

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut cmd = Command::new(&claude_bin);
    cmd.args([
        "--print",
        "--output-format",
        "stream-json",
        "--verbose",
        "--permission-mode=bypassPermissions",
        "--allowedTools",
        "Read,Glob,Grep", // 只读：正文由 Rust 落盘，claude 只出文本
    ])
    .current_dir(&cwd)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    polaris_kernel::doctor::harden_child_env(&mut cmd); // loopback NO_PROXY + 清 DEBUG/LD_PRELOAD
    polaris_kernel::provider::scope_child_claude(&mut cmd); // 隔离模式 → 私有会话账本
    no_window(&mut cmd);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0); // 让 kill_tree 能带走扇出的子孙
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("调起 claude 失败：{e}"))?;

    // 先把管道句柄摘出来，再把 Child 交给 CHILDREN 托管（cancel 靠它杀树）。
    let mut stdin = child.stdin.take();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let key = child_key(job_id);
    CHILDREN.insert(&key, child);

    if let Some(mut si) = stdin.take() {
        let _ = si.write_all(prompt.as_bytes());
        // drop si → 关 stdin，claude 收到 EOF 开始产出
    }

    // stderr 后台排空，失败时带上诊断。
    let stderr_buf = std::sync::Arc::new(Mutex::new(String::new()));
    if let Some(se) = stderr {
        let buf = stderr_buf.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(se).lines().map_while(Result::ok) {
                if !line.trim().is_empty() {
                    let mut b = buf.lock();
                    b.push_str(&line);
                    b.push('\n');
                }
            }
        });
    }

    // 墙钟看门狗：到点杀进程树，stdout 随之 EOF，读循环自然结束。
    let timed_out = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
    let watchdog = {
        let flag = timed_out.clone();
        let key_w = key.clone();
        std::thread::spawn(move || {
            if matches!(
                done_rx.recv_timeout(std::time::Duration::from_secs(GENERATE_TIMEOUT_SECS)),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout)
            ) {
                flag.store(true, Ordering::SeqCst);
                CHILDREN.kill(&key_w);
            }
        })
    };

    let mut collected = String::new();
    let mut result_err: Option<String> = None;
    if let Some(so) = stdout {
        for line in BufReader::new(so).lines().map_while(Result::ok) {
            if line.trim().is_empty() {
                continue;
            }
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            let ty = v.get("type").and_then(|x| x.as_str()).unwrap_or("");
            if ty == "result" {
                if let Some(st) = v.get("subtype").and_then(|x| x.as_str()) {
                    if st.starts_with("error") {
                        result_err = Some(format!("claude 返回错误：{st}"));
                        break;
                    }
                }
                continue;
            }
            if ty != "assistant" {
                continue;
            }
            let Some(content) = v
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_array())
            else {
                continue;
            };
            for block in content {
                match block.get("type").and_then(|x| x.as_str()) {
                    Some("tool_use") => {
                        let name = block.get("name").and_then(|x| x.as_str()).unwrap_or("");
                        log_line(log_path, &format!("claude 调用工具：{name}"));
                    }
                    Some("text") => {
                        if let Some(t) = block.get("text").and_then(|x| x.as_str()) {
                            collected.push_str(t);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    drop(done_tx); // 通知看门狗提前退出
    let _ = watchdog.join();
    // 正常读完：从池里摘回 child 收尸；若已被 cancel/超时摘走则为 None。
    let removed = CHILDREN.remove(&key);
    let had_child = removed.is_some();
    let mut status_ok = false;
    if let Some(mut c) = removed {
        status_ok = matches!(c.wait(), Ok(s) if s.success());
    }

    if timed_out.load(Ordering::SeqCst) {
        return Err(format!("claude 生成超时（{GENERATE_TIMEOUT_SECS}s）已终止"));
    }
    if job_is_canceled(job_id) {
        return Err("已取消".to_string());
    }
    if let Some(e) = result_err {
        return Err(e);
    }
    if !had_child {
        // child 被别处摘走（cancel）且非超时/取消标记——保守当失败。
        return Err("claude 子进程被中断".to_string());
    }
    if !status_ok && collected.trim().is_empty() {
        let se = stderr_buf.lock().clone();
        return Err(format!(
            "claude 异常退出{}",
            if se.trim().is_empty() {
                String::new()
            } else {
                format!("：{}", se.trim())
            }
        ));
    }
    if collected.trim().is_empty() {
        return Err("claude 未产出正文".to_string());
    }
    Ok(collected)
}

/// 剥掉 LLM 有时给正文套的 ```markdown / ``` 代码围栏。
fn strip_code_fence(s: &str) -> String {
    let t = s.trim();
    if let Some(rest) = t.strip_prefix("```") {
        // 去掉首行语言标注
        let after = rest.splitn(2, '\n').nth(1).unwrap_or("");
        let body = after.trim_end();
        if let Some(inner) = body.strip_suffix("```") {
            return inner.trim().to_string();
        }
    }
    t.to_string()
}

// ───────────────────────── 阶段执行 ─────────────────────────

/// generate：拼「主笔基础画像 + 平台补丁」系统设定 + 任务指令 → claude → 落 .md。
fn stage_generate(job: &MediaJob, log_path: &Path) -> Result<PathBuf, String> {
    let system = crate::expert::expert_media_doc("media-writer".to_string(), job.platform.clone());
    let cn = platform_cn(&job.platform);
    let topic = if job.topic.trim().is_empty() {
        "（未提供额外方向，按标题自行立意）".to_string()
    } else {
        job.topic.trim().to_string()
    };
    let prompt = format!(
        "{system}\n\n---\n\n# 写作任务\n\n\
你现在为「{cn}」平台撰写一篇成品正文。\n\n\
选题标题：{title}\n\
选题方向 / 要点：{topic}\n\n\
硬性要求：\n\
1. 直接输出 Markdown 正文，不要任何前后缀说明，不要用代码围栏包裹。\n\
2. 第一行用 `# 标题` 给出成品标题（可在选题标题上优化打磨）。\n\
3. 结构化：含吸睛引子、若干带 `## 小标题` 的主体段落、收束结尾。\n\
4. 正文不少于 1200 字，信息密度高，有观点有细节，杜绝空话套话与 AI 腔。\n\
5. 严格遵循上文系统设定里的平台调性、标题公式与红线。\n",
        system = system,
        cn = cn,
        title = job.title,
        topic = topic,
    );

    log_line(log_path, &format!("generate：调 Claude 为「{cn}」写《{}》", job.title));
    let raw = run_claude_collect(&job.id, &prompt, log_path)?;
    let body = strip_code_fence(&raw);
    let words = body.chars().count();
    log_line(log_path, &format!("generate：产出正文 {words} 字符"));

    let path = article_path_for(&job.platform, &job.title);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("建产物目录失败：{e}"))?;
    }
    std::fs::write(&path, &body).map_err(|e| format!("写文章失败：{e}"))?;
    log_line(log_path, &format!("generate：已落盘 {}", path.display()));
    Ok(path)
}

/// typeset：仅公众号有实义——md → 语义 HTML body（供 wechat_yiban --body-file）。
/// 其余平台空转（draft_uploader 直接吃 .md），返回 None 表示不改产物路径。
fn stage_typeset(job: &MediaJob, md_path: &Path, log_path: &Path) -> Result<Option<PathBuf>, String> {
    if job.platform != "wechat" {
        log_line(log_path, "typeset：非公众号平台，跳过（脚本直接吃 .md）");
        return Ok(None);
    }
    let md = std::fs::read_to_string(md_path).map_err(|e| format!("读文章失败：{e}"))?;
    let html = markdown_to_semantic_html(&md);
    let html_path = md_path.with_extension("body.html");
    std::fs::write(&html_path, &html).map_err(|e| format!("写语义 HTML 失败：{e}"))?;
    log_line(
        log_path,
        &format!("typeset：公众号语义 HTML → {}", html_path.display()),
    );
    Ok(Some(html_path))
}

/// upload：spawn python 跑对应平台脚本，捕获输出进日志，按退出码判成败。
fn stage_upload(job: &MediaJob, content_path: &Path, log_path: &Path) -> Result<String, String> {
    let python = resolve_python()?;

    let (skill, script, args): (&str, &str, Vec<String>) = if job.platform == "wechat" {
        (
            "wechat-md-typesetter",
            "wechat_yiban.py",
            vec![
                "--mode".into(),
                "publish".into(),
                "--body-file".into(),
                content_path.to_string_lossy().to_string(),
                "--title".into(),
                job.title.clone(),
            ],
        )
    } else {
        (
            "media-publisher",
            "draft_uploader.py",
            vec![
                "--platform".into(),
                job.platform.clone(),
                "--title".into(),
                job.title.clone(),
                "--content-file".into(),
                content_path.to_string_lossy().to_string(),
                "--close-after".into(),
            ],
        )
    };

    let (script_path, source) = resolve_skill_script(skill, script)?;
    log_line(
        log_path,
        &format!("upload：脚本 {}（{source}）", script_path.display()),
    );

    let mut cmd = Command::new(&python);
    cmd.arg(&script_path)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(dir) = script_path.parent() {
        cmd.current_dir(dir);
    }
    polaris_kernel::doctor::harden_child_env(&mut cmd);
    no_window(&mut cmd);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("调起 python 投递脚本失败：{e}"))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let key = child_key(&job.id);
    CHILDREN.insert(&key, child);

    // 边读边落日志，并记住「最后一行 result JSON」用于判定草稿是否成功。
    let last_result = std::sync::Arc::new(Mutex::new(String::new()));
    let mut handles = Vec::new();
    if let Some(so) = stdout {
        let lp = log_path.to_path_buf();
        let lr = last_result.clone();
        handles.push(std::thread::spawn(move || {
            for line in BufReader::new(so).lines().map_while(Result::ok) {
                let line = line.trim().to_string();
                if line.is_empty() {
                    continue;
                }
                log_line(&lp, &format!("  py> {line}"));
                if line.contains("\"result\"") {
                    *lr.lock() = line;
                }
            }
        }));
    }
    if let Some(se) = stderr {
        let lp = log_path.to_path_buf();
        handles.push(std::thread::spawn(move || {
            for line in BufReader::new(se).lines().map_while(Result::ok) {
                let line = line.trim().to_string();
                if !line.is_empty() {
                    log_line(&lp, &format!("  py! {line}"));
                }
            }
        }));
    }
    for h in handles {
        let _ = h.join();
    }

    // 管道 EOF：进程已（近乎）退出。摘回 child 收尸拿退出码；被 cancel 摘走则为 None。
    let removed = CHILDREN.remove(&key);
    if job_is_canceled(&job.id) {
        return Err("已取消".to_string());
    }
    let status = match removed {
        Some(mut c) => c.wait().map_err(|e| format!("等待投递脚本失败：{e}"))?,
        None => return Err("投递脚本被中断".to_string()),
    };

    let result_line = last_result.lock().clone();
    let result_val: Option<String> = serde_json::from_str::<serde_json::Value>(&result_line)
        .ok()
        .and_then(|v| v.get("result").and_then(|r| r.as_str()).map(|s| s.to_string()));

    if !status.success() {
        return Err(format!(
            "投递脚本退出码 {:?}{}",
            status.code(),
            result_val
                .as_ref()
                .map(|r| format!("（result={r}）"))
                .unwrap_or_default()
        ));
    }
    // 退出码 0：draft_uploaded / manual_assist / need_login 都算「已推进到平台后台」；
    // 明确 failed 才当失败。
    if result_val.as_deref() == Some("failed") {
        return Err("投递脚本报告 result=failed".to_string());
    }
    Ok(result_val.unwrap_or_else(|| "draft_uploaded".to_string()))
}

// ───────────────────────── job 主流程 ─────────────────────────

fn run_job(job_id: String) {
    let Some(job0) = get_job(&job_id) else {
        return;
    };
    let log_path = PathBuf::from(&job0.log_path);
    log_line(
        &log_path,
        &format!(
            "job {job_id} 开跑：platform={} title=《{}》 stages={:?}",
            job0.platform, job0.title, job0.stages
        ),
    );
    update_job(&job_id, |j| j.status = "running".to_string());

    // 队列跟进：置 running；度量 run。
    if let Some(qid) = &job0.queue_id {
        let _ = crate::mediaops::mediaops_queue_update(
            qid.clone(),
            Some("running".to_string()),
            None,
            None,
        );
    }
    let _ = crate::mediaops::mediaops_metric_add(
        job0.platform.clone(),
        "run".to_string(),
        None,
        None,
        Some(format!("job {job_id} 开跑")),
    );

    // 当前用于投递的内容路径：generate 产出 .md；typeset 可能改成 .html body。
    let mut content_path: Option<PathBuf> = job0.article_path.as_ref().map(PathBuf::from);

    let fail = |job_id: &str, log_path: &Path, platform: &str, queue_id: &Option<String>, stage: &str, err: String| {
        log_line(log_path, &format!("{stage} 失败：{err}"));
        update_job(job_id, |j| {
            j.status = "failed".to_string();
            j.error = Some(err.clone());
        });
        if let Some(qid) = queue_id {
            let _ = crate::mediaops::mediaops_queue_update(
                qid.clone(),
                Some("failed".to_string()),
                Some(err.clone()),
                None,
            );
        }
        let _ = crate::mediaops::mediaops_metric_add(
            platform.to_string(),
            "fail".to_string(),
            None,
            None,
            Some(format!("{stage}：{err}")),
        );
    };

    for stage in &job0.stages {
        if job_is_canceled(&job_id) {
            log_line(&log_path, "检测到取消，停止后续阶段");
            return;
        }
        update_job(&job_id, |j| j.stage = stage.clone());

        match stage.as_str() {
            STAGE_GENERATE => match stage_generate(&job0, &log_path) {
                Ok(path) => {
                    content_path = Some(path.clone());
                    let ps = path.to_string_lossy().to_string();
                    update_job(&job_id, |j| j.article_path = Some(ps.clone()));
                    if let Some(qid) = &job0.queue_id {
                        let _ = crate::mediaops::mediaops_queue_update(
                            qid.clone(),
                            None,
                            None,
                            Some(ps),
                        );
                    }
                }
                Err(e) => {
                    return fail(&job_id, &log_path, &job0.platform, &job0.queue_id, "generate", e);
                }
            },
            STAGE_TYPESET => {
                let Some(md) = content_path.clone() else {
                    return fail(
                        &job_id,
                        &log_path,
                        &job0.platform,
                        &job0.queue_id,
                        "typeset",
                        "缺正文（未先 generate 也未提供 article_path）".to_string(),
                    );
                };
                match stage_typeset(&job0, &md, &log_path) {
                    Ok(Some(html)) => content_path = Some(html),
                    Ok(None) => {}
                    Err(e) => {
                        return fail(&job_id, &log_path, &job0.platform, &job0.queue_id, "typeset", e)
                    }
                }
            }
            STAGE_UPLOAD => {
                let Some(cp) = content_path.clone() else {
                    return fail(
                        &job_id,
                        &log_path,
                        &job0.platform,
                        &job0.queue_id,
                        "upload",
                        "缺正文（未先 generate 也未提供 article_path）".to_string(),
                    );
                };
                match stage_upload(&job0, &cp, &log_path) {
                    Ok(result) => {
                        log_line(&log_path, &format!("upload：成功（result={result}）"));
                        if let Some(qid) = &job0.queue_id {
                            let _ = crate::mediaops::mediaops_queue_update(
                                qid.clone(),
                                Some("draft_uploaded".to_string()),
                                Some(format!("result={result}")),
                                None,
                            );
                        }
                        let _ = crate::mediaops::mediaops_metric_add(
                            job0.platform.clone(),
                            "draft".to_string(),
                            None,
                            None,
                            Some(format!("job {job_id} result={result}")),
                        );
                    }
                    Err(e) => {
                        return fail(&job_id, &log_path, &job0.platform, &job0.queue_id, "upload", e)
                    }
                }
            }
            other => {
                log_line(&log_path, &format!("未知阶段 {other}，跳过"));
            }
        }
    }

    if job_is_canceled(&job_id) {
        return;
    }
    update_job(&job_id, |j| {
        j.status = "done".to_string();
        j.stage = "done".to_string();
    });
    log_line(&log_path, &format!("job {job_id} 完成"));
}

// ───────────────────────── Commands ─────────────────────────

/// 启动一条投递流水线 job（异步跑，立即返回 job 快照）。
///
/// 入参二选一确定平台/标题：
///   - 传 `queue_id`：从 mediaops 队列取平台/标题（跟进它的状态机与产物路径）；
///   - 或直接传 `platform` + `title`（+可选 `topic` 选题方向）。
/// `stages` 省略=跑全部 `[generate, typeset, upload]`；可只跑子集。
/// 跳过 generate 时用 `article_path`（或队列项已存的产物路径）作为正文来源。
#[cfg_attr(feature = "desktop", tauri::command)]
#[allow(clippy::too_many_arguments)]
pub fn media_job_start(
    queue_id: Option<String>,
    platform: Option<String>,
    title: Option<String>,
    topic: Option<String>,
    stages: Option<Vec<String>>,
    article_path: Option<String>,
) -> Result<MediaJob, String> {
    // 解析平台 / 标题 / 已存产物：优先 queue_id，回落显式入参。
    let (platform, title, queue_article) = if let Some(qid) = &queue_id {
        let state = crate::mediaops::mediaops_state();
        let item = state
            .queue
            .into_iter()
            .find(|q| &q.id == qid)
            .ok_or_else(|| format!("队列项不存在：{qid}"))?;
        (item.platform, item.title, item.article_path)
    } else {
        let p = platform
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "缺 platform（或改传 queue_id）".to_string())?;
        let t = title
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "缺 title（或改传 queue_id）".to_string())?;
        (p, t, None)
    };

    // 阶段清洗：只保留合法阶段，去重，按 ALL_STAGES 固定顺序排列。
    let requested: Vec<String> = stages.unwrap_or_else(|| ALL_STAGES.iter().map(|s| s.to_string()).collect());
    let mut stages_norm: Vec<String> = Vec::new();
    for s in ALL_STAGES {
        if requested.iter().any(|r| r.trim() == *s) && !stages_norm.iter().any(|x| x == s) {
            stages_norm.push(s.to_string());
        }
    }
    if stages_norm.is_empty() {
        return Err(format!(
            "无有效阶段（合法值：{}）",
            ALL_STAGES.join(" / ")
        ));
    }
    // 不跑 generate 就必须有现成正文来源。
    let seed_article = article_path.filter(|s| !s.trim().is_empty()).or(queue_article);
    if !stages_norm.iter().any(|s| s == STAGE_GENERATE) && seed_article.is_none() {
        return Err("未包含 generate 阶段时必须提供 article_path（或队列项已有产物）".to_string());
    }

    let job_id = gen_id();
    let log_path = log_path_for(&job_id);
    let job = MediaJob {
        id: job_id.clone(),
        queue_id: queue_id.clone(),
        platform,
        title,
        topic: topic.unwrap_or_default(),
        stages: stages_norm,
        status: "pending".to_string(),
        stage: String::new(),
        article_path: seed_article,
        log_path: log_path.to_string_lossy().to_string(),
        error: None,
        created_at: now_secs(),
        updated_at: now_secs(),
    };
    JOBS.lock().insert(job_id.clone(), job.clone());

    // 后台线程跑，阻塞式子进程不占用命令线程。
    std::thread::spawn(move || run_job(job_id));
    Ok(job)
}

/// 查一条 job 的运行态。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn media_job_status(job_id: String) -> Result<MediaJob, String> {
    get_job(&job_id).ok_or_else(|| format!("job 不存在：{job_id}"))
}

/// 列出全部 job（新→旧）。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn media_job_list() -> Vec<MediaJob> {
    let mut v: Vec<MediaJob> = JOBS.lock().values().cloned().collect();
    v.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    v
}

/// 取消一条 job：置 canceled + 杀当前在跑的子进程树（claude 或 python）。
/// 已结束的 job 返回 Ok（幂等）。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn media_job_cancel(job_id: String) -> Result<(), String> {
    let existed = {
        let mut jobs = JOBS.lock();
        match jobs.get_mut(&job_id) {
            Some(j) => {
                if j.status == "done" || j.status == "failed" {
                    return Ok(()); // 已终态，无需取消
                }
                j.status = "canceled".to_string();
                j.error = Some("用户取消".to_string());
                j.updated_at = now_secs();
                true
            }
            None => false,
        }
    };
    if !existed {
        return Err(format!("job 不存在：{job_id}"));
    }
    // 杀进程树（复用 runtime kill_tree）；不在跑则 no-op。
    CHILDREN.kill(&child_key(&job_id));
    log_line(&log_path_for(&job_id), "job 已被用户取消");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_keeps_alnum_and_cjk() {
        assert_eq!(slugify("Hello World 测试!!"), "Hello-World-测试");
        assert_eq!(slugify("***"), "untitled");
    }

    #[test]
    fn md_to_html_basic() {
        let md = "# 标题\n\n引子段落 **重点**。\n\n## 小节\n\n- 一\n- 二\n\n1. 甲\n2. 乙\n";
        let html = markdown_to_semantic_html(md);
        assert!(html.contains("<h1>标题</h1>"));
        assert!(html.contains("<h2>小节</h2>"));
        assert!(html.contains("<strong>重点</strong>"));
        assert!(html.contains("<ul>\n<li>一</li>"));
        assert!(html.contains("<ol>\n<li>甲</li>"));
    }

    #[test]
    fn inline_escapes_then_marks() {
        let out = inline_md("a<b> **粗** `c`");
        assert!(out.contains("a&lt;b&gt;"));
        assert!(out.contains("<strong>粗</strong>"));
        assert!(out.contains("<code>c</code>"));
    }

    #[test]
    fn strip_fence_unwraps() {
        let s = "```markdown\n# 标题\n正文\n```";
        assert_eq!(strip_code_fence(s), "# 标题\n正文");
        assert_eq!(strip_code_fence("# 无围栏"), "# 无围栏");
    }

    #[test]
    fn stages_normalize_order_and_dedup() {
        // 借 media_job_start 的清洗逻辑间接验证顺序/去重（纯函数部分在此重演）。
        let requested = vec!["upload".to_string(), "generate".to_string(), "upload".to_string()];
        let mut norm: Vec<String> = Vec::new();
        for s in ALL_STAGES {
            if requested.iter().any(|r| r.trim() == *s) && !norm.iter().any(|x| x == s) {
                norm.push(s.to_string());
            }
        }
        assert_eq!(norm, vec!["generate", "upload"]);
    }
}
