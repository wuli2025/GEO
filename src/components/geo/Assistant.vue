<script setup lang="ts">
/**
 * 助手 —— 右侧那块板，全应用**唯一**的 AI 对话框。
 *
 * - 一条会话。顶上一排能横着滑的「控制范围」：**全部** + 每个媒体一枚。
 *   选中谁，就等于对助手说「接下来聊的是它」——上下文注进提示词、生成记录也只看它。
 *   范围也可以在对话里改口（「发一篇知乎的」→ 自动切到知乎），或跟着门户导航自动跟随。
 * - 输入框长在自己身上（那块玻璃），底部原来那条常驻输入坞已经撤掉。
 * - 输入框上方是**推荐**：点一下要么直接开跑（选题 / 发一篇），
 *   要么把一句写好的短提示填进输入框，人改两个字再发。
 * - 记录流 = 对话消息 + 壳层记录（跳转/排产/告警）+ media_job 生成记录，按时间汇成一条。
 *   行里**不显示时间**；「它现在在干什么」看贴在输入框上方那个虚化小框。
 */
import { ref, computed, watch, nextTick, onMounted, onBeforeUnmount, useTemplateRef } from "vue";
import {
  chat, convApi, listen, mediaJob, mediaOps, MEDIA_PLATFORMS,
  type Message, type ChatStreamEvent, type MediaJob, type MediaPlatform,
} from "../../tauri";
import { toast } from "../../composables/useToast";
import { planRequest, clearPlan, requestPlan, type PlanRequest } from "./planBus";
import { openJobDetail } from "./jobsBus";
import { pico, P, PLATFORMS } from "./data";
import { records, pushRecord, goTo } from "./assistantBus";
import { parse, VIEW_LABEL, type Parsed } from "./intent";

const props = defineProps<{
  /** 当前视图名（注入模型的「人此刻站在哪」） */
  viewLabel: string;
  /** 当前视图的补充上下文（子标签 / 打开着的 job 等） */
  viewCtx: string;
  /** 人此刻打开的媒体门户（不在门户上则为 null）——控制范围会跟着它走 */
  platform: string | null;
}>();
const emit = defineEmits<{ (e: "close"): void }>();

const PROJECT_NAME = "GEO 全局助手";
const LS_CONV = "geo.assistant.conv";
/** 多泳道时代的「泳道 → 会话」映射；单会话之后只取总控那条继续用。 */
const LS_CONV_LEGACY_MAP = "geo.globalChat.convs";
/** 更早的全局单会话。 */
const LS_CONV_LEGACY = "geo.globalChat.conv";
const LS_SCOPE = "geo.assistant.scope";

function readConv(): string | null {
  const own = localStorage.getItem(LS_CONV);
  if (own) return own;
  // 迁移：优先接上总控那条，其次是更早的全局会话——别让人的历史断在版本升级上。
  let picked: string | null = null;
  try {
    const map = JSON.parse(localStorage.getItem(LS_CONV_LEGACY_MAP) || "null");
    if (map && typeof map === "object" && !Array.isArray(map)) picked = map.hub ?? null;
  } catch { /* 老数据坏了就当没有 */ }
  picked = picked ?? localStorage.getItem(LS_CONV_LEGACY);
  if (picked) localStorage.setItem(LS_CONV, picked);
  localStorage.removeItem(LS_CONV_LEGACY_MAP);
  localStorage.removeItem(LS_CONV_LEGACY);
  return picked;
}

/* ── 控制范围：全部 / 某个媒体 ───────────────────────────────────── */
const ALL = "all";
const scope = ref<string>(
  (() => {
    const saved = localStorage.getItem(LS_SCOPE);
    return saved && (saved === ALL || PLATFORMS.some((p) => p.id === saved)) ? saved : ALL;
  })(),
);
const scopePlat = computed(() => (scope.value === ALL ? null : scope.value));
const scopeMeta = computed(() => (scopePlat.value ? P(scopePlat.value) : undefined));
function setScope(v: string) {
  if (scope.value === v) return;
  scope.value = v;
  localStorage.setItem(LS_SCOPE, v);
}
// 人走到哪个门户，助手就跟着控制哪个媒体——省得再手点一次。
watch(() => props.platform, (p) => { if (p) setScope(p); }, { immediate: true });

const msgs = ref<Message[]>([]);
const sending = ref(false);
const streamText = ref("");
const streamTool = ref("");
const convId = ref<string | null>(readConv());
const listEl = ref<HTMLDivElement | null>(null);
const taEl = useTemplateRef<HTMLTextAreaElement>("ta");
const dockEl = ref<HTMLDivElement | null>(null);
const draft = ref("");
/** 让较早发出的历史请求不能覆盖较晚的本地消息。 */
let historyLoadSeq = 0;
let reqId: string | null = null;
let unlisten: (() => void) | null = null;
const streamSubscribed = ref(false);
let disposed = false;

/* ── 生成记录：media_job（按控制范围过滤，「全部」看全部） ──────────── */
const jobs = ref<MediaJob[]>([]);
let jobTimer: ReturnType<typeof setInterval> | null = null;

async function loadJobs() {
  try { jobs.value = await mediaJob.list(); } catch { /* 后端不在（web 预览）则静默留空 */ }
}
const scopeJobs = computed(() => {
  const plat = scopePlat.value;
  const rows = plat ? jobs.value.filter((j) => j.platform === plat) : jobs.value;
  return [...rows].sort((a, b) => a.createdAt - b.createdAt).slice(-25);
});
/** 各媒体在跑的 job 数——范围条上亮个数字，不用切过去也知道那边有活。 */
const runningBy = computed(() => {
  const m: Record<string, number> = {};
  for (const j of jobs.value) {
    if (j.status !== "running" && j.status !== "pending") continue;
    m[j.platform] = (m[j.platform] ?? 0) + 1;
    m[ALL] = (m[ALL] ?? 0) + 1;
  }
  return m;
});
function jobDot(j: MediaJob): string {
  if (j.status === "running" || j.status === "pending") return "run";
  if (j.status === "done") return "ok";
  if (j.status === "failed") return "bad";
  return "idle";
}
function jobTail(j: MediaJob): string {
  const last = j.steps?.[j.steps.length - 1];
  const name = P(j.platform)?.name ?? j.platform;
  if (j.status === "failed" && j.error) return `${name} · ${j.error.slice(0, 70)}`;
  if (last) return `${name} · ${last.label}${last.status === "fail" ? "（失败）" : ""}`;
  return `${name} · ${j.stage || j.status}`;
}

/* ── 时间轴汇流：对话消息 + 壳层记录 + 生成记录 ───────────────────── */
type Row =
  | { k: "msg"; id: string; ts: number; role: string; text: string }
  | { k: "rec"; id: string; ts: number; kind: string; text: string }
  | { k: "job"; id: string; ts: number; job: MediaJob };

/**
 * 用户消息的显示文本：把系统注入的部分剥掉，只留人真说的那句。
 * 发给模型的 prompt = 【上下文…】+ "────" + 人说的话（+ 有时追一段「（系统提示：…）」），
 * 后端整条落库；直接渲染就会把这些脚手架摊给人看。
 */
const CTX_END = "\n────\n";
function displayText(m: Message): string {
  if (m.role !== "user") return m.content;
  let t = m.content;
  if (t.startsWith("【")) {
    const i = t.indexOf(CTX_END);
    if (i >= 0) t = t.slice(i + CTX_END.length);
  }
  const hint = t.indexOf("\n\n（系统提示：");
  if (hint >= 0) t = t.slice(0, hint);
  return t.trim() || m.content;
}

const rows = computed<Row[]>(() => {
  const out: Row[] = [];
  for (const m of msgs.value) {
    out.push({ k: "msg", id: `m-${m.id}`, ts: m.createdAt || 0, role: m.role, text: displayText(m) });
  }
  for (const r of records.value) {
    out.push({ k: "rec", id: r.id, ts: r.ts, kind: r.kind, text: r.text });
  }
  for (const j of scopeJobs.value) {
    out.push({ k: "job", id: `j-${j.id}`, ts: j.createdAt, job: j });
  }
  return out.sort((a, b) => a.ts - b.ts || (a.k === "job" ? 1 : 0) - (b.k === "job" ? 1 : 0));
});

/* ── 选题规划预览 ─────────────────────────────────────────────────── */
type PlanPhase = "gen" | "ready" | "failed" | "starting" | "started";
const plan = ref<{ reqId: string; title: string; text: string; phase: PlanPhase; jobId?: string } | null>(null);
/** 规划预览走独立的 chat_send（不落库），单独跟踪它的 reqId。 */
let planStreamReqId: string | null = null;
/** 使较早、较慢返回的 chat_send 不能夺走新规划的流归属。 */
let planStartSeq = 0;
/** 已接过的 planBus 请求 id，避免重复触发。 */
const handledPlanId = ref<string | null>(null);
/** 规划生成/排产中时，普通发送先让路。 */
const planBusy = computed(() => plan.value?.phase === "gen" || plan.value?.phase === "starting");
const busy = computed(() => sending.value || planBusy.value);

/* ── 「在干什么」小框 ─────────────────────────────────────────────
   跑起来时人最想知道的只有一件事：它此刻在做哪一步。收成一个贴在输入框上方的
   虚化小框，只说一句话，跑完自己消失。                                  */
const runningJob = computed(
  () => scopeJobs.value.find((j) => j.status === "running" || j.status === "pending") ?? null,
);
const activity = computed<{ what: string; sub: string } | null>(() => {
  if (plan.value?.phase === "gen") return { what: "正在规划这篇怎么写", sub: `《${plan.value.title}》` };
  if (plan.value?.phase === "starting") return { what: "正在排产启动流水线", sub: `《${plan.value.title}》` };
  if (sending.value) return { what: streamTool.value || "正在思考", sub: "" };
  const j = runningJob.value;
  if (j) {
    const last = j.steps?.[j.steps.length - 1];
    return { what: last?.label || j.stage || "排队等待开跑", sub: `《${j.title || "未命名"}》` };
  }
  if (!streamSubscribed.value) return { what: "正在连接对话流", sub: "" };
  return null;
});

/* ── 推荐：点一下直接开跑，或把一句写好的短提示填进输入框 ───────────── */
interface Rec {
  /** 键面上的字 */
  label: string;
  /** 真正那句话（填进输入框或直接发出去的都是它） */
  text: string;
  /** true = 点了就跑；false = 填进输入框等人改 */
  fire: boolean;
}
const recs = computed<Rec[]>(() => {
  const n = scopeMeta.value?.name;
  if (n) {
    return [
      { label: `为${n}选题`, text: `为${n}进行选题`, fire: true },
      { label: `发一篇${n}`, text: `发送一篇${n}的内容`, fire: true },
      { label: "指定标题开写", text: `发一篇《》到${n}`, fire: false },
      { label: "看看最近数据", text: `${n}最近一周数据怎么样，哪类选题最能打`, fire: false },
    ];
  }
  return [
    { label: "今天发什么", text: "今天各平台分别发什么合适", fire: true },
    { label: "为公众号选题", text: "为微信公众号进行选题", fire: true },
    { label: "发一篇小红书", text: "发送一篇小红书", fire: true },
    { label: "排一下本周", text: "把这周排个期，每个平台各发几篇、发什么", fire: false },
  ];
});
function useRec(r: Rec) {
  if (r.fire) { submit(r.text); return; }
  draft.value = r.text;
  nextTick(() => {
    autoGrow();
    const el = taEl.value;
    if (!el) return;
    el.focus();
    // 「发一篇《》到知乎」这种模板，光标直接落进书名号里，人接着打标题就行
    const at = r.text.indexOf("《》");
    if (at >= 0) el.setSelectionRange(at + 1, at + 1);
  });
}

/* ── 会话 ────────────────────────────────────────────────────────── */
async function ensureConv(): Promise<string> {
  if (convId.value) return convId.value;
  const projects = await convApi.listProjects();
  const proj = projects.find((p) => p.name === PROJECT_NAME && !p.archived)
    ?? (await convApi.createProject(PROJECT_NAME));
  const conv = await convApi.createConversation(proj.id);
  await convApi.renameConversation(conv.id, "GEO 助手").catch(() => {});
  localStorage.setItem(LS_CONV, conv.id);
  convId.value = conv.id;
  return conv.id;
}

async function loadHistory(id = convId.value) {
  const seq = ++historyLoadSeq;
  if (!id) {
    if (seq === historyLoadSeq) msgs.value = [];
    return;
  }
  try {
    const loaded = (await convApi.getMessages(id)).filter((m) => m.role !== "tool");
    if (seq !== historyLoadSeq || convId.value !== id) return;
    msgs.value = loaded;
    scrollBottom();
  } catch {
    if (seq === historyLoadSeq && convId.value === id) msgs.value = [];
  }
}

async function scrollBottom() {
  await nextTick();
  if (listEl.value) listEl.value.scrollTop = listEl.value.scrollHeight;
}

/** 注进提示词的上下文：此刻控制着谁、人站在哪。 */
function ctxBlock(): string {
  const p = scopeMeta.value;
  return [
    "【上下文 · 系统自动注入，无需向用户复述】",
    "你是 GEO 自媒体运营中心的运营助手，全站只有你这一个助手，跨平台的事都归你。",
    p
      ? `当前控制范围：「${p.name}」（平台 id=${scopePlat.value}）——没特别说明就默认在聊这个平台。`
      : "当前控制范围：全部平台——按跨平台的全局事务处理。",
    p ? `平台档案：适配=${p.adapterText}；发送方式=${p.sendMode === "auto" ? "AI 直传草稿箱" : "手动辅助"}；文风=${p.style}` : "",
    p ? `红线：${p.redline}` : "",
    `打开的界面：${props.viewLabel}${props.viewCtx ? `（${props.viewCtx}）` : ""}`,
    "────",
  ].filter(Boolean).join("\n");
}

/* ── 选题规划预览 ─────────────────────────────────────────────────── */
function planPrompt(req: PlanRequest): string {
  return [
    "你是 GEO 自媒体运营中心的选题规划师。下面这条选题还没开始写，",
    "请只产出一份【撰写规划】给人过目——人看完会点「开始」或「否决」。",
    `平台：${req.platformName}（${req.platform}）`,
    `选题标题：《${req.title}》`,
    req.angle ? `切入角度：${req.angle}` : "",
    req.keywords?.length ? `关键词：${req.keywords.join("、")}` : "",
    "────",
    "用简洁中文输出，只包含这三块，别写正文、别调用任何工具：",
    "① 选题角度：1-2 句，怎么切入、写给谁看、要立什么观点；",
    "② 核心要点：3-5 条 bullet；",
    "③ 结构大纲：各段小标题 + 一句话作用。",
  ].filter(Boolean).join("\n");
}

async function startPlan(req: PlanRequest) {
  const startSeq = ++planStartSeq;
  // 新选题覆盖旧规划时先停掉旧请求，避免后台继续耗模型且流事件无人接收。
  if (planStreamReqId) {
    await chat.cancel(planStreamReqId).catch(() => {});
    planStreamReqId = null;
  }
  plan.value = { reqId: req.id, title: req.title, text: "", phase: "gen" };
  scrollBottom();
  try {
    // 不传 conversationId：规划是临时预览，不落库、不污染对话历史。
    const startedReqId = await chat.send({
      prompt: planPrompt(req),
      permissionMode: "auto_current",
      workMode: "office",
    });
    if (startSeq !== planStartSeq || plan.value?.reqId !== req.id) {
      chat.cancel(startedReqId).catch(() => {});
      return;
    }
    planStreamReqId = startedReqId;
  } catch (e: any) {
    toast.error(e?.message ?? String(e));
    if (plan.value?.reqId === req.id) plan.value.phase = "failed";
  }
}

/** 挂载时补捞一次：请求可能在本组件挂起来之前就投好了。 */
function maybePickupPlan() {
  if (!streamSubscribed.value) return;
  const req = planRequest.value;
  if (req && handledPlanId.value !== req.id) {
    handledPlanId.value = req.id;
    startPlan(req);
  }
}

function stopPlan() {
  planStartSeq += 1;
  if (planStreamReqId) { chat.cancel(planStreamReqId).catch(() => {}); planStreamReqId = null; }
  if (plan.value) plan.value.phase = "ready";
}

function retryPlan() {
  const p = plan.value;
  const req = planRequest.value;
  if (!p || !req || req.id !== p.reqId) return;
  startPlan(req);
}

async function approvePlan() {
  const p = plan.value;
  if (!p || p.phase !== "ready") return;
  const req = planRequest.value;
  p.phase = "starting";
  try {
    const r = req?.id === p.reqId ? await req.onApprove() : undefined;
    p.jobId = r?.jobId;
    p.phase = "started";
    pushRecord("sys", `已开始跑流水线${r?.jobId ? `（job ${r.jobId.slice(0, 8)}）` : ""}`);
    clearPlan(p.reqId);
    loadJobs();
  } catch (e: any) {
    toast.error(e?.message ?? String(e));
    p.phase = "ready"; // 排产失败可重试
  }
}

function rejectPlan() {
  const p = plan.value;
  if (!p) return;
  if (p.phase === "gen") stopPlan();
  clearPlan(p.reqId);
  plan.value = null;
  handledPlanId.value = null;
}

/* ── 发送 ────────────────────────────────────────────────────────── */
async function ask(text: string, echo?: string) {
  if (busy.value || !streamSubscribed.value) {
    pushRecord("warn", "还有一条在生成中，先等它跑完或点停止。");
    return;
  }
  sending.value = true;
  streamText.value = ""; streamTool.value = "";
  try {
    const id = await ensureConv();
    // 任何在途历史读取都不得覆盖这条刚发送的本地消息。
    historyLoadSeq += 1;
    msgs.value.push({
      id: `local-${Date.now()}`, conversationId: id,
      role: "user", content: echo ?? text, createdAt: Math.floor(Date.now() / 1000),
    });
    scrollBottom();
    reqId = await chat.send({
      prompt: `${ctxBlock()}\n${text}`,
      permissionMode: "auto_current",
      conversationId: id,
      workMode: "office",
    });
  } catch (e: any) {
    sending.value = false;
    toast.error(e?.message ?? String(e));
    // 发失败时那句话还没变成用户消息落进记录 —— 补一条，别让人的输入凭空消失
    pushRecord("warn", `这条没发出去（${e?.message ?? e}）：${echo ?? text}`);
  }
}

async function startJob(platform: MediaPlatform, title: string, topic?: string): Promise<{ jobId: string }> {
  // 先入规划队列，再启流水线——这样门户的「规划队列」里也看得见这条，不是野生 job
  try {
    const q = await mediaOps.queueAdd(platform, title);
    const j = await mediaJob.start({ queueId: q.id, topic });
    return { jobId: j.id };
  } catch {
    const j = await mediaJob.start({ platform, title, topic });
    return { jobId: j.id };
  }
}

function doProduce(p: Parsed, text: string) {
  const plat = p.platform ?? scopePlat.value;
  if (!plat) {
    // 没说是哪个媒体，控制范围又是「全部」→ 别乱猜，让助手先问清楚
    ask(`${text}\n\n（系统提示：用户没有指明媒体平台。先反问他要发哪个平台，或按主阵地优先级给建议，别擅自开写。）`, text);
    return;
  }
  if (p.platform) setScope(p.platform); // 话里点了名 → 控制范围跟着改口
  const pf = P(plat);
  const supported = MEDIA_PLATFORMS.some((m) => m.id === plat);

  if (!supported) {
    pushRecord("warn", `${pf?.name ?? plat} 尚未接入投递引擎，这条只当选题讨论处理。`);
    ask(`${text}\n\n（系统提示：该平台尚未接入自动投递，只做选题与稿件讨论。）`, text);
    return;
  }

  if (p.title) {
    // 有明确标题 → 走「先规划、再定夺」：这里流式出撰写规划，人点「开始」才真跑
    pushRecord("sys", `已按《${p.title}》生成撰写规划，看完点「开始」才会真排产。`);
    requestPlan({
      platform: plat as MediaPlatform,
      platformName: pf?.name ?? plat,
      title: p.title,
      angle: text,
      onApprove: async () => {
        const r = await startJob(plat as MediaPlatform, p.title!, text);
        toast.success(`已排产并启动流水线（job ${r.jobId.slice(0, 8)}）`);
        return r;
      },
    });
    return;
  }

  // 没标题 → 先出 3 个选题，人挑定了再回一句《标题》即可开写
  ask(
    [
      text,
      "",
      `（系统提示：用户想在「${pf?.name ?? plat}」上发一篇，但还没定选题。`,
      "先给 3 个候选选题，每条一行：标题 + 一句话角度，贴合本平台文风与红线；",
      "末尾告诉他：回一句《标题》就开写。别现在写正文、别调工具。）",
    ].join("\n"),
    text,
  );
}

function doNav(p: Parsed, text: string) {
  if (p.view) {
    goTo(p.view);
    pushRecord("sys", `「${text}」→ 已跳到「${VIEW_LABEL[p.view] ?? p.view}」`);
    return;
  }
  const plat = p.platform!;
  goTo("portal", plat);
  setScope(plat);
  pushRecord("sys", `「${text}」→ 已切到「${P(plat)?.name ?? plat}」门户`);
}

function submit(raw?: string) {
  const text = (raw ?? draft.value).trim();
  if (!text || busy.value) return;
  if (raw === undefined) { draft.value = ""; nextTick(autoGrow); }
  const p = parse(text, scopePlat.value);
  if (p.intent === "produce") doProduce(p, text);
  else if (p.intent === "nav") doNav(p, text);
  else {
    if (p.platform) setScope(p.platform);
    ask(text);
  }
}

async function stop() {
  if (reqId) { try { await chat.cancel(reqId); } catch { /* 已结束则忽略 */ } }
  if (planBusy.value) stopPlan();
}

async function finish() {
  const finishedConvId = convId.value;
  sending.value = false;
  reqId = null;
  streamTool.value = "";
  // 在 await 历史读取前清掉旧流；否则用户立刻发下一条时，旧 finish 返回后会误清新流。
  streamText.value = "";
  if (finishedConvId) await loadHistory(finishedConvId);
  scrollBottom();
  loadJobs();
}

/* ── 输入框：跟着内容长高，并把列表底部垫出同样的高度 ────────────── */
/** 输入区实际高度 → 列表底部留白，免得最后一行被那块玻璃压住。 */
const dockH = ref(96);
function measureDock() {
  if (dockEl.value) dockH.value = dockEl.value.offsetHeight;
}
function autoGrow() {
  const el = taEl.value;
  if (el) {
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 132)}px`;
  }
  nextTick(measureDock);
}

/* ── 外部请求 ───────────────────────────────────────────────────── */
watch(planRequest, (req) => {
  if (!streamSubscribed.value || !req || handledPlanId.value === req.id) return;
  handledPlanId.value = req.id;
  startPlan(req);
});
// 推荐随范围变化会改变高度（一行 / 两行），跟着量一次
watch([recs, activity, () => plan.value?.phase], () => nextTick(measureDock));

let ro: ResizeObserver | null = null;

onMounted(async () => {
  // 先订阅再启动规划；否则组件因待处理规划而首次挂载时，首段流事件可能先于监听丢失。
  let off: (() => void) | null = null;
  try {
    off = await listen<ChatStreamEvent>("chat:stream", (ev) => {
      // 规划预览的流：走独立通道，不落库，画进规划卡。
      if (planStreamReqId && ev.reqId === planStreamReqId) {
        if (!plan.value) return;
        if (ev.kind === "delta" && ev.text) { plan.value.text += ev.text; scrollBottom(); }
        else if (ev.kind === "error") { toast.error(ev.text || "规划生成出错"); plan.value.phase = "failed"; planStreamReqId = null; }
        else if (ev.kind === "done") { plan.value.phase = "ready"; planStreamReqId = null; }
        return;
      }
      if (!reqId || ev.reqId !== reqId) return;
      if (ev.kind === "delta" && ev.text) { streamText.value += ev.text; scrollBottom(); }
      else if (ev.kind === "tool") streamTool.value = ev.tool || ev.text || "";
      else if (ev.kind === "error") { toast.error(ev.text || "对话出错"); finish(); }
      else if (ev.kind === "done") finish();
    });
  } catch (e: any) {
    if (!disposed) toast.error(e?.message ?? "无法连接对话流");
  }
  if (disposed) { off?.(); return; }
  unlisten = off;
  streamSubscribed.value = !!off;
  if (dockEl.value && "ResizeObserver" in window) {
    ro = new ResizeObserver(measureDock);
    ro.observe(dockEl.value);
  }
  measureDock();
  await loadHistory();
  maybePickupPlan();
  await loadJobs();
  jobTimer = setInterval(() => { if (!document.hidden) loadJobs(); }, 5000);
  scrollBottom();
  taEl.value?.focus();
});
onBeforeUnmount(() => {
  disposed = true;
  streamSubscribed.value = false;
  historyLoadSeq += 1;
  planStartSeq += 1;
  if (jobTimer) clearInterval(jobTimer);
  if (planStreamReqId) chat.cancel(planStreamReqId).catch(() => {});
  ro?.disconnect();
  unlisten?.();
});
</script>

<template>
  <aside class="as">
    <!-- 控制范围：横着滑，选谁就等于对助手说「接下来聊它」 -->
    <div class="as-top">
      <div class="as-scope">
        <button class="as-s" :class="{ on: scope === 'all' }" title="全部平台" @click="setScope('all')">
          <span class="as-all">◆</span>全部
          <span v-if="runningBy.all" class="as-n">{{ runningBy.all }}</span>
        </button>
        <button
          v-for="p in PLATFORMS"
          :key="p.id"
          class="as-s"
          :class="{ on: scope === p.id }"
          :title="`只聊${p.name}`"
          @click="setScope(p.id)"
        >
          <span class="as-ic" v-html="pico(p.id)"></span>{{ p.name }}
          <span v-if="runningBy[p.id]" class="as-n">{{ runningBy[p.id] }}</span>
        </button>
      </div>
      <button class="as-fold" title="收起助手" @click="emit('close')">›</button>
    </div>

    <!-- 记录流：对话 + 系统记录 + 生成记录 -->
    <div ref="listEl" class="as-body" :style="{ paddingBottom: `${dockH + 14}px` }">
      <p v-if="!rows.length && !sending && !plan" class="as-empty">
        <template v-if="scopeMeta">这里只聊「{{ scopeMeta.name }}」。</template>
        <template v-else>控制范围是「全部平台」。</template><br />
        说一句，或点下面的推荐开始。
      </p>

      <template v-for="r in rows" :key="r.id">
        <div v-if="r.k === 'msg'" class="as-line" :class="r.role">
          <span class="as-gut">{{ r.role === "user" ? "›" : "‹" }}</span>
          <div class="as-txt">{{ r.text }}</div>
        </div>
        <div v-else-if="r.k === 'rec'" class="as-line rec" :class="r.kind">
          <span class="as-gut">{{ r.kind === "warn" ? "!" : "·" }}</span>
          <div class="as-txt">{{ r.text }}</div>
        </div>
        <button v-else class="as-job" :class="jobDot(r.job)" @click="openJobDetail(r.job.id)">
          <span class="as-gut"><span class="as-jdot" :class="jobDot(r.job)"></span></span>
          <span class="as-jbody">
            <b>《{{ r.job.title || "未命名" }}》</b>
            <span class="as-jmeta">{{ jobTail(r.job) }}</span>
          </span>
        </button>
      </template>

      <!-- 流式中 -->
      <div v-if="sending" class="as-line assistant">
        <span class="as-gut">‹</span>
        <div class="as-txt">
          <template v-if="streamText">{{ streamText }}<span class="as-caret"></span></template>
          <span v-else class="as-shine">正在思考</span>
        </div>
      </div>

      <!-- 选题规划预览卡 -->
      <div v-if="plan" class="as-plan">
        <div class="as-plan-h">撰写规划 ·《{{ plan.title }}》</div>
        <div class="as-plan-body">
          <template v-if="plan.text">{{ plan.text }}</template>
          <span v-else-if="plan.phase === 'gen'" class="as-shine">正在规划这篇怎么写</span>
          <span v-else-if="plan.phase === 'failed'" class="as-quiet">规划生成失败，可重试或否决。</span>
        </div>
        <div class="as-plan-act">
          <template v-if="plan.phase === 'gen'">
            <button class="btn sm danger" @click="stopPlan">停止规划</button>
          </template>
          <template v-else-if="plan.phase === 'ready'">
            <button class="btn sm" @click="approvePlan">▶ 开始</button>
            <button class="btn sm ghost" @click="rejectPlan">否决</button>
          </template>
          <template v-else-if="plan.phase === 'failed'">
            <button class="btn sm" @click="retryPlan">重试规划</button>
            <button class="btn sm ghost" @click="rejectPlan">否决</button>
          </template>
          <template v-else-if="plan.phase === 'starting'">
            <span class="as-shine">排产启动中</span>
          </template>
          <template v-else>
            <span class="as-quiet">已开始跑流水线。</span>
            <button v-if="plan.jobId" class="btn sm ghost" @click="plan.jobId && openJobDetail(plan.jobId)">看流程 →</button>
            <button class="btn sm ghost" @click="plan = null">收起</button>
          </template>
        </div>
      </div>
    </div>

    <!-- 玻璃琉璃：推荐 + 输入，浮在记录流之上，底下的字从它身后糊过去 -->
    <div ref="dockEl" class="as-dock">
      <!-- 「它在干什么」 -->
      <div v-if="activity" class="as-act">
        <span class="as-dots"><i></i><i></i><i></i></span>
        <span class="as-act-b">
          <span class="as-act-t">{{ activity.what }}</span>
          <span v-if="activity.sub" class="as-act-s">{{ activity.sub }}</span>
        </span>
      </div>

      <!-- 推荐：点了要么直接开跑，要么把话填进输入框 -->
      <div v-if="!draft.trim() && !busy" class="as-recs">
        <button v-for="r in recs" :key="r.label" class="as-rec" :class="{ go: r.fire }" :title="r.text" @click="useRec(r)">
          {{ r.label }}
        </button>
      </div>

      <div class="as-row">
        <textarea
          ref="ta"
          v-model="draft"
          rows="1"
          class="as-ta"
          :placeholder="busy ? '正在生成…（可继续打字，跑完再发）' : `对助手说一句，比如「发一篇${scopeMeta?.name ?? '公众号'}的内容」`"
          @input="autoGrow"
          @keydown.enter.exact.prevent="submit()"
        ></textarea>
        <button v-if="busy" class="as-send stop" title="打断当前生成" @click="stop">■</button>
        <button v-else class="as-send" :disabled="!draft.trim()" title="发送（Enter）" @click="submit()">↑</button>
      </div>
    </div>
  </aside>
</template>

<style scoped>
/* ═══ 助手栏：一块实心白板，浮在内容右侧 ═══ */
.as {
  position: absolute;
  top: 12px; right: 12px; bottom: 12px;
  width: 392px;
  z-index: 30;
  display: flex;
  flex-direction: column;
  min-height: 0;
  border-radius: 14px;
  border: 1px solid var(--line);
  background: var(--panel);
  box-shadow: 0 12px 32px rgba(28, 40, 80, .10);
  overflow: hidden;
}

/* ── 控制范围条 ── */
.as-top {
  flex: none;
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 7px 8px;
  border-bottom: 1px solid var(--line);
}
.as-scope {
  flex: 1;
  min-width: 0;
  display: flex;
  gap: 2px;
  overflow-x: auto;
  scrollbar-width: none;
  /* 右缘渐隐：这排能横着滑，得让人看出来后面还有 */
  mask-image: linear-gradient(90deg, #000 0, #000 calc(100% - 22px), transparent 100%);
  -webkit-mask-image: linear-gradient(90deg, #000 0, #000 calc(100% - 22px), transparent 100%);
}
.as-scope::-webkit-scrollbar { display: none; }
.as-s {
  flex: none;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 6px 11px;
  border-radius: var(--radius-pill);
  border: 0;
  background: transparent;
  color: var(--muted);
  font-family: inherit;
  font-size: var(--text-xs);
  line-height: 1.5;
  white-space: nowrap;
  cursor: pointer;
  transition: background-color var(--dur-fast) var(--ease-out), color var(--dur-fast) var(--ease-out);
}
.as-s:hover { background: var(--card2); color: var(--ink2); }
.as-s.on { color: var(--ink); font-weight: 600; background: var(--card2); }
.as-ic { display: inline-flex; }
/* 没选中的平台徽标去色：十一枚彩色小方块并排就是一排噪点 */
.as-ic :deep(.pi) {
  width: 15px; height: 15px; display: block; border-radius: 4px;
  filter: grayscale(1); opacity: .5;
  transition: filter var(--dur-fast) var(--ease-out), opacity var(--dur-fast) var(--ease-out);
}
.as-s.on .as-ic :deep(.pi) { filter: none; opacity: 1; }
.as-all { color: var(--line-2); font-size: 11px; }
.as-s.on .as-all { color: var(--accent); }
.as-n {
  min-width: 17px; height: 17px; padding: 0 5px;
  border-radius: var(--radius-pill);
  background: var(--accent);
  color: #fff;
  font-size: var(--text-2xs);
  font-variant-numeric: tabular-nums;
  display: inline-flex; align-items: center; justify-content: center;
}
.as-fold {
  flex: none;
  display: inline-flex; align-items: center; justify-content: center;
  width: 34px; height: 34px;
  border: 1px solid var(--line);
  background: var(--panel);
  color: var(--dim);
  cursor: pointer;
  font-family: inherit;
  font-size: 20px;
  line-height: 1;
  border-radius: 10px;
  transition: background-color var(--dur-fast) var(--ease-out), color var(--dur-fast) var(--ease-out);
}
.as-fold:hover { background: var(--card2); color: var(--ink); }

/* ── 记录流 ── */
.as-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 12px 12px 0;
  display: flex;
  flex-direction: column;
  gap: 3px;
  font-size: var(--text-s);
  line-height: 1.75;
  scrollbar-width: thin;
}
/* 会伸缩的顶部垫片：内容少时把记录压到底部，内容溢出时它自己缩成 0。 */
.as-body::before { content: ""; flex: 1 1 0%; }
.as-empty { margin: 0 8px 10px; color: var(--muted); font-size: var(--text-s); line-height: 1.9; }

.as-line {
  display: grid;
  grid-template-columns: 14px minmax(0, 1fr);
  gap: 8px;
  align-items: start;
  padding: 5px 8px;
  border-radius: 8px;
}
.as-gut {
  font-size: var(--text-xs);
  color: var(--line-2);
  line-height: 1.85;
  text-align: center;
  user-select: none;
}
.as-txt { white-space: pre-wrap; word-break: break-word; color: var(--ink2); }
.as-line.user { background: var(--card2); }
.as-line.user .as-gut { color: var(--accent); }
.as-line.user .as-txt { color: var(--ink); }
.as-line.rec .as-txt { color: var(--muted); font-size: var(--text-xs); }
.as-line.rec.warn .as-gut, .as-line.rec.warn .as-txt { color: var(--tag-bad-ink); }
.as-quiet { color: var(--muted); }

/* 跑着的时候：文字自己在流光，不用另开一圈转菊花 */
.as-shine {
  background: linear-gradient(90deg, var(--muted) 18%, var(--ink) 42%, var(--muted) 66%);
  background-size: 220% 100%;
  -webkit-background-clip: text;
  background-clip: text;
  color: transparent;
  animation: asshine 1.9s linear infinite;
}
@keyframes asshine { from { background-position: 220% 0; } to { background-position: -20% 0; } }
.as-caret {
  display: inline-block;
  width: 7px; height: 1em;
  margin-left: 3px;
  vertical-align: -2px;
  border-radius: 1px;
  background: var(--accent);
  animation: asblink 1s steps(2, start) infinite;
}
@keyframes asblink { to { opacity: 0; } }

/* 生成记录行 */
.as-job {
  position: relative;
  display: grid;
  grid-template-columns: 14px minmax(0, 1fr);
  gap: 8px;
  align-items: start;
  width: 100%;
  text-align: left;
  padding: 8px;
  border-radius: 10px;
  border: 1px solid var(--line);
  background: var(--card);
  cursor: pointer;
  font-family: inherit;
  font-size: var(--text-s);
  color: var(--ink2);
  overflow: hidden;
  transition: background-color var(--dur-fast) var(--ease-out), border-color var(--dur-fast) var(--ease-out);
}
.as-job:hover { background: var(--card2); border-color: var(--line-2); }
.as-jbody { display: flex; flex-direction: column; min-width: 0; }
.as-jbody b { font-weight: 600; color: var(--ink); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.as-jmeta { font-size: var(--text-xs); color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.as-jdot {
  width: 8px; height: 8px; margin-top: 7px;
  border-radius: 50%; display: inline-block;
  background: var(--line-2);
}
.as-jdot.ok { background: var(--muted); }
.as-jdot.bad { background: var(--bad); }
.as-jdot.run { background: var(--accent); animation: aspulse 1.6s ease-out infinite; }
@keyframes aspulse {
  0% { box-shadow: 0 0 0 0 rgba(66, 99, 235, .38); }
  70%, 100% { box-shadow: 0 0 0 6px rgba(66, 99, 235, 0); }
}
/* 在跑的那条底下有一道扫过去的光 */
.as-job.run::after {
  content: "";
  position: absolute;
  left: 0; right: 0; bottom: 0;
  height: 2px;
  background: linear-gradient(90deg, transparent, var(--accent), transparent);
  background-size: 42% 100%;
  background-repeat: no-repeat;
  opacity: .8;
  animation: assweep 1.8s ease-in-out infinite;
}
@keyframes assweep { from { background-position: -45% 0; } to { background-position: 145% 0; } }

/* 规划卡 */
.as-plan {
  margin: 8px 2px 2px;
  border: 1px solid var(--line);
  background: var(--card2);
  border-radius: 12px;
  padding: 12px 14px;
}
.as-plan-h { font-size: var(--text-s); font-weight: 600; color: var(--ink); margin-bottom: 7px; }
.as-plan-body { font-size: var(--text-s); line-height: 1.8; white-space: pre-wrap; word-break: break-word; color: var(--ink2); }
.as-plan-act {
  display: flex; align-items: center; gap: 8px; flex-wrap: wrap;
  margin-top: 12px; padding-top: 10px;
  border-top: 1px solid var(--line);
}

/* ═══ 玻璃琉璃：推荐 + 输入 ═══
   浮在记录流之上，记录从它身后虚化着滑过去。里面不再有第二层框
   （输入框透明、无 focus 光环），光都打在同一片玻璃上。 */
.as-dock {
  position: absolute;
  left: 10px; right: 10px; bottom: 10px;
  z-index: 3;
  padding: 10px 10px 10px 16px;
  border-radius: 18px;
  border: 1px solid rgba(255, 255, 255, .6);
  background: linear-gradient(155deg, rgba(255, 255, 255, .72) 0%, rgba(236, 241, 255, .5) 55%, rgba(255, 255, 255, .6) 100%);
  backdrop-filter: blur(30px) saturate(190%);
  -webkit-backdrop-filter: blur(30px) saturate(190%);
  box-shadow:
    0 14px 40px rgba(28, 40, 80, .14),
    0 2px 6px rgba(28, 40, 80, .05),
    inset 0 1px 0 rgba(255, 255, 255, .95),
    inset 0 -1px 0 rgba(255, 255, 255, .35);
  transition: box-shadow var(--dur-base) var(--ease-out), border-color var(--dur-base) var(--ease-out);
}
/* 打字时玻璃自己微微亮起来 */
.as-dock:focus-within {
  border-color: rgba(255, 255, 255, .85);
  box-shadow:
    0 18px 48px rgba(28, 40, 80, .18),
    0 2px 6px rgba(28, 40, 80, .06),
    inset 0 1px 0 rgba(255, 255, 255, 1),
    inset 0 -1px 0 rgba(255, 255, 255, .45);
}

/* 「它在干什么」 */
.as-act {
  display: flex;
  align-items: center;
  gap: 10px;
  margin: 0 6px 8px 0;
  animation: asrise var(--dur-base) var(--ease-out);
}
@keyframes asrise { from { opacity: 0; transform: translateY(6px); } }
.as-dots { flex: none; display: inline-flex; gap: 4px; }
.as-dots i {
  width: 5px; height: 5px;
  border-radius: 50%;
  background: var(--accent);
  animation: asdot 1.15s ease-in-out infinite;
}
.as-dots i:nth-child(2) { animation-delay: .16s; }
.as-dots i:nth-child(3) { animation-delay: .32s; }
@keyframes asdot {
  0%, 100% { opacity: .28; transform: translateY(0); }
  42% { opacity: 1; transform: translateY(-3px); }
}
.as-act-b { flex: 1; min-width: 0; display: flex; flex-direction: column; }
.as-act-t {
  font-size: var(--text-xs);
  line-height: 1.5;
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  background: linear-gradient(90deg, var(--dim) 18%, var(--ink) 42%, var(--dim) 66%);
  background-size: 220% 100%;
  -webkit-background-clip: text;
  background-clip: text;
  color: transparent;
  animation: asshine 1.9s linear infinite;
}
.as-act-s { font-size: var(--text-2xs); color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

/* 推荐 */
.as-recs { display: flex; flex-wrap: wrap; gap: 5px; margin: 0 4px 9px 0; }
.as-rec {
  padding: 3px 11px;
  border-radius: var(--radius-pill);
  border: 1px solid rgba(255, 255, 255, .7);
  background: rgba(255, 255, 255, .42);
  color: var(--dim);
  font-family: inherit;
  font-size: var(--text-xs);
  line-height: 1.7;
  cursor: pointer;
  white-space: nowrap;
  transition: background-color var(--dur-fast) var(--ease-out), color var(--dur-fast) var(--ease-out);
}
.as-rec:hover { background: rgba(255, 255, 255, .9); color: var(--ink); }
/* 点了直接开跑的那几枚，前面点一颗蓝点——跟「填进输入框」的区分开 */
.as-rec.go::before {
  content: "";
  display: inline-block;
  width: 5px; height: 5px;
  margin-right: 6px;
  vertical-align: 1px;
  border-radius: 50%;
  background: var(--accent);
}

/* 输入行 */
.as-row { display: flex; align-items: flex-end; gap: 8px; }
.as-ta {
  flex: 1;
  min-width: 0;
  resize: none;
  max-height: 132px;
  padding: 7px 0;
  border: none;
  background: transparent;
  color: var(--ink);
  font: inherit;
  font-size: var(--text-s);
  line-height: 1.55;
  overflow-y: auto;
}
.as-ta:focus { outline: none; }
.as-ta::placeholder { color: var(--muted); }
.as-send {
  flex: none;
  width: 36px; height: 36px;
  border-radius: 50%;
  border: 1px solid transparent;
  background: var(--accent);
  color: #fff;
  font-family: inherit;
  font-size: 16px;
  cursor: pointer;
  transition: filter var(--dur-fast) var(--ease-out), opacity var(--dur-fast) var(--ease-out);
}
.as-send:hover { filter: brightness(1.08); }
.as-send:disabled { opacity: .3; cursor: not-allowed; }
.as-send.stop { background: var(--bad); }

@media (prefers-reduced-motion: reduce) {
  .as-shine, .as-act-t, .as-dots i, .as-job.run::after, .as-jdot.run, .as-caret { animation: none; }
  .as-shine, .as-act-t { color: var(--ink); background: none; -webkit-text-fill-color: currentColor; }
}

@media (max-width: 1180px) {
  .as { width: 330px; }
}
</style>
