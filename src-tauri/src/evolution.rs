//! 循环工程（M10 大脑·进化）— insight 卡库 / 进化时间线 / prompt 版本树 / 飞轮健康度。
//!
//! 设计铁律（来自 PRD v2 不变式⑥）：任何 prompt / skill / 专家 / 调度的变更都必须在
//! 进化时间线上留一张卡（谁提议、改了什么、diff、预期、7 天后实际、状态）。
//! 「飞轮健康度」= 本月「度量改变行为」的证据数：有多少变更能追溯到某次具体度量；
//! 为 0 说明系统退化成流水线（度量不再改变行为），大屏亮红灯。
//!
//! 三类 insight 卡：anti_pattern（教训）/ rule（规则）/ playbook（打法）。
//! 检索权重 = 相似度 × (1 + λ × 功劳分)；固化一次进化 → 相关卡功劳分 +1。
//!
//! prompt 版本树：只允许改专家文件里 evolvable 锚点段落（style_notes / opening_formula …），
//! 角色骨架与红线不可自改；全部版本化、可回滚。本模块存版本历史与激活指针，
//! 真正写回专家 overlay 文件由前端/调用方拿返回内容再走 expert_media_overlay_set。
//!
//! 落盘：`~/PolarisGEO/data/evolution.json`，原子写入（与 mediaops.rs 同一套纪律）。

use once_cell::sync::Lazy;
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

// ───────────────────────── 数据类型 ─────────────────────────

/// insight 卡：一条可注入写作/决策的经验。人可增删改——写一张卡 = 直接教主 Agent 一条经验。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InsightCard {
    pub id: String,
    /// "anti_pattern" | "rule" | "playbook"
    pub kind: String,
    pub title: String,
    pub content: String,
    /// 生效范围："全局" 或平台 id（wechat/zhihu/…）或专家 id
    #[serde(default)]
    pub scope: String,
    #[serde(default)]
    pub tags: Vec<String>,
    /// 功劳分：进化固化时 +1，检索权重 = 相似度 × (1 + λ × credit)
    #[serde(default)]
    pub credit: f64,
    /// 证据链接（度量事件 id / 进化卡 id / 外部链接）
    #[serde(default)]
    pub evidence: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 进化时间线条目：一次 prompt/skill/专家/调度变更的完整档案卡。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvolutionEntry {
    pub id: String,
    /// "prompt" | "skill" | "expert" | "schedule"
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub detail: String,
    /// 变更 diff（或变更描述）
    #[serde(default)]
    pub diff: String,
    /// 谁提议："autopilot" | "human" | 专家 id
    #[serde(default)]
    pub proposer: String,
    /// 预期效果（观察期结束时对照）
    #[serde(default)]
    pub expect: String,
    /// 7 天观察期后的实际结果
    #[serde(default)]
    pub actual: String,
    /// "观察中" | "已固化" | "已回滚"
    pub status: String,
    /// 关联 insight 卡（固化时这些卡功劳分 +1）
    #[serde(default)]
    pub insight_ids: Vec<String>,
    /// 度量证据（度量事件 id / 描述）——飞轮健康度按「本月有证据的变更」计数
    #[serde(default)]
    pub evidence: Vec<String>,
    pub created_at: i64,
    /// 固化/回滚的时刻，观察中为 None
    #[serde(default)]
    pub decided_at: Option<i64>,
}

/// prompt 版本：某专家某 evolvable 锚点（可含平台补丁维度）的一个历史版本。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptVersion {
    pub id: String,
    pub expert_id: String,
    /// 平台补丁维度；空串 = 基础画像
    #[serde(default)]
    pub platform: String,
    /// evolvable 锚点名（style_notes / opening_formula / …）
    pub anchor: String,
    /// 版本号 v1 起单调递增（同 expert+platform+anchor 内）
    pub version: u32,
    pub content: String,
    /// "active" | "superseded" | "rolled_back"
    pub status: String,
    /// 该版本期间绩效备注（过审率/CTR 等，人或 autopilot 补记）
    #[serde(default)]
    pub perf_note: String,
    pub created_at: i64,
}

/// 前端一次拉全的进化状态快照。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvolutionState {
    #[serde(default)]
    pub insights: Vec<InsightCard>,
    #[serde(default)]
    pub timeline: Vec<EvolutionEntry>,
    #[serde(default)]
    pub prompt_versions: Vec<PromptVersion>,
}

/// 飞轮健康度汇总（本月窗口）。
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlywheelSummary {
    /// 本月「度量改变行为」证据数：本月创建且 evidence 非空的进化条目数。>0 即闭环成立。
    pub health: u64,
    /// 本月新增 insight 卡数（目标 ≥3/周）
    pub month_insights: u64,
    /// 本月固化数
    pub solidified: u64,
    /// 本月回滚数
    pub rolled_back: u64,
    /// 当前观察期进行中的条目数（不限本月）
    pub observing: u64,
}

// ───────────────────────── 持久化 store ─────────────────────────

type EvoStore = EvolutionState;

static STORE: Lazy<RwLock<EvoStore>> = Lazy::new(|| RwLock::new(load()));
static IO_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));
static SEQ: AtomicU64 = AtomicU64::new(0);

/// 时间线滚动上限：进化档案要长留，但防极端膨胀。
const TIMELINE_CAP: usize = 2000;

fn home() -> PathBuf {
    directories::UserDirs::new()
        .map(|u| u.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// `~/PolarisGEO/data/evolution.json`
fn data_path() -> PathBuf {
    home().join("PolarisGEO").join("data").join("evolution.json")
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn gen_id() -> String {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    format!("{ms:x}{:04x}", seq & 0xffff)
}

fn load() -> EvoStore {
    let path = data_path();
    if !path.exists() {
        return EvoStore::default();
    }
    let txt = fs::read_to_string(&path).unwrap_or_default();
    if txt.trim().is_empty() {
        return EvoStore::default();
    }
    match serde_json::from_str(&txt) {
        Ok(s) => s,
        Err(_) => {
            // 解析失败别静默清空：留 .corrupt.bak 供抢救。
            let mut bak = path.as_os_str().to_owned();
            bak.push(".corrupt.bak");
            let _ = fs::copy(&path, PathBuf::from(bak));
            EvoStore::default()
        }
    }
}

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

fn persist() {
    let _io = IO_LOCK.lock();
    let path = data_path();
    if let Ok(txt) = serde_json::to_string_pretty(&*STORE.read()) {
        let _ = atomic_write(&path, &txt);
    }
}

/// 本月起点（本地时区近似：用 UTC 月初，健康度是趋势指标，时区级误差可接受）。
fn month_start(now: i64) -> i64 {
    // 无 chrono 依赖的月初计算：从天数反推年月。
    let days = now / 86_400;
    let mut y = 1970i64;
    let mut rem = days;
    loop {
        let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
        let len = if leap { 366 } else { 365 };
        if rem < len {
            break;
        }
        rem -= len;
        y += 1;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let ml: [i64; 12] = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    for len in ml {
        if rem < len {
            break;
        }
        rem -= len;
    }
    (days - rem) * 86_400
}

// ───────────────────────── Commands: 状态 ─────────────────────────

/// 一次拉全进化状态（insight 卡 / 时间线 / prompt 版本树）。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn evolution_state() -> EvolutionState {
    STORE.read().clone()
}

// ───────────────────────── Commands: insight 卡 ─────────────────────────

/// 手写/沉淀一张 insight 卡。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn insight_add(
    kind: String,
    title: String,
    content: String,
    scope: Option<String>,
    tags: Option<Vec<String>>,
    evidence: Option<Vec<String>>,
) -> Result<InsightCard, String> {
    if !matches!(kind.as_str(), "anti_pattern" | "rule" | "playbook") {
        return Err(format!("未知卡类型：{kind}（合法：anti_pattern/rule/playbook）"));
    }
    let now = now_secs();
    let card = InsightCard {
        id: gen_id(),
        kind,
        title: title.trim().to_string(),
        content,
        scope: scope.unwrap_or_else(|| "全局".to_string()),
        tags: tags.unwrap_or_default(),
        credit: 0.0,
        evidence: evidence.unwrap_or_default(),
        created_at: now,
        updated_at: now,
    };
    STORE.write().insights.push(card.clone());
    persist();
    Ok(card)
}

/// 修改 insight 卡（传什么改什么；credit 只能经固化机制加，不开放直改以防刷分）。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn insight_update(
    id: String,
    title: Option<String>,
    content: Option<String>,
    scope: Option<String>,
    tags: Option<Vec<String>>,
    evidence: Option<Vec<String>>,
) -> Result<InsightCard, String> {
    let updated = {
        let mut store = STORE.write();
        let c = store
            .insights
            .iter_mut()
            .find(|c| c.id == id)
            .ok_or_else(|| format!("insight 卡不存在：{id}"))?;
        if let Some(v) = title {
            c.title = v;
        }
        if let Some(v) = content {
            c.content = v;
        }
        if let Some(v) = scope {
            c.scope = v;
        }
        if let Some(v) = tags {
            c.tags = v;
        }
        if let Some(v) = evidence {
            c.evidence = v;
        }
        c.updated_at = now_secs();
        c.clone()
    };
    persist();
    Ok(updated)
}

/// 删除 insight 卡。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn insight_delete(id: String) -> Result<(), String> {
    STORE.write().insights.retain(|c| c.id != id);
    persist();
    Ok(())
}

// ───────────────────────── Commands: 进化时间线 ─────────────────────────

/// 登记一次进化（status=观察中）。不变式⑥：一切变更先留卡再生效。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn evolution_add(
    kind: String,
    title: String,
    detail: Option<String>,
    diff: Option<String>,
    proposer: Option<String>,
    expect: Option<String>,
    insight_ids: Option<Vec<String>>,
    evidence: Option<Vec<String>>,
) -> Result<EvolutionEntry, String> {
    if !matches!(kind.as_str(), "prompt" | "skill" | "expert" | "schedule") {
        return Err(format!("未知进化类型：{kind}（合法：prompt/skill/expert/schedule）"));
    }
    let entry = EvolutionEntry {
        id: gen_id(),
        kind,
        title: title.trim().to_string(),
        detail: detail.unwrap_or_default(),
        diff: diff.unwrap_or_default(),
        proposer: proposer.unwrap_or_else(|| "human".to_string()),
        expect: expect.unwrap_or_default(),
        actual: String::new(),
        status: "观察中".to_string(),
        insight_ids: insight_ids.unwrap_or_default(),
        evidence: evidence.unwrap_or_default(),
        created_at: now_secs(),
        decided_at: None,
    };
    {
        let mut store = STORE.write();
        store.timeline.push(entry.clone());
        let len = store.timeline.len();
        if len > TIMELINE_CAP {
            store.timeline.drain(0..len - TIMELINE_CAP);
        }
    }
    persist();
    Ok(entry)
}

/// 观察期裁决：固化或回滚。
/// - 固化（已固化）：相关 insight 卡功劳分 +1（度量改变行为的正反馈）。
/// - 回滚（已回滚）：自动沉淀一张 anti_pattern 卡（证据链指回本条目），教训不白吃。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn evolution_decide(
    id: String,
    status: String,
    actual: Option<String>,
) -> Result<EvolutionEntry, String> {
    if !matches!(status.as_str(), "已固化" | "已回滚" | "观察中") {
        return Err(format!("未知状态：{status}（合法：观察中/已固化/已回滚）"));
    }
    let (decided, auto_card) = {
        let mut store = STORE.write();
        let e = store
            .timeline
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or_else(|| format!("进化条目不存在：{id}"))?;
        e.status = status.clone();
        if let Some(v) = actual {
            e.actual = v;
        }
        e.decided_at = if status == "观察中" {
            None
        } else {
            Some(now_secs())
        };
        let snapshot = e.clone();

        let mut auto_card: Option<InsightCard> = None;
        if status == "已固化" {
            let ids = snapshot.insight_ids.clone();
            for c in store.insights.iter_mut() {
                if ids.contains(&c.id) {
                    c.credit += 1.0;
                    c.updated_at = now_secs();
                }
            }
        } else if status == "已回滚" {
            let now = now_secs();
            let card = InsightCard {
                id: gen_id(),
                kind: "anti_pattern".to_string(),
                title: format!("回滚教训：{}", snapshot.title),
                content: if snapshot.actual.is_empty() {
                    format!("进化「{}」观察期未达预期被回滚。预期：{}", snapshot.title, snapshot.expect)
                } else {
                    format!(
                        "进化「{}」观察期未达预期被回滚。预期：{}；实际：{}",
                        snapshot.title, snapshot.expect, snapshot.actual
                    )
                },
                scope: "全局".to_string(),
                tags: vec![snapshot.kind.clone(), "auto".to_string()],
                credit: 0.0,
                evidence: vec![snapshot.id.clone()],
                created_at: now,
                updated_at: now,
            };
            store.insights.push(card.clone());
            auto_card = Some(card);
        }
        (snapshot, auto_card)
    };
    persist();
    let _ = auto_card; // 卡已入库；返回值只回条目本身，前端拉 state 即见新卡
    Ok(decided)
}

// ───────────────────────── Commands: prompt 版本树 ─────────────────────────

/// 提交某专家某锚点的新版本：旧 active 版转 superseded，新版本号 = 最大版本 + 1。
/// 返回新版本记录。真正写回专家 overlay 由调用方拿 content 再走 expert_media_overlay_set。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn prompt_version_add(
    expert_id: String,
    platform: Option<String>,
    anchor: String,
    content: String,
    perf_note: Option<String>,
) -> Result<PromptVersion, String> {
    let platform = platform.unwrap_or_default();
    let created = {
        let mut store = STORE.write();
        let mut max_ver = 0u32;
        for v in store.prompt_versions.iter_mut() {
            if v.expert_id == expert_id && v.platform == platform && v.anchor == anchor {
                max_ver = max_ver.max(v.version);
                if v.status == "active" {
                    v.status = "superseded".to_string();
                }
            }
        }
        let ver = PromptVersion {
            id: gen_id(),
            expert_id,
            platform,
            anchor,
            version: max_ver + 1,
            content,
            status: "active".to_string(),
            perf_note: perf_note.unwrap_or_default(),
            created_at: now_secs(),
        };
        store.prompt_versions.push(ver.clone());
        ver
    };
    persist();
    Ok(created)
}

/// 一键回滚到指定历史版本：当前 active 转 rolled_back，目标版本重新 active。
/// 返回目标版本（调用方拿 content 写回专家文件）。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn prompt_version_rollback(id: String) -> Result<PromptVersion, String> {
    let target = {
        let mut store = STORE.write();
        let (expert_id, platform, anchor) = {
            let t = store
                .prompt_versions
                .iter()
                .find(|v| v.id == id)
                .ok_or_else(|| format!("prompt 版本不存在：{id}"))?;
            (t.expert_id.clone(), t.platform.clone(), t.anchor.clone())
        };
        for v in store.prompt_versions.iter_mut() {
            if v.expert_id == expert_id
                && v.platform == platform
                && v.anchor == anchor
                && v.status == "active"
            {
                v.status = "rolled_back".to_string();
            }
        }
        let t = store
            .prompt_versions
            .iter_mut()
            .find(|v| v.id == id)
            .expect("just found");
        t.status = "active".to_string();
        t.clone()
    };
    persist();
    Ok(target)
}

// ───────────────────────── Commands: 飞轮健康度 ─────────────────────────

/// 飞轮健康度汇总（本月窗口）。health = 本月创建且带度量证据的进化条目数。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn flywheel_summary() -> FlywheelSummary {
    let store = STORE.read();
    let now = now_secs();
    let from = month_start(now);

    let mut s = FlywheelSummary::default();
    for e in &store.timeline {
        if e.status == "观察中" {
            s.observing += 1;
        }
        if e.created_at >= from && !e.evidence.is_empty() {
            s.health += 1;
        }
        if let Some(d) = e.decided_at {
            if d >= from {
                match e.status.as_str() {
                    "已固化" => s.solidified += 1,
                    "已回滚" => s.rolled_back += 1,
                    _ => {}
                }
            }
        }
    }
    s.month_insights = store
        .insights
        .iter()
        .filter(|c| c.created_at >= from)
        .count() as u64;
    s
}
