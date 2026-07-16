/**
 * 大脑·进化视图的数据源（M10 循环工程）。
 *
 * 现阶段返回设计稿 mock；后续接真实后端时只需替换本文件内部实现（保持返回结构不变），
 * vBrain 视图与壳层无需改动。这就是「可替换的数据获取 composable」的边界。
 */
import { ref, type Ref } from "vue";

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

/**
 * 返回进化数据（现为 mock，同步就绪）。返回 ref 以便将来平滑切成异步真实数据源：
 * 届时在此内部拉后端并回填 data.value，视图端零改动。
 */
export function useEvolution(): { data: Ref<EvolutionData>; loading: Ref<boolean> } {
  const data = ref<EvolutionData>(MOCK_EVOLUTION);
  const loading = ref(false);
  return { data, loading };
}
