<script setup lang="ts">
/**
 * 流程对话：这条 job 的**全过程**和人的对话汇成同一条流，长得像正常的聊天——
 * 后端每做完一件事就「发一条消息」（步骤名 + 结果 + 这一步里干的每个动作），
 * 一部分一部分地淌下来；人在最下面接着说，模型回复流式续上。
 *
 * - 一步 = 一条消息：后端工作流日志按时间归到它所属的那一步里，不再和步骤节点各排各的
 * - 在跑/失败的那步默认摊开过程，跑完的折成一行——像终端里正在执行的任务
 * - 每条 job 绑一个内核会话（jobId → conversationId 存 localStorage，可续聊）
 * - 每次发送自动注入该 job 的实时快照（状态/步骤/日志尾部/文件路径），
 *   所以问「现在卡在哪一步」「为什么失败」得到的是真实发生的东西
 */
import { ref, computed, watch, nextTick, onMounted, onBeforeUnmount } from "vue";
import {
  chat, convApi, listen,
  type MediaJob, type Message, type ChatStreamEvent, type AttachedFile,
} from "../../tauri";
import { toast } from "../../composables/useToast";
import { useFileDrop, type DropPayload } from "../../composables/useFileDrop";
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

/* ── 汇流：一步 = 一条消息 ───────────────────────────────────────────
   原来步骤节点和后端日志是两路事件各排各的，排到最后就成了「先一串步骤，
   再一大坨日志」。现在按时间把每条日志归给它所属的那一步——一步一条消息，
   点开就是这一步里 claude 和脚本干的每件事。                        */
type ActRow = {
  k: "act"; id: string; ts: number;
  label: string; status: string; detail?: string; events: WfEvent[];
};
type MsgRow = { k: "msg"; id: string; ts: number; role: string; text: string };
type Row = ActRow | MsgRow;

const acts = computed<ActRow[]>(() => {
  const steps: ActRow[] = (props.job.steps ?? [])
    .map<ActRow>((s) => ({
      k: "act", id: `s-${s.key}`, ts: s.startedAt || s.at || 0,
      label: s.label, status: s.status, detail: s.detail, events: [],
    }))
    .sort((a, b) => a.ts - b.ts);

  // 日志事件按时间归到「最后一条不晚于它的步骤」；第一步之前的那几行自成一条前导消息
  const lead: WfEvent[] = [];
  let i = -1;
  for (const e of parseWorkflow(props.log, props.dayStart)) {
    while (i + 1 < steps.length && steps[i + 1].ts <= e.abs) i += 1;
    if (i < 0) lead.push(e);
    else steps[i].events.push(e);
  }
  if (!lead.length) return steps;
  return [
    { k: "act", id: "w-lead", ts: lead[0].abs, label: "流水线开跑", status: "ok", events: lead },
    ...steps,
  ];
});

const rows = computed<Row[]>(() => {
  const out: Row[] = [...acts.value];
  for (const m of msgs.value) {
    out.push({ k: "msg", id: `m-${m.id}`, ts: m.createdAt || 0, role: m.role, text: displayText(m) });
  }
  // 同一秒里：先是流水线做的事，再是人的追问
  out.sort((a, b) => a.ts - b.ts || (a.k === "act" ? 0 : 1) - (b.k === "act" ? 0 : 1));
  return out;
});

/** 机器这一侧：流水线消息和模型回复共用左边那条身份线 */
function aiSide(r?: Row): boolean {
  return !!r && (r.k === "act" || r.role !== "user");
}
/** 连着的机器消息只在最上面挂一次头像——一个人一口气说了好几段，不是好几个人 */
function showAvatar(i: number): boolean {
  return aiSide(rows.value[i]) && !aiSide(rows.value[i - 1]);
}

/* 过程默认折着；在跑的、失败的那步自动摊开——正在执行的任务本来就该看得见。 */
const actOpen = ref<Record<string, boolean>>({});
function isActOpen(r: ActRow): boolean {
  return r.id in actOpen.value ? actOpen.value[r.id] : r.status === "run" || r.status === "fail";
}
function toggleAct(r: ActRow) {
  actOpen.value = { ...actOpen.value, [r.id]: !isActOpen(r) };
}

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

/* ── 拖入上传：整块面板都收，落进这条 job 的会话 uploads 目录 ─────────
   桌面端 HTML5 的 drop 拿不到文件本体，统一走 useFileDrop 拿绝对路径。
   典型用法：把参考稿 / 甲方要求 / 一张要配的图拖进来，接着说「按这个改」。 */
const jcEl = ref<HTMLElement | null>(null);
const attached = ref<AttachedFile[]>([]);
const attaching = ref(false);

async function takeFiles(p: DropPayload) {
  if (!p.paths.length) {
    if (p.files.length) toast.error("这个形态下拿不到文件路径——请在桌面应用或已连后端的浏览器里拖");
    return;
  }
  attaching.value = true;
  try {
    const id = await ensureConv();
    const rows = await chat.attachFiles(id, p.paths);
    for (const r of rows.filter((x) => !x.ok)) toast.error(`「${r.name}」没收下：${r.error ?? "未知原因"}`);
    attached.value.push(...rows.filter((x) => x.ok));
  } catch (e: any) {
    toast.error(e?.message ?? String(e));
  } finally {
    attaching.value = false;
  }
}
const { over: dropOver } = useFileDrop(jcEl, takeFiles, { disabled: attaching });
function dropAttach(i: number) {
  attached.value.splice(i, 1); // 只是不带它了，文件仍在 uploads 目录
}

/** 发送前注入的实时快照：模型看到的就是这条泳道真实发生的东西 */
function ctxBlock(files: AttachedFile[] = []): string {
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
    // 附件只给路径：几十兆的资料塞进提示词是自找截断，让它按需 Read
    ...(files.length
      ? ["本条附件（已存在本机，用 Read 工具按需读取）：",
         ...files.map((f) => `- ${f.name}（${f.kind}）：${f.path}`)]
      : []),
    "你是 GEO 运营中心的流程管家。基于以上真实状态回答；需要更多细节可直接读上述文件路径。",
    "────",
  ].filter(Boolean).join("\n");
}

async function send() {
  const text = draft.value.trim();
  if (!text || sending.value) return;
  sending.value = true;
  streamText.value = ""; streamTool.value = "";
  // 这条带走当前挂着的附件；发出去才清空，发失败还留着可重发
  const files = attached.value.slice();
  const note = files.length ? `\n\n（附件：${files.map((f) => f.name).join("、")}）` : "";
  try {
    const id = await ensureConv();
    msgs.value.push({
      id: `local-${msgs.value.length}`, conversationId: id,
      role: "user", content: `${text}${note}`, createdAt: Math.floor(Date.now() / 1000),
    });
    draft.value = "";
    scrollBottom();
    reqId = await chat.send({
      prompt: `${ctxBlock(files)}\n${text}${note}`,
      permissionMode: "auto_current",
      conversationId: id,
      workMode: "office",
    });
    attached.value = attached.value.filter((a) => !files.includes(a));
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
  <div ref="jcEl" class="jc">
    <div ref="listEl" class="jc-list">
      <p v-if="!rows.length" class="jc-hint">
        这条流程的每一步都会在这儿发一条消息。也可以直接问，比如「现在卡在哪一步」「为什么失败了」。
      </p>

      <template v-for="(r, i) in rows" :key="r.id">
        <!-- 人说的：右侧气泡 -->
        <div v-if="r.k === 'msg' && r.role === 'user'" class="row me">
          <div class="bub mine">{{ r.text }}</div>
        </div>

        <!-- 机器这一侧：流水线做完一件事发一条，模型回复也是一条 -->
        <div v-else class="row ai">
          <div class="av" :class="{ hide: !showAvatar(i) }">◈</div>
          <div class="bcol">
            <!-- 流水线消息：标题一行 + 结论一行 + 折起来的过程 -->
            <template v-if="r.k === 'act'">
              <div class="bub bot" :class="r.status">
                <div class="bh">
                  <span class="sdot" :class="STEP_DOT[r.status] || 'idle'"></span>
                  <b class="bl">{{ r.label }}</b>
                  <span class="bs">{{ STEP_TEXT[r.status] || r.status }}</span>
                  <span class="bt">{{ hhmm(r.ts) }}</span>
                </div>
                <p v-if="r.detail" class="bd">{{ r.detail }}</p>
                <div v-if="r.events.length" class="proc" :class="{ open: isActOpen(r) }">
                  <button class="proc-h" @click="toggleAct(r)">
                    <span class="proc-c">{{ isActOpen(r) ? "▾" : "▸" }}</span>
                    {{ isActOpen(r) ? "收起过程" : `过程 ${r.events.length} 条` }}
                  </button>
                  <JobWorkflow v-if="isActOpen(r)" :events="r.events" />
                </div>
                <div v-if="r.status === 'run'" class="dots"><i></i><i></i><i></i></div>
              </div>
            </template>
            <!-- 模型回的 -->
            <div v-else class="bub bot say">{{ r.text }}</div>
          </div>
        </div>
      </template>

      <!-- 正在回：打字气泡 -->
      <div v-if="sending" class="row ai">
        <div class="av" :class="{ hide: aiSide(rows[rows.length - 1]) }">◈</div>
        <div class="bcol">
          <div class="bub bot say">
            <template v-if="streamText">{{ streamText }}<span class="caret">▍</span></template>
            <span v-else class="dots"><i></i><i></i><i></i></span>
            <div v-if="streamTool" class="ai-tool">{{ streamTool }}</div>
          </div>
        </div>
      </div>
    </div>

    <!-- 挂着的附件：跟着下一条消息一起走 -->
    <div v-if="attached.length || attaching" class="jc-atts">
      <span v-if="attaching" class="jc-att">收下文件中…</span>
      <button
        v-for="(a, i) in attached" :key="a.path" class="jc-att"
        :title="`${a.path}（点击取消携带）`" @click="dropAttach(i)"
      >📎 {{ a.name }} ✕</button>
    </div>

    <div class="jc-input">
      <textarea
        v-model="draft"
        rows="2"
        placeholder="对这条流程说点什么…（Enter 发送，Shift+Enter 换行；文件可直接拖进来）"
        @keydown.enter.exact.prevent="send"
      ></textarea>
      <button v-if="sending" class="btn sm danger" @click="stop">停止</button>
      <button v-else class="btn sm" :disabled="!draft.trim()" @click="send">发送</button>
    </div>

    <div v-if="dropOver" class="jc-drop">松手，把文件交给这条流程</div>
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
  padding: 16px 16px 18px;
  scrollbar-width: thin;
}
.jc-hint { color: var(--muted); font-size: var(--text-xs); line-height: 1.75; margin: 0; }

/* ── 一条消息 ──
   机器在左（头像 + 气泡），人在右。连着的机器消息只在最上面挂一次头像，
   下面几条留出同宽的缩进——读起来是「同一个人一口气说了好几段」。 */
.row { display: flex; gap: 9px; align-items: flex-start; animation: jcin var(--dur-base) var(--ease-out) both; }
.row.me { justify-content: flex-end; }
@keyframes jcin { from { opacity: 0; transform: translateY(6px); } }

.av {
  flex: none;
  width: 26px; height: 26px;
  margin-top: 2px;
  display: flex; align-items: center; justify-content: center;
  border-radius: 50%;
  background: linear-gradient(160deg, #6d8bf5, var(--accent));
  color: #fff;
  font-size: 12px;
  box-shadow: 0 2px 6px rgba(66, 99, 235, .28);
  user-select: none;
}
.av.hide { visibility: hidden; }
.bcol { min-width: 0; flex: 1; display: flex; flex-direction: column; gap: 6px; }

.bub {
  max-width: 100%;
  padding: 9px 13px;
  border-radius: 14px;
  font-size: var(--text-s);
  line-height: 1.75;
  word-break: break-word;
}
/* 人说的：右下角切一刀 */
.bub.mine {
  max-width: 84%;
  border-radius: 14px 14px 4px 14px;
  background: rgba(255, 255, 255, .94);
  border: 1px solid rgba(255, 255, 255, .95);
  box-shadow: 0 1px 3px rgba(20, 30, 62, .07);
  color: var(--ink);
  white-space: pre-wrap;
}
/* 机器说的：左上角切一刀 */
.bub.bot {
  border-radius: 4px 14px 14px 14px;
  background: rgba(250, 251, 255, .86);
  border: 1px solid rgba(255, 255, 255, .9);
  box-shadow: 0 1px 3px rgba(20, 30, 62, .05);
  color: var(--ink2);
}
.bub.bot.say { white-space: pre-wrap; }
.bub.bot.run { border-color: color-mix(in srgb, var(--warn) 34%, transparent); }
.bub.bot.fail { border-color: color-mix(in srgb, var(--bad) 30%, transparent); background: rgba(208, 59, 59, .05); }

/* 流水线消息的抬头：状态点 + 步骤名 + 结果 + 时间 */
.bh { display: flex; align-items: center; gap: 7px; }
.bl { flex: 0 1 auto; min-width: 0; font-size: var(--text-s); font-weight: 600; color: var(--ink); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.bs { flex: none; font-size: var(--text-2xs); color: var(--muted); white-space: nowrap; }
.bub.bot.fail .bs { color: var(--bad); }
.bub.bot.run .bs { color: var(--warn); }
.bt { flex: none; margin-left: auto; padding-left: 6px; font-size: var(--text-2xs); color: var(--muted); font-variant-numeric: tabular-nums; }
.bd { margin: 4px 0 0; font-size: var(--text-xs); line-height: 1.7; color: var(--ink2); word-break: break-word; }

/* 这一步里干的每件事：跑完的折成一行，在跑/失败的自动摊开 */
.proc { margin-top: 6px; }
.proc-h {
  display: inline-flex; align-items: center; gap: 5px;
  border: none; background: transparent; padding: 0;
  font-family: inherit; font-size: var(--text-2xs); color: var(--muted);
  cursor: pointer;
}
.proc-h:hover { color: var(--ink2); }
.proc-c { font-size: 9px; }
.proc.open :deep(.wf) {
  margin-top: 4px;
  padding-left: 10px;
  border-left: 1px solid rgba(120, 130, 165, .22);
}

/* 在跑：三点呼吸，像终端里那条还没结束的任务 */
.dots { display: inline-flex; align-items: center; gap: 4px; margin-top: 6px; }
.dots i {
  width: 5px; height: 5px; border-radius: 50%;
  background: color-mix(in srgb, var(--accent) 60%, transparent);
  animation: jcdot 1.2s ease-in-out infinite;
}
.dots i:nth-child(2) { animation-delay: .18s; }
.dots i:nth-child(3) { animation-delay: .36s; }
@keyframes jcdot { 0%, 60%, 100% { opacity: .25; transform: translateY(0); } 30% { opacity: 1; transform: translateY(-2px); } }

.ai-tool { margin-top: 6px; font-size: var(--text-2xs); color: var(--muted); }
.caret { color: var(--accent); animation: jcblink 1.1s steps(2, start) infinite; }
@keyframes jcblink { to { opacity: 0; } }

/* 附件：挂在输入框上方，点一下取消携带 */
.jc-atts { flex: none; display: flex; flex-wrap: wrap; gap: 5px; padding: 0 14px; }
.jc-att {
  padding: 3px 10px;
  border-radius: var(--radius-pill);
  border: 1px solid rgba(255, 255, 255, .95);
  background: rgba(255, 255, 255, .8);
  color: var(--ink2);
  font-family: inherit;
  font-size: var(--text-xs);
  line-height: 1.7;
  cursor: pointer;
  max-width: 100%;
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
}
.jc-att:hover { color: var(--bad); }

/* 拖拽悬停：整块面板接住 */
.jc { position: relative; }
.jc-drop {
  position: absolute;
  inset: 0;
  z-index: 9;
  display: flex; align-items: center; justify-content: center;
  background: var(--panel);
  box-shadow: inset 0 0 0 2px var(--accent);
  color: var(--accent-ink);
  font-size: var(--text-s);
  pointer-events: none;
}

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
