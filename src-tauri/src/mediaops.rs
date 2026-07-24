//! 自媒体「运营中心」— 题库 / 规划队列 / 平台设置 / 度量事件的本地持久化与命令。
//!
//! 背景：自媒体运营是一条「写作 → 配图 → 排版 → 投递」的流水线（= 执行面真正会跑的
//! 四阶段），跨 7 个平台（公众号 / 小红书 / 知乎 / 头条 / 百家号 / B站 / 抖音）。本模块给
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

/// 平台中文名（例行任务标题 / 进化卡标题用）。
fn platform_name(p: &str) -> &str {
    match p {
        "wechat" => "公众号",
        "xhs" => "小红书",
        "zhihu" => "知乎",
        "toutiao" => "头条",
        "baijia" => "百家号",
        "bilibili" => "B站",
        "douyin" => "抖音",
        "csdn" => "CSDN",
        "juejin" => "掘金",
        other => other,
    }
}

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
    /// alias 容错：外部 agent 曾直接以蛇形键写过此文件，一个键名不认导致整库被判损坏太脆。
    #[serde(alias = "created_at")]
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

/// 发文排期：每平台一条，每 `interval_days` 天自动往规划队列塞一条「例行发文」任务。
/// 只入队、不发布——后续仍走正常 pipeline + 人工审批（红线：绝不自动发布）。
/// 人与大脑（autopilot）共用 `mediaops_schedule_set` 调参，实际变更自动在进化时间线留卡。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishSchedule {
    pub platform: String,
    pub enabled: bool,
    /// 发文周期（天/篇），默认 3
    #[serde(default = "default_interval_days")]
    pub interval_days: u32,
    /// 上次触发时刻（unix 秒）；种子时锚定为创建时刻，首次触发在一个周期之后
    #[serde(default)]
    pub last_fired_at: Option<i64>,
    /// 最近一次调参来源："seed" | "human" | "brain"
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub note: String,
    /// 派发上下文：到期入队时附在任务上，主 agent 领任务时以此为工作指令。
    /// 人在 UI 改、大脑经 apihub 改都走 mediaops_schedule_set；空串则触发时回退默认指令。
    #[serde(default)]
    pub context: String,
    pub updated_at: i64,
}

fn default_interval_days() -> u32 {
    3
}

/// 平台默认派发上下文（种子值 & 空上下文的触发回退）。
fn default_schedule_context(platform: &str) -> String {
    format!(
        "从题库为{name}选一条 pool 状态选题（题库无货则先按缺口清单补 2 个候选再选其一），\
按{name}的平台工作流跑完整流水线：写作→配图→排版→投递，产出到待审批为止。\
标题与首段须与其他平台差异化；遵守全局日配额、错峰窗口与红线——绝不自动发布，发布由人审批。",
        name = platform_name(platform)
    )
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
    pub schedules: Vec<PublishSchedule>,
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

/// 执行面真正会跑的环节名，顺序即执行顺序（与 media_engine::ALL_STAGES 一一对应）。
///
/// 编排里**只放会执行的环节**。此前还声明了「选题/调研/质检/AI痕迹优化」四步并各配了
/// 专家与技能，但执行面从来不跑它们——用户在 UI 上换掉「质检专家」不会有任何效果，
/// 编排页成了装饰。宁可少四格是真的，也不要四格永远不生效的旋钮。
pub const EXECUTED_STEPS: &[&str] = &["写作", "配图", "排版", "投递"];

/// 单平台默认工作流：写作→配图→排版→投递（= 执行面的 generate/image/typeset/upload）。
fn default_workflow(platform: &str) -> Vec<WorkflowStep> {
    let s = |step: &str, expert: &str, skill: String| WorkflowStep {
        step: step.to_string(),
        expert_id: expert.to_string(),
        skill_id: skill,
        note: String::new(),
    };
    vec![
        s("写作", "media-writer", write_skill(platform)),
        s("配图", "media-imagedirector", "media-publisher".to_string()),
        s("排版", "media-typesetter", typeset_skill(platform)),
        s("投递", "media-publisher", "media-publisher".to_string()),
    ]
}

/// 旧存档规整：把编排里执行面不认识的环节剔掉，并按执行顺序排好。
/// 只动 workflow 一处，其余用户数据（题库/队列/排期/度量）一字不改。返回是否改过。
fn migrate_workflows(store: &mut MediaStore) -> bool {
    let mut changed = false;
    for st in store.settings.iter_mut() {
        let before = st.workflow.len();
        let mut kept: Vec<WorkflowStep> = Vec::new();
        for name in EXECUTED_STEPS {
            if let Some(w) = st.workflow.iter().find(|w| w.step == *name) {
                kept.push(w.clone());
            }
        }
        // 存档里连一个执行环节都没有（极旧/被清空）→ 重新种默认，别留个空编排。
        if kept.is_empty() {
            kept = default_workflow(&st.platform);
        }
        if before != kept.len() || st.workflow.iter().map(|w| &w.step).ne(kept.iter().map(|w| &w.step)) {
            st.workflow = kept;
            changed = true;
        }
    }
    changed
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

fn default_schedule(platform: &str) -> PublishSchedule {
    PublishSchedule {
        platform: platform.to_string(),
        enabled: true,
        interval_days: default_interval_days(),
        // 锚定为创建时刻：首次触发在一个周期之后，而不是首启就 9 条任务糊脸。
        last_fired_at: Some(now_secs()),
        source: "seed".to_string(),
        note: String::new(),
        context: default_schedule_context(platform),
        updated_at: now_secs(),
    }
}

/// 给缺失的平台补上默认排期（幂等，默认 3 天/篇）；给老记录回填空的派发上下文。返回是否有变更。
fn seed_missing_schedules(store: &mut MediaStore) -> bool {
    let mut changed = false;
    for &p in PLATFORMS {
        if !store.schedules.iter().any(|s| s.platform == p) {
            store.schedules.push(default_schedule(p));
            changed = true;
        }
    }
    for s in store.schedules.iter_mut() {
        if s.context.trim().is_empty() {
            s.context = default_schedule_context(&s.platform);
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

    let seeded =
        seed_missing_settings(&mut store) | seed_missing_schedules(&mut store) | migrate_workflows(&mut store);
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

// ───────────────────────── Commands: 发文排期 ─────────────────────────

/// 列出各平台发文排期。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn mediaops_schedule_list() -> Vec<PublishSchedule> {
    STORE.read().schedules.clone()
}

/// 调整某平台排期（周期/启停/备注，传什么改什么）。
/// `source`：谁在调——"human"（默认，UI 手调）| "brain"（大脑·autopilot 经 apihub 调）。
/// 不变式⑥：周期或启停的实际变更自动在进化时间线留一张 schedule 卡（best-effort，不阻断调参）。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn mediaops_schedule_set(
    platform: String,
    interval_days: Option<u32>,
    enabled: Option<bool>,
    note: Option<String>,
    context: Option<String>,
    source: Option<String>,
) -> Result<PublishSchedule, String> {
    if !PLATFORMS.contains(&platform.as_str()) {
        return Err(format!("未知平台：{platform}"));
    }
    if let Some(d) = interval_days {
        if !(1..=60).contains(&d) {
            return Err("发文周期须在 1–60 天之间".to_string());
        }
    }
    let src = source.unwrap_or_else(|| "human".to_string());
    let (result, change_desc) = {
        let mut store = STORE.write();
        if !store.schedules.iter().any(|s| s.platform == platform) {
            store.schedules.push(default_schedule(&platform));
        }
        let s = store
            .schedules
            .iter_mut()
            .find(|s| s.platform == platform)
            .expect("just ensured present");
        let mut changes: Vec<String> = Vec::new();
        if let Some(v) = interval_days {
            if v != s.interval_days {
                changes.push(format!("周期 {} → {} 天/篇", s.interval_days, v));
                s.interval_days = v;
            }
        }
        if let Some(v) = enabled {
            if v != s.enabled {
                changes.push(if v { "启用排期" } else { "停用排期" }.to_string());
                s.enabled = v;
            }
        }
        if let Some(v) = note {
            s.note = v;
        }
        if let Some(v) = context {
            if v.trim() != s.context.trim() {
                changes.push("更新派发上下文".to_string());
                s.context = v.trim().to_string();
            }
        }
        if !changes.is_empty() {
            s.source = src.clone();
        }
        s.updated_at = now_secs();
        (s.clone(), changes.join("；"))
    };
    persist();
    if !change_desc.is_empty() {
        let _ = crate::evolution::evolution_add(
            "schedule".to_string(),
            format!("调度：{} {}", platform_name(&result.platform), change_desc),
            Some(result.note.clone()),
            // 上下文变更把全文放进 diff，时间线上可追溯大脑/人到底改成了什么
            change_desc
                .contains("派发上下文")
                .then(|| result.context.clone()),
            Some(src),
            None,
            None,
            None,
            // 调参保存下去就是生效了，不是待验证的提案 —— 记「已固化」，
            // 否则每调一次周期就多一张永远无人裁决的观察卡。
            Some("已固化".to_string()),
        );
    }
    Ok(result)
}

/// 例行任务的 note 前缀：排期巡检自己下的单，据此与人工排的稿子区分开。
const SCHEDULE_MARK: &str = "schedule:auto";

/// 排期巡检：到期的平台自动往规划队列塞一条「例行发文」任务，返回本轮新入队的任务。
/// - 只入队、不发布——任务照常走 pipeline + 人工审批（红线：绝不自动发布）。
/// - 平台被停用、或**上一条例行任务还没被消化**时跳过，防堆积；跳过也推进
///   last_fired_at——积压消化后从下一个周期重新起算，不连环补发。
///
/// 防堆积此前判的是「该平台有任何 queued/running 任务」，而例行任务的消费者是人
/// （点开跑），没人点它就永远 queued —— 于是一条没人管的例行任务会把该平台的自动排期
/// **永久掐死**，且静默无错。现在只看「有没有另一条**例行**任务还没消化」：人工排的
/// 稿子再多也不挡自动排期，而例行任务本身仍然最多同时存在一条。
pub fn schedule_tick_internal() -> Vec<QueueItem> {
    let now = now_secs();
    let mut fired: Vec<QueueItem> = Vec::new();
    let mut advanced = false;
    {
        let mut store = STORE.write();
        let due: Vec<(String, u32, String)> = store
            .schedules
            .iter()
            .filter(|s| s.enabled)
            .filter(|s| {
                s.last_fired_at
                    .map_or(true, |t| now - t >= s.interval_days as i64 * 86_400)
            })
            .map(|s| (s.platform.clone(), s.interval_days, s.context.clone()))
            .collect();
        for (p, days, ctx) in due {
            let platform_off = store
                .settings
                .iter()
                .any(|s| s.platform == p && !s.enabled);
            // 只挡「上一条例行任务还没消化」，不挡人工排的稿子。
            let busy = store.queue.iter().any(|q| {
                q.platform == p
                    && q.note.contains(SCHEDULE_MARK)
                    && matches!(q.status.as_str(), "queued" | "running")
            });
            if let Some(s) = store.schedules.iter_mut().find(|s| s.platform == p) {
                s.last_fired_at = Some(now);
                advanced = true;
            }
            if platform_off || busy {
                continue;
            }
            // 派发上下文附在 note 里：主 agent 领任务时以此为工作指令（空则用平台默认）
            let ctx = if ctx.trim().is_empty() {
                default_schedule_context(&p)
            } else {
                ctx
            };
            let item = QueueItem {
                id: gen_id(),
                platform: p.clone(),
                topic_id: None,
                title: format!("【例行】{}：选题待定（每 {} 天一篇）", platform_name(&p), days),
                scheduled_at: None,
                status: "queued".to_string(),
                article_path: None,
                note: format!("{SCHEDULE_MARK}\n派发上下文：{ctx}"),
                updated_at: now,
            };
            store.queue.push(item.clone());
            fired.push(item);
        }
    }
    if advanced {
        persist();
    }
    fired
}

/// 该队列项是不是排期巡检下的「选题待定」占位——它的 title 只是占位文案
/// （「【例行】微信公众号：选题待定…」），直接拿去当文章标题会写出一篇标题荒唐的稿子。
/// 执行面据此要求调用方先给真标题。
pub fn is_schedule_placeholder(item: &QueueItem) -> bool {
    item.note.contains(SCHEDULE_MARK) && item.topic_id.is_none()
}

/// 手动触发一轮排期巡检（UI「立即巡检」按钮 / apihub）。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn mediaops_schedule_tick() -> Vec<QueueItem> {
    schedule_tick_internal()
}

/// 后台定时器：启动即巡检一次（补上应用关着期间错过的排期），此后每 30 分钟一轮。
pub fn start_schedule_ticker() {
    std::thread::spawn(|| loop {
        let fired = schedule_tick_internal();
        if !fired.is_empty() {
            eprintln!("[schedule] 例行发文入队 {} 条", fired.len());
        }
        std::thread::sleep(std::time::Duration::from_secs(30 * 60));
    });
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

// ───────────────────────── Commands: 平台后台数据爬虫（metrics_crawler.py 桥） ─────────────────────────
//
// 度量事件（上面那套）记的是**我们自己干了什么**：跑了几次、出了几篇草稿、烧了多少 token。
// 看板上「阅读 / 点击」「平台 × 指标热力表」问的是另一件事：**发出去之后平台上发生了什么**。
// 那些数字只有平台创作者后台有，得去抓 —— 就是 media-publisher/scripts/metrics_crawler.py。
//
// 这层桥补的是一个断链：脚本本来就在仓库里，也能跑，但**应用从没调用过它，看板也从没读过
// 它的产物**，于是那几张卡永远是「—」。这里给出两个命令：
//   · `mediaops_crawl_run`      跑爬虫（同步，一个平台约 25 秒）
//   · `mediaops_crawl_snapshots` 读快照（秒回，看板加载时用）
// 快照契约见脚本 `_write_snapshot`：{platform, name, crawledAt, ok, summary{…}}。

/// 一个平台的最新抓取快照（读 `~/PolarisGEO/data/metrics_latest/{platform}.json`）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrawlSnapshot {
    pub platform: String,
    #[serde(default)]
    pub name: String,
    // alias 是必需的，不是保险：本结构体对**两个方向**说不同的话——写文件的是 python
    // （蛇形键 crawled_at），读文件的是这里，而 rename_all=camelCase 又要求出口给前端的是
    // crawledAt。没有 alias 时 serde 找不到 crawledAt，配合 #[serde(default)] 会**静默**
    // 落成空串 / 0 / false，看板于是拿到一份「抓过但全空」的快照——不报错，只是永远没数。
    #[serde(default, alias = "crawled_at")]
    pub crawled_at: String,
    #[serde(default, alias = "crawled_ts")]
    pub crawled_ts: i64,
    /// false = 跑通了但一个指标都没抓到（看板要能区分「没抓到」与「真是 0」）
    #[serde(default)]
    pub ok: bool,
    /// 规范化指标：fans / reads / impressions / likes / comments / shares / income
    #[serde(default)]
    pub summary: HashMap<String, f64>,
}

/// `~/PolarisGEO/data/metrics_latest/`
fn metrics_latest_dir() -> PathBuf {
    home().join("PolarisGEO").join("data").join("metrics_latest")
}

/// 读所有平台的最新抓取快照，按抓取时间倒序。没抓过就是空数组（不是错误）。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn mediaops_crawl_snapshots() -> Vec<CrawlSnapshot> {
    let dir = metrics_latest_dir();
    let mut out: Vec<CrawlSnapshot> = Vec::new();
    let Ok(entries) = fs::read_dir(&dir) else {
        return out;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        // 单个快照坏了不该让整块面板空掉——跳过它，其余照读。
        if let Ok(text) = fs::read_to_string(&p) {
            if let Ok(snap) = serde_json::from_str::<CrawlSnapshot>(&text) {
                out.push(snap);
            }
        }
    }
    out.sort_by(|a, b| b.crawled_ts.cmp(&a.crawled_ts));
    out
}

/// 跑一轮抓取。`platforms` 为空 = 抓所有已配置平台（脚本 `--all`）。
///
/// **同步阻塞**：一个平台约 25 秒（页面 settle + 滚动触发懒加载），前端要给转圈。
/// 脚本每步吐一行 JSON，这里只把最后的汇总行捡出来当返回值；完整输出进 stderr 便于排障。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn mediaops_crawl_run(platforms: Option<Vec<String>>) -> Result<Vec<CrawlSnapshot>, String> {
    let python = crate::media_engine::resolve_python()?;
    let (script, source) =
        crate::media_engine::resolve_skill_script("media-publisher", "metrics_crawler.py")?;

    let mut cmd = std::process::Command::new(&python);
    cmd.arg(&script);
    match platforms.as_ref().filter(|v| !v.is_empty()) {
        Some(list) => {
            cmd.arg("--platform").arg(list.join(","));
        }
        None => {
            cmd.arg("--all");
        }
    }
    // 脚本内部统一 UTF-8 输出；Windows 上父进程若不声明，中文平台名会以 GBK 回来变乱码。
    cmd.env("PYTHONIOENCODING", "utf-8")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    crate::media_engine::no_window(&mut cmd);

    let out = cmd
        .output()
        .map_err(|e| format!("调起 metrics_crawler.py 失败（{source}）：{e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    eprintln!("[crawl] {source} exit={:?}\n{stdout}\n{stderr}", out.status.code());

    if !out.status.success() {
        // 脚本的失败行本身就是可读的原因，优先把它抛给用户，而不是干巴巴一个退出码。
        let reason = stdout
            .lines()
            .rev()
            .find(|l| l.contains("\"result\": \"failed\"") || l.contains("\"result\":\"failed\""))
            .map(|l| l.to_string())
            .unwrap_or_else(|| stderr.lines().last().unwrap_or_default().to_string());
        return Err(format!("抓取失败：{reason}"));
    }
    // 产物就是快照文件，直接回读——省得在这儿重复解析脚本的进度行。
    Ok(mediaops_crawl_snapshots())
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 钉死跨语言契约：python 端 `metrics_crawler.py::_write_snapshot` 写蛇形键，
    /// 这边 `rename_all = "camelCase"` 出口给前端驼峰键——两头不一致时 serde 不报错，
    /// 只会把带默认值的字段静默填空，症状是「看板永远没数」而不是「报了个错」。
    /// 下面这段 JSON 是脚本真实产出的形状（照抄自 metrics_latest/toutiao.json）。
    #[test]
    fn crawl_snapshot_reads_python_snake_case() {
        let raw = r#"{
            "platform": "toutiao",
            "name": "今日头条",
            "crawled_at": "2026-07-25 03:12:07",
            "crawled_ts": 1785000727,
            "ok": true,
            "summary": {"reads": 1.0, "impressions": 469.0, "fans": 0.0},
            "views": {}
        }"#;
        let snap: CrawlSnapshot = serde_json::from_str(raw).expect("快照应能反序列化");
        assert_eq!(snap.platform, "toutiao");
        assert_eq!(snap.crawled_at, "2026-07-25 03:12:07");
        assert_eq!(snap.crawled_ts, 1785000727);
        assert!(snap.ok);
        assert_eq!(snap.summary.get("impressions"), Some(&469.0));
    }

    /// 反向：出口给前端必须是驼峰（vDashboard 读的是 crawledAt / crawledTs）。
    #[test]
    fn crawl_snapshot_writes_camel_case() {
        let snap = CrawlSnapshot {
            platform: "toutiao".into(),
            crawled_at: "2026-07-25 03:12:07".into(),
            crawled_ts: 1785000727,
            ok: true,
            ..Default::default()
        };
        let out = serde_json::to_string(&snap).expect("应能序列化");
        assert!(out.contains("\"crawledAt\""), "前端契约是驼峰：{out}");
        assert!(out.contains("\"crawledTs\""), "前端契约是驼峰：{out}");
    }

    /// 编排 == 执行面：编排里出现的每一环都必须是执行面真会跑的那四步之一，顺序也一致。
    /// 一旦有人往默认编排里加一格「质检」而执行面不跑它，UI 上就又多了一个不生效的旋钮。
    #[test]
    fn default_workflow_matches_executed_steps() {
        let w = default_workflow("wechat");
        let names: Vec<&str> = w.iter().map(|s| s.step.as_str()).collect();
        assert_eq!(names, EXECUTED_STEPS, "默认编排必须与执行面四阶段一一对应且同序");
        assert!(w.iter().all(|s| !s.expert_id.is_empty()), "每一格都要有专家");
    }

    /// 旧存档里那些从不执行的环节（选题/调研/质检/AI痕迹优化）读进来要被剔干净，
    /// 剩下的执行环节保留用户自己配的专家，不被默认值覆盖。
    #[test]
    fn migrate_drops_never_executed_steps_and_keeps_user_picks() {
        let step = |s: &str, e: &str| WorkflowStep {
            step: s.into(),
            expert_id: e.into(),
            skill_id: String::new(),
            note: String::new(),
        };
        let mut store = MediaStore {
            settings: vec![PlatformSettings {
                platform: "wechat".into(),
                enabled: true,
                send_mode: "ai".into(),
                weekly_quota: 3,
                workflow: vec![
                    step("选题", "media-strategist"),
                    step("调研", "media-researcher"),
                    step("写作", "my-writer"), // 用户换过的人，必须留住
                    step("质检", "media-reviewer"),
                    step("AI痕迹优化", "media-deaiflavor"),
                    step("配图", "media-imagedirector"),
                    step("排版", "media-typesetter"),
                    step("投递", "media-publisher"),
                ],
            }],
            ..Default::default()
        };
        assert!(migrate_workflows(&mut store), "有可剔的环节时应报告改动");
        let names: Vec<&str> = store.settings[0].workflow.iter().map(|s| s.step.as_str()).collect();
        assert_eq!(names, EXECUTED_STEPS);
        assert_eq!(store.settings[0].workflow[0].expert_id, "my-writer", "用户配的专家不能被覆盖");
        assert!(!migrate_workflows(&mut store), "已规整过的存档再跑一次应无改动（幂等）");
    }

    /// 例行任务的占位标题不能被当成文章标题：执行面靠这个判定拦下来。
    #[test]
    fn schedule_placeholder_is_detectable() {
        let mut q = QueueItem {
            id: "q1".into(),
            platform: "wechat".into(),
            topic_id: None,
            title: "【例行】公众号：选题待定（每 3 天一篇）".into(),
            scheduled_at: None,
            status: "queued".into(),
            article_path: None,
            note: format!("{SCHEDULE_MARK}\n派发上下文：…"),
            updated_at: 0,
        };
        assert!(is_schedule_placeholder(&q), "排期下的占位任务应被识别");
        q.topic_id = Some("t1".into()); // 挂上真选题 = 有真标题了
        assert!(!is_schedule_placeholder(&q));
        q.topic_id = None;
        q.note = "人工排的".into();
        assert!(!is_schedule_placeholder(&q), "人工排的稿子不该被当成占位");
    }
}
