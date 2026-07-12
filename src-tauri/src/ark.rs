//! 火山引擎方舟（Ark）API 中心 — OpenAI 兼容协议下的生图 / 连通测试 / 模型列表 / 聊天测试。
//!
//! 定位：自媒体运营里「配图」等能力需要一套**独立于供应商坞**的图像/多模态 API。
//! 供应商坞（provider/store.rs）里 55 家清一色是 Anthropic 协议的文本/代码大模型，
//! 没有一个能生图；方舟走的是 OpenAI 兼容协议（base `…/api/v3`），生图 / 聊天各一条路。
//!
//! 配置持久化在 `~/PolarisGEO/data/ark.json`：
//!   { "api_key": "...", "base_url": "...", "image_model": "...", "chat_model": "..." }
//! 首次读取时若文件不存在，用「粉丝福利」默认 key 播种（用户可在设置页改）。
//!
//! HTTP 用 ureq（app crate 已有的阻塞客户端，见 infer.rs 同款用法），网络命令挂
//! `tauri::command(async)`（沿用 accounts.rs 的做法：阻塞活儿丢到独立线程，不钉死主线程）。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// ───────────────────────── 默认值（粉丝福利种子） ─────────────────────────

/// 粉丝福利默认 key（用户可在设置页改）。仅连通测试可用；生图/聊天需在方舟控制台
/// 为该账号**开通对应模型**后才能调用（未开通会收到 ModelNotOpen）。
const DEFAULT_API_KEY: &str = "ARK_API_KEY_REMOVED_FROM_HISTORY";
const DEFAULT_BASE_URL: &str = "https://ark.cn-beijing.volces.com/api/v3";
/// 生图模型：doubao-seedream 4.5（2026-07 实测该账号区域内存在且在线的最新稳定版本 id）。
/// 方舟 OpenAI 兼容接口按**完整版本 id** 校验（短别名 `doubao-seedream-4-5` 会 NotFound），故写全。
const DEFAULT_IMAGE_MODEL: &str = "doubao-seedream-4-5-251128";
/// 聊天模型：doubao-seed 2.1 turbo（seed 系「快档」的当代继任，旧 `seed-1-6-flash` 已退役）。
const DEFAULT_CHAT_MODEL: &str = "doubao-seed-2-1-turbo-260628";

fn default_api_key() -> String {
    DEFAULT_API_KEY.to_string()
}
fn default_base_url() -> String {
    DEFAULT_BASE_URL.to_string()
}
fn default_image_model() -> String {
    DEFAULT_IMAGE_MODEL.to_string()
}
fn default_chat_model() -> String {
    DEFAULT_CHAT_MODEL.to_string()
}

// ───────────────────────── 配置模型 ─────────────────────────

/// 方舟配置（`~/PolarisGEO/data/ark.json`）。后端返回全量 api_key，打码显示由前端处理。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArkConfig {
    #[serde(default = "default_api_key", alias = "api_key")]
    pub api_key: String,
    #[serde(default = "default_base_url", alias = "base_url")]
    pub base_url: String,
    #[serde(default = "default_image_model", alias = "image_model")]
    pub image_model: String,
    #[serde(default = "default_chat_model", alias = "chat_model")]
    pub chat_model: String,
}

impl Default for ArkConfig {
    fn default() -> Self {
        ArkConfig {
            api_key: default_api_key(),
            base_url: default_base_url(),
            image_model: default_image_model(),
            chat_model: default_chat_model(),
        }
    }
}

/// 配置补丁（部分字段更新，None = 不改）。
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArkConfigPatch {
    #[serde(default, alias = "api_key")]
    pub api_key: Option<String>,
    #[serde(default, alias = "base_url")]
    pub base_url: Option<String>,
    #[serde(default, alias = "image_model")]
    pub image_model: Option<String>,
    #[serde(default, alias = "chat_model")]
    pub chat_model: Option<String>,
}

// ───────────────────────── 路径 / 持久化 ─────────────────────────

fn home() -> PathBuf {
    directories::UserDirs::new()
        .map(|u| u.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// 配置文件路径 `~/PolarisGEO/data/ark.json`。
fn config_path() -> PathBuf {
    home().join("PolarisGEO").join("data").join("ark.json")
}

/// 生图默认落盘目录 `~/PolarisGEO/output/images/`。
fn image_out_dir() -> PathBuf {
    home()
        .join("PolarisGEO")
        .join("output")
        .join("images")
}

/// 原子落盘：先写同目录临时文件，再 rename 覆盖 —— 与 provider/store.rs 同款，
/// 断电/半途崩溃只会留下 `.tmp`，目标文件要么旧要么新，绝不残缺半截 JSON。
fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".polaris.tmp");
    let tmp = PathBuf::from(tmp);
    {
        use std::io::Write;
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)
}

/// 读取配置：文件存在则解析（缺字段由 serde 默认补齐），不存在则返回默认并**播种**落盘，
/// 让用户在设置页直接看到这份 ark.json。解析失败（坏文件）静默回落默认，不影响使用。
fn load_config() -> ArkConfig {
    let path = config_path();
    if path.exists() {
        if let Ok(txt) = fs::read_to_string(&path) {
            if let Ok(cfg) = serde_json::from_str::<ArkConfig>(&txt) {
                return cfg;
            }
        }
        // 坏文件：回落默认，但不覆盖（保留用户可能手工抢救的原文件）。
        return ArkConfig::default();
    }
    // 首次：种子默认配置落盘（best-effort）。
    let cfg = ArkConfig::default();
    if let Ok(txt) = serde_json::to_string_pretty(&cfg) {
        let _ = atomic_write(&path, txt.as_bytes());
    }
    cfg
}

fn save_config(cfg: &ArkConfig) -> Result<(), String> {
    let path = config_path();
    let txt = serde_json::to_string_pretty(cfg)
        .map_err(|e| format!("序列化 ark.json 失败: {e}"))?;
    atomic_write(&path, txt.as_bytes()).map_err(|e| format!("写 ark.json 失败: {e}"))
}

// ───────────────────────── HTTP 助手 ─────────────────────────

/// 通用 Agent（连通/模型/聊天）。连接 8s，整体 60s。
fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(8))
        .timeout(Duration::from_secs(60))
        .build()
}

/// 生图 Agent：出图可能较慢，整体放宽到 120s。
fn image_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(8))
        .timeout(Duration::from_secs(120))
        .build()
}

fn bearer(key: &str) -> String {
    format!("Bearer {}", key.trim())
}

/// base_url 去掉尾 `/`，拼接子路径。
fn join_url(base: &str, path: &str) -> String {
    format!("{}{}", base.trim().trim_end_matches('/'), path)
}

/// 从方舟错误响应体里抽取一句人类可读的 message（`{"error":{"message":..}}`），
/// 抽不到就回落原始文本（截断），再不行给个兜底串。
fn extract_err_message(body: &str) -> String {
    if let Ok(v) = serde_json::from_str::<Value>(body) {
        if let Some(m) = v
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
        {
            return m.to_string();
        }
    }
    let t = body.trim();
    if t.is_empty() {
        "无响应体".to_string()
    } else {
        t.chars().take(300).collect()
    }
}

// ───────────────────────── 结果模型 ─────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArkTestResult {
    pub ok: bool,
    pub latency_ms: u64,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct ArkImageResult {
    pub path: String,
    pub model: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArkChatResult {
    pub ok: bool,
    pub content: String,
    pub latency_ms: u64,
}

// ───────────────────────── Commands: 配置 ─────────────────────────

/// 读取方舟配置（首次自动种子默认粉丝福利 key）。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn ark_config_get() -> Result<ArkConfig, String> {
    Ok(load_config())
}

/// 更新方舟配置（部分字段，None 不改）。空串按「清空/回落」处理：若把 key 清空，
/// 下次读取仍是空 key（不自动回落默认，尊重用户显式清空）。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn ark_config_set(patch: ArkConfigPatch) -> Result<ArkConfig, String> {
    let mut cfg = load_config();
    if let Some(v) = patch.api_key {
        cfg.api_key = v.trim().to_string();
    }
    if let Some(v) = patch.base_url {
        let v = v.trim().to_string();
        cfg.base_url = if v.is_empty() { default_base_url() } else { v };
    }
    if let Some(v) = patch.image_model {
        let v = v.trim().to_string();
        cfg.image_model = if v.is_empty() {
            default_image_model()
        } else {
            v
        };
    }
    if let Some(v) = patch.chat_model {
        let v = v.trim().to_string();
        cfg.chat_model = if v.is_empty() {
            default_chat_model()
        } else {
            v
        };
    }
    save_config(&cfg)?;
    Ok(cfg)
}

// ───────────────────────── Commands: 连通 / 模型 ─────────────────────────

/// 连通性测试：GET {base}/models，Bearer 鉴权。
/// - 2xx → ok，已连通。
/// - 429 → ok（已连通，仅限速）。
/// - 401/403 → 非 ok（密钥无效）。
/// - 其它状态 / 网络错误 → 非 ok，带原因。
#[cfg_attr(feature = "desktop", tauri::command(async))]
pub fn ark_test() -> Result<ArkTestResult, String> {
    let cfg = load_config();
    if cfg.api_key.trim().is_empty() {
        return Ok(ArkTestResult {
            ok: false,
            latency_ms: 0,
            message: "未配置 API Key。".to_string(),
        });
    }
    let url = join_url(&cfg.base_url, "/models");
    let started = Instant::now();
    let resp = agent()
        .get(&url)
        .set("Authorization", &bearer(&cfg.api_key))
        .call();
    let latency_ms = started.elapsed().as_millis() as u64;

    let (ok, message) = match resp {
        Ok(_) => (true, "连通正常。".to_string()),
        Err(ureq::Error::Status(code, r)) => {
            let body = r.into_string().unwrap_or_default();
            match code {
                429 => (true, "已连通（触发限速 429）。".to_string()),
                401 | 403 => (
                    false,
                    format!("密钥无效（{code}）：{}", extract_err_message(&body)),
                ),
                _ => (
                    false,
                    format!("HTTP {code}：{}", extract_err_message(&body)),
                ),
            }
        }
        Err(ureq::Error::Transport(t)) => (false, format!("网络错误：{t}")),
    };
    Ok(ArkTestResult {
        ok,
        latency_ms,
        message,
    })
}

/// 模型列表：GET {base}/models，返回可用模型 id 列表；失败返回空列表。
#[cfg_attr(feature = "desktop", tauri::command(async))]
pub fn ark_models() -> Result<Vec<String>, String> {
    let cfg = load_config();
    if cfg.api_key.trim().is_empty() {
        return Ok(vec![]);
    }
    let url = join_url(&cfg.base_url, "/models");
    let resp = agent()
        .get(&url)
        .set("Authorization", &bearer(&cfg.api_key))
        .call();
    let v: Value = match resp {
        Ok(r) => match r.into_json() {
            Ok(v) => v,
            Err(_) => return Ok(vec![]),
        },
        Err(_) => return Ok(vec![]),
    };
    let ids = v
        .get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m.get("id").and_then(|i| i.as_str()).map(|s| s.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(ids)
}

// ───────────────────────── Commands: 生图 ─────────────────────────

/// 生图：POST {base}/images/generations，response_format=b64_json，落盘 PNG。
/// - `prompt` 提示词
/// - `size`   默认 "2048x2048"
/// - `out_path` 落盘绝对路径；缺省 → `~/PolarisGEO/output/images/ark-{ts}.png`
///
/// 返回 { path, model }。注意：模型需在方舟控制台为该账号**开通**后才可调用。
#[cfg_attr(feature = "desktop", tauri::command(async))]
pub fn ark_image_generate(
    prompt: String,
    size: Option<String>,
    out_path: Option<String>,
) -> Result<ArkImageResult, String> {
    let cfg = load_config();
    if cfg.api_key.trim().is_empty() {
        return Err("未配置 API Key。".to_string());
    }
    if prompt.trim().is_empty() {
        return Err("提示词为空。".to_string());
    }
    let size = size
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "2048x2048".to_string());
    let model = cfg.image_model.clone();

    let url = join_url(&cfg.base_url, "/images/generations");
    let body = serde_json::json!({
        "model": model,
        "prompt": prompt,
        "size": size,
        "response_format": "b64_json",
        "watermark": false,
    });

    let resp = image_agent()
        .post(&url)
        .set("Authorization", &bearer(&cfg.api_key))
        .set("Content-Type", "application/json")
        .send_json(body);

    let v: Value = match resp {
        Ok(r) => r
            .into_json()
            .map_err(|e| format!("响应解析失败: {e}"))?,
        Err(ureq::Error::Status(code, r)) => {
            let body = r.into_string().unwrap_or_default();
            return Err(format!(
                "生图失败 HTTP {code}：{}",
                extract_err_message(&body)
            ));
        }
        Err(ureq::Error::Transport(t)) => return Err(format!("网络错误：{t}")),
    };

    let b64 = v
        .get("data")
        .and_then(|d| d.as_array())
        .and_then(|arr| arr.first())
        .and_then(|first| first.get("b64_json"))
        .and_then(|b| b.as_str())
        .ok_or_else(|| "响应缺 data[0].b64_json 字段（模型可能未返回 base64）".to_string())?;

    use base64::Engine as _;
    let png = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| format!("base64 解码失败: {e}"))?;

    let path = match out_path {
        Some(p) if !p.trim().is_empty() => PathBuf::from(p.trim()),
        _ => {
            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            image_out_dir().join(format!("ark-{ts}.png"))
        }
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建输出目录失败: {e}"))?;
    }
    fs::write(&path, &png).map_err(|e| format!("写图片失败: {e}"))?;

    Ok(ArkImageResult {
        path: path.to_string_lossy().into_owned(),
        model,
    })
}

// ───────────────────────── Commands: 聊天测试 ─────────────────────────

/// 聊天测试：POST {base}/chat/completions（用于 API 联通/可用性验证）。
/// `model` 缺省用配置里的 chat_model。返回 { ok, content, latency_ms }。
#[cfg_attr(feature = "desktop", tauri::command(async))]
pub fn ark_chat_test(prompt: String, model: Option<String>) -> Result<ArkChatResult, String> {
    let cfg = load_config();
    if cfg.api_key.trim().is_empty() {
        return Ok(ArkChatResult {
            ok: false,
            content: "未配置 API Key。".to_string(),
            latency_ms: 0,
        });
    }
    let model = model
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| cfg.chat_model.clone());

    let url = join_url(&cfg.base_url, "/chat/completions");
    let body = serde_json::json!({
        "model": model,
        "messages": [{ "role": "user", "content": prompt }],
    });

    let started = Instant::now();
    let resp = agent()
        .post(&url)
        .set("Authorization", &bearer(&cfg.api_key))
        .set("Content-Type", "application/json")
        .send_json(body);
    let latency_ms = started.elapsed().as_millis() as u64;

    match resp {
        Ok(r) => {
            let v: Value = r
                .into_json()
                .map_err(|e| format!("响应解析失败: {e}"))?;
            let content = v
                .get("choices")
                .and_then(|c| c.as_array())
                .and_then(|arr| arr.first())
                .and_then(|first| first.get("message"))
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_str())
                .unwrap_or("")
                .to_string();
            Ok(ArkChatResult {
                ok: true,
                content,
                latency_ms,
            })
        }
        Err(ureq::Error::Status(code, r)) => {
            let body = r.into_string().unwrap_or_default();
            Ok(ArkChatResult {
                ok: false,
                content: format!("HTTP {code}：{}", extract_err_message(&body)),
                latency_ms,
            })
        }
        Err(ureq::Error::Transport(t)) => Ok(ArkChatResult {
            ok: false,
            content: format!("网络错误：{t}"),
            latency_ms,
        }),
    }
}
