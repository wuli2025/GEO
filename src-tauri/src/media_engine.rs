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
const STAGE_IMAGE: &str = "image";
const STAGE_TYPESET: &str = "typeset";
const STAGE_UPLOAD: &str = "upload";
const ALL_STAGES: &[&str] = &[STAGE_GENERATE, STAGE_IMAGE, STAGE_TYPESET, STAGE_UPLOAD];

/// 生成阶段的墙钟超时（秒）——卡住必须能放手，别永久钉死后台线程。
const GENERATE_TIMEOUT_SECS: u64 = 420;

// ───────────────────────── 数据类型 ─────────────────────────

/// 流程详情视图里的一格步骤（细粒度事件，按时间序追加）。
///
/// 归因字段（expert_id / skill_id / prompt …）是「点开可见全程留痕」的数据源：每一格都要
/// 答得出「谁干的、用哪个技能、喂进去的提示词长什么样」。全部 `serde(default)`——老 job
/// 快照没有这些字段，反序列化回来留空即可，UI 侧按空值降级显示。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobStep {
    /// 稳定 key：粗粒度用阶段名（generate/typeset/upload），
    /// upload 内的细步骤用 "upload:title_filled" 这类脚本回执 step 名。
    pub key: String,
    /// 人话标签（直接进 UI）。
    pub label: String,
    /// "run" | "ok" | "fail" | "skip"
    pub status: String,
    /// 补充说明（产物路径 / 字数 / 失败原因摘要…）。
    #[serde(default)]
    pub detail: String,
    pub at: i64,

    // ── 归因（留痕三问：哪个专家 / 哪个 skill / 什么提示词） ──
    /// 编排里负责本环节的专家 id（来自 PlatformSettings.workflow，空=本步不由专家驱动）。
    #[serde(default)]
    pub expert_id: String,
    /// 专家人话名（快照当时的名字，专家改名不影响历史留痕）。
    #[serde(default)]
    pub expert_name: String,
    /// 编排里本环节挂的技能 id。
    #[serde(default)]
    pub skill_id: String,
    /// 本步实际落地的技能脚本（upload 阶段是 py 文件绝对路径）。
    #[serde(default)]
    pub skill_script: String,
    /// 喂给模型的提示词全文快照（仅 generate 有）。留全文而非 hash——
    /// 原型的价值就在于点开能逐字看到当时到底喂了什么。
    #[serde(default)]
    pub prompt: String,
    /// 快照当时该专家在本平台的 overlay 版本 id（evolution.rs 的 PromptVersion.id）。
    /// 有值即可在详情里一键跳到那一版并回滚。
    #[serde(default)]
    pub prompt_version_id: String,
    /// 专家卡上的推荐模型档。注意：generate 走 claude CLI 默认模型，未显式 --model 下发，
    /// 故这里是「专家建议用什么」而非「实际跑的是什么」，UI 文案不许含混。
    #[serde(default)]
    pub model_hint: String,
    /// 本步耗时（run → ok/fail 时算出；仍在跑或无起点则 0）。
    #[serde(default)]
    pub duration_ms: i64,
    /// 本步开始时刻（算耗时用）。
    #[serde(default)]
    pub started_at: i64,
}

/// push_step 的归因附件。用 Default + 结构体字面量，避免给 push_step 加一长串位置参数。
#[derive(Debug, Clone, Default)]
pub struct StepAttr {
    pub expert_id: String,
    pub expert_name: String,
    pub skill_id: String,
    pub skill_script: String,
    pub prompt: String,
    pub prompt_version_id: String,
    pub model_hint: String,
}

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
    /// 生成阶段下发给 claude CLI 的 `--model`（空=CLI 默认模型）。跑高档模型
    /// （如 claude-opus-4-8）出正式稿时由启动方显式指定，随 job 持久化、续跑不变。
    #[serde(default)]
    pub model: String,
    /// 本 job 要跑的阶段（generate/typeset/upload 的子集，按此顺序执行）。
    /// ★ 这是**原始**阶段列表，续跑不会改写它——详情时间线与二次续跑都依赖它保持完整。
    pub stages: Vec<String>,
    /// 续跑时判定为「已完成、本轮跳过」的阶段。run_job 遍历 stages 时据此 continue。
    /// 每次 start/resume 都会重算，非续跑场景恒为空。
    #[serde(default)]
    pub skip_stages: Vec<String>,
    /// "pending" | "running" | "done" | "failed" | "canceled"
    pub status: String,
    /// 当前/最后所处阶段（空=未开跑）。
    #[serde(default)]
    pub stage: String,
    /// 结构化步骤时间线（流程详情视图的数据源）。
    #[serde(default)]
    pub steps: Vec<JobStep>,
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

/// 一条 job 的「总规划提示词」——整篇文章怎么写的那一整段（详情卡中栏的默认内容）。
/// 与步骤级提示词区分开：那是某一格局部喂了什么，这是整篇的总纲。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaJobPlan {
    /// 提示词全文（空=这条 job 不含 generate 阶段，用的是现成正文）。
    pub prompt: String,
    /// 站在「写作」这一格上的专家（改提示词入口就指向他）。
    pub expert_id: String,
    pub expert_name: String,
    /// true=generate 已跑过，这是当时真正喂进去的原文快照；
    /// false=还没跑到，按当前专家画像+平台补丁+品牌契约现拼的预览。
    pub recorded: bool,
}

// ───────────────────────── 进程内 job 注册表（落盘持久化，重启不丢历史） ─────────────────────────

/// 历史 job 快照文件：`~/PolarisGEO/logs/jobs/index.json`（原子写，保留最近 JOBS_KEEP 条）。
const JOBS_KEEP: usize = 200;

fn jobs_index_path() -> PathBuf {
    home()
        .join("PolarisGEO")
        .join("logs")
        .join("jobs")
        .join("index.json")
}

/// 启动时回灌历史：上次进程里还在跑的 job 已随进程死掉，标成 failed 给个可读说明。
fn load_jobs() -> HashMap<String, MediaJob> {
    let mut map = HashMap::new();
    let Ok(raw) = std::fs::read_to_string(jobs_index_path()) else {
        return map;
    };
    let Ok(list) = serde_json::from_str::<Vec<MediaJob>>(&raw) else {
        return map;
    };
    for mut j in list {
        if j.status == "running" || j.status == "pending" {
            j.status = "failed".to_string();
            j.error = Some("应用重启中断：进程死掉时此 job 正在跑——点「继续」可从断点重跑".to_string());
            for s in j.steps.iter_mut().filter(|s| s.status == "run") {
                s.status = "fail".to_string();
                // 无条件覆盖：残留的「正在写作…」之类进行时文案配上 fail 状态极具误导性。
                s.detail = "应用重启中断，此步没有跑完——点「继续」从断点重跑".to_string();
            }
        }
        map.insert(j.id.clone(), j);
    }
    map
}

/// 快照落盘（best-effort 原子写）。持有 JOBS 锁时勿调用。
///
/// **读-合并-写**，不是全量覆盖：桌面壳与 `polaris-cli media-run` 共用同一份 index.json，
/// 全量覆盖会把对方刚写的 job 整条抹掉。合并规则——同 id 取 `updated_at` 更新的那份，
/// 盘上独有的 id 一律保留。启动时被判死的 job（[`load_jobs`] 不动 `updated_at`）因此
/// 打不过真正在跑它的那个进程的后续写入，会被对方自动纠正回来。
fn save_jobs() {
    let mine: Vec<MediaJob> = JOBS.lock().values().cloned().collect();
    let mut merged: HashMap<String, MediaJob> = std::fs::read_to_string(jobs_index_path())
        .ok()
        .and_then(|raw| serde_json::from_str::<Vec<MediaJob>>(&raw).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|j| (j.id.clone(), j))
        .collect();
    for j in mine {
        match merged.get(&j.id) {
            Some(disk) if disk.updated_at > j.updated_at => {} // 盘上更新 → 别人在驱动，让位
            _ => {
                merged.insert(j.id.clone(), j);
            }
        }
    }
    let mut list: Vec<MediaJob> = merged.into_values().collect();
    list.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    list.truncate(JOBS_KEEP);
    let path = jobs_index_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string(&list) {
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

static JOBS: Lazy<Mutex<HashMap<String, MediaJob>>> = Lazy::new(|| Mutex::new(load_jobs()));

/// 本进程里还没跑完的 job 数（pending + running）。
/// 桌面壳据此决定「点窗口 ✕ 是退出还是转入后台」——有活就不能让进程死，
/// 否则 `CHILDREN.kill_all()` 会把在飞的 claude/python 一锅端（见 lib.rs 退出钩子）。
pub fn live_job_count() -> usize {
    JOBS.lock()
        .values()
        .filter(|j| j.status == "running" || j.status == "pending")
        .count()
}
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
    {
        let mut jobs = JOBS.lock();
        if let Some(j) = jobs.get_mut(job_id) {
            f(j);
            j.updated_at = now_secs();
        }
    }
    save_jobs(); // 每次状态推进都落快照——job 步频低、文件小，重启不丢换这点 IO 值得
}

/// 记一格步骤：同 key 已存在则原位更新状态/说明（run→ok/fail），否则追加。
fn push_step(job_id: &str, key: &str, label: &str, status: &str, detail: &str) {
    push_step_attr(job_id, key, label, status, detail, StepAttr::default());
}

/// 带归因的记步。原位更新时归因只做「非空覆盖」：run 那一格已经写下了专家/提示词，
/// 收尾的 ok/fail 传空 attr 不该把它们抹掉。
fn push_step_attr(
    job_id: &str,
    key: &str,
    label: &str,
    status: &str,
    detail: &str,
    attr: StepAttr,
) {
    update_job(job_id, |j| {
        let now = now_secs();
        if let Some(s) = j.steps.iter_mut().rev().find(|s| s.key == key) {
            s.status = status.to_string();
            if !detail.is_empty() {
                s.detail = detail.to_string();
            }
            let set = |dst: &mut String, src: String| {
                if !src.is_empty() {
                    *dst = src;
                }
            };
            set(&mut s.expert_id, attr.expert_id);
            set(&mut s.expert_name, attr.expert_name);
            set(&mut s.skill_id, attr.skill_id);
            set(&mut s.skill_script, attr.skill_script);
            set(&mut s.prompt, attr.prompt);
            set(&mut s.prompt_version_id, attr.prompt_version_id);
            set(&mut s.model_hint, attr.model_hint);
            if s.started_at > 0 && status != "run" {
                s.duration_ms = (now - s.started_at) * 1000;
            }
            s.at = now;
        } else {
            j.steps.push(JobStep {
                key: key.to_string(),
                label: label.to_string(),
                status: status.to_string(),
                detail: detail.to_string(),
                at: now,
                expert_id: attr.expert_id,
                expert_name: attr.expert_name,
                skill_id: attr.skill_id,
                skill_script: attr.skill_script,
                prompt: attr.prompt,
                prompt_version_id: attr.prompt_version_id,
                model_hint: attr.model_hint,
                duration_ms: 0,
                started_at: now,
            });
        }
    });
}

/// 取某平台某环节的「谁 + 什么技能」，并把专家卡上的人话名 / 推荐模型一并带出。
/// 编排里查无此环节时回空 attr——调用方照常记步，UI 侧显示「未编排」而非崩掉。
fn attr_for(platform: &str, step: &str) -> StepAttr {
    let Some(w) = crate::mediaops::workflow_step_for(platform, step) else {
        return StepAttr::default();
    };
    let card = crate::expert::expert_card_by_id(&w.expert_id);
    StepAttr {
        expert_name: card
            .as_ref()
            .map(|c| c.name.clone())
            .unwrap_or_else(|| w.expert_id.clone()),
        model_hint: card.map(|c| c.model_hint).unwrap_or_default(),
        expert_id: w.expert_id,
        skill_id: w.skill_id,
        ..Default::default()
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
        "csdn" => "CSDN",
        "juejin" => "掘金",
        _ => "自媒体平台",
    }
}

/// 阶段的人话标签（步骤时间线用）。
fn stage_label(stage: &str) -> &'static str {
    match stage {
        STAGE_GENERATE => "生成正文（Claude 主笔）",
        STAGE_IMAGE => "配图（读文出画面描述 → AI 生成封面与插图）",
        STAGE_TYPESET => "排版（Markdown → 语义 HTML）",
        STAGE_UPLOAD => "投递草稿（平台后台）",
        _ => "未知阶段",
    }
}

/// 执行面阶段 → 编排（PlatformSettings.workflow）里的环节名。
///
/// 两套「流程」概念的接缝：编排是 8 步声明式蓝图（选题/调研/写作/…/投递），执行面只跑
/// 其中 3 步。这个映射就是把执行到的那 3 步认领回蓝图里的对应环节，好取出它配的专家。
fn stage_workflow_step(stage: &str) -> &'static str {
    match stage {
        STAGE_GENERATE => "写作",
        STAGE_IMAGE => "配图",
        STAGE_TYPESET => "排版",
        STAGE_UPLOAD => "投递",
        _ => "",
    }
}

/// upload 脚本回执 step 名 → 人话标签（未识别的原样返回 step 名）。
fn upload_step_label(step: &str) -> String {
    match step {
        "browser_launched" => "浏览器已拉起".to_string(),
        "cdp_attached" => "CDP 已接管浏览器".to_string(),
        "page_opened" => "编辑页已打开".to_string(),
        "login_ok" => "登录态正常".to_string(),
        "need_login" => "等待扫码登录".to_string(),
        "title_filled" => "标题已填入".to_string(),
        "body_injected" => "正文已注入".to_string(),
        "cover_set" => "封面已设置".to_string(),
        "images_uploaded" => "配图已上传".to_string(),
        "draft_saved" => "草稿已保存".to_string(),
        other => other.replace('_', " "),
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
        if t.starts_with("![") {
            // 图片行：`![alt](src)` → <img>（配图阶段插入的本地文件路径）
            close_lists(&mut out, &mut in_ul, &mut in_ol);
            if let Some((alt, src)) = t.strip_prefix("![").and_then(|r| r.split_once("](")).map(|(a, b)| (a, b.trim_end_matches(')'))) {
                out.push_str(&format!("<p><img src=\"{}\" alt=\"{}\" style=\"max-width:100%\"/></p>\n", src, alt));
            }
        } else if let Some(rest) = t.strip_prefix("### ") {
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
fn run_claude_collect(
    job_id: &str,
    prompt: &str,
    log_path: &Path,
    model: &str,
) -> Result<String, String> {
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
    ]);
    if !model.trim().is_empty() {
        cmd.args(["--model", model.trim()]);
    }
    cmd
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

/// 拼这条 job 的**总规划提示词**：写作专家基础画像 + 本平台补丁 + insight 卡 + 写作任务
/// + 品牌植入契约。这一整段就是「整篇文章怎么写」的全部依据，generate 原样喂给模型。
///
/// 专家取自平台编排的「写作」环节（mediaops 设置里可改），不再硬编码 media-writer——
/// 否则详情页里的「哪个专家」永远是同一个常量，留痕就成了摆设。编排缺「写作」环节时
/// 回落 media-writer 并在日志里说明，保证老配置照跑。
///
/// 抽成纯拼装函数是为了详情卡：中栏要在 job 还没跑到 generate（甚至还在排队）时就能把
/// 总纲原文摊开，不能干等步骤留痕。`log` 传 None 即静默——预览路径不该往运行日志里写字。
fn build_generate_prompt(job: &MediaJob, log: Option<&Path>) -> (StepAttr, String) {
    let mut attr = attr_for(&job.platform, "写作");
    if attr.expert_id.is_empty() {
        if let Some(lp) = log {
            log_line(lp, "generate：平台编排里没有「写作」环节，回落 media-writer");
        }
        attr = StepAttr {
            expert_id: "media-writer".to_string(),
            expert_name: crate::expert::expert_card_by_id("media-writer")
                .map(|c| c.name)
                .unwrap_or_else(|| "自媒体主笔".to_string()),
            ..Default::default()
        };
    }
    let system = crate::expert::expert_media_doc(attr.expert_id.clone(), job.platform.clone());
    // 留痕到具体版本：这一步用的是该专家在本平台的第几版补丁。
    attr.prompt_version_id = crate::evolution::active_prompt_version(
        &attr.expert_id,
        &job.platform,
        crate::evolution::ANCHOR_PLATFORM_OVERLAY,
    )
    .map(|v| v.id)
    .unwrap_or_default();
    // 循环工程闭环：insight 卡按范围/功劳分选出，注入系统设定——卡库改变行为，而非死账本。
    let insights = crate::evolution::insights_for_prompt(&attr.expert_id, &job.platform);
    if !insights.is_empty() {
        if let Some(lp) = log {
            log_line(
                lp,
                &format!("generate：注入 insight 卡 {} 张", insights.matches("\n- ").count()),
            );
        }
    }
    let cn = platform_cn(&job.platform);
    let topic = if job.topic.trim().is_empty() {
        "（未提供额外方向，按标题自行立意）".to_string()
    } else {
        job.topic.trim().to_string()
    };
    let mut prompt = format!(
        "{system}{insights}\n\n---\n\n# 写作任务\n\n\
为「{cn}」写一篇成品正文。选题标题：{title}\n\
方向/要点：{topic}\n\n\
硬性要求：\n\
1. 只输出 Markdown 正文，无前后缀说明与代码围栏。\n\
2. 首行 `# 标题`（可在选题标题上打磨）。\n\
3. 吸睛引子 + 若干 `## 小标题` 主体段 + 收束结尾。\n\
4. 不少于 1200 字，信息密度高、有观点有细节，杜绝套话与 AI 腔。\n\
5. 严格遵循上文系统设定的平台调性、标题公式与红线。\n",
        system = system,
        cn = cn,
        title = job.title,
        topic = topic,
    );
    // ── 推广植入（方案 A：提示词层注入）：读 brand.json，把「品牌植入契约」织进
    // 写作提示词——在写作时织入而不是写完后再贴，正文与品牌同源才自然、才不触发
    // 平台硬广风控。档案未启用则一字不加，老行为零变化。
    if let Some((strength, block)) = crate::brand::contract_for(&job.platform) {
        if let Some(lp) = log {
            log_line(lp, &format!("generate：织入品牌植入契约（{strength}）"));
        }
        prompt.push_str("\n");
        prompt.push_str(&block);
    }
    (attr, prompt)
}

/// generate：拿总规划提示词喂 claude → 落 .md。
fn stage_generate(job: &MediaJob, log_path: &Path) -> Result<PathBuf, String> {
    let (attr, prompt) = build_generate_prompt(job, Some(log_path));
    let cn = platform_cn(&job.platform);

    log_line(
        log_path,
        &format!(
            "generate：{}（{}）为「{cn}」写《{}》{}",
            attr.expert_name,
            attr.expert_id,
            job.title,
            if attr.prompt_version_id.is_empty() {
                String::new()
            } else {
                format!("，补丁版本 {}", attr.prompt_version_id)
            }
        ),
    );
    // 提示词全文随步骤落盘——原型的核心就是点开这一格能逐字看到当时喂了什么。
    push_step_attr(
        &job.id,
        "generate",
        stage_label(STAGE_GENERATE),
        "run",
        &format!("{} 正在为「{cn}」写作…", attr.expert_name),
        StepAttr { prompt: prompt.clone(), ..attr.clone() },
    );
    let raw = run_claude_collect(&job.id, &prompt, log_path, &job.model)?;
    let body = strip_code_fence(&raw);
    let words = body.chars().count();
    log_line(log_path, &format!("generate：产出正文 {words} 字符"));
    push_step(&job.id, "generate:guard", "内容守卫（CLI 错误话术 + 最小字数）", "run", &format!("产出 {words} 字符，校验中"));

    // ── 内容守卫（板块H E2E 教训）：claude CLI 会把「模型不可用」等错误当普通文本
    // 输出（exit 0），无守卫时 121 字报错串曾一路排版、投递成真草稿。两道闸：
    // 1) 错误形态识别——CLI 已知错误话术直接判失败；2) 最小字数——正文要求 1200 字，
    // 低于 300 字符必是异常产物，宁可失败也不许污染草稿箱。
    let guard_err = |msg: String| -> String {
        log_line(log_path, &format!("generate：内容守卫拦截——{msg}"));
        push_step(&job.id, "generate:guard", "内容守卫（CLI 错误话术 + 最小字数）", "fail", &msg);
        format!("generate 内容守卫拦截：{msg}")
    };
    let low = body.to_ascii_lowercase();
    const CLI_ERROR_MARKS: &[&str] = &[
        "issue with the selected model",
        "run --model to pick a different model",
        "may not exist or you may not have access",
        "api error",
        "invalid api key",
        "credit balance is too low",
        "rate limit",
        "please run /login",
    ];
    if let Some(mark) = CLI_ERROR_MARKS.iter().find(|m| low.contains(**m)) {
        return Err(guard_err(format!(
            "产出命中 CLI 错误话术「{mark}」，疑为模型/通道故障回显而非正文（前 200 字符：{}）",
            body.chars().take(200).collect::<String>()
        )));
    }
    const MIN_ARTICLE_CHARS: usize = 300;
    if words < MIN_ARTICLE_CHARS {
        return Err(guard_err(format!(
            "正文仅 {words} 字符（下限 {MIN_ARTICLE_CHARS}），不足以成稿（前 200 字符：{}）",
            body.chars().take(200).collect::<String>()
        )));
    }

    push_step(&job.id, "generate:guard", "内容守卫（CLI 错误话术 + 最小字数）", "ok", &format!("{words} 字符，通过"));

    // ── 硬广守卫（推广植入方案四件套之③）：Rust 确定性正则拦截，不靠模型自觉。
    // 弱/零植入平台正文命中裸链/域名/微信号/手机号/二维码话术即判失败——防封的
    // backstop，宁可 fail 也绝不污染草稿箱。
    push_step(&job.id, "generate:brandguard", "硬广守卫（分平台植入强度）", "run", "按平台强度扫描引流特征");
    let violations = crate::brand::hard_ad_guard(&job.platform, &body);
    if !violations.is_empty() {
        let msg = format!("命中 {} 项：{}", violations.len(), violations.join("；"));
        log_line(log_path, &format!("generate：硬广守卫拦截——{msg}"));
        push_step(&job.id, "generate:brandguard", "硬广守卫（分平台植入强度）", "fail", &msg);
        return Err(format!("generate 硬广守卫拦截：{msg}"));
    }
    push_step(&job.id, "generate:brandguard", "硬广守卫（分平台植入强度）", "ok", "未命中引流特征，通过");

    let path = article_path_for(&job.platform, &job.title);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("建产物目录失败：{e}"))?;
    }
    std::fs::write(&path, &body).map_err(|e| format!("写文章失败：{e}"))?;
    log_line(log_path, &format!("generate：已落盘 {}", path.display()));
    Ok(path)
}

/// 配图阶段产物：封面 + 已插入正文的插图路径。
#[derive(Debug, Default)]
struct ImageAssets {
    cover: Option<PathBuf>,
    inline: Vec<PathBuf>,
}

/// image：配图导演（claude）通读正文自己决定画什么——输出封面 + 至多 2 张插图的画面
/// 描述 JSON，再逐张调 ark_image.py（火山方舟 Seedream，密钥走 providers 体系）落 PNG；
/// 插图按小标题就地插回 Markdown。**不写死任何画面 prompt**，全部由文本模型按文章内容定。
fn stage_image(job: &MediaJob, md_path: &Path, log_path: &Path) -> Result<ImageAssets, String> {
    let article = std::fs::read_to_string(md_path).map_err(|e| format!("读文章失败：{e}"))?;
    let excerpt: String = article.chars().take(4000).collect();

    let mut attr = attr_for(&job.platform, "配图");
    if attr.expert_id.is_empty() {
        attr.expert_id = "media-imagedirector".to_string();
        attr.expert_name = "配图导演".to_string();
    }
    let system = crate::expert::expert_media_doc(attr.expert_id.clone(), job.platform.clone());
    let cn = platform_cn(&job.platform);
    let prompt = format!(
        "{system}\n\n---\n\n# 配图任务\n\n\
通读将发布到「{cn}」的下文，自行决定封面与插图各画什么。只输出一个 JSON 对象，无解释无围栏：\n\
{{\"cover\": \"封面画面描述\", \"inline\": [{{\"heading\": \"小标题原文\", \"prompt\": \"插图画面描述\"}}]}}\n\
- cover 必填：60–120 字中文画面描述（主体/构图/色调/风格），画面**不出现任何文字**；\n\
- inline 0–2 张，heading 逐字取自正文某个 `## 小标题`；\n\
- 画面须来自文章具体信息点，禁通用模板。\n\n\
# 文章全文\n\n{excerpt}\n",
    );

    push_step_attr(
        &job.id,
        STAGE_IMAGE,
        stage_label(STAGE_IMAGE),
        "run",
        &format!("{} 通读正文，构思封面与插图…", attr.expert_name),
        StepAttr { prompt: prompt.clone(), ..attr.clone() },
    );

    // 1) 文本模型出画面描述（解析失败降级：用标题兜一个封面描述，不断流）
    let mut cover_prompt = String::new();
    let mut inline_plans: Vec<(String, String)> = Vec::new();
    match run_claude_collect(&job.id, &prompt, log_path, &job.model) {
        Ok(raw) => {
            let cleaned = strip_code_fence(&raw);
            let json_str = cleaned
                .find('{')
                .and_then(|s| cleaned.rfind('}').map(|e| &cleaned[s..=e]))
                .unwrap_or(cleaned.as_str());
            match serde_json::from_str::<serde_json::Value>(json_str) {
                Ok(v) => {
                    cover_prompt = v["cover"].as_str().unwrap_or("").trim().to_string();
                    if let Some(arr) = v["inline"].as_array() {
                        for it in arr.iter().take(2) {
                            let h = it["heading"].as_str().unwrap_or("").trim().to_string();
                            let p = it["prompt"].as_str().unwrap_or("").trim().to_string();
                            if !h.is_empty() && !p.is_empty() {
                                inline_plans.push((h, p));
                            }
                        }
                    }
                    log_line(log_path, &format!(
                        "image：配图导演出稿——封面 1 张，插图 {} 张", inline_plans.len()
                    ));
                }
                Err(e) => log_line(log_path, &format!("image：画面描述 JSON 解析失败（{e}），降级标题兜底")),
            }
        }
        Err(e) => log_line(log_path, &format!("image：配图导演调用失败（{e}），降级标题兜底")),
    }
    if cover_prompt.is_empty() {
        cover_prompt = format!(
            "为一篇题为《{}》的文章设计主题封面插画：画面主体紧扣标题含义，构图简洁、层次分明、色调统一，画面中不出现任何文字。",
            job.title
        );
    }

    // 2) 逐张调 ark_image.py 落盘
    let python = resolve_python()?;
    let (script_path, source) = resolve_skill_script("media-publisher", "ark_image.py")?;
    log_line(log_path, &format!("image：生图脚本 {}（{source}）", script_path.display()));
    let gen_one = |img_prompt: &str, out: &Path, size: &str, tag: &str| -> Result<(), String> {
        log_line(log_path, &format!("image：生成{tag}——{img_prompt}"));
        let out_s = out.to_string_lossy().to_string();
        let output = Command::new(&python)
            .arg(&script_path)
            .args(["--prompt", img_prompt, "--out", out_s.as_str(), "--size", size])
            .output()
            .map_err(|e| format!("调起生图脚本失败：{e}"))?;
        for line in String::from_utf8_lossy(&output.stdout).lines().chain(String::from_utf8_lossy(&output.stderr).lines()) {
            if !line.trim().is_empty() {
                log_line(log_path, &format!("  img> {line}"));
            }
        }
        if output.status.success() && out.is_file() {
            Ok(())
        } else {
            Err(format!("{tag}生成失败（exit={:?}）", output.status.code()))
        }
    };

    let mut assets = ImageAssets::default();
    let cover_out = md_path.with_extension("cover.png");
    match gen_one(&cover_prompt, &cover_out, "2048x1152", "封面") {
        Ok(()) => {
            push_step(&job.id, "image:cover", "封面图", "ok", &format!("{}", cover_out.display()));
            assets.cover = Some(cover_out);
        }
        Err(e) => push_step(&job.id, "image:cover", "封面图", "fail", &e),
    }

    // 3) 插图：生成成功的按小标题插回 Markdown
    let mut md_new = article.clone();
    for (i, (heading, p)) in inline_plans.iter().enumerate() {
        let out = md_path.with_extension(format!("img{}.png", i + 1));
        match gen_one(p, &out, "2048x1152", &format!("插图{}", i + 1)) {
            Ok(()) => {
                let anchor = format!("## {heading}");
                if let Some(pos) = md_new.find(anchor.as_str()) {
                    let line_end = md_new[pos..].find('\n').map(|o| pos + o + 1).unwrap_or(md_new.len());
                    md_new.insert_str(line_end, &format!("\n![配图]({})\n", out.display()));
                } else {
                    md_new.push_str(&format!("\n\n![配图]({})\n", out.display()));
                    log_line(log_path, &format!("image：正文未找到小标题「{heading}」，插图追加到文末"));
                }
                push_step(&job.id, &format!("image:i{}", i + 1), &format!("插图{}（{heading}）", i + 1), "ok", &format!("{}", out.display()));
                assets.inline.push(out);
            }
            Err(e) => push_step(&job.id, &format!("image:i{}", i + 1), &format!("插图{}（{heading}）", i + 1), "fail", &e),
        }
    }
    if md_new != article {
        std::fs::write(md_path, &md_new).map_err(|e| format!("回写插图失败：{e}"))?;
        log_line(log_path, "image：插图已插回正文 Markdown");
    }

    if assets.cover.is_none() && assets.inline.is_empty() {
        return Err("封面与插图全部生成失败（详见日志 img> 行；可到 API 中心检查生图模型配置）".to_string());
    }
    Ok(assets)
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
fn stage_upload(job: &MediaJob, content_path: &Path, assets: &ImageAssets, log_path: &Path) -> Result<String, String> {
    let python = resolve_python()?;

    let (skill, script, mut args): (&str, &str, Vec<String>) = if job.platform == "wechat" {
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
    // 配图阶段的产物接到投递参数：公众号走 --cover；其余平台把封面+插图并进 --images。
    if job.platform == "wechat" {
        if let Some(c) = &assets.cover {
            args.push("--cover".into());
            args.push(c.to_string_lossy().to_string());
        }
    } else {
        let imgs: Vec<String> = assets
            .cover
            .iter()
            .chain(assets.inline.iter())
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        if !imgs.is_empty() {
            args.push("--images".into());
            args.push(imgs.join(","));
        }
    }

    let (script_path, source) = resolve_skill_script(skill, script)?;
    log_line(
        log_path,
        &format!("upload：脚本 {}（{source}）", script_path.display()),
    );
    // 留痕这一步真正落地的技能脚本：编排里配的 skill_id 是意图，这里是实际跑的那个文件。
    push_step_attr(
        &job.id,
        STAGE_UPLOAD,
        stage_label(STAGE_UPLOAD),
        "run",
        "",
        StepAttr {
            skill_script: format!("{}（{source}）", script_path.display()),
            ..attr_for(&job.platform, "投递")
        },
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
    // 同时把脚本的每步 JSON 回执（{"step":"title_filled",...}）解析成结构化步骤，
    // 前端流程详情的「投递子步骤」就来自这里。
    let last_result = std::sync::Arc::new(Mutex::new(String::new()));
    let mut handles = Vec::new();
    if let Some(so) = stdout {
        let lp = log_path.to_path_buf();
        let lr = last_result.clone();
        let jid = job.id.clone();
        handles.push(std::thread::spawn(move || {
            for line in BufReader::new(so).lines().map_while(Result::ok) {
                let line = line.trim().to_string();
                if line.is_empty() {
                    continue;
                }
                log_line(&lp, &format!("  py> {line}"));
                if line.contains("\"result\"") {
                    *lr.lock() = line.clone();
                }
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
                    if let Some(step) = v.get("step").and_then(|s| s.as_str()) {
                        let ok = v.get("ok").and_then(|b| b.as_bool()).unwrap_or(true);
                        let detail = ["note", "detail", "method", "engine", "browser"]
                            .iter()
                            .find_map(|k| v.get(*k).and_then(|x| x.as_str()))
                            .unwrap_or("")
                            .to_string();
                        push_step(
                            &jid,
                            &format!("upload:{step}"),
                            &upload_step_label(step),
                            if ok { "ok" } else { "fail" },
                            &detail,
                        );
                    }
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
    // 配图产物（跳过 image 阶段时按约定位置捡历史产物：正文旁的 .cover.png / .imgN.png）
    let mut image_assets = ImageAssets::default();
    if let Some(md) = &content_path {
        let c = md.with_extension("cover.png");
        if c.is_file() {
            image_assets.cover = Some(c);
        }
        for i in 1..=2 {
            let p = md.with_extension(format!("img{i}.png"));
            if p.is_file() {
                image_assets.inline.push(p);
            }
        }
    }

    let fail = |job_id: &str, log_path: &Path, platform: &str, queue_id: &Option<String>, stage: &str, err: String| {
        log_line(log_path, &format!("{stage} 失败：{err}"));
        push_step(job_id, stage, stage_label(stage), "fail", &err);
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
        // 续跑：上一轮已完成且产物尚在的阶段本轮跳过。时间线里它那一格保持原来的 ok 不动。
        if job0.skip_stages.contains(stage) {
            log_line(&log_path, &format!("阶段 {stage} 上轮已完成，续跑跳过"));
            continue;
        }
        if job_is_canceled(&job_id) {
            log_line(&log_path, "检测到取消，停止后续阶段");
            return;
        }
        update_job(&job_id, |j| j.stage = stage.clone());
        // 开跑即写下归因：这一格由编排里的哪个专家 / 哪个技能负责。阶段函数随后会用更细的
        // attr（提示词全文、脚本实际路径）原位补齐——非空覆盖，不会把这里写的抹掉。
        push_step_attr(
            &job_id,
            stage,
            stage_label(stage),
            "run",
            "",
            attr_for(&job0.platform, stage_workflow_step(stage)),
        );

        match stage.as_str() {
            STAGE_GENERATE => match stage_generate(&job0, &log_path) {
                Ok(path) => {
                    content_path = Some(path.clone());
                    let ps = path.to_string_lossy().to_string();
                    push_step(&job_id, stage, stage_label(stage), "ok", &format!("正文已落盘：{ps}"));
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
            STAGE_IMAGE => {
                let Some(md) = content_path.clone() else {
                    return fail(
                        &job_id,
                        &log_path,
                        &job0.platform,
                        &job0.queue_id,
                        "image",
                        "缺正文（未先 generate 也未提供 article_path）".to_string(),
                    );
                };
                // 配图失败不断流：留 fail 步骤与日志，正文照常排版投递（无封面时门禁自会拦公众号）。
                match stage_image(&job0, &md, &log_path) {
                    Ok(a) => {
                        push_step(
                            &job_id, stage, stage_label(stage), "ok",
                            &format!(
                                "封面 {} · 插图 {} 张",
                                a.cover.as_ref().map(|c| c.display().to_string()).unwrap_or_else(|| "未生成".into()),
                                a.inline.len()
                            ),
                        );
                        image_assets = a;
                    }
                    Err(e) => {
                        log_line(&log_path, &format!("image：{e}（不断流，继续后续阶段）"));
                        push_step(&job_id, stage, stage_label(stage), "fail", &e);
                    }
                }
            }
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
                    Ok(Some(html)) => {
                        push_step(&job_id, stage, stage_label(stage), "ok", &format!("语义 HTML：{}", html.display()));
                        content_path = Some(html);
                    }
                    Ok(None) => {
                        push_step(&job_id, stage, stage_label(stage), "skip", "非公众号平台，投递脚本直接吃 .md");
                    }
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
                match stage_upload(&job0, &cp, &image_assets, &log_path) {
                    Ok(result) => {
                        log_line(&log_path, &format!("upload：成功（result={result}）"));
                        push_step(&job_id, stage, stage_label(stage), "ok", &format!("result={result}（草稿已推进到平台后台，窗口保留供预览）"));
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
    model: Option<String>,
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
        model: model.map(|m| m.trim().to_string()).unwrap_or_default(),
        stages: stages_norm,
        skip_stages: Vec::new(),
        status: "pending".to_string(),
        stage: String::new(),
        steps: Vec::new(),
        article_path: seed_article,
        log_path: log_path.to_string_lossy().to_string(),
        error: None,
        created_at: now_secs(),
        updated_at: now_secs(),
    };
    JOBS.lock().insert(job_id.clone(), job.clone());
    save_jobs();

    // 后台线程跑，阻塞式子进程不占用命令线程。
    std::thread::spawn(move || run_job(job_id));
    Ok(job)
}

/// 查一条 job 的运行态。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn media_job_status(job_id: String) -> Result<MediaJob, String> {
    get_job(&job_id).ok_or_else(|| format!("job 不存在：{job_id}"))
}

/// 这条 job 的「总规划提示词」——整篇文章怎么写的那一整段，不是某个步骤的局部提示词。
/// 详情卡中栏打开即读它，人不用点任何一格就能看见总纲。
///
/// 两条来源，留痕优先：
///   - generate 已跑过 → 原样回放当时落盘的快照（专家/补丁事后被改也不影响历史回放）；
///   - 还没跑到（排队中/刚开跑）→ 按当前专家画像 + 平台补丁 + 品牌契约现拼一份预览。
/// 不含 generate 阶段的 job（直接拿现成正文排版投递）没有这东西，返回空串由 UI 说明。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn media_job_plan_prompt(job_id: String) -> Result<MediaJobPlan, String> {
    let job = get_job(&job_id).ok_or_else(|| format!("job 不存在：{job_id}"))?;
    if let Some(s) = job
        .steps
        .iter()
        .rev()
        .find(|s| s.key == STAGE_GENERATE && !s.prompt.is_empty())
    {
        return Ok(MediaJobPlan {
            prompt: s.prompt.clone(),
            expert_id: s.expert_id.clone(),
            expert_name: s.expert_name.clone(),
            recorded: true,
        });
    }
    if !job.stages.iter().any(|s| s == STAGE_GENERATE) {
        return Ok(MediaJobPlan {
            prompt: String::new(),
            expert_id: String::new(),
            expert_name: String::new(),
            recorded: false,
        });
    }
    let (attr, prompt) = build_generate_prompt(&job, None);
    Ok(MediaJobPlan {
        prompt,
        expert_id: attr.expert_id,
        expert_name: attr.expert_name,
        recorded: false,
    })
}

/// 续跑一条失败/被取消的 job：从断点重跑，已完成且产物还在的阶段直接跳过。
/// - generate 已成且 .md 还在 → 跳过（产物路径原样复用）；
/// - image 已成 → 跳过（封面/插图按约定位置从盘上捡，见 run_job）；
/// - typeset / upload 幂等且便宜，一律重跑。
/// 队列项随 run_job 重新置 running；步骤时间线原位翻转 fail→run→ok，历史不清空。
/// 算出续跑计划：`(本轮要跑的阶段, 本轮跳过的阶段)`。
/// 从 `media_job_resume` 里抽出来的纯函数——不碰全局 JOBS、不 spawn 线程，可直接单测。
/// `article_alive` 由调用方探盘后传入，测试里可直接给定。
fn resume_plan(job: &MediaJob, article_alive: bool) -> Result<(Vec<String>, Vec<String>), String> {
    // 判定某阶段是否已完成：时间线里该 key 的最后一格是 ok/skip。
    let stage_ok = |key: &str| {
        job.steps
            .iter()
            .rev()
            .find(|s| s.key == key)
            .map(|s| s.status == "ok" || s.status == "skip")
            .unwrap_or(false)
    };

    let remaining: Vec<String> = job
        .stages
        .iter()
        .filter(|s| match s.as_str() {
            STAGE_GENERATE => !(stage_ok(STAGE_GENERATE) && article_alive),
            STAGE_IMAGE => !stage_ok(STAGE_IMAGE),
            _ => true, // typeset / upload 幂等，重跑
        })
        .cloned()
        .collect();

    // 要跑 typeset/upload 就必须有正文；而本轮又不打算跑 generate 补它 → 无从下手。
    let needs_article = remaining
        .iter()
        .any(|s| s.as_str() != STAGE_GENERATE && s.as_str() != STAGE_IMAGE);
    let will_generate = remaining.iter().any(|s| s == STAGE_GENERATE);
    if needs_article && !will_generate && !article_alive {
        return Err("正文产物已不在磁盘上，无法跳过 generate——请整条重跑".to_string());
    }

    let skipped: Vec<String> = job
        .stages
        .iter()
        .filter(|s| !remaining.contains(s))
        .cloned()
        .collect();
    Ok((remaining, skipped))
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub fn media_job_resume(job_id: String) -> Result<MediaJob, String> {
    let job = get_job(&job_id).ok_or_else(|| format!("job 不存在：{job_id}"))?;
    match job.status.as_str() {
        "failed" | "canceled" => {}
        "done" => return Err("该 job 已完成，无需续跑".to_string()),
        other => return Err(format!("该 job 仍在 {other}，不能续跑")),
    }

    let article_alive = job
        .article_path
        .as_ref()
        .map(|p| Path::new(p).is_file())
        .unwrap_or(false);
    let (remaining, skipped) = resume_plan(&job, article_alive)?;

    log_line(
        &log_path_for(&job_id),
        &format!("job 续跑：重跑 {remaining:?}，跳过已完成 {skipped:?}"),
    );
    // ★ 只记「本轮跳过谁」，绝不改写 stages——否则原始阶段列表就此丢失，
    //   同一 job 二次续跑与详情页阶段展示都会基于被截短的列表。
    update_job(&job_id, |j| {
        j.status = "pending".to_string();
        j.error = None;
        j.stage = String::new();
        j.skip_stages = skipped;
    });
    std::thread::spawn(move || run_job(job_id.clone()));
    get_job(&job.id).ok_or_else(|| "job 状态异常".to_string())
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
                // 还挂着 run 的步骤一并收尾，详情时间线不留悬空转圈。
                for s in j.steps.iter_mut().filter(|s| s.status == "run") {
                    s.status = "fail".to_string();
                    s.detail = "用户取消".to_string();
                    s.at = now_secs();
                }
                j.updated_at = now_secs();
                true
            }
            None => false,
        }
    };
    if !existed {
        return Err(format!("job 不存在：{job_id}"));
    }
    save_jobs();
    // 杀进程树（复用 runtime kill_tree）；不在跑则 no-op。
    CHILDREN.kill(&child_key(&job_id));
    log_line(&log_path_for(&job_id), "job 已被用户取消");
    Ok(())
}

/// 读一条 job 的日志尾部（流程详情视图的「实时日志」数据源）。
/// `tail_lines` 缺省 400 行；日志本身就小，够回放全程。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn media_job_log(job_id: String, tail_lines: Option<usize>) -> Result<String, String> {
    let job = get_job(&job_id).ok_or_else(|| format!("job 不存在：{job_id}"))?;
    let raw = match std::fs::read_to_string(&job.log_path) {
        Ok(s) => s,
        Err(_) => return Ok(String::new()), // 日志尚未产生（pending）不算错
    };
    let keep = tail_lines.unwrap_or(400).max(1);
    let lines: Vec<&str> = raw.lines().collect();
    let start = lines.len().saturating_sub(keep);
    Ok(lines[start..].join("\n"))
}

/// 读一条 job 的正文产物（.md 或公众号语义 .html），供详情视图预览。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn media_job_article(job_id: String) -> Result<String, String> {
    const MAX_BYTES: u64 = 512 * 1024; // 正文预览上限，防手滑读进大文件
    let job = get_job(&job_id).ok_or_else(|| format!("job 不存在：{job_id}"))?;
    let path = job
        .article_path
        .filter(|p| !p.trim().is_empty())
        .ok_or_else(|| "该 job 尚无正文产物".to_string())?;
    let meta = std::fs::metadata(&path).map_err(|e| format!("读产物失败：{e}"))?;
    if meta.len() > MAX_BYTES {
        return Err(format!("产物过大（{} KB），请直接打开文件查看：{path}", meta.len() / 1024));
    }
    std::fs::read_to_string(&path).map_err(|e| format!("读产物失败：{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 编排 → 专家的查找必须回真值：这根线断了，详情页的「哪个专家」就退化成常量。
    #[test]
    fn attr_for_resolves_expert_from_workflow() {
        let a = attr_for("wechat", "写作");
        assert_eq!(a.expert_id, "media-writer");
        assert!(!a.expert_name.is_empty(), "专家卡应带出人话名");
        assert_ne!(a.expert_name, a.expert_id, "人话名不该回落成 id");
        assert_eq!(a.skill_id, "media-pipeline-wechat");

        // 公众号排版环节挂的是已内嵌的 wechat-md-typesetter（非占位）。
        assert_eq!(attr_for("wechat", "排版").skill_id, "wechat-md-typesetter");
        assert_eq!(attr_for("wechat", "投递").expert_id, "media-publisher");

        // 执行面三阶段都能认领回蓝图里的环节。
        for stage in ALL_STAGES {
            let step = stage_workflow_step(stage);
            assert!(!step.is_empty(), "{stage} 没映射到编排环节");
            assert!(
                !attr_for("wechat", step).expert_id.is_empty(),
                "{stage}→{step} 在编排里查不到专家"
            );
        }

        // 蓝图里没有的环节回空 attr，不 panic（调用方照常记步，UI 显示「未编排」）。
        assert!(attr_for("wechat", "查无此环节").expert_id.is_empty());
    }

    /// 归因用非空覆盖：run 那格写下的提示词，不能被收尾的 ok/fail（传空 attr）抹掉。
    #[test]
    fn push_step_attr_merges_without_erasing_prompt() {
        let job_id = "test-attr-merge";
        JOBS.lock().insert(
            job_id.to_string(),
            MediaJob {
                id: job_id.to_string(),
                queue_id: None,
                platform: "wechat".into(),
                title: "t".into(),
                topic: String::new(),
                model: String::new(),
                stages: vec![STAGE_GENERATE.into()],
                skip_stages: Vec::new(),
                status: "running".into(),
                stage: String::new(),
                steps: vec![],
                article_path: None,
                log_path: String::new(),
                error: None,
                created_at: now_secs(),
                updated_at: now_secs(),
            },
        );

        push_step_attr(
            job_id,
            "generate",
            "生成",
            "run",
            "写作中",
            StepAttr {
                expert_id: "media-writer".into(),
                expert_name: "主笔".into(),
                prompt: "系统设定全文".into(),
                ..Default::default()
            },
        );
        // 收尾走老的 push_step（空 attr）——这是执行面里真实的调用序列。
        push_step(job_id, "generate", "生成", "ok", "已落盘");

        let steps = get_job(job_id).expect("job 应在册").steps;
        assert_eq!(steps.len(), 1, "同 key 应原位更新而非追加");
        let s = &steps[0];
        assert_eq!(s.status, "ok");
        assert_eq!(s.detail, "已落盘");
        assert_eq!(s.prompt, "系统设定全文", "收尾不该抹掉 run 时记下的提示词");
        assert_eq!(s.expert_name, "主笔");
        assert!(s.started_at > 0, "首次记步应落起点，否则算不出耗时");

        JOBS.lock().remove(job_id);
    }

    /// 造一条只有阶段和时间线的 job，供 resume_plan 用。
    fn job_for_resume(stages: &[&str], done: &[&str]) -> MediaJob {
        MediaJob {
            id: "t".into(),
            queue_id: None,
            platform: "wechat".into(),
            title: "t".into(),
            topic: String::new(),
            model: String::new(),
            stages: stages.iter().map(|s| s.to_string()).collect(),
            skip_stages: Vec::new(),
            status: "failed".into(),
            stage: String::new(),
            steps: done
                .iter()
                .map(|k| JobStep {
                    key: k.to_string(),
                    status: "ok".into(),
                    ..Default::default()
                })
                .collect(),
            article_path: None,
            log_path: String::new(),
            error: None,
            created_at: now_secs(),
            updated_at: now_secs(),
        }
    }

    /// 续跑只跳过「已完成且产物还在」的阶段，typeset/upload 幂等一律重跑。
    #[test]
    fn resume_plan_skips_only_finished_stages() {
        let job = job_for_resume(
            &[STAGE_GENERATE, STAGE_IMAGE, STAGE_TYPESET],
            &[STAGE_GENERATE],
        );
        let (remaining, skipped) = resume_plan(&job, true).expect("正文还在，应能续跑");
        assert_eq!(skipped, vec![STAGE_GENERATE.to_string()], "generate 已成且产物在盘，应跳过");
        assert_eq!(
            remaining,
            vec![STAGE_IMAGE.to_string(), STAGE_TYPESET.to_string()],
            "image 未成 + typeset 幂等，都要跑"
        );
    }

    /// ★ 回归锁：续跑**不得**改写 job.stages。否则同一 job 二次续跑会基于被截短的
    ///   列表，已跳过的阶段永久消失，详情页阶段时间线也跟着残缺。
    #[test]
    fn resume_plan_never_shrinks_original_stages() {
        let all = [STAGE_GENERATE, STAGE_IMAGE, STAGE_TYPESET];
        let mut job = job_for_resume(&all, &[STAGE_GENERATE]);

        // 第一次续跑：跳过 generate，只记进 skip_stages。
        let (_, skipped1) = resume_plan(&job, true).expect("第一次续跑");
        job.skip_stages = skipped1;
        assert_eq!(job.stages.len(), 3, "stages 必须保持原始三段");

        // 第二次续跑（比如 image 又挂了）：仍然从完整的三段里重算。
        job.steps.push(JobStep {
            key: STAGE_IMAGE.into(),
            status: "ok".into(),
            ..Default::default()
        });
        let (remaining2, skipped2) = resume_plan(&job, true).expect("第二次续跑");
        assert_eq!(job.stages.len(), 3, "二次续跑后 stages 依然完整");
        assert_eq!(
            skipped2,
            vec![STAGE_GENERATE.to_string(), STAGE_IMAGE.to_string()],
            "两段都已完成，二次续跑应一并跳过"
        );
        assert_eq!(remaining2, vec![STAGE_TYPESET.to_string()], "只剩排版要跑");
    }

    /// 正文产物已不在磁盘上、本轮又不跑 generate → 明确报错，不要闷头跑排版。
    #[test]
    fn resume_plan_rejects_typeset_without_article() {
        let job = job_for_resume(&[STAGE_TYPESET], &[]);
        let err = resume_plan(&job, false).expect_err("没正文不该放行");
        assert!(err.contains("正文产物"), "错误应说清是正文缺失：{err}");
    }

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
