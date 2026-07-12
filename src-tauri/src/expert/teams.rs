//! 业务专家团 — 「配几个做对应业务的专家团队」。
//!
//! 每个团 = 一位领衔战略/统帅 + 4 位对应业务专家。智能匹配优先在团内召人，
//! 让一句话需求能稳定命中一支成建制的队伍，而不是零散个人。
//!
//! 团本体仍是一段编排型 CLAUDE.md（运行时由成员卡片组装），复用既有
//! persona_apply → 写项目 CLAUDE.md 链路。团里永不写死「先谁后谁」——
//! 顺序由战略师运行时按任务决定（Kimi Agent Swarm 哲学）。

use crate::expert::ExpertCard;
use serde::Serialize;

/// 一支业务专家团（对外给前端市场卡片用）。
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ExpertTeam {
    pub id: String,
    pub name: String,
    pub icon: String,
    /// 一句话定位
    pub tagline: String,
    /// 详细说明
    pub description: String,
    /// 领衔者（战略/统帅）专家 id
    pub lead_id: String,
    /// 成员专家 id（不含 lead）
    pub member_ids: Vec<String>,
    /// 业务标签，喂智能匹配 + 卡片展示
    pub tags: Vec<String>,
}

fn t(
    id: &str,
    name: &str,
    icon: &str,
    tagline: &str,
    description: &str,
    lead_id: &str,
    member_ids: &[&str],
    tags: &[&str],
) -> ExpertTeam {
    ExpertTeam {
        id: id.into(),
        name: name.into(),
        icon: icon.into(),
        tagline: tagline.into(),
        description: description.into(),
        lead_id: lead_id.into(),
        member_ids: member_ids.iter().map(|s| s.to_string()).collect(),
        tags: tags.iter().map(|s| s.to_string()).collect(),
    }
}

/// 唯一在役业务团：自媒体统一专家团。
///
/// 2026-07 自媒体改造：历史 8 支业务团整建制退役（源定义见下方注释保留），
/// 只保留这一支覆盖「选题→调研→写作→去AI→核查→评审→排版→配图→投递→复盘」全链路的团。
/// 领衔 = 选题战略官（media-strategist），成员 = 其余 13 位自媒体专家。
///
/// 向后兼容：`expert_team_get` / `expert_recommend_from_kb` / `team_apply` 找不到旧团 id 时，
/// 天然回落到「无匹配」路径；`expert_recommend_from_kb` 在唯一团上打分，弱信号也会推荐本团，
/// 不会 panic。
pub fn all_teams() -> Vec<ExpertTeam> {
    vec![t(
        "team-media",
        "自媒体统一专家团",
        "📣",
        "选题→写作→去AI→核查→排版→配图→投递→复盘 全链路",
        "选题战略官领衔的自媒体成建制团队：从热点选题、素材调研、正文写作、去 AI 腔、事实核查、七维评审，到排版、配图、多平台草稿投递与数据复盘，一条龙但按需组阵。平台文风由「平台补丁」运行时叠加，同一支团适配公众号/小红书/知乎/头条/百家号/B站/抖音 7 个平台。",
        "media-strategist",
        &[
            "media-researcher",
            "media-writer",
            "media-typesetter",
            "media-publisher",
            "media-reviewer",
            "media-critic",
            "media-factchecker",
            "media-deaiflavor",
            "media-imagedirector",
            "media-analytics",
            "media-accountkeeper",
            "media-competitor",
            "media-georestructurer",
        ],
        &[
            "自媒体", "选题", "爆款", "写作", "排版", "投递", "公众号", "小红书", "知乎",
            "运营", "涨粉", "内容",
        ],
    )]
}

/// 用业务团 + 成员卡片组装一段「战略师领衔·按需召集」的编排型 CLAUDE.md。
/// 团里永不写死执行顺序：列「谁能干什么活、何时召」，顺序运行时算。
pub fn build_team_doc(team: &ExpertTeam, lead: &ExpertCard, members: &[ExpertCard]) -> String {
    let mut s = String::new();
    s.push_str(&format!("# {} {}\n\n", team.icon, team.name));
    s.push_str(&format!("> {}\n\n", team.tagline));
    s.push_str(&format!("{}\n\n", team.description));

    s.push_str("## 你是这支团队的编排者（战略师领衔）\n\n");
    s.push_str(&format!(
        "由 **{}** 领衔。读懂用户目标后，**按情况临时组阵**——\
         不是每次都把全队拉上场。默认先用单 agent；当任务确实需要分工、\
         且并行有收益时，才召集对应专家。成本纪律：一次最多 4~5 人，\
         独立子任务才并行，紧耦合任务退回串行（防 fake parallelism）。\n\n",
        lead.name
    ));

    s.push_str("## 候选专家（能力候选池，不是执行顺序）\n\n");
    s.push_str(&format!(
        "- 🧭 **{}**（领衔）— {}。何时召：{}\n",
        lead.name, lead.role, lead.description
    ));
    for m in members {
        s.push_str(&format!(
            "- **{}** — {}。何时召：{}\n",
            m.name, m.role, m.description
        ));
    }
    s.push('\n');

    s.push_str("## 工作方式\n\n");
    s.push_str("1. **先拆子任务**：把目标拆成若干「子任务」，每个子任务才去召对应专家；简单任务不拆，直接干。\n");
    s.push_str(
        "2. **召集即解释**：召一个专家时，简述「为什么是 TA」（命中的需求点 + 补的能力维度）。\n",
    );
    s.push_str(
        "3. **默认并行、紧耦合克制**：独立子任务可并行推进；有先后依赖的串行做，别假并行。\n",
    );
    s.push_str("4. **单一收口**：多分支产出由你（领衔者）合并成一份交付，不堆砌半成品。\n\n");

    // 统一质量标尺 —— 只留可判定门槛, 不留清单回声; 领衔者收口即按此打回。
    s.push_str("## 交付门槛（团队铁律 · 任一不满足即未完成，领衔者打回重做）\n\n");
    s.push_str("- **视觉**：统一设计语言（≤3 主色、统一字阶、8 的倍数间距）；严禁 emoji 当图标、占位符残留、AI 套版感；视觉体系不自创——从前端大师库（`skills/polaris-deck-studio/designers/INDEX.md` 的 11 位设计师花名册）选一位并守其色板与禁忌，用户指定风格则按用户的。\n");
    s.push_str("- **内容**：文案有钩子、信息准确、无「待补充」残留。\n");
    s.push_str("- **工程**：成品自包含可直接打开、响应式不破版、无碎图与控制台报错。\n");
    s.push_str("- **收口**：多分支产出由领衔者合并为一份打磨过的成品，任一门槛不过即退回对应专家重做。\n\n");
    s.push_str("## 冲突消解\n\n");
    s.push_str("用户的明确要求 > 团队铁律 > 通用风格。用户要求与铁律冲突时，一句话点明代价，然后按用户的来；信息不足且猜错代价高时，先问一个最小澄清问题再动手。\n\n");

    s.push_str("---\n_本团由北极星「业务专家团」自动组装；成员可在对话中追加 / 换人。_\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn teams_reference_real_experts() {
        let experts = crate::expert::all_experts_for_test();
        let ids: std::collections::HashSet<_> = experts.iter().map(|e| e.id.as_str()).collect();
        for team in all_teams() {
            assert!(
                ids.contains(team.lead_id.as_str()),
                "{} 的 lead {} 不存在",
                team.id,
                team.lead_id
            );
            for m in &team.member_ids {
                assert!(ids.contains(m.as_str()), "{} 的成员 {} 不存在", team.id, m);
            }
        }
    }

    #[test]
    fn team_ids_unique() {
        let teams = all_teams();
        let mut ids: Vec<&str> = teams.iter().map(|t| t.id.as_str()).collect();
        let before = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(before, ids.len(), "团 id 不应重复");
    }
}
