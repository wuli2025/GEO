//! 自媒体「运营中心」— 题库 / 规划队列 / 平台设置 / 度量事件的本地持久化与命令。
//!
//! 背景：自媒体运营是一条「选题 → 调研 → 写作 → 质检 → 去 AI 味 → 配图 → 排版 → 投递」的
//! 流水线，跨 7 个平台（公众号 / 小红书 / 知乎 / 头条 / 百家号 / B站 / 抖音）。本模块给
//! 「运营中心」面板提供 ground-truth 数据面：
//! - **题库 Topic**：选题池，带状态机（pool→picked→drafted→published/rejected）。
//! - **规划队列 QueueItem**：待发/在跑的稿件，带状态机（queued→running→draft_uploaded→done/failed）。
//! - **平台设置 PlatformSettings**：每平台的开关 / 发送模式（ai 直传 vs manual 手动辅助）/
//!   周配额 / 专家+技能编排的 workflow。首次加载 seed 9 平台默认工作流。
//! - **度量事件 MetricEvent**：每次跑任务/出草稿/发布/失败落一条，滚动保留最近 500 条，
//!   `mediaops_metrics_summary` 汇总成 7/30 天 KPI 与分平台 KPI。
//!
//! 落盘：`~/PolarisGEO/data/mediaops.json`，原子写入（临时文件 + rename，参考 provider/store.rs）。

use once_cell::sync::Lazy;
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

// ───────────────────────── 平台表（全局统一，顺序固定） ─────────────────────────

/// 9 平台 id，顺序与全项目契约一致。
const PLATFORMS: &[&str] = &[
    "wechat", "xhs", "zhihu", "toutiao", "baijia", "bilibili", "douyin", "csdn", "juejin",
];

// ───────────────────────── 数据类型 ─────────────────────────

/// 题库条目：一个选题从进池到发布/否决的全生命周期。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Topic {
    /// uuid 简式（时间戳 + 进程内自增序号，十六进制）
    pub id: String,
    pub platform: String,
    pub title: String,
    #[serde(default)]
    pub angle: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    /// "pool" | "picked" | "drafted" | "published" | "rejected"
    pub status: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub note: String,
    pub created_at: i64,
}

/// 规划队列条目：待发/在跑的一篇稿件。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueItem {
    pub id: String,
    pub platform: String,
    #[serde(default)]
    pub topic_id: Option<String>,
    pub title: String,
    /// ISO8601 排期时间，None = 未排期
    #[serde(default)]
    pub scheduled_at: Option<String>,
    /// "queued" | "running" | "draft_uploaded" | "done" | "failed"
    pub status: String,
    #[serde(default)]
    pub article_path: Option<String>,
    #[serde(default)]
    pub note: String,
    pub updated_at: i64,
}

/// 工作流单步：某个专家 + 某个技能负责一环。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStep {
    pub step: String,
    pub expert_id: String,
    pub skill_id: String,
    #[serde(default)]
    pub note: String,
}

/// 平台设置：开关 / 发送模式 / 周配额 / 工作流编排。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformSettings {
    pub platform: String,
    pub enabled: bool,
    /// "ai"（AI 直传草稿）| "manual"（手动辅助：打开编辑页 + 内容进剪贴板）
    pub send_mode: String,
    pub weekly_quota: u32,
    pub workflow: Vec<WorkflowStep>,
}

/// 度量事件：一次运营动作的原子记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricEvent {
    pub id: String,
    pub platform: String,
    /// "run" | "draft" | "publish" | "fail"
    pub kind: String,
    #[serde(default)]
    pub tokens: u64,
    #[serde(default)]
    pub cost: f64,
    #[serde(default)]
    pub detail: String,
    pub at: i64,
}

/// 前端一次拉全的运营状态快照。metrics 只回最近 500 条（存储侧已滚动裁剪）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaOpsState {
    #[serde(default)]
    pub topics: Vec<Topic>,
    #[serde(default)]
    pub queue: Vec<QueueItem>,
    #[serde(default)]
    pub settings: Vec<PlatformSettings>,
    #[serde(default)]
    pub metrics: Vec<MetricEvent>,
}

/// 平台设置增量补丁：只改传入的字段，其余保留。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformSettingsPatch {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub send_mode: Option<String>,
    #[serde(default)]
    pub weekly_quota: Option<u32>,
    #[serde(default)]
    pub workflow: Option<Vec<WorkflowStep>>,
}

/// 单档 KPI 汇总。
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Kpi {
    pub runs: u64,
    pub drafts: u64,
    pub published: u64,
    pub failed: u64,
    /// published / (published + failed)，无样本为 0
    pub success_rate: f64,
    pub tokens: u64,
    pub cost: f64,
}

/// 度量汇总：近 7 天 / 近 30 天 / 分平台（近 30 天窗口）。
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricsSummary {
    pub d7: Kpi,
    pub d30: Kpi,
    pub per_platform: HashMap<String, Kpi>,
}

// ───────────────────────── 持久化 store ─────────────────────────

/// 落盘结构 == 运营状态快照（同构，直接复用）。
type MediaStore = MediaOpsState;

/// 进程内 store 单例；首次访问时从磁盘加载 + seed 9 平台默认设置。
static STORE: Lazy<RwLock<MediaStore>> = Lazy::new(|| RwLock::new(load_or_seed()));
/// 串行化「读-改-写」磁盘，防并发命令交错撕裂 JSON（与 atomic_write 联合根治损坏）。
static IO_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));
/// id 生成的进程内自增序号，保证同毫秒多次生成也不撞。
static SEQ: AtomicU64 = AtomicU64::new(0);

fn home() -> PathBuf {
    directories::UserDirs::new()
        .map(|u| u.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// `~/PolarisGEO/data/mediaops.json`
fn data_path() -> PathBuf {
    home()
        .join("PolarisGEO")
        .join("data")
        .join("mediaops.json")
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// uuid 简式：毫秒时间戳 + 进程内自增序号（十六进制），无 uuid crate 依赖也够唯一。
fn gen_id() -> String {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    format!("{ms:x}{:04x}", seq & 0xffff)
}

// ───────────────────────── 默认工作流 seed ─────────────────────────

/// 平台写作 pipeline 技能 id（每平台一套文风流水线，先给占位，用户可在设置里改）。
fn write_skill(platform: &str) -> String {
    format!("media-pipeline-{platform}")
}

/// 平台排版技能 id：公众号复用已内嵌的 wechat-md-typesetter，其余给占位。
fn typeset_skill(platform: &str) -> String {
    match platform {
        "wechat" => "wechat-md-typesetter".to_string(),
        other => format!("media-typeset-{other}"),
    }
}

/// 单平台默认工作流：选题→调研→写作→质检→AI痕迹优化→配图→排版→投递。
fn default_workflow(platform: &str) -> Vec<WorkflowStep> {
    let s = |step: &str, expert: &str, skill: String| WorkflowStep {
        step: step.to_string(),
        expert_id: expert.to_string(),
        skill_id: skill,
        note: String::new(),
    };
    vec![
        s("选题", "media-strategist", "hot-topic-radar".to_string()),
        s("调研", "media-researcher", "deep-research".to_string()),
        s("写作", "media-writer", write_skill(platform)),
        s("质检", "media-reviewer", String::new()),
        s("AI痕迹优化", "media-deaiflavor", String::new()),
        s("配图", "media-imagedirector", "media-publisher".to_string()),
        s("排版", "media-typesetter", typeset_skill(platform)),
        s("投递", "media-publisher", "media-publisher".to_string()),
    ]
}

/// 供执行面（media_engine）查询某平台的工作流编排——「哪个环节由哪个专家 + 哪个技能负责」
/// 的唯一真相源。此前执行面把专家/脚本硬编码，UI 里配的编排是装饰性的；开放这个读口后
/// 执行面按编排取人，流程详情里的「哪个专家」才是真值而非常量。
pub fn workflow_for(platform: &str) -> Vec<WorkflowStep> {
    STORE
        .read()
        .settings
        .iter()
        .find(|s| s.platform == platform)
        .map(|s| s.workflow.clone())
        .unwrap_or_else(|| default_workflow(platform))
}

/// 按环节名（"写作" / "排版" / "投递" …）取该平台编排的那一格。
pub fn workflow_step_for(platform: &str, step: &str) -> Option<WorkflowStep> {
    workflow_for(platform).into_iter().find(|w| w.step == step)
}

fn default_settings(platform: &str) -> PlatformSettings {
    PlatformSettings {
        platform: platform.to_string(),
        enabled: true,
        send_mode: "ai".to_string(),
        weekly_quota: 3,
        workflow: default_workflow(platform),
    }
}

/// 给缺失的平台补上默认设置（幂等）。返回是否有新增。
fn seed_missing_settings(store: &mut MediaStore) -> bool {
    let mut changed = false;
    for &p in PLATFORMS {
        if !store.settings.iter().any(|s| s.platform == p) {
            store.settings.push(default_settings(p));
            changed = true;
        }
    }
    changed
}

/// 从磁盘加载 store；不存在/损坏则空 store。随后 seed 缺失的平台设置，若有变更即落盘。
fn load_or_seed() -> MediaStore {
    let path = data_path();
    let mut store: MediaStore = if path.exists() {
        let txt = fs::read_to_string(&path).unwrap_or_default();
        if txt.trim().is_empty() {
            MediaStore::default()
        } else {
            match serde_json::from_str(&txt) {
                Ok(s) => s,
                Err(_) => {
                    // 解析失败别静默清空用户数据：留一份 .corrupt.bak 供抢救，再回落空 store。
                    let mut bak = path.as_os_str().to_owned();
                    bak.push(".corrupt.bak");
                    let _ = fs::copy(&path, PathBuf::from(bak));
                    MediaStore::default()
                }
            }
        }
    } else {
        MediaStore::default()
    };

    let seeded = seed_missing_settings(&mut store);
    if seeded {
        // 首启/升级补种后立即落盘，之后重启不再重复种。
        write_store(&path, &store);
    }
    store
}

/// 原子落盘：先写同目录临时文件（sync_all 刷盘）再 rename 覆盖，杜绝 torn write 破坏 JSON。
fn atomic_write(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".polaris.tmp");
    let tmp = PathBuf::from(tmp);
    {
        use std::io::Write;
        let mut f = fs::File::create(&tmp)?;
        f.write_all(contents.as_bytes())?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)
}

fn write_store(path: &Path, store: &MediaStore) {
    if let Ok(txt) = serde_json::to_string_pretty(store) {
        let _ = atomic_write(path, &txt);
    }
}

/// 把内存 store 持久化到磁盘（IO 串行化）。
fn persist() {
    let _io = IO_LOCK.lock();
    let path = data_path();
    write_store(&path, &STORE.read());
}

// ───────────────────────── Commands: 状态 ─────────────────────────

/// 一次拉全运营状态（题库 / 队列 / 平台设置 / 最近 500 度量）。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn mediaops_state() -> MediaOpsState {
    STORE.read().clone()
}

// ───────────────────────── Commands: 题库 Topic ─────────────────────────

/// 新增选题（进池 status=pool）。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn mediaops_topic_add(
    platform: String,
    title: String,
    angle: Option<String>,
    keywords: Option<Vec<String>>,
    source: Option<String>,
) -> Topic {
    let topic = Topic {
        id: gen_id(),
        platform,
        title: title.trim().to_string(),
        angle: angle.unwrap_or_default(),
        keywords: keywords.unwrap_or_default(),
        status: "pool".to_string(),
        source: source.unwrap_or_default(),
        note: String::new(),
        created_at: now_secs(),
    };
    STORE.write().topics.push(topic.clone());
    persist();
    topic
}

/// 更新选题（状态/标题/角度/备注，传什么改什么）。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn mediaops_topic_update(
    id: String,
    status: Option<String>,
    title: Option<String>,
    angle: Option<String>,
    note: Option<String>,
) -> Result<Topic, String> {
    let updated = {
        let mut store = STORE.write();
        let t = store
            .topics
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or_else(|| format!("选题不存在：{id}"))?;
        if let Some(v) = status {
            t.status = v;
        }
        if let Some(v) = title {
            t.title = v;
        }
        if let Some(v) = angle {
            t.angle = v;
        }
        if let Some(v) = note {
            t.note = v;
        }
        t.clone()
    };
    persist();
    Ok(updated)
}

/// 删除选题。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn mediaops_topic_delete(id: String) -> Result<(), String> {
    STORE.write().topics.retain(|t| t.id != id);
    persist();
    Ok(())
}

// ───────────────────────── Commands: 规划队列 QueueItem ─────────────────────────

/// 入队一篇稿件（status=queued）。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn mediaops_queue_add(
    platform: String,
    topic_id: Option<String>,
    title: String,
    scheduled_at: Option<String>,
) -> QueueItem {
    let item = QueueItem {
        id: gen_id(),
        platform,
        topic_id,
        title: title.trim().to_string(),
        scheduled_at,
        status: "queued".to_string(),
        article_path: None,
        note: String::new(),
        updated_at: now_secs(),
    };
    STORE.write().queue.push(item.clone());
    persist();
    item
}

/// 更新队列项（状态/备注/文章路径，传什么改什么）。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn mediaops_queue_update(
    id: String,
    status: Option<String>,
    note: Option<String>,
    article_path: Option<String>,
) -> Result<QueueItem, String> {
    let updated = {
        let mut store = STORE.write();
        let q = store
            .queue
            .iter_mut()
            .find(|q| q.id == id)
            .ok_or_else(|| format!("队列项不存在：{id}"))?;
        if let Some(v) = status {
            q.status = v;
        }
        if let Some(v) = note {
            q.note = v;
        }
        if let Some(v) = article_path {
            q.article_path = Some(v);
        }
        q.updated_at = now_secs();
        q.clone()
    };
    persist();
    Ok(updated)
}

/// 删除队列项。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn mediaops_queue_delete(id: String) -> Result<(), String> {
    STORE.write().queue.retain(|q| q.id != id);
    persist();
    Ok(())
}

// ───────────────────────── Commands: 平台设置 ─────────────────────────

/// 增量修改某平台设置；平台不存在则以默认设置为底再套补丁。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn mediaops_settings_set(
    platform: String,
    patch: PlatformSettingsPatch,
) -> Result<PlatformSettings, String> {
    let result = {
        let mut store = STORE.write();
        // 找到或按默认新建。
        if !store.settings.iter().any(|s| s.platform == platform) {
            store.settings.push(default_settings(&platform));
        }
        let s = store
            .settings
            .iter_mut()
            .find(|s| s.platform == platform)
            .expect("just ensured present");
        if let Some(v) = patch.enabled {
            s.enabled = v;
        }
        if let Some(v) = patch.send_mode {
            s.send_mode = v;
        }
        if let Some(v) = patch.weekly_quota {
            s.weekly_quota = v;
        }
        if let Some(v) = patch.workflow {
            s.workflow = v;
        }
        s.clone()
    };
    persist();
    Ok(result)
}

// ───────────────────────── Commands: 度量 ─────────────────────────

/// 最多保留的度量事件条数（滚动窗口）。
const METRIC_CAP: usize = 500;

/// 追加一条度量事件，滚动保留最近 METRIC_CAP 条。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn mediaops_metric_add(
    platform: String,
    kind: String,
    tokens: Option<u64>,
    cost: Option<f64>,
    detail: Option<String>,
) -> Result<(), String> {
    {
        let mut store = STORE.write();
        store.metrics.push(MetricEvent {
            id: gen_id(),
            platform,
            kind,
            tokens: tokens.unwrap_or(0),
            cost: cost.unwrap_or(0.0),
            detail: detail.unwrap_or_default(),
            at: now_secs(),
        });
        let len = store.metrics.len();
        if len > METRIC_CAP {
            store.metrics.drain(0..len - METRIC_CAP);
        }
    }
    persist();
    Ok(())
}

/// 把一批事件累加进一个 KPI 桶。
fn accumulate(kpi: &mut Kpi, e: &MetricEvent) {
    match e.kind.as_str() {
        "run" => kpi.runs += 1,
        "draft" => kpi.drafts += 1,
        "publish" => kpi.published += 1,
        "fail" => kpi.failed += 1,
        _ => {}
    }
    kpi.tokens += e.tokens;
    kpi.cost += e.cost;
}

/// 成功率 = 发布 /（发布 + 失败）。
fn finalize_rate(kpi: &mut Kpi) {
    let denom = kpi.published + kpi.failed;
    kpi.success_rate = if denom > 0 {
        kpi.published as f64 / denom as f64
    } else {
        0.0
    };
}

/// KPI 汇总：近 7 天 / 近 30 天 / 分平台（近 30 天窗口）。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn mediaops_metrics_summary() -> MetricsSummary {
    let store = STORE.read();
    let now = now_secs();
    let d7_from = now - 7 * 86_400;
    let d30_from = now - 30 * 86_400;

    let mut d7 = Kpi::default();
    let mut d30 = Kpi::default();
    let mut per_platform: HashMap<String, Kpi> = HashMap::new();

    for e in &store.metrics {
        if e.at >= d7_from {
            accumulate(&mut d7, e);
        }
        if e.at >= d30_from {
            accumulate(&mut d30, e);
            accumulate(per_platform.entry(e.platform.clone()).or_default(), e);
        }
    }

    finalize_rate(&mut d7);
    finalize_rate(&mut d30);
    for kpi in per_platform.values_mut() {
        finalize_rate(kpi);
    }

    MetricsSummary {
        d7,
        d30,
        per_platform,
    }
}
