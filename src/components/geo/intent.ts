/**
 * 一句话 → 意图。助手输入框里打的每句话都先过这里：
 *
 *  · 「发一篇公众号的内容」    → produce（没标题就先出选题）
 *  · 「发一篇《标题》到知乎」  → produce（有标题走「先规划再定夺」）
 *  · 「打开账号矩阵 / 切到头条」→ nav（顶栏功能键 / 媒体门户）
 *  · 其它                      → ask（就是跟助手说话）
 *
 * 纯函数、没有副作用：谁调用谁负责真去干活。
 */
import { ZONES } from "./data";

/* ── 平台别名：人怎么叫都得认出来 ─────────────────────────────────── */
const PLATFORM_ALIAS: Record<string, string[]> = {
  wechat: ["公众号", "微信", "订阅号", "服务号", "wechat", "mp"],
  zhihu: ["知乎", "zhihu"],
  toutiao: ["今日头条", "头条号", "头条", "toutiao"],
  baijia: ["百家号", "百家", "百度号", "baijia"],
  xhs: ["小红书", "红书", "xhs"],
  bilibili: ["b站", "B站", "哔哩", "bilibili"],
  douyin: ["抖音", "douyin"],
  csdn: ["csdn", "CSDN"],
  juejin: ["掘金", "juejin"],
  channels: ["视频号", "channels"],
};

/* ── 视图别名：导航类指令 ─────────────────────────────────────────── */
const VIEW_ALIAS: Record<string, string[]> = {
  dashboard: ["看板", "数据看板", "仪表盘", "大盘"],
  approvals: ["审批", "审批队列", "待审"],
  autopilot: ["自动规划", "定时任务", "cron", "排期"],
  brain: ["大脑", "进化", "飞轮"],
  accounts: ["账号", "账号矩阵", "登录态", "扫码"],
  experts: ["专家", "专家阵容", "专家团"],
  brand: ["品牌", "品牌档案", "品牌形象"],
  promo: ["推广", "植入", "推广植入"],
  kb: ["知识库", "资料库", "星图"],
  questions: ["题库", "选题池"],
  engine: ["投递引擎", "引擎", "流水线总表"],
  gate: ["门禁", "质检"],
  layout: ["排版", "排版中心", "封面规格"],
  settings: ["设置", "模型通道", "环境医生", "更新"],
};

/** 视图 id → 顶栏上写的那个名字。 */
export const VIEW_LABEL: Record<string, string> = (() => {
  const m: Record<string, string> = {};
  ZONES.forEach((z) => z.keys.forEach((k) => { m[k[0]] = k[2]; }));
  return m;
})();

const PRODUCE_HINTS = [
  "发一", "发个", "发篇", "发条", "发布", "发送", "写一", "写个", "写篇", "来一", "来个",
  "做一", "出一", "生成", "排产", "投稿", "推送", "更新一", "开写", "起草", "选题",
];
const NAV_HINTS = ["打开", "切到", "切换", "进入", "跳到", "回到", "查看", "看看", "显示"];
/** 「去品牌形象那页」也是导航；但只认句首的「去/到」——不然「过去 7 天公众号发了几篇」
 *  会被当成「跳去公众号门户」。 */
const navHinted = (t: string) => NAV_HINTS.some((v) => t.includes(v)) || /^[去到]/.test(t);

export interface Parsed {
  intent: "produce" | "nav" | "ask";
  /** 话里点名的平台（没点名则 null，调用方拿当前控制范围兜底） */
  platform: string | null;
  view: string | null;
  title: string | null;
}

export function detectPlatform(text: string): string | null {
  let best: { id: string; at: number; len: number } | null = null;
  for (const [id, names] of Object.entries(PLATFORM_ALIAS)) {
    for (const n of names) {
      const at = text.indexOf(n);
      if (at < 0) continue;
      // 命中更长的别名优先（「今日头条」压过「头条」），同长取更靠前的
      if (!best || n.length > best.len || (n.length === best.len && at < best.at)) {
        best = { id, at, len: n.length };
      }
    }
  }
  return best?.id ?? null;
}

function detectView(text: string): string | null {
  let best: { id: string; len: number } | null = null;
  for (const [id, names] of Object.entries(VIEW_ALIAS)) {
    for (const n of names) {
      if (!text.includes(n)) continue;
      if (!best || n.length > best.len) best = { id, len: n.length };
    }
  }
  return best?.id ?? null;
}

function detectTitle(text: string): string | null {
  const quoted = text.match(/[《「『"“]([^》」』"”]{2,60})[》」』"”]/);
  if (quoted) return quoted[1].trim();
  const about = text.match(/关于(.{2,40}?)(?:的(?:文章|内容|稿|笔记|推文|长文)|$)/);
  if (about) return about[1].trim();
  return null;
}

/**
 * @param fallbackPlatform 当前控制的媒体（「全部」传 null）——话里没点名时拿它兜底。
 */
export function parse(text: string, fallbackPlatform: string | null): Parsed {
  const platform = detectPlatform(text);
  const title = detectTitle(text);
  const produce = PRODUCE_HINTS.some((v) => text.includes(v));
  // 只丢一个《标题》进来、且此刻正控制着某个媒体 → 视同「就写这篇」
  const bareTitle = !!title && text.replace(/[《》「」『』"”“]/g, "").trim().length - title.length < 6;
  if (produce || (bareTitle && (platform || fallbackPlatform))) {
    return { intent: "produce", platform, view: null, title };
  }
  const view = detectView(text);
  if (navHinted(text) && (view || platform)) {
    return { intent: "nav", platform, view, title };
  }
  return { intent: "ask", platform, view: null, title };
}
