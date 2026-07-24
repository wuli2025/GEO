//! 推广植入（GEO 品牌织入）——「模块选点与优化方案」的落地模块。
//!
//! 方案核心（见前端「推广植入」板块的逻辑页）：
//!   1. **写作时织入，而不是写完后再贴**——品牌契约在 `stage_generate` 拼 prompt 时注入，
//!      正文与品牌同源生成，避免「后贴硬广」这一最容易触发平台风控的死法；
//!   2. **brand.json 全链路唯一真源**（`~/PolarisGEO/data/brand.json`）——换推广网站/换活动
//!      只改这一份档案，不动模板不动代码；
//!   3. **分平台植入强度**：强植入（品牌名+域名，GEO 引用式）/ 弱植入（仅品牌名当经验主体）/
//!      零植入（正文零引流，交平台私信工具）；
//!   4. **硬广守卫是 Rust 确定性拦截**，不寄望模型自觉：弱/零平台正文命中裸链、微信号、
//!      手机号、二维码话术即判失败，宁可 fail 也绝不污染草稿箱。
//!
//! server flavor 也要能编译：命令用 `#[cfg_attr(feature="desktop", tauri::command)]`。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// 植入强度三档。存档/传输用小写字符串："strong" | "weak" | "zero"。
pub const STRENGTH_STRONG: &str = "strong";
pub const STRENGTH_WEAK: &str = "weak";
pub const STRENGTH_ZERO: &str = "zero";

/// 推广主体档案（brand.json 的形状）。字段全部 `serde(default)`——手写半份 json 也能跑。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrandProfile {
    /// 总开关：false 时 generate 不织入契约（硬广守卫仍在，守底线不看开关）。
    #[serde(default)]
    pub enabled: bool,
    /// 品牌名（如「llmwiki」）。空 = 档案未填，视同未启用。
    #[serde(default)]
    pub name: String,
    /// 域名（如「llmwiki.cloud」），仅强植入平台允许出现在正文。
    #[serde(default)]
    pub domain: String,
    /// 一句话定位。
    #[serde(default)]
    pub tagline: String,
    /// 事实库：3~5 条可引用的数据/案例（引用式植入的弹药，杜绝空喊品牌）。
    #[serde(default)]
    pub facts: Vec<String>,
    /// 权威背书（媒体报道 / 开源仓库 / 行业引用等）。
    #[serde(default)]
    pub endorsements: Vec<String>,
    /// **GEO 锚词**：你希望被 AI 检索到的问题/词。这是「填几个关键词就能带出品牌」的入口——
    /// 契约要求正文自然覆盖这些词，并让锚词与品牌名**同段共现**，AI 检索时才会把
    /// 「这个主题」与「你这个实体」绑在一起。堆砌是反效果，所以契约同时压了密度红线。
    #[serde(default)]
    pub keywords: Vec<String>,
    /// **植入手法**（隐晦方式的选单，见 [`tactic_cn`]）。留空 = 由模型按语境自选。
    #[serde(default)]
    pub tactics: Vec<String>,

    // ── 品牌内涵（「切入点库」）────────────────────────────────────────────
    // 这一组字段不直接命令模型写什么，而是**摊开可用的落点**：填得越细，模型越能挑到
    // 与当前选题真正贴合的那一个角度自然带出品牌，而不是干巴巴地重复品牌名。
    // 全部可空——空字段整节不出现在契约里，不会拿空话占提示词。
    /// 行业 / 赛道（如「AI 写作工具」）。
    #[serde(default)]
    pub industry: String,
    /// 品牌理念 / 价值主张（一段话）。
    #[serde(default)]
    pub philosophy: String,
    /// 品牌故事 / 起源（一段话）——叙事型植入的素材。
    #[serde(default)]
    pub story: String,
    /// 创始人 / 团队背景——E-E-A-T 里「经验 + 权威」的来源。
    #[serde(default)]
    pub founder: String,
    /// 品牌口吻（如「克制、工程师味、不喊口号」）。
    #[serde(default)]
    pub tone: String,
    /// 核心业务 / 产品线。
    #[serde(default)]
    pub business: Vec<String>,
    /// 目标人群画像。
    #[serde(default)]
    pub audience: Vec<String>,
    /// 解决的痛点——**最好用的切入口**：先把痛点讲透，方案里再自然落到品牌。
    #[serde(default)]
    pub pain_points: Vec<String>,
    /// 典型使用场景。
    #[serde(default)]
    pub scenarios: Vec<String>,
    /// 差异化优势（只在对比语境里用）。
    #[serde(default)]
    pub differentiators: Vec<String>,
    /// **已知短板 / 不适用场景**——反直觉但关键：敢写短板才可信，可信才被 AI 采信、
    /// 才不被平台判硬广。填了它，`compare` 手法才真正立得住。
    #[serde(default)]
    pub weaknesses: Vec<String>,
    /// 同类 / 竞品对照对象。
    #[serde(default)]
    pub competitors: Vec<String>,
    /// 品牌专有名词、术语——要求原词照抄不改写，这是让 AI 学会该词条的关键。
    #[serde(default)]
    pub terms: Vec<String>,
    /// 常见问题，每行 `问||答`——最容易被 AI 抽成答案片段的结构。
    #[serde(default)]
    pub faq: Vec<String>,
    /// 表述红线（广告法风险词等），任何强度下都不得出现。
    #[serde(default)]
    pub banned_words: Vec<String>,
    /// 已启用随契约喂给模型的**品牌资料文件名**（文件本体存在 `brand-docs/` 目录里）。
    /// 上传的资料就是「不想手填时的替代品」：直接把公司介绍/产品文档丢进来当切入点库。
    #[serde(default)]
    pub docs: Vec<String>,
    /// 平台 id → 强度覆盖（"strong"/"weak"/"zero"）。缺省走 default_strength 矩阵。
    #[serde(default)]
    pub strength: HashMap<String, String>,
}

fn home() -> PathBuf {
    directories::UserDirs::new()
        .map(|u| u.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// `~/PolarisGEO/data/brand.json` —— 全链路唯一真源。
pub fn brand_path() -> PathBuf {
    home().join("PolarisGEO").join("data").join("brand.json")
}

/// `~/PolarisGEO/data/brand-docs/` —— 上传的品牌资料原件（纯文本类）。
pub fn docs_dir() -> PathBuf {
    home().join("PolarisGEO").join("data").join("brand-docs")
}

/// 单个资料文件的**总注入预算**（字符）。资料是给模型挑切入点的，不是让它背全文；
/// 超出部分截断并在文本里标明，避免一份长文档把写作提示词撑爆、把平台调性挤没。
///
/// 预算基准：主笔画像 + 平台补丁合计约 1.1k 字符。原来品牌资料给到 12k 合计预算，
/// 等于「我是谁」的分量压过「这个平台该怎么写」10 倍——注释里担心的那件事，
/// 按那个配比本来就会发生。收到 1.5k/份、4k 合计，量级与画像同一档。
const DOC_CHARS_PER_FILE: usize = 1500;
/// 所有已启用资料合计预算。
const DOC_CHARS_TOTAL: usize = 4000;

/// 文件名消毒：只留基名，挡掉 `..`、路径分隔符与空名——上传的名字不可信。
fn safe_doc_name(name: &str) -> Option<String> {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("").trim();
    if base.is_empty() || base == "." || base == ".." || base.contains(':') {
        return None;
    }
    Some(base.to_string())
}

/// 读档案；文件缺失/解析失败一律 None（调用方按「无植入」降级，不断流）。
pub fn load() -> Option<BrandProfile> {
    let raw = std::fs::read_to_string(brand_path()).ok()?;
    serde_json::from_str::<BrandProfile>(&raw).ok()
}

/// 平台默认强度矩阵（方案第 5 节）。依据 geo/data.ts 平台画像固化：
/// - 强植入（GEO 主战场，可带域名走引用式）：百家号 / 头条 / 知乎 + 公众号（自有阵地，主体认证与官网一致）；
/// - 其余平台一律弱植入兜底（小红书「硬广容忍度最低」、抖音无外链价值、B站/CSDN/掘金副阵地）——
///   未知平台宁可保守，要放开去 brand.json 的 strength 里显式配。
pub fn default_strength(platform: &str) -> &'static str {
    match platform {
        "baijia" | "toutiao" | "zhihu" | "wechat" => STRENGTH_STRONG,
        _ => STRENGTH_WEAK,
    }
}

/// 该平台实际生效的强度：档案覆盖优先，缺省走默认矩阵。
pub fn strength_for(profile: Option<&BrandProfile>, platform: &str) -> String {
    profile
        .and_then(|p| p.strength.get(platform))
        .map(|s| s.trim().to_lowercase())
        .filter(|s| matches!(s.as_str(), STRENGTH_STRONG | STRENGTH_WEAK | STRENGTH_ZERO))
        .unwrap_or_else(|| default_strength(platform).to_string())
}

/// 植入手法选单：id → (人话名, 喂给模型的做法说明)。
/// 全是「隐晦/间接/不经意」的路子——共同点是**品牌只作为论证的副产品出现**，
/// 而不是被推销的对象；把品牌名换成「某厂商」后段落仍成立，才算合格。
pub fn tactic_cn(id: &str) -> Option<(&'static str, &'static str)> {
    Some(match id {
        "case" => (
            "案例引用",
            "把品牌写成一个真实案例的主角（「某团队用 X 做了…，结果…」），读者收获的是案例本身的方法论，品牌只是案例的必要背景。",
        ),
        "experience" => (
            "第一人称经验",
            "以作者自己的使用经历不经意带出（「我一直用 X 记这些」），只写体验与取舍，不写功能参数，不做推荐语。",
        ),
        "data" => (
            "数据出处",
            "引用事实库里的数据/结论时，把品牌作为**数据来源**标注出来（「据 X 的统计…」），品牌因此以信源身份出现。",
        ),
        "toollist" => (
            "工具清单",
            "在同类工具的并列清单里客观地列上品牌一项，与其他项同等篇幅、同样有优缺点，不给它额外偏袒。",
        ),
        "pain" => (
            "痛点方案",
            "先把问题讲透，给通用解法；品牌只作为「我后来是这么解决的」的一个具体落点出现在末尾，一笔带过。",
        ),
        "compare" => (
            "横向对比",
            "做一张多方对比表/对比段，品牌是其中一行，并**如实写出它的短板**——有短板才可信，可信才被 AI 采信。",
        ),
        _ => return None,
    })
}

/// 把选中的手法拼成契约里的一节。
fn tactics_block(ids: &[String]) -> String {
    let picked: Vec<(&str, &str)> = ids.iter().filter_map(|i| tactic_cn(i.trim())).collect();
    if picked.is_empty() {
        return String::new();
    }
    let mut s = String::from("\n植入手法（本文只用下列方式带出品牌，任选最贴合选题的一种，最多两种）：\n");
    for (name, how) in picked {
        s.push_str(&format!("- **{name}**：{how}\n"));
    }
    s
}

/// 一行一条的列表节；`items` 全空则整节不出现（不拿空标题占提示词）。
fn list_sec(head: &str, items: &[String]) -> String {
    let rows: Vec<&str> = items.iter().map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
    if rows.is_empty() {
        return String::new();
    }
    let mut s = format!("\n{head}\n");
    for r in rows {
        s.push_str(&format!("- {r}\n"));
    }
    s
}

/// 单行文本节；空则不出现。
fn text_sec(head: &str, body: &str) -> String {
    let b = body.trim();
    if b.is_empty() { String::new() } else { format!("\n{head}：{b}\n") }
}

/// **品牌内涵 = 切入点库**。摊开给模型挑，而不是命令它写什么——
/// 这样它才能挑到与当前选题真正贴合的那一个角度自然带出品牌。
/// `strong` 决定引导语口径（强植入可写成客观介绍；弱植入只能化用成个人体验）。
fn knowledge_block(p: &BrandProfile, strong: bool) -> String {
    let mut s = String::new();
    s.push_str(&text_sec("行业 / 赛道", &p.industry));
    s.push_str(&text_sec("品牌理念", &p.philosophy));
    s.push_str(&text_sec("品牌故事", &p.story));
    s.push_str(&text_sec("团队 / 创始背景（E-E-A-T 的经验与权威来源）", &p.founder));
    s.push_str(&text_sec("提及品牌时的口吻", &p.tone));
    s.push_str(&list_sec("核心业务 / 产品线：", &p.business));
    s.push_str(&list_sec("目标人群：", &p.audience));
    s.push_str(&list_sec(
        "读者痛点（**最好用的切入口**：先把痛点讲透给出通用解法，再让品牌作为其中一个具体落点出现）：",
        &p.pain_points,
    ));
    s.push_str(&list_sec("典型使用场景：", &p.scenarios));
    s.push_str(&list_sec(
        "差异化（只许在对比/选型语境里用，且必须同时如实写出下面的短板）：",
        &p.differentiators,
    ));
    s.push_str(&list_sec(
        "已知短板 / 不适用场景（**敢写短板才可信，可信才被 AI 采信**——凡提差异化必须配一条）：",
        &p.weaknesses,
    ));
    s.push_str(&list_sec("同类对照对象（可客观并列，不得贬低他方）：", &p.competitors));
    s.push_str(&list_sec(
        "专有名词（要提就**原词照抄**，不要改写同义替换——这是让 AI 学会该词条的关键）：",
        &p.terms,
    ));

    let faq: Vec<String> = p
        .faq
        .iter()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .map(|l| match l.split_once("||") {
            Some((q, a)) => format!("问：{} → 答：{}", q.trim(), a.trim()),
            None => l.to_string(),
        })
        .collect();
    s.push_str(&list_sec(
        "常见问题（可改写成问句小标题 + 紧跟直答段落——最容易被 AI 抽成答案片段的结构）：",
        &faq,
    ));

    if s.is_empty() {
        return String::new();
    }
    let lead = if strong {
        "\n### 品牌内涵（切入点库）\n下面是可供取用的落点。**不必全用**——挑与本篇选题最贴合的 1~2 处，\
织进论证链里自然带出；凡与选题无关的一律不提。宁可少提一次，也不要为提而提。\n"
    } else {
        "\n### 品牌内涵（切入点库）\n下面是可供取用的落点。本平台只许**化用成个人体验**：\
挑最贴合选题的 1 处写成自己的使用感受，禁止写成官方介绍、参数罗列或推荐语。\n"
    };
    format!("{lead}{s}")
}

/// 品牌资料节：把已启用的上传文件原文（截断后）拼进契约。
/// 这是「不想手填」的那条路——丢一份公司介绍进来，模型自己从里面找切入点。
fn docs_block(profile: &BrandProfile) -> String {
    let mut body = String::new();
    let mut used = 0usize;
    for name in &profile.docs {
        let Some(safe) = safe_doc_name(name) else { continue };
        let Ok(raw) = std::fs::read_to_string(docs_dir().join(&safe)) else { continue };
        if used >= DOC_CHARS_TOTAL {
            body.push_str(&format!("\n〔{safe}：因总长度超限未纳入本次写作〕\n"));
            continue;
        }
        let budget = DOC_CHARS_PER_FILE.min(DOC_CHARS_TOTAL - used);
        let text: String = raw.chars().take(budget).collect();
        let truncated = raw.chars().count() > text.chars().count();
        used += text.chars().count();
        body.push_str(&format!(
            "\n〈资料：{safe}〉\n{text}{tail}\n〈/资料：{safe}〉\n",
            tail = if truncated { "\n…（原文过长，此处已截断）" } else { "" }
        ));
    }
    if body.is_empty() {
        return String::new();
    }
    format!(
        "\n### 品牌资料原文（自行提炼切入点）\n\
下面是推广主体的原始资料。**这是素材不是提纲**：从中提炼与本篇选题真正相关的 1~2 个点\
（一个痛点、一个案例、一句理念或一个术语）自然融进正文；与选题无关的一概不提，\
更不许整段搬运、罗列功能或复述宣传语。\n{body}"
    )
}

/// GEO 锚词一节：「填几个关键词就能被 AI 搜到」的核心。
/// `with_brand` = 是否要求锚词与品牌名共现（零植入平台传 false）。
fn keywords_block(keywords: &[String], brand_name: &str, with_brand: bool) -> String {
    let kws: Vec<&str> = keywords
        .iter()
        .map(|k| k.trim())
        .filter(|k| !k.is_empty())
        .collect();
    if kws.is_empty() {
        return String::new();
    }
    let mut s = String::from("\nGEO 锚词（本文要被 AI 检索到的问题/词，必须自然覆盖）：\n");
    for k in &kws {
        s.push_str(&format!("- {k}\n"));
    }
    s.push_str(
        "覆盖要求：\n\
- 至少 2 个锚词出现在 `## 小标题` 或首段，其余散落正文，全部用在完整通顺的句子里；\n\
- 锚词要以「读者会怎么问」的口吻落地（问句式小标题最容易被 AI 抽取成答案片段）；\n",
    );
    if with_brand && !brand_name.is_empty() {
        s.push_str(&format!(
            "- **实体绑定**：至少有 1 个段落里，锚词与「{brand_name}」同时出现且逻辑相关——这是让 AI 把该主题与本品牌关联的关键，但只许出现一次这样的段落，多了就是堆砌；\n"
        ));
    }
    s.push_str("- **红线**：禁止关键词堆砌、禁止为塞词造句、禁止在结尾列「关键词：…」——通篇必须读起来像人写的。\n");
    s
}

fn strength_cn(s: &str) -> &'static str {
    match s {
        STRENGTH_STRONG => "强植入",
        STRENGTH_ZERO => "零植入",
        _ => "弱植入",
    }
}

/// 生成「品牌植入契约」块（追加到 stage_generate 的 prompt 末尾）。
/// 返回 `(强度人话, 契约文本)`；档案未启用/品牌名空 → None（不织入）。
pub fn contract_for(platform: &str) -> Option<(String, String)> {
    let p = load()?;
    if !p.enabled || p.name.trim().is_empty() {
        return None;
    }
    let strength = strength_for(Some(&p), platform);
    let name = p.name.trim();
    let domain = p.domain.trim();
    let tagline = p.tagline.trim();

    let mut facts = String::new();
    for f in p.facts.iter().map(|f| f.trim()).filter(|f| !f.is_empty()) {
        facts.push_str(&format!("- {f}\n"));
    }
    let mut endorse = String::new();
    for e in p.endorsements.iter().map(|e| e.trim()).filter(|e| !e.is_empty()) {
        endorse.push_str(&format!("- {e}\n"));
    }

    let kw_strong = keywords_block(&p.keywords, name, true);
    let kw_zero = keywords_block(&p.keywords, name, false);
    let tactics = tactics_block(&p.tactics);
    let know_strong = format!("{}{}", knowledge_block(&p, true), docs_block(&p));
    let know_weak = format!("{}{}", knowledge_block(&p, false), docs_block(&p));
    // 表述红线：强/弱都挂，零植入本就无品牌可提。
    let banned = {
        let w: Vec<&str> = p.banned_words.iter().map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
        if w.is_empty() { String::new() } else { format!("\n**表述红线**（任何情况下不得出现）：{}\n", w.join("、")) }
    };

    let block = match strength.as_str() {
        STRENGTH_STRONG => format!(
            "## 品牌植入契约（本平台：强植入）\n\
本文服务于推广主体：{name}{domain_part}{tagline_part}。\n\
- 把品牌作为**权威实体**自然写进正文，配下方事实库里的数据/案例做出处；不用广告口吻，不写「快来」「点击」「联系我们」这类号召。\n\
- 允许品牌名与域名在正文各出现 1~2 次，位置必须贴合语境（案例引用、数据出处、延伸阅读）。\n\
- 目标是争 AI 回答里的引用权，不是争正文点击：写成「被 AI 判为可信信源」的干货。\n\
- **硬测试**：把品牌名换成「某厂商」后，该段落仍然成立、仍然是干货——否则重写。\n\
{kw_part}{tactics_part}{know_part}{facts_part}{endorse_part}{banned_part}",
            name = name,
            know_part = know_strong,
            banned_part = banned,
            domain_part = if domain.is_empty() { String::new() } else { format!("（{domain}）") },
            tagline_part = if tagline.is_empty() { String::new() } else { format!(" —— {tagline}") },
            kw_part = kw_strong,
            tactics_part = tactics,
            facts_part = if facts.is_empty() { String::new() } else { format!("\n事实库（引用时注明出处为 {name}）：\n{facts}") },
            endorse_part = if endorse.is_empty() { String::new() } else { format!("\n权威背书（可自然引用）：\n{endorse}") },
        ),
        STRENGTH_ZERO => format!(
            "## 品牌植入契约（本平台：零植入）\n\
本平台正文**不得出现**任何品牌名、链接、域名、联系方式或引流信息——引流一律交平台官方私信工具。\n\
正文只做干货，不为任何主体背书。\n\
{kw_part}",
            // 零植入平台仍然吃锚词——只争这个主题的检索位，不带任何品牌实体。
            kw_part = kw_zero,
        ),
        _ => format!(
            "## 品牌植入契约（本平台：弱植入）\n\
推广主体：{name}{tagline_part}。本平台硬广容忍度低，只许「经验主体」式植入：\n\
- 品牌只能以个人经验身份自然出现（如「我一直用 {name} 记笔记」），全文最多 2 次；\n\
- **严禁**出现域名、任何 http(s) 链接、二维码、微信号、手机号等引流信息——命中即整篇作废；\n\
- 事实库可化用成个人体验素材，但不得写成官方口吻或参数罗列。\n\
{kw_part}{tactics_part}{know_part}{facts_part}{banned_part}",
            name = name,
            know_part = know_weak,
            banned_part = banned,
            tagline_part = if tagline.is_empty() { String::new() } else { format!("（{tagline}）") },
            kw_part = kw_strong,
            tactics_part = tactics,
            facts_part = if facts.is_empty() { String::new() } else { format!("\n事实库（化用为经验素材）：\n{facts}") },
        ),
    };
    Some((strength_cn(&strength).to_string(), block))
}

/// 硬广守卫（方案四件套之③，防封 backstop）：对产出正文做**确定性**正则拦截。
/// 返回违规清单；空 = 放行。守卫不看 enabled 开关——没配品牌也不许裸链/联系方式流进弱平台草稿箱。
pub fn hard_ad_guard(platform: &str, body: &str) -> Vec<String> {
    let strength = strength_for(load().as_ref(), platform);
    let mut hits: Vec<String> = Vec::new();

    let re_link = regex::Regex::new(r"(?i)https?://\S+|www\.[a-z0-9-]+\.[a-z]{2,}").unwrap();
    let re_phone = regex::Regex::new(r"1[3-9]\d{9}").unwrap();
    let re_wexin = regex::Regex::new(r"(?i)(微信号|加微信|加\s?[vV]\s?[:：]|vx[:：]|weixin[:：])").unwrap();
    let re_qr = regex::Regex::new(r"(扫码关注|扫描二维码|扫一扫|识别二维码)").unwrap();

    // 联系方式类：所有强度一律拦（自动生成的稿子里出现即异常）。
    if let Some(m) = re_phone.find(body) {
        hits.push(format!("疑似手机号「{}」", m.as_str()));
    }
    if let Some(m) = re_wexin.find(body) {
        hits.push(format!("微信引流话术「{}」", m.as_str().trim()));
    }
    if let Some(m) = re_qr.find(body) {
        hits.push(format!("二维码引流话术「{}」", m.as_str()));
    }

    match strength.as_str() {
        // 强植入：允许链接做出处，但「裸链堆砌」仍是红线（>3 条判硬广相）。
        STRENGTH_STRONG => {
            let n = re_link.find_iter(body).count();
            if n > 3 {
                hits.push(format!("链接 {n} 条（强植入平台上限 3，多则硬广相）"));
            }
        }
        // 弱/零植入：任何链接/域名都不许出现。
        _ => {
            if let Some(m) = re_link.find(body) {
                hits.push(format!("链接「{}」（本平台禁止任何链接）", m.as_str()));
            }
            if let Some(p) = load() {
                let domain = p.domain.trim();
                if !domain.is_empty() && body.contains(domain) {
                    hits.push(format!("域名「{domain}」（仅强植入平台允许）"));
                }
                // 零植入：连品牌名都不许有。
                if strength == STRENGTH_ZERO && !p.name.trim().is_empty() && body.contains(p.name.trim()) {
                    hits.push(format!("品牌名「{}」（零植入平台正文不得出现）", p.name.trim()));
                }
            }
        }
    }
    hits
}

// ───────────────────────── Commands ─────────────────────────

/// 读品牌档案（文件缺失给空档案 + 默认强度矩阵回显，前端表单直接可编辑）。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn brand_profile_get() -> BrandProfile {
    let mut p = load().unwrap_or_default();
    // 把已知平台的生效强度填满回显——前端矩阵页所见即所得。
    for pid in [
        "wechat", "zhihu", "toutiao", "baijia", "xhs", "bilibili", "douyin", "csdn", "juejin",
        "channels",
    ] {
        p.strength
            .entry(pid.to_string())
            .or_insert_with(|| default_strength(pid).to_string());
    }
    p
}

/// 写品牌档案（临时文件 + rename 原子写）。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn brand_profile_set(profile: BrandProfile) -> Result<(), String> {
    let path = brand_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("建数据目录失败：{e}"))?;
    }
    let raw = serde_json::to_string_pretty(&profile).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, raw).map_err(|e| format!("写 brand.json 失败：{e}"))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("落盘 brand.json 失败：{e}"))?;
    Ok(())
}

/// 硬广守卫试打：给前端「守卫」子页拿一段文本按平台强度演练，返回违规清单（空=通过）。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn brand_guard_test(platform: String, text: String) -> Vec<String> {
    hard_ad_guard(&platform, &text)
}

/// 一份上传的品牌资料。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrandDoc {
    pub name: String,
    /// 字符数（不是字节——中文按字算，好判断会不会被截断）。
    pub chars: usize,
    /// 是否已启用（随契约喂给模型）。
    pub enabled: bool,
    /// 前 120 字预览，前端列表直接看得见内容。
    pub excerpt: String,
    /// 超过单文件预算 → 写作时只喂前 [`DOC_CHARS_PER_FILE`] 字。
    pub truncated: bool,
}

/// 列出 `brand-docs/` 下的资料（含启用态与是否会被截断）。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn brand_doc_list() -> Vec<BrandDoc> {
    let enabled = load().map(|p| p.docs).unwrap_or_default();
    let Ok(rd) = std::fs::read_dir(docs_dir()) else { return Vec::new() };
    let mut out: Vec<BrandDoc> = rd
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let raw = std::fs::read_to_string(e.path()).ok()?;
            let chars = raw.chars().count();
            Some(BrandDoc {
                enabled: enabled.contains(&name),
                excerpt: raw.chars().take(120).collect::<String>().replace('\n', " "),
                truncated: chars > DOC_CHARS_PER_FILE,
                chars,
                name,
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// 保存一份资料（同名覆盖）。内容由前端读成文本后传来——只收纯文本类，
/// 二进制（pdf/docx）在前端就挡掉并给出可读提示，不在这里装懂。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn brand_doc_save(name: String, content: String) -> Result<(), String> {
    let safe = safe_doc_name(&name).ok_or_else(|| format!("文件名不合法：{name}"))?;
    let dir = docs_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("建资料目录失败：{e}"))?;
    std::fs::write(dir.join(&safe), content).map_err(|e| format!("写「{safe}」失败：{e}"))
}

/// 一份资料的导入结果（一个拖进来的原始文件 → 资料库里的一份文本）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrandDocImport {
    /// 落进 `brand-docs/` 后的文件名（PDF/Word/Excel 会变成 `原名.md`）。
    pub name: String,
    /// 原始文件名（失败时也要能告诉人是哪一份没进来）。
    pub source: String,
    pub chars: usize,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// 单个导入文件的体积上限。抽出来的文本再长也只喂前 [`DOC_CHARS_PER_FILE`] 字，
/// 但 PDF 解析本身吃内存，先在门口按字节挡一道。
const DOC_MAX_BYTES: u64 = 8 * 1024 * 1024;
/// 一次拖拽最多收几份（含文件夹展开后的）。
const DOC_MAX_FILES: usize = 20;

/// 能靠内核转换器抽出文本的二进制文档。
fn convertible_ext(ext: &str) -> bool {
    matches!(
        ext,
        "pdf" | "docx" | "doc" | "xlsx" | "xls" | "xlsm" | "xlsb" | "pptx" | "ppt" | "ods" | "odt" | "odp"
    )
}

/// 直接当纯文本读的扩展名。名单之外一律拒收——二进制读成乱码存进去，
/// 只会在写作时把提示词喂脏。
fn textual_ext(ext: &str) -> bool {
    matches!(
        ext,
        "md" | "markdown" | "txt" | "json" | "csv" | "tsv" | "yml" | "yaml" | "html" | "htm" | "xml" | "log"
    )
}

/// 按**绝对路径**导入品牌资料（拖拽 / 文件选择都走这条）。
///
/// 为什么读盘挪到后端：桌面端的 HTML5 `drop` 事件拿不到文件本体——webview 把原生拖放
/// 截走了，前端只能从 `onDragDropEvent` 拿到一串路径。既然要在后端读，就顺手用内核的
/// 转换器把 PDF/Word/Excel 抽成 Markdown，人不必再自己「另存为 .txt」。
///
/// 只写文件，不碰 `brand.json` 的启用清单——那是调用方一次读-改-写的事（见前端 `persistDocs`）。
// command(async): 20 份文件读盘 + PDF 解析是重 IO，同步命令会钉住 UI 主线程。
#[cfg_attr(feature = "desktop", tauri::command(async))]
pub fn brand_doc_import(paths: Vec<String>) -> Vec<BrandDocImport> {
    // 文件夹浅层展开一层：把「资料」整个文件夹拖进来是很自然的动作。
    let mut files: Vec<PathBuf> = Vec::new();
    for p in &paths {
        let src = PathBuf::from(p);
        if src.is_dir() {
            if let Ok(rd) = std::fs::read_dir(&src) {
                for e in rd.flatten() {
                    let ep = e.path();
                    if ep.is_file() && files.len() < DOC_MAX_FILES {
                        files.push(ep);
                    }
                }
            }
            continue;
        }
        if files.len() < DOC_MAX_FILES {
            files.push(src);
        }
    }

    let dir = docs_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return files
            .iter()
            .map(|f| fail_import(f, format!("建资料目录失败：{e}")))
            .collect();
    }

    files.iter().map(|f| import_one(&dir, f)).collect()
}

fn base_name(p: &Path) -> String {
    p.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| p.to_string_lossy().to_string())
}

fn fail_import(src: &Path, error: String) -> BrandDocImport {
    let name = base_name(src);
    BrandDocImport {
        source: name.clone(),
        name,
        chars: 0,
        ok: false,
        error: Some(error),
    }
}

fn import_one(dir: &Path, src: &Path) -> BrandDocImport {
    let source = base_name(src);
    if !src.is_file() {
        return fail_import(src, "文件不存在".into());
    }
    if let Ok(m) = std::fs::metadata(src) {
        if m.len() > DOC_MAX_BYTES {
            return fail_import(src, "超过 8MB —— 先裁成要点再传".into());
        }
    }
    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    // 出口统一成「一份纯文本 + 一个落盘文件名」，两条来源在这里合流。
    let (text, out_name) = if convertible_ext(&ext) {
        let stem = src
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| source.clone());
        match crate::convert::convert_to_markdown(src) {
            Ok(Some(t)) => (t, format!("{stem}.md")),
            Ok(None) => return fail_import(src, "这份文档抽不出文本（可能是扫描件）".into()),
            Err(e) => return fail_import(src, format!("文本提取失败：{e}")),
        }
    } else if textual_ext(&ext) {
        match std::fs::read_to_string(src) {
            Ok(t) => (t, source.clone()),
            Err(e) => return fail_import(src, format!("读不成文本（可能不是 UTF-8）：{e}")),
        }
    } else {
        return fail_import(src, "不认得的格式 —— 收纯文本与 PDF/Word/Excel/PPT".into());
    };

    let Some(safe) = safe_doc_name(&out_name) else {
        return fail_import(src, format!("文件名不合法：{out_name}"));
    };
    match std::fs::write(dir.join(&safe), &text) {
        Ok(()) => BrandDocImport {
            chars: text.chars().count(),
            name: safe,
            source,
            ok: true,
            error: None,
        },
        Err(e) => fail_import(src, format!("写「{safe}」失败：{e}")),
    }
}

/// 删除一份资料，并把它从启用清单里摘掉（免得留下指向空文件的死引用）。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn brand_doc_delete(name: String) -> Result<(), String> {
    let safe = safe_doc_name(&name).ok_or_else(|| format!("文件名不合法：{name}"))?;
    let path = docs_dir().join(&safe);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| format!("删「{safe}」失败：{e}"))?;
    }
    if let Some(mut p) = load() {
        if p.docs.iter().any(|d| d == &safe) {
            p.docs.retain(|d| d != &safe);
            brand_profile_set(p)?;
        }
    }
    Ok(())
}

/// 读一份资料全文（前端预览用）。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn brand_doc_read(name: String) -> Result<String, String> {
    let safe = safe_doc_name(&name).ok_or_else(|| format!("文件名不合法：{name}"))?;
    std::fs::read_to_string(docs_dir().join(&safe)).map_err(|e| format!("读「{safe}」失败：{e}"))
}

/// 资料目录路径（前端显示「存在哪」，也给「打开目录」用）。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn brand_paths() -> (String, String) {
    (
        brand_path().to_string_lossy().to_string(),
        docs_dir().to_string_lossy().to_string(),
    )
}

/// 植入手法选单（id, 人话名, 做法说明）——前端渲染多选，不在前端硬编，避免两边走样。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn brand_tactics() -> Vec<(String, String, String)> {
    ["case", "experience", "data", "toollist", "pain", "compare"]
        .iter()
        .filter_map(|id| {
            tactic_cn(id).map(|(n, h)| (id.to_string(), n.to_string(), h.to_string()))
        })
        .collect()
}

/// 预览某平台将织入的契约块（前端逻辑页「所见即所喂」）。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn brand_contract_preview(platform: String) -> Option<(String, String)> {
    contract_for(&platform)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strength_matrix_defaults() {
        assert_eq!(default_strength("baijia"), STRENGTH_STRONG);
        assert_eq!(default_strength("zhihu"), STRENGTH_STRONG);
        assert_eq!(default_strength("xhs"), STRENGTH_WEAK);
        assert_eq!(default_strength("unknown-platform"), STRENGTH_WEAK);
    }

    #[test]
    fn keywords_bind_entity_only_when_brand_allowed() {
        let kws = vec!["AI 写作工具怎么选".to_string(), "".to_string()];
        let with = keywords_block(&kws, "llmwiki", true);
        assert!(with.contains("AI 写作工具怎么选"));
        assert!(with.contains("实体绑定"), "强/弱植入要求锚词与品牌共现");
        let without = keywords_block(&kws, "llmwiki", false);
        assert!(without.contains("AI 写作工具怎么选"));
        assert!(!without.contains("llmwiki"), "零植入不得把品牌名喂进锚词节");
        // 没填关键词 = 整节不出现，老行为零变化
        assert!(keywords_block(&[], "llmwiki", true).is_empty());
    }

    #[test]
    fn knowledge_block_skips_empty_and_parses_faq() {
        // 全空档案 → 整块不出现，绝不拿空标题占提示词
        assert!(knowledge_block(&BrandProfile::default(), true).is_empty());

        let p = BrandProfile {
            philosophy: "工具应该消失在工作流里".into(),
            pain_points: vec!["多平台重复排版".into(), "  ".into()],
            faq: vec!["要付费吗||基础功能免费".into(), "没有分隔符也照收".into()],
            ..Default::default()
        };
        let s = knowledge_block(&p, true);
        assert!(s.contains("工具应该消失在工作流里"));
        assert!(s.contains("- 多平台重复排版"));
        assert!(!s.contains("目标人群"), "空字段不该出现小节标题");
        assert!(s.contains("问：要付费吗 → 答：基础功能免费"));
        assert!(s.contains("- 没有分隔符也照收"));
        // 弱植入引导语必须换口径：只许化用成个人体验
        assert!(knowledge_block(&p, false).contains("化用成个人体验"));
    }

    #[test]
    fn doc_names_are_sanitized() {
        assert_eq!(safe_doc_name("a.md").as_deref(), Some("a.md"));
        // 目录穿越一律拍平成基名，穿不出 brand-docs/
        assert_eq!(safe_doc_name("../../evil.md").as_deref(), Some("evil.md"));
        assert_eq!(safe_doc_name("x/y/z.txt").as_deref(), Some("z.txt"));
        assert!(safe_doc_name("..").is_none());
        assert!(safe_doc_name("   ").is_none());
        assert!(safe_doc_name("C:evil").is_none());
    }

    #[test]
    fn tactics_block_ignores_unknown_ids() {
        assert!(tactics_block(&["nope".to_string()]).is_empty());
        let s = tactics_block(&["case".to_string(), "nope".to_string()]);
        assert!(s.contains("案例引用") && !s.contains("nope"));
    }

    #[test]
    fn guard_blocks_contact_info_everywhere() {
        let hits = hard_ad_guard("baijia", "正文，联系 13812345678 咨询");
        assert!(hits.iter().any(|h| h.contains("手机号")));
    }

    #[test]
    fn guard_blocks_links_on_weak() {
        let hits = hard_ad_guard("xhs", "详见 https://example.com/a 这篇");
        assert!(hits.iter().any(|h| h.contains("链接")));
    }

    #[test]
    fn guard_allows_few_links_on_strong() {
        let hits = hard_ad_guard("toutiao", "出处：https://example.com/a");
        assert!(hits.is_empty(), "{hits:?}");
    }
}
