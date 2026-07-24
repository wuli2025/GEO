/**
 * 把 media_engine 落的那份「后端工作流」文本解析成结构化事件。
 *
 * 它不是运行日志——每一行都是后端跑这条流水线时的一个动作：
 * claude CLI 调了哪个工具、哪个阶段做了什么、外挂脚本吐了什么。
 * 原文形如：
 *   [14:03:07] generate：自媒体主笔（media-writer）为「公众号」写《…》
 *   [14:03:09] claude 调用工具：Read
 *   [14:05:22]   py> {"result": "ok"}
 *   [14:05:23] upload：成功（result=ok）
 * 解析成事件后交给 JobWorkflow.vue 排版，人看到的是流程本身而不是一堵字。
 */

export type WfTone = "info" | "ok" | "warn" | "bad";

/**
 * 脚本输出的一行：能认出 JSON 的拆成「动作 — 说明」，认不出的留原文。
 * bad = 这一行本身是坏消息；fatal = 整个脚本以失败收场（中途 ok:false 后被兜底救回来的不算）。
 */
export type WfOutLine = { raw: string; label?: string; detail?: string; bad?: boolean; fatal?: boolean };

export type WfEvent =
  /** claude CLI 的一次工具调用（同名连发合并计数） */
  | { t: "tool"; ts: string; abs: number; name: string; n: number }
  /** 某个阶段的一句自述 */
  | { t: "note"; ts: string; abs: number; scope: string; text: string; tone: WfTone }
  /** 外挂脚本（生图 / 投递）的输出，整段折叠 */
  | { t: "out"; ts: string; abs: number; kind: string; lines: WfOutLine[]; bad: boolean };

/** 阶段前缀 → 中文名；认不出的原样留着。 */
const SCOPE: Record<string, string> = {
  generate: "生成",
  image: "配图",
  typeset: "排版",
  upload: "投递",
  publish: "发布",
  job: "流水线",
};
/** 脚本输出前缀 → 中文名。 */
const OUT_KIND: Record<string, string> = { py: "脚本", img: "生图脚本" };

const LINE = /^\[(\d{2}:\d{2}:\d{2})\]\s?(.*)$/;
const OUT = /^\s*(py|img)([>!])\s?(.*)$/;
const TOOL = /^claude 调用工具[：:]\s*(.+)$/;
const SCOPED = /^([a-zA-Z_]+)\s*[：:]\s*(.+)$/;
const STAGE_FAIL = /^([a-zA-Z_]+)\s+失败[：:]\s*(.+)$/;
const JOB_ID = /^job\s+[0-9a-z]{8,}\s+(.+)$/i;
const JOB_ANY = /^job\s+(.+)$/;

const BAD = /失败|错误|异常|拦截|中断|取消|超时/;
const WARN = /跳过|降级|回落|兜底|未找到|不断流|重试/;
const OK = /完成|成功|已落盘|产出|出稿/;

/**
 * 脚本吐的多是 JSON 一行一条：{"step":"content_loaded","ok":true} /
 * {"result":"failed","detail":"…"}。拆成人话，比原样贴 JSON 好读。
 */
function outLine(raw: string, forcedBad: boolean): WfOutLine {
  const t = raw.trim();
  if (t.startsWith("{") && t.endsWith("}")) {
    try {
      const o = JSON.parse(t) as Record<string, unknown>;
      let label = String(o.step ?? o.result ?? o.event ?? "").trim();
      const detail = String(o.detail ?? o.message ?? o.error ?? "").trim();
      const fatal = forcedBad || o.result === "failed";
      const bad = fatal || o.ok === false || !!o.error;
      // 只有 ok 字段的裸行（壹伴脚本的收尾行就是这样）也别掉回原始 JSON
      if (!label && !detail && typeof o.ok === "boolean") label = o.ok ? "完成" : "失败";
      if (label || detail) return { raw, label: label || undefined, detail: detail || undefined, bad, fatal };
    } catch { /* 不是合法 JSON 就当普通输出 */ }
  }
  return { raw, bad: forcedBad, fatal: forcedBad };
}

/**
 * 「开跑」那行把平台、标题、阶段编排全抄了一遍——这些左栏都写着，
 * 流里只留「开跑（平台）」。「续跑」里的 JSON 数组也换成人读的顿号列表。
 */
function tidyJobLine(text: string): string {
  if (text.startsWith("开跑")) {
    const plat = /platform=(\S+)/.exec(text)?.[1];
    return plat ? `开跑（${plat}）` : "开跑";
  }
  return text.replace(/\[([^\]]*)\]/g, (_, inner: string) => {
    const items = inner.split(",").map((s) => s.trim().replace(/^"|"$/g, "")).filter(Boolean);
    return items.length ? items.join("、") : "无";
  });
}

function toneOf(text: string): WfTone {
  if (BAD.test(text)) return "bad";
  if (WARN.test(text)) return "warn";
  if (OK.test(text)) return "ok";
  return "info";
}

/**
 * @param src      工作流文本
 * @param dayStart 这条 job 开跑那天的零点（秒）。日志行只有 HH:MM:SS，
 *                 靠它把每行还原成绝对秒，才能和步骤、对话消息排在同一条时间轴上；
 *                 中途时钟倒退就是隔天续跑，往后推一天。
 */
export function parseWorkflow(src: string, dayStart = 0): WfEvent[] {
  const out: WfEvent[] = [];
  if (!src) return out;
  let dayOff = 0;
  let prevSod = -1;

  for (const raw of src.split("\n")) {
    if (!raw.trim()) continue;
    const m = LINE.exec(raw);
    // 没有时间戳的续行：脚本多行输出的后续行，粘到上一段里
    if (!m) {
      const last = out[out.length - 1];
      if (last?.t === "out") last.lines.push(outLine(raw.trimEnd(), false));
      else if (last?.t === "note") last.text += `\n${raw.trim()}`;
      else out.push({ t: "note", ts: "", abs: 0, scope: "", text: raw.trim(), tone: "info" });
      continue;
    }
    const ts = m[1].slice(0, 5); // 只留 HH:MM，秒对人没用
    const body = m[2];
    const sod = +m[1].slice(0, 2) * 3600 + +m[1].slice(3, 5) * 60 + +m[1].slice(6, 8);
    if (prevSod >= 0 && sod < prevSod - 60) dayOff += 86400;
    prevSod = sod;
    const abs = dayStart ? dayStart + dayOff + sod : 0;

    const o = OUT.exec(body);
    if (o) {
      const kind = OUT_KIND[o[1]] ?? o[1];
      const line = outLine(o[3], o[2] === "!");
      const last = out[out.length - 1];
      // 同一个脚本的连续输出并成一段；以失败收场的整段标红摊开
      if (last?.t === "out" && last.kind === kind) {
        last.lines.push(line);
        last.bad = last.bad || !!line.fatal;
      } else {
        out.push({ t: "out", ts, abs, kind, lines: [line], bad: !!line.fatal });
      }
      continue;
    }

    const tl = TOOL.exec(body);
    if (tl) {
      const name = tl[1].trim();
      const last = out[out.length - 1];
      if (last?.t === "tool" && last.name === name) last.n += 1;
      else out.push({ t: "tool", ts, abs, name, n: 1 });
      continue;
    }

    // job 那几行：把冗长的 job id 收掉，只留它在说的事
    const jb = JOB_ID.exec(body) ?? JOB_ANY.exec(body);
    if (jb) {
      const text = tidyJobLine(jb[1].trim());
      out.push({ t: "note", ts, abs, scope: SCOPE.job, text, tone: toneOf(text) });
      continue;
    }

    const sf = STAGE_FAIL.exec(body);
    if (sf) {
      out.push({ t: "note", ts, abs, scope: SCOPE[sf[1]] ?? sf[1], text: sf[2], tone: "bad" });
      continue;
    }

    const sc = SCOPED.exec(body);
    if (sc && (SCOPE[sc[1]] || sc[1].length <= 10)) {
      out.push({ t: "note", ts, abs, scope: SCOPE[sc[1]] ?? sc[1], text: sc[2], tone: toneOf(sc[2]) });
      continue;
    }

    out.push({ t: "note", ts, abs, scope: "", text: body.trim(), tone: toneOf(body) });
  }
  return out;
}
