<script setup lang="ts">
/**
 * 流程对话：这条 job 的**全过程**和人的对话汇成同一条流——
 * 步骤事件（开始/完成/失败）按时间插进来，点开就地展开这一步的运行日志，
 * 模型回复流式往下淌。人在最下面接着说，像正常聊天一样。
 *
 * - 每条 job 绑一个内核会话（jobId → conversationId 存 localStorage，可续聊）
 * - 每次发送自动注入该 job 的实时快照（状态/步骤/日志尾部/文件路径），
 *   所以问「现在卡在哪一步」「为什么失败」得到的是真实发生的东西
 */
import { ref, computed, watch, nextTick, onMounted, onBeforeUnmount } from "vue";
import {
  chat, convApi, listen,
  type MediaJob, type Message, type ChatStreamEvent,
} from "../../tauri";
import { toast } from "../../composables/useToast";
import JobWorkflow from "./JobWorkflow.vue";
import { parseWorkflow, type WfEvent } from "./jobLog";

const props = defineProps<{
  job: MediaJob;
  log: string;
  /** 这条 job 开跑那天的零点（秒）——工作流日志只有时分秒，靠它归到同一条时间轴上 */
  dayStart: number;
}>();

const PROJECT_NAME = "GEO 流程对话";
const LS_KEY = "geo.jobChat.convs";
/** 注入块与人说的话之间的分界；渲染时要把注入部分剥掉。 */
const CTX_END = "\n────\n";

const msgs = ref<Message[]>([]);
const draft = ref("");
const sending = ref(false);
const streamText = ref("");
const streamTool = ref("");
const convId = ref<string | null>(null);
const listEl = ref<HTMLDivElement | null>(null);
let reqId: string | null = null;
let unlisten: (() => void) | null = null;

const STEP_TEXT: Record<string, string> = { run: "进行中", ok: "完成", fail: "失败", skip: "跳过" };
const STEP_DOT: Record<string, string> = { run: "warn", ok: "ok", fail: "bad", skip: "idle" };

function convMap(): Record<string, string> {
  try { return JSON.parse(localStorage.getItem(LS_KEY) || "{}"); } catch { return {}; }
}
function rememberConv(jobId: string, id: string) {
  const m = convMap(); m[jobId] = id;
  localStorage.setItem(LS_KEY, JSON.stringify(m));
}

/** 用户消息落库的是「注入块 + 人说的话」，直接渲染会把脚手架摊给人看。 */
function displayText(m: Message): string {
  if (m.role !== "user") return m.content;
  let t = m.content;
  if (t.startsWith("【")) {
    const i = t.indexOf(CTX_END);
    if (i >= 0) t = t.slice(i + CTX_END.length);
  }
  return t.trim() || m.content;
}

/* ── 时间轴汇流：步骤节点 + 后端工作流 + 对话消息，一条流按时间往下淌 ── */
type Row =
  | { k: "step"; id: string; ts: number; label: string; status: string; detail?: string }
  | { k: "wf"; id: string; ts: number; events: WfEvent[] }
  | { k: "msg"; id: string; ts: number; role: string; text: string };

const rows = computed<Row[]>(() => {
  const out: Row[] = [];
  for (const s of props.job.steps ?? []) {
    out.push({
      k: "step", id: `s-${s.key}`, ts: s.startedAt || s.at,
      label: s.label, status: s.status, detail: s.detail,
    });
  }
  parseWorkflow(props.log, props.dayStart).forEach((e, i) => {
    out.push({ k: "wf", id: `w-${i}`, ts: e.abs, events: [e] });
  });
  for (const m of msgs.value) {
    out.push({ k: "msg", id: `m-${m.id}`, ts: m.createdAt || 0, role: m.role, text: displayText(m) });
  }
  // 同一秒里：步骤节点在前，然后是它做的事，最后才是人的追问
  const rank = (r: Row) => (r.k === "step" ? 0 : r.k === "wf" ? 1 : 2);
  out.sort((a, b) => a.ts - b.ts || rank(a) - rank(b));
  // 连着的工作流事件并成一段，省得每条都单独占一块
  const merged: Row[] = [];
  for (const r of out) {
    const last = merged[merged.length - 1];
    if (r.k === "wf" && last?.k === "wf") last.events.push(...r.events);
    else merged.push(r);
  }
  return merged;
});

function hhmm(ts: number): string {
  if (!ts) return "";
  const d = new Date(ts * 1000);
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

async function ensureConv(): Promise<string> {
  if (convId.value) return convId.value;
  const known = convMap()[props.job.id];
  if (known) { convId.value = known; return known; }
  const projects = await convApi.listProjects();
  const proj = projects.find((p) => p.name === PROJECT_NAME && !p.archived)
    ?? (await convApi.createProject(PROJECT_NAME));
  const conv = await convApi.createConversation(proj.id);
  await convApi.renameConversation(conv.id, `流程 ${props.job.id.slice(0, 8)} ·《${props.job.title}》`).catch(() => {});
  rememberConv(props.job.id, conv.id);
  convId.value = conv.id;
  return conv.id;
}

async function loadHistory() {
  const known = convMap()[props.job.id];
  if (!known) { msgs.value = []; return; }
  convId.value = known;
  try {
    msgs.value = (await convApi.getMessages(known)).filter((m) => m.role !== "tool");
    scrollBottom();
  } catch { msgs.value = []; }
}

function atBottom(): boolean {
  const el = listEl.value;
  return !el || el.scrollTop + el.clientHeight >= el.scrollHeight - 40;
}
async function scrollBottom() {
  await nextTick();
  if (listEl.value) listEl.value.scrollTop = listEl.value.scrollHeight;
}

/** 发送前注入的实时快照：模型看到的就是这条泳道真实发生的东西 */
function ctxBlock(): string {
  const j = props.job;
  const steps = j.steps.map((s) =>
    `- ${s.label}[${s.status}]${s.expertName ? ` 专家:${s.expertName}` : ""}${s.detail ? ` — ${s.detail}` : ""}`
  ).join("\n");
  const tail = props.log ? props.log.split("\n").slice(-40).join("\n") : "（暂无日志）";
  return [
    "【流程实时快照 · 系统自动注入，无需向用户复述】",
    `job ${j.id} · 平台 ${j.platform} ·《${j.title}》 · 状态 ${j.status} · 阶段 ${j.stage}`,
    j.topic ? `选题方向：${j.topic}` : "",
    j.error ? `失败原因：${j.error}` : "",
    `步骤时间线：\n${steps || "（还没有步骤）"}`,
    `日志文件：${j.logPath}`,
    j.articlePath ? `产物文件：${j.articlePath}` : "",
    `最近日志尾部：\n${tail}`,
    "你是 GEO 运营中心的流程管家。基于以上真实状态回答；需要更多细节可直接读上述文件路径。",
    "────",
  ].filter(Boolean).join("\n");
}

async function send() {
  const text = draft.value.trim();
  if (!text || sending.value) return;
  sending.value = true;
  streamText.value = ""; streamTool.value = "";
  try {
    const id = await ensureConv();
    msgs.value.push({
      id: `local-${msgs.value.length}`, conversationId: id,
      role: "user", content: text, createdAt: Math.floor(Date.now() / 1000),
    });
    draft.value = "";
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
  }
}

async function stop() {
  if (reqId) { try { await chat.cancel(reqId); } catch { /* 已结束则忽略 */ } }
}

async function finish() {
  sending.value = false;
  reqId = null;
  streamTool.value = "";
  if (convId.value) {
    try {
      msgs.value = (await convApi.getMessages(convId.value)).filter((m) => m.role !== "tool");
    } catch { /* 保留本地流式文本兜底 */ }
  }
  streamText.value = "";
  scrollBottom();
}

onMounted(async () => {
  await loadHistory();
  unlisten = await listen<ChatStreamEvent>("chat:stream", (ev) => {
    if (!reqId || ev.reqId !== reqId) return;
    if (ev.kind === "delta" && ev.text) { streamText.value += ev.text; scrollBottom(); }
    else if (ev.kind === "tool") streamTool.value = ev.tool || ev.text || "";
    else if (ev.kind === "error") { toast.error(ev.text || "对话出错"); finish(); }
    else if (ev.kind === "done") finish();
  });
});
onBeforeUnmount(() => { unlisten?.(); });

watch(() => props.job.id, () => { convId.value = null; streamText.value = ""; loadHistory(); });
// 跑着的时候新事件/新日志自己往下走——但人往回翻看历史时不抢滚动条。
watch(() => [props.job.updatedAt, props.log.length], () => { if (atBottom()) scrollBottom(); });
</script>

<template>
  <div class="jc">
    <div ref="listEl" class="jc-list">
      <p v-if="!rows.length" class="jc-hint">
        这条流程的每一步都会落在这里。也可以直接问，比如「现在卡在哪一步」「为什么失败了」。
      </p>

      <template v-for="r in rows" :key="r.id">
        <!-- 步骤节点：流里的小标题 -->
        <div v-if="r.k === 'step'" class="evt" :class="r.status">
          <div class="evt-h">
            <span class="sdot" :class="STEP_DOT[r.status] || 'idle'"></span>
            <span class="evt-l">{{ r.label }}</span>
            <span class="evt-s">{{ STEP_TEXT[r.status] || r.status }}</span>
            <span class="evt-t">{{ hhmm(r.ts) }}</span>
          </div>
          <p v-if="r.detail" class="evt-d">{{ r.detail }}</p>
        </div>

        <!-- 后端在这段时间里干的事 -->
        <JobWorkflow v-else-if="r.k === 'wf'" :events="r.events" />

        <!-- 人说的 -->
        <div v-else-if="r.role === 'user'" class="me"><span>{{ r.text }}</span></div>
        <!-- 模型回的：不套气泡，正常一段话 -->
        <div v-else class="ai">{{ r.text }}</div>
      </template>

      <div v-if="sending" class="ai">
        <template v-if="streamText">{{ streamText }}<span class="caret">▍</span></template>
        <span v-else class="jc-hint inline"><span class="spin">◔</span> 思考中…</span>
        <div v-if="streamTool" class="ai-tool">{{ streamTool }}</div>
      </div>
    </div>

    <div class="jc-input">
      <textarea
        v-model="draft"
        rows="2"
        placeholder="对这条流程说点什么…（Enter 发送，Shift+Enter 换行）"
        @keydown.enter.exact.prevent="send"
      ></textarea>
      <button v-if="sending" class="btn sm danger" @click="stop">停止</button>
      <button v-else class="btn sm" :disabled="!draft.trim()" @click="send">发送</button>
    </div>
  </div>
</template>

<style scoped>
.jc { flex: 1; min-height: 0; display: flex; flex-direction: column; }
.jc-list {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 14px 16px;
  scrollbar-width: thin;
}
.jc-hint { color: var(--muted); font-size: var(--text-xs); line-height: 1.75; margin: 0; }
.jc-hint.inline { display: inline; }

/* 步骤节点 */
.evt { display: flex; flex-direction: column; gap: 4px; }
.evt-h {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 5px 8px;
  border: none;
  border-radius: 9px;
  background: transparent;
  cursor: pointer;
  font-family: inherit;
  text-align: left;
  transition: background-color var(--dur-fast) var(--ease-out);
}
.evt-h:hover { background: rgba(255, 255, 255, .6); }
.evt-l { flex: 0 1 auto; min-width: 0; font-size: var(--text-xs); color: var(--ink2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.evt-s { flex: none; font-size: var(--text-2xs); color: var(--muted); white-space: nowrap; }
.evt.fail .evt-s { color: var(--bad); }
.evt.run .evt-s { color: var(--warn); }
.evt-t { flex: none; margin-left: auto; padding-left: 6px; font-size: var(--text-2xs); color: var(--muted); font-variant-numeric: tabular-nums; }
.evt-d { margin: 0 8px; font-size: var(--text-2xs); color: var(--muted); line-height: 1.6; word-break: break-word; }
/* 后端工作流：左边一道细线，看得出是挂在上面那个步骤节点下面的 */
.jc-list > :deep(.wf) {
  margin: -4px 0 0 9px;
  padding-left: 12px;
  border-left: 1px solid rgba(120, 130, 165, .2);
}

/* 人说的：右侧一颗浅色气泡 */
.me { display: flex; justify-content: flex-end; }
.me > span {
  max-width: 86%;
  padding: 8px 12px;
  border-radius: 14px 14px 4px 14px;
  background: rgba(255, 255, 255, .92);
  border: 1px solid rgba(255, 255, 255, .95);
  box-shadow: 0 1px 3px rgba(20, 30, 62, .07);
  font-size: var(--text-s);
  line-height: 1.7;
  color: var(--ink);
  white-space: pre-wrap;
  word-break: break-word;
}
/* 模型回的：不套壳，直接是一段话 */
.ai {
  font-size: var(--text-s);
  line-height: 1.8;
  color: var(--ink2);
  white-space: pre-wrap;
  word-break: break-word;
  padding: 0 2px;
}
.ai-tool { margin-top: 6px; font-size: var(--text-2xs); color: var(--muted); }
.caret { color: var(--accent); animation: jcblink 1.1s steps(2, start) infinite; }
@keyframes jcblink { to { opacity: 0; } }

/* 输入 */
.jc-input {
  flex: none;
  display: flex;
  align-items: flex-end;
  gap: 8px;
  padding: 10px 14px 12px;
  border-top: 1px solid rgba(255, 255, 255, .6);
  background: linear-gradient(0deg, rgba(255, 255, 255, .5), rgba(255, 255, 255, .1));
}
.jc-input textarea {
  flex: 1;
  resize: none;
  background: rgba(255, 255, 255, .8);
  border: 1px solid rgba(255, 255, 255, .95);
  border-radius: 12px;
  box-shadow: inset 0 1px 3px rgba(20, 30, 62, .05);
  padding: 8px 11px;
  font: inherit;
  font-size: var(--text-s);
  color: var(--ink2);
}
.jc-input textarea:focus { outline: 1px solid var(--line-2); }
</style>
