//! 自媒体「账号管理」— 9 平台登录态（浏览器 profile）的探测、打开登录窗口与解绑。
//!
//! 背景：发文（草稿投递）依赖各平台的**持久化浏览器 profile 目录**保存登录态，扫一次码即可
//! 长期复用。本模块给「账号管理」面板提供 ground-truth 与操作入口：
//! - 探测：profile 目录存在且非空 = 已绑定；`Cookies` / `Default/Cookies` mtime = 最近活动。
//! - 打开：`media_account_open(platform, target)` detached 拉起 python 持久浏览器窗口
//!   （account_window.py，cloakbrowser 回退 playwright），窗口**由用户自己关**，登录态
//!   永久留在 profile 目录。
//! - 解绑：删 profile 目录，下次重新扫码。
//!
//! profile 目录约定（向后兼容）：
//! - wechat：`~/.polaris-mp-profile`（沿用 post-to-wechat / mp_draft.py）
//! - xhs：`%LOCALAPPDATA%\Google\Chrome\XiaohongshuProfiles\default`（沿用 post-to-xhs），
//!   回退旧路径 `...\XiaohongshuProfile`
//! - 其余新平台（zhihu/toutiao/baijia/bilibili/douyin）：`~/PolarisGEO/browser-profiles/{platform}`

use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// 打开登录/发文窗口的 python 脚本（内嵌模板，运行期释放到 ~/PolarisGEO/runtime/）。
const ACCOUNT_WINDOW_PY: &str =
    include_str!("templates/skills/media-publisher/scripts/account_window.py");

// ───────────────────────── 平台表（全局统一 9 平台，顺序固定） ─────────────────────────

/// 平台静态配置。profile 目录另由 `profile_candidates()` 推导（wechat/xhs 有历史路径）。
struct Platform {
    id: &'static str,
    name: &'static str,
    login_url: &'static str,
    draft_url: &'static str,
}

const PLATFORMS: &[Platform] = &[
    Platform {
        id: "wechat",
        name: "微信公众号",
        login_url: "https://mp.weixin.qq.com/",
        draft_url: "https://mp.weixin.qq.com/",
    },
    Platform {
        id: "xhs",
        name: "小红书",
        login_url: "https://creator.xiaohongshu.com/login",
        draft_url: "https://creator.xiaohongshu.com/publish/publish",
    },
    Platform {
        id: "zhihu",
        name: "知乎",
        login_url: "https://www.zhihu.com/signin",
        draft_url: "https://zhuanlan.zhihu.com/write",
    },
    Platform {
        id: "toutiao",
        name: "今日头条",
        login_url: "https://mp.toutiao.com/auth/page/login",
        draft_url: "https://mp.toutiao.com/profile_v4/graphic/publish",
    },
    Platform {
        id: "baijia",
        name: "百家号",
        login_url: "https://baijiahao.baidu.com/builder/theme/bjh/login",
        draft_url: "https://baijiahao.baidu.com/builder/rc/edit?type=news",
    },
    Platform {
        id: "bilibili",
        name: "B站专栏",
        login_url: "https://passport.bilibili.com/login",
        draft_url: "https://member.bilibili.com/read/editor/#/new",
    },
    Platform {
        id: "douyin",
        name: "抖音图文",
        login_url: "https://creator.douyin.com/",
        draft_url: "https://creator.douyin.com/creator-micro/content/publish-media/text",
    },
    Platform {
        id: "csdn",
        name: "CSDN",
        login_url: "https://passport.csdn.net/login",
        // 只用裸 /md/：带 ?not_checkout=1 实测会被重定向去内容管理页，落不到编辑器。
        // 与 draft_uploader.py 的 csdn.draft_url 必须保持一致（两处是各自独立的常量）。
        draft_url: "https://editor.csdn.net/md/",
    },
    Platform {
        id: "juejin",
        name: "掘金",
        login_url: "https://juejin.cn/login",
        draft_url: "https://juejin.cn/editor/drafts/new?v=2",
    },
];

fn platform_by_id(id: &str) -> Option<&'static Platform> {
    PLATFORMS.iter().find(|p| p.id == id)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountStatus {
    /// 平台 id："wechat" | "xhs" | "zhihu" | "toutiao" | "baijia" | "bilibili" | "douyin"
    pub platform: String,
    /// 展示名（沿用旧字段名，兼容既有前端）
    pub label: String,
    /// 展示名（新字段，与契约一致，内容同 label）
    pub name: String,
    /// 登录页 URL
    pub login_url: String,
    /// 发文/草稿编辑页 URL
    pub draft_url: String,
    /// 是否已绑定（profile 目录存在且非空 = 扫过码）
    pub bound: bool,
    /// 登录态所在 profile 目录（绝对路径，给用户看 / 排查）
    pub profile_dir: String,
    /// profile 最近活动时间（unix 秒）；未绑定为 None
    pub last_active: Option<i64>,
    /// 一句话说明
    pub detail: String,
}

fn home() -> PathBuf {
    directories::UserDirs::new()
        .map(|u| u.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

fn local_app_data() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(home)
}

/// 各平台 profile 候选目录（首个为「首选」；wechat/xhs 沿用历史路径向后兼容）。
fn profile_candidates(platform: &str) -> Vec<PathBuf> {
    match platform {
        // 与 post-to-wechat/scripts/mp_draft.py 的 PROFILE_DIR 一致
        "wechat" => vec![home().join(".polaris-mp-profile")],
        // 与 post-to-xhs 的 account_manager 一致（新路径优先，回退旧路径）
        "xhs" => {
            let lad = local_app_data();
            vec![
                lad.join("Google")
                    .join("Chrome")
                    .join("XiaohongshuProfiles")
                    .join("default"),
                lad.join("Google").join("Chrome").join("XiaohongshuProfile"),
            ]
        }
        // 新平台统一 ~/PolarisGEO/browser-profiles/{platform}
        other => vec![home()
            .join("PolarisGEO")
            .join("browser-profiles")
            .join(other)],
    }
}

/// 目录是否存在且非空（= 浏览器写过 profile = 扫过码）。
fn dir_bound(p: &Path) -> bool {
    fs::read_dir(p)
        .map(|mut it| it.next().is_some())
        .unwrap_or(false)
}

fn mtime_secs(path: &Path) -> Option<i64> {
    let meta = fs::metadata(path).ok()?;
    let m = meta.modified().ok()?;
    let d = m.duration_since(UNIX_EPOCH).ok()?;
    Some(d.as_secs() as i64)
}

/// 取 profile 最近活动时间（unix 秒）。
/// 优先看会话文件：`Cookies` / `Default/Cookies` / `Default/Network/Cookies`
/// （Chromium 持久 profile 的几种布局都覆盖，浏览器一开着就会持续写它，
/// mtime 最能代表「最近登录活动」）；都没有则退化为目录自身 + 顶层条目 mtime。
/// 只看一层，避免遍历整个 Chrome profile。
fn dir_last_active(p: &Path) -> Option<i64> {
    let session_files = [
        p.join("Cookies"),
        p.join("Default").join("Cookies"),
        p.join("Default").join("Network").join("Cookies"),
    ];
    if let Some(latest) = session_files.iter().filter_map(|f| mtime_secs(f)).max() {
        return Some(latest);
    }
    // 退化：目录自身 + 顶层条目 mtime
    let mut latest = mtime_secs(p);
    if let Ok(entries) = fs::read_dir(p) {
        for e in entries.flatten().take(64) {
            if let Some(secs) = mtime_secs(&e.path()) {
                if latest.map_or(true, |cur| secs > cur) {
                    latest = Some(secs);
                }
            }
        }
    }
    latest
}

fn platform_status(pf: &Platform) -> AccountStatus {
    let candidates = profile_candidates(pf.id);
    // 选第一个已绑定的；都没有则用首选路径作为「未绑定」展示。
    let chosen = candidates.iter().find(|p| dir_bound(p)).cloned();
    let (dir, bound) = match chosen {
        Some(p) => (p, true),
        None => (candidates[0].clone(), false),
    };
    AccountStatus {
        platform: pf.id.into(),
        label: pf.name.into(),
        name: pf.name.into(),
        login_url: pf.login_url.into(),
        draft_url: pf.draft_url.into(),
        bound,
        last_active: if bound { dir_last_active(&dir) } else { None },
        profile_dir: dir.to_string_lossy().into_owned(),
        detail: if bound {
            format!("已绑定{}，投递草稿复用此浏览器登录态；session 过期后重新登录一次即可。", pf.name)
        } else {
            format!("尚未绑定。点「打开登录窗口」登录一次{}，之后投递草稿不再重复扫码。", pf.name)
        },
    }
}

/// 列出各平台登录态（账号管理面板），固定 9 平台、顺序固定。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn media_accounts_status() -> Vec<AccountStatus> {
    PLATFORMS.iter().map(platform_status).collect()
}

// ───────────────────────── 打开登录/发文窗口（detached python） ─────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenResult {
    pub ok: bool,
    pub message: String,
}

/// 把内嵌的 account_window.py 释放到 ~/PolarisGEO/runtime/（内容变了才重写）。
fn ensure_account_window_script() -> Result<PathBuf, String> {
    let dir = home().join("PolarisGEO").join("runtime");
    fs::create_dir_all(&dir).map_err(|e| format!("创建 {} 失败：{e}", dir.display()))?;
    let path = dir.join("account_window.py");
    let stale = fs::read_to_string(&path)
        .map(|cur| cur != ACCOUNT_WINDOW_PY)
        .unwrap_or(true);
    if stale {
        fs::write(&path, ACCOUNT_WINDOW_PY)
            .map_err(|e| format!("写入 {} 失败：{e}", path.display()))?;
    }
    Ok(path)
}

/// 探测可用的 python 可执行文件：先 `python`，再 `python3`（macOS/Linux 常规入口），
/// 最后回落 Windows 启动器 `py`。
fn find_python() -> Option<&'static str> {
    for exe in ["python", "python3", "py"] {
        let mut cmd = std::process::Command::new(exe);
        cmd.arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW：探测别闪黑控制台
        }
        if matches!(cmd.status(), Ok(s) if s.success()) {
            return Some(exe);
        }
    }
    None
}

/// 打开某平台的登录 / 发文窗口：detached 启动 python 持久浏览器窗口（account_window.py）。
/// 关键行为：窗口**保持到用户自己关闭**——登录完不许自动关闭；登录态永久保留在 profile 目录；
/// 子进程与本应用彻底脱钩，Polaris 重启也不影响已打开的窗口。
/// (async)：find_python + 释放脚本有毫秒级 IO，别占 UI 线程。
#[cfg_attr(feature = "desktop", tauri::command(async))]
pub fn media_account_open(platform: String, target: String) -> Result<OpenResult, String> {
    let pf = platform_by_id(&platform).ok_or_else(|| format!("未知平台：{platform}"))?;
    let url = match target.as_str() {
        "login" => pf.login_url,
        "draft" => pf.draft_url,
        other => return Err(format!("未知 target：{other}（只支持 login | draft）")),
    };
    // profile：优先复用已绑定的历史目录（如 xhs 旧路径），否则用首选目录
    let candidates = profile_candidates(pf.id);
    let profile_dir = candidates
        .iter()
        .find(|p| dir_bound(p))
        .cloned()
        .unwrap_or_else(|| candidates[0].clone());

    let script = ensure_account_window_script()?;
    let python = find_python().ok_or_else(|| {
        "找不到 python（试过 `python` 与 `py`），请先安装 Python 3 并加入 PATH".to_string()
    })?;

    let mut cmd = std::process::Command::new(python);
    cmd.arg(&script)
        .arg("--platform")
        .arg(pf.id)
        .arg("--target")
        .arg(&target)
        .arg("--url")
        .arg(url)
        .arg("--profile-dir")
        .arg(profile_dir.as_os_str())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW(0x08000000) | DETACHED_PROCESS(0x00000008)：
        // 无控制台窗口、与本进程彻底脱钩——应用退出/重启不影响登录窗口。
        cmd.creation_flags(0x0800_0008);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0); // 独立进程组，应用退出的信号不带走登录窗口
    }
    cmd.spawn().map_err(|e| format!("启动登录窗口失败：{e}"))?;

    Ok(OpenResult {
        ok: true,
        message: format!(
            "已打开{}的{}窗口。请在弹出的浏览器里完成操作，登录态会自动保留；用完后自己关闭窗口即可。",
            pf.name,
            if target == "draft" { "发文" } else { "登录" }
        ),
    })
}

/// 解绑某平台：删除其 profile 目录，强制下次重新扫码登录。
/// 安全：只允许删本模块固定推导出的已知路径，杜绝任意路径删除。
/// (async)：remove_dir_all 数百 MB 的 Chrome profile 要跑好几秒，同步命令会钉死主线程。
#[cfg_attr(feature = "desktop", tauri::command(async))]
pub fn media_account_forget(platform: String) -> Result<String, String> {
    if platform_by_id(&platform).is_none() {
        return Err(format!("未知平台：{platform}"));
    }
    let mut removed = 0usize;
    for dir in profile_candidates(&platform) {
        if dir.exists() {
            fs::remove_dir_all(&dir).map_err(|e| format!("删除 {} 失败：{e}", dir.display()))?;
            removed += 1;
        }
    }
    Ok(if removed > 0 {
        "已解绑：登录态已清除，下次发文需重新扫码。".into()
    } else {
        "本来就没有登录态，无需解绑。".into()
    })
}
