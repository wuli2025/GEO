<script setup lang="ts">
/**
 * 全局 AI 对话坞：常驻运营中心右侧，不再只在流程抽屉里。
 * - 一条泳道一条独立会话：每个媒体门户各自续聊，历史互不串台，存 localStorage。
 * - 泳道锚点：每次发送都带上「当前锚定的泳道」上下文（当前视图 / 门户平台 /
 *   打开着的流程 job）。顶部锚点条常显，让模型每次都知道你此刻站在哪条泳道上。
 */
import { ref, computed, watch, nextTick, onMounted, onBeforeUnmount } from "vue";
import {
  chat, convApi, listen,
  type Message, type ChatStreamEvent,
} from "../../tauri";
import { toast } from "../../composables/useToast";
import { planRequest, clearPlan, type PlanRequest } from "./planBus";
import { openJobDetail } from "./jobsBus";

const props = defineProps<{
  /**
   * 泳道标识：一个媒体门户一条泳道（如 `portal:wechat`），非门户视图统一走 `hub`。
   * 切换泳道 = 换一条独立会话，历史不串台。
   */
  laneKey: string;
  /** 顶部锚点条展示文案，如「知乎门户」「数据看板」。 */
  anchorLabel: string;
  /** 发送时注入模型的锚点上下文（当前视图/平台/job 的简述）。 */
  anchorCtx: string;
}>();
const emit = defineEmits<{ (e: "close"): void }>();

const PROJECT_NAME = "GEO 全局助手";
/** 泳道 → 会话 id 的映射表。 */
const LS_MAP = "geo.globalChat.convs";
/** 旧版全局单会话，首次加载时迁移进 hub 泳道。 */
const LS_LEGACY = "geo.globalChat.conv";

function readMap(): Record<string, string> {
  try {
    const parsed = JSON.parse(localStorage.getItem(LS_MAP) || "{}");
    return parsed && typeof parsed === "object" && !Array.isArray(parsed) ? parsed : {};
  } catch { return {}; }
}
function writeMap(m: Record<string, string>) {
  localStorage.setItem(LS_MAP, JSON.stringify(m));
}
// 迁移：老版本攒下的那条共用会话归到总控泳道，别丢历史。
(function migrateLegacy() {
  const legacy = localStorage.getItem(LS_LEGACY);
  if (!legacy) return;
  const m = readMap();
  if (!m.hub) { m.hub = legacy; writeMap(m); }
  localStorage.removeItem(LS_LEGACY);
})();

const msgs = ref<Message[]>([]);
const draft = ref("");
const sending = ref(false);
/** 正在生成的那条请求属于哪条泳道——切走后不许把流式文本画到别的泳道上。 */
const sendingLane = ref<string | null>(null);
const streamText = ref("");
const streamTool = ref("");
const convId = ref<string | null>(readMap()[props.laneKey] ?? null);
const listEl = ref<HTMLDivElement | null>(null);
/** 各泳道分别保留未发送草稿，切平台时不串稿也不丢稿。 */
const laneDrafts = new Map<string, string>();
let activeDraftLane = props.laneKey;
/** 让较早发出的历史请求不能覆盖较晚的泳道/本地消息。 */
let historyLoadSeq = 0;
let reqId: string | null = null;
let unlisten: (() => void) | null = null;
const streamSubscribed = ref(false);
let disposed = false;

// ── 选题规划预览：门户点「生成→投递」投来的请求，在对话框里流式出规划 + 开始/否决 ──
type PlanPhase = "gen" | "ready" | "failed" | "starting" | "started";
const plan = ref<{ reqId: string; title: string; text: string; phase: PlanPhase; jobId?: string } | null>(null);
/** 规划预览走独立的 chat_send（不落库），单独跟踪它的 reqId。 */
let planStreamReqId: string | null = null;
/** 使较早、较慢返回的 chat_send 不能夺走新规划的流归属。 */
let planStartSeq = 0;
/** 已接过的 planBus 请求 id，避免重复触发。 */
const handledPlanId = ref<string | null>(null);
/** 规划生成/排产中时，本泳道普通发送先让路。 */
const planBusy = computed(() => plan.value?.phase === "gen" || plan.value?.phase === "starting");

/** 当前泳道就是正在生成的那条泳道时，才渲染流式气泡。 */
const streamHere = computed(() => sending.value && sendingLane.value === props.laneKey);
/** 别的泳道占着生成中，本泳道暂时不能发（全局只跟踪一个 reqId）。 */
const busyElsewhere = computed(() => sending.value && sendingLane.value !== props.laneKey);

async function ensureConv(lane: string, anchorLabel: string): Promise<string> {
  const mapped = readMap()[lane];
  if (mapped) {
    if (props.laneKey === lane) convId.value = mapped;
    return mapped;
  }
  const projects = await convApi.listProjects();
  const proj = projects.find((p) => p.name === PROJECT_NAME && !p.archived)
    ?? (await convApi.createProject(PROJECT_NAME));
  const conv = await convApi.createConversation(proj.id);
  await convApi.renameConversation(conv.id, `GEO 运营助手 · ${anchorLabel}`).catch(() => {});
  const m = readMap();
  // 只写回发起发送时的泳道；异步创建期间用户可能已切到另一平台。
  m[lane] = conv.id;
  writeMap(m);
  if (props.laneKey === lane) convId.value = conv.id;
  return conv.id;
}

async function loadHistory(lane = props.laneKey, id = convId.value) {
  const seq = ++historyLoadSeq;
  if (!id) {
    if (props.laneKey === lane && seq === historyLoadSeq) msgs.value = [];
    return;
  }
  try {
    const loaded = (await convApi.getMessages(id)).filter((m) => m.role !== "tool");
    if (seq !== historyLoadSeq || props.laneKey !== lane || convId.value !== id) return;
    msgs.value = loaded;
    scrollBottom();
  } catch {
    if (seq === historyLoadSeq && props.laneKey === lane && convId.value === id) msgs.value = [];
  }
}

async function scrollBottom() {
  await nextTick();
  if (listEl.value) listEl.value.scrollTop = listEl.value.scrollHeight;
}

function ctxBlock(anchorLabel: string, anchorCtx: string): string {
  return [
    "【泳道锚点 · 系统自动注入，无需向用户复述】",
    `你现在站在：${anchorLabel}`,
    anchorCtx,
    "你是 GEO 自媒体运营中心的运营助手。基于用户当前所在的泳道/视图回答与协作。",
    "每个媒体是一条独立泳道，本会话只服务这一条——不要把别的平台的选题、口径、历史混进来。",
    "────",
  ].filter(Boolean).join("\n");
}

// ── 选题规划预览 ──
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
    // 不传 conversationId：规划是临时预览，不落库、不污染本泳道历史。
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

/** 泳道切到有待处理规划的那条时接住它。 */
function maybePickupPlan() {
  if (!streamSubscribed.value) return;
  const req = planRequest.value;
  if (req && req.laneKey === props.laneKey && handledPlanId.value !== req.id) {
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
    clearPlan(p.reqId);
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

async function send() {
  const text = draft.value.trim();
  if (!text || sending.value || planBusy.value || !streamSubscribed.value) return;
  const lane = props.laneKey;
  const anchorLabel = props.anchorLabel;
  const anchorCtx = props.anchorCtx;
  sending.value = true;
  sendingLane.value = lane;
  streamText.value = ""; streamTool.value = "";
  try {
    const id = await ensureConv(lane, anchorLabel);
    // 任何在途历史读取都不得覆盖这条刚发送的本地消息。
    historyLoadSeq += 1;
    if (props.laneKey === lane) {
      msgs.value.push({
        id: `local-${Date.now()}`, conversationId: id,
        role: "user", content: text, createdAt: Math.floor(Date.now() / 1000),
      });
      draft.value = "";
      scrollBottom();
    }
    laneDrafts.set(lane, "");
    reqId = await chat.send({
      prompt: `${ctxBlock(anchorLabel, anchorCtx)}\n${text}`,
      permissionMode: "auto_current",
      conversationId: id,
      workMode: "office",
    });
  } catch (e: any) {
    sending.value = false;
    sendingLane.value = null;
    toast.error(e?.message ?? String(e));
  }
}

async function stop() {
  if (reqId) { try { await chat.cancel(reqId); } catch { /* 已结束则忽略 */ } }
}

async function finish() {
  const lane = sendingLane.value;
  const finishedConvId = lane ? readMap()[lane] ?? null : null;
  sending.value = false;
  sendingLane.value = null;
  reqId = null;
  streamTool.value = "";
  // 在 await 历史读取前清掉旧流；否则用户立刻发下一条时，旧 finish 返回后会误清新流。
  streamText.value = "";
  // 切走了就别覆盖当前泳道的消息——后端已落库，切回来时 loadHistory 会捞到。
  if (lane && lane === props.laneKey && finishedConvId) await loadHistory(lane, finishedConvId);
  scrollBottom();
}

function newChat() {
  if (sending.value) return;
  const m = readMap();
  delete m[props.laneKey];
  writeMap(m);
  convId.value = null;
  historyLoadSeq += 1;
  msgs.value = [];
  streamText.value = "";
}

// 换泳道 = 换会话：把上一条泳道的消息与流式残留清干净再载入新的。
watch(() => props.laneKey, async (lane) => {
  laneDrafts.set(activeDraftLane, draft.value);
  activeDraftLane = lane;
  draft.value = laneDrafts.get(lane) ?? "";
  convId.value = readMap()[lane] ?? null;
  msgs.value = [];
  if (sendingLane.value !== lane) { streamText.value = ""; streamTool.value = ""; }
  // 规划卡只属于它自己那条泳道：切走就收起（未定夺的请求留在总线，切回来再接）。
  if (plan.value && planRequest.value?.laneKey !== lane) {
    if (plan.value.phase === "gen") stopPlan();
    plan.value = null;
    handledPlanId.value = null;
  }
  await loadHistory();
  maybePickupPlan();
});

// 门户投来新的规划请求（且属于本泳道）→ 在对话框里接住。
watch(planRequest, (req) => {
  if (!streamSubscribed.value || !req || req.laneKey !== props.laneKey || handledPlanId.value === req.id) return;
  handledPlanId.value = req.id;
  startPlan(req);
});

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
  await loadHistory();
  maybePickupPlan();
});
onBeforeUnmount(() => {
  disposed = true;
  streamSubscribed.value = false;
  historyLoadSeq += 1;
  planStartSeq += 1;
  if (planStreamReqId) chat.cancel(planStreamReqId).catch(() => {});
  unlisten?.();
});
</script>

<template>
  <aside class="gchat">
    <div class="gchat-h">
      <span class="gchat-title">💬 运营助手</span>
      <span class="gchat-anchor" :title="`当前泳道锚点：${anchorLabel}`">⚓ {{ anchorLabel }}</span>
      <span class="gchat-sp"></span>
      <button class="xbtn" title="开新对话" @click="newChat">＋</button>
      <button class="xbtn" title="收起" @click="emit('close')">✕</button>
    </div>
    <div ref="listEl" class="gchat-list">
      <p v-if="!msgs.length && !streamHere && !plan" class="foot gchat-hint">
        这是「{{ anchorLabel }}」这条泳道的独立会话，历史与其它媒体互不相通。
      </p>
      <div v-for="mm in msgs" :key="mm.id" class="gchat-msg" :class="mm.role">
        <div class="gchat-bubble">{{ mm.content }}</div>
      </div>
      <div v-if="streamHere" class="gchat-msg assistant">
        <div class="gchat-bubble">
          <template v-if="streamText">{{ streamText }}</template>
          <span v-else class="foot"><span class="spin">◔</span> 思考中…</span>
          <div v-if="streamTool" class="foot gchat-tool">🔧 {{ streamTool }}</div>
        </div>
      </div>
      <!-- 选题规划预览卡：先看怎么规划，再点开始 / 否决 -->
      <div v-if="plan" class="gchat-plan">
        <div class="gchat-plan-h">📋 选题规划 ·《{{ plan.title }}》</div>
        <div class="gchat-plan-body">
          <template v-if="plan.text">{{ plan.text }}</template>
          <span v-else-if="plan.phase === 'gen'" class="foot"><span class="spin">◔</span> 正在规划这篇怎么写…</span>
          <span v-else-if="plan.phase === 'failed'" class="foot">规划生成失败，可重试或否决。</span>
        </div>
        <div class="gchat-plan-act">
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
            <span class="foot"><span class="spin">◔</span> 排产启动中…</span>
          </template>
          <template v-else>
            <span class="foot">已开始跑流水线。</span>
            <button v-if="plan.jobId" class="btn sm ghost" @click="plan.jobId && openJobDetail(plan.jobId)">看流程 →</button>
            <button class="btn sm ghost" @click="plan = null">收起</button>
          </template>
        </div>
      </div>
    </div>
    <div class="gchat-input">
      <textarea
        v-model="draft"
        rows="2"
        :placeholder="!streamSubscribed ? '正在连接对话流…' : planBusy ? '选题规划生成中…' : busyElsewhere ? '另一条泳道正在生成中…' : '说点什么…（Enter 发送，Shift+Enter 换行）'"
        @keydown.enter.exact.prevent="send"
      ></textarea>
      <button v-if="streamHere" class="btn sm danger" @click="stop">停止</button>
      <button v-else class="btn sm" :disabled="!streamSubscribed || !draft.trim() || busyElsewhere || planBusy" @click="send">发送</button>
    </div>
  </aside>
</template>

<style scoped>
.gchat {
  flex: none;
  width: 340px;
  display: flex;
  flex-direction: column;
  min-height: 0;
  background: var(--panel);
  border-left: 1px solid var(--line);
}
.gchat-h {
  flex: none;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 12px;
  border-bottom: 1px solid var(--line);
}
.gchat-title { font-size: var(--text-s); font-weight: 600; white-space: nowrap; }
.gchat-anchor {
  display: inline-flex;
  align-items: center;
  max-width: 150px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: var(--text-2xs);
  color: var(--accent-ink);
  background: var(--accent-soft);
  border: 1px solid var(--line-2);
  border-radius: var(--radius-pill);
  padding: 2px 8px;
}
.gchat-sp { flex: 1; }
.xbtn {
  flex: none;
  width: 24px; height: 24px;
  border: none; background: transparent;
  color: var(--muted); cursor: pointer; font-family: inherit;
  border-radius: var(--radius-ctl);
}
.xbtn:hover { background: var(--card2); color: var(--ink); }
.gchat-list { flex: 1; min-height: 0; overflow-y: auto; display: flex; flex-direction: column; gap: 10px; padding: 12px; }
.gchat-hint { margin: auto 4px; text-align: center; }
.gchat-msg { display: flex; }
.gchat-msg.user { justify-content: flex-end; }
.gchat-bubble {
  max-width: 88%;
  padding: 8px 12px;
  border-radius: 12px;
  font-size: var(--text-s);
  line-height: 1.6;
  white-space: pre-wrap;
  word-break: break-word;
  background: var(--card2);
  border: 1px solid var(--line);
  color: var(--ink2);
}
.gchat-msg.user .gchat-bubble { background: var(--accent-soft); border-color: var(--line-2); }
.gchat-tool { margin-top: 6px; }
.gchat-plan {
  border: 1px solid var(--line-2);
  background: var(--accent-soft);
  border-radius: 12px;
  padding: 10px 12px;
}
.gchat-plan-h { font-size: var(--text-s); font-weight: 600; color: var(--accent-ink); margin-bottom: 6px; }
.gchat-plan-body {
  font-size: var(--text-s);
  line-height: 1.6;
  white-space: pre-wrap;
  word-break: break-word;
  color: var(--ink2);
}
.gchat-plan-act {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
  margin-top: 10px;
  padding-top: 8px;
  border-top: 1px solid var(--line-2);
}
.gchat-input { flex: none; display: flex; align-items: flex-end; gap: 8px; padding: 10px 12px; border-top: 1px solid var(--line); }
.gchat-input textarea {
  flex: 1;
  resize: none;
  background: var(--card2);
  border: 1px solid var(--line);
  border-radius: 10px;
  padding: 8px 10px;
  font: inherit;
  font-size: var(--text-s);
  color: var(--ink2);
}
.gchat-input textarea:focus { outline: 1px solid var(--line-2); }
</style>
