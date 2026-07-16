/**
 * 大脑·进化视图的数据源（M10 循环工程）。
 *
 * 已接真实后端（evolution.rs：evolution_state / flywheel_summary）。
 * 账本为空（全新库）时回落设计稿示例数据并标记 source="mock"——真实账本绝不混入示例；
 * 一旦用户写入第一张卡/第一条进化，视图即切换 source="live" 全量真数据。
 * 视图端契约保持不变：EvolutionData 元组结构与 v2 设计稿一致。
 */
import { ref, type Ref } from "vue";
import { evolutionApi, type InsightCard, type EvolutionEntry, type PromptVersion } from "../tauri";

export interface EvolutionData {
  /** 进化时间线：[cssClass, when, what, detail, status] */
  timeline: string[][];
  /** insight 卡库：[type, title, content, scope, merit, date] */
  cards: string[][];
  /** prompt 版本树：[expert, version, statusPerf, changeOutcome] */
  tree: string[][];
  /** 飞轮健康度指标 */
  flywheel: {
    health: number;
    cardsThisMonth: number;
    solidified: number;
    rolledBack: number;
    evidence: string[];
  };
}

const MOCK_EVOLUTION: EvolutionData = {
  timeline: [
    ["evo-sched", "07-16 05:30", "调度进化 · L1 自动生效", "知乎被引率连涨 3 周 → 周篇数 3→4；观察期至 07-23", "观察中"],
    ["evo-prompt", "07-12 05:30", "Prompt 进化 · L1 自动生效", "公众号 writer evolvable[opening_formula] v2→v3（注入 rule▸标题公式v3）→ 过审率 +18%", "已固化"],
    ["evo-expert", "07-11 14:20", "专家团进化 · L1 自动生效", "头条 writer 连续 5 篇一次过审率 38% → 回滚 prompt 至 v3", "已固化"],
    ["evo-sched", "07-11 02:00", "调度进化 · 事件触发 L1", "头条同题 3 篇触发多样性锁 → 当日该平台配额降至 1，沉淀 anti_pattern 卡", "已固化"],
    ["evo-skill", "07-09 05:30", "Skill 进化 · L2 已审批", "发现头条团缺「标题 AB 测试」能力 → 从白名单货架安装 skill，7 天观察期", "观察中"],
    ["evo-prompt", "07-08 05:30", "Prompt 进化 · 自动回滚", "小红书 writer v4 观察期 CTR 低于对照 11% → 自动回滚 v3，生成 anti_pattern 卡", "已回滚"],
  ],
  cards: [
    ["anti_pattern", "单日发布配额硬锁未生效", "cron bug 致一天发 14 篇（事故一）。配额检查在 dispatch 后才跑，多 Agent 并发绕过。→ 改为 dispatch 前原子扣减", "头条·全局", "★★★★★", "07-11"],
    ["anti_pattern", "模板兜底文数据全面垫底", "无素材时用模板兜底，产出的稿全平台阅读垫底且拉低账号权重（事故二）。→ 整个删除，宁可少一篇", "全平台", "★★★★★", "07-09"],
    ["anti_pattern", "FAQ 规则设 warning 模型持续偷懒", "软约束等于没约束（事故三）。→ 升 error 才老实", "全平台", "★★★★☆", "07-08"],
    ["rule", "公众号标题公式 v3", "「数字 + 场景 + 结论」式标题过审率 +18%；禁标题党（拉低被引意愿）", "公众号", "★★★★☆", "07-12"],
    ["rule", "知乎「某厂商」替换测试", "提交前把品牌名替换成「某厂商」，仍有干货才允许提交", "知乎", "★★★★★", "07-05"],
    ["playbook", "头条×抖音双号联动", "同选题：抖音图文讲「是什么」、头条长文讲「怎么做」，互相引流；字节系内部关联信号有信源加成", "头条·抖音", "★★★☆☆", "07-14"],
    ["playbook", "可引用短句公式", "40–110 字、含数字、非疑问句——被 AI 摘引概率最高的句式", "全平台", "★★★★☆", "07-06"],
  ],
  tree: [
    ["writer（公众号补丁）", "v5", "当前 · 过审率 78% · 07-12 起", "v4 → v5 diff：opening_formula 段注入 rule▸标题公式v3"],
    ["writer（公众号补丁）", "v4", "已归档 · 过审率 66% · 06-28–07-12", "—"],
    ["writer（头条补丁）", "v3", "当前（v4 已回滚）· 过审率 71%", "v4 观察期一次过审率 38% → 07-11 自动回滚"],
    ["writer（小红书补丁）", "v3", "当前（v4 已回滚）· CTR 基线", "v4 观察期 CTR 低于对照 11% → 07-08 自动回滚"],
  ],
  flywheel: {
    health: 6,
    cardsThisMonth: 7,
    solidified: 3,
    rolledBack: 1,
    evidence: [
      "探测「知乎被引率连涨 3 周」→ 知乎周篇数 3→4（调度进化）",
      "rule▸公众号标题公式 v3 → writer opening_formula v2→v3 → 过审率 +18%（Prompt 进化）",
      "头条 writer 过审率 38% → 回滚 prompt v3（专家团进化）",
      "头条同题 3 篇 → 多样性锁 + anti_pattern 卡（调度进化）",
      "头条团能力缺口 → 安装标题 AB 测试 skill（Skill 进化）",
      "小红书 v4 观察期 CTR -11% → 自动回滚 + anti_pattern 卡（Prompt 进化）",
    ],
  },
};

const KIND_LABEL: Record<string, string> = {
  prompt: "Prompt 进化", skill: "Skill 进化", expert: "专家团进化", schedule: "调度进化",
};

function fmtDate(secs: number, withTime = false): string {
  if (!secs) return "—";
  const d = new Date(secs * 1000);
  const p = (n: number) => String(n).padStart(2, "0");
  const md = `${p(d.getMonth() + 1)}-${p(d.getDate())}`;
  return withTime ? `${md} ${p(d.getHours())}:${p(d.getMinutes())}` : md;
}

/** 功劳分 → ★ 显示（封顶 5 星，半档用 ☆ 补位） */
function merit(credit: number): string {
  const full = Math.max(0, Math.min(5, Math.round(credit)));
  return "★".repeat(full) + "☆".repeat(5 - full);
}

function mapLive(
  insights: InsightCard[],
  timeline: EvolutionEntry[],
  versions: PromptVersion[],
  fly: { health: number; monthInsights: number; solidified: number; rolledBack: number },
): EvolutionData {
  const tl = [...timeline].sort((a, b) => b.createdAt - a.createdAt);
  return {
    timeline: tl.map((e) => [
      `evo-${e.kind}`,
      fmtDate(e.createdAt, true),
      `${KIND_LABEL[e.kind] ?? e.kind} · ${e.proposer === "autopilot" ? "主Agent提案" : "人工"}`,
      [e.title, e.detail, e.expect && `预期：${e.expect}`, e.actual && `实际：${e.actual}`]
        .filter(Boolean).join(" — "),
      e.status,
    ]),
    cards: [...insights].sort((a, b) => b.createdAt - a.createdAt).map((c) => [
      c.kind, c.title, c.content, c.scope || "全局", merit(c.credit), fmtDate(c.createdAt),
    ]),
    tree: [...versions]
      .sort((a, b) => (a.expertId + a.platform + a.anchor).localeCompare(b.expertId + b.platform + b.anchor) || b.version - a.version)
      .map((v) => [
        `${v.expertId}（${v.platform ? v.platform + "补丁" : "基础画像"}·${v.anchor}）`,
        `v${v.version}`,
        v.status === "active" ? `当前${v.perfNote ? " · " + v.perfNote : ""}` : v.status === "rolled_back" ? "已回滚" : "已归档",
        v.perfNote || "—",
      ]),
    flywheel: {
      health: fly.health,
      cardsThisMonth: fly.monthInsights,
      solidified: fly.solidified,
      rolledBack: fly.rolledBack,
      evidence: tl.filter((e) => e.evidence.length).map((e) => `${e.title}（${KIND_LABEL[e.kind] ?? e.kind}）`),
    },
  };
}

/**
 * 进化数据源。source="live" 表示后端账本有真数据；"mock" 表示账本为空、显示设计稿示例。
 * 另暴露 live 原始记录与操作函数（手写卡 / 观察期裁决 / prompt 回滚），操作后自动 refresh。
 */
export function useEvolution() {
  const data = ref<EvolutionData>(MOCK_EVOLUTION) as Ref<EvolutionData>;
  const loading = ref(false);
  const source = ref<"mock" | "live">("mock");
  const liveTimeline = ref<EvolutionEntry[]>([]);
  const liveVersions = ref<PromptVersion[]>([]);

  async function refresh() {
    loading.value = true;
    try {
      const [st, fly] = await Promise.all([evolutionApi.state(), evolutionApi.flywheel()]);
      liveTimeline.value = st.timeline;
      liveVersions.value = st.promptVersions;
      const empty = !st.insights.length && !st.timeline.length && !st.promptVersions.length;
      if (empty) {
        source.value = "mock";
        data.value = MOCK_EVOLUTION;
      } else {
        source.value = "live";
        data.value = mapLive(st.insights, st.timeline, st.promptVersions, fly);
      }
    } catch {
      // 后端不可用（如纯浏览器预览）：保持 mock，不打断视图。
      source.value = "mock";
      data.value = MOCK_EVOLUTION;
    } finally {
      loading.value = false;
    }
  }

  async function addCard(kind: string, title: string, content: string, scope?: string) {
    await evolutionApi.insightAdd(kind, title, content, scope);
    await refresh();
  }
  async function addEntry(kind: string, title: string, opts?: { detail?: string; expect?: string; evidence?: string[] }) {
    await evolutionApi.add(kind, title, opts);
    await refresh();
  }
  async function decideEntry(id: string, status: "已固化" | "已回滚", actual?: string) {
    await evolutionApi.decide(id, status, actual);
    await refresh();
  }
  async function rollbackPrompt(id: string) {
    const v = await evolutionApi.promptVersionRollback(id);
    await refresh();
    return v;
  }

  void refresh();
  return { data, loading, source, liveTimeline, liveVersions, refresh, addCard, addEntry, decideEntry, rollbackPrompt };
}
