<script setup lang="ts">
/**
 * 流程对话面板：在流程详情抽屉右栏，用自然语言查看/操控这条泳道。
 * - 每条 job 绑定一个内核会话（jobId → conversationId 存 localStorage，可续聊）
 * - 每次发送前自动注入该 job 的实时快照（状态/步骤/日志尾部/文件路径），
 *   所以问「现在卡在哪一步」「为什么失败」得到的是真实发生的东西
 * - 回复走 chat:stream 事件流式渲染；生成中可打断
 */
import { ref, watch, nextTick, onMounted, onBeforeUnmount } from "vue";
import {
  chat, convApi, listen,
  type MediaJob, type Message, type ChatStreamEvent,
} from "../../tauri";
import { toast } from "../../composables/useToast";

const props = defineProps<{ job: MediaJob; log: string }>();

const PROJECT_NAME = "GEO 流程对话";
const LS_KEY = "geo.jobChat.convs";

const msgs = ref<Message[]>([]);
const draft = ref("");
const sending = ref(false);
const streamText = ref("");
const streamTool = ref("");
const convId = ref<string | null>(null);
const listEl = ref<HTMLDivElement | null>(null);
let reqId: string | null = null;
let unlisten: (() => void) | null = null;

function convMap(): Record<string, string> {
  try { return JSON.parse(localStorage.getItem(LS_KEY) || "{}"); } catch { return {}; }
}
function rememberConv(jobId: string, id: string) {
  const m = convMap(); m[jobId] = id;
  localStorage.setItem(LS_KEY, JSON.stringify(m));
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
</script>

<template>
  <div class="jc">
    <div ref="listEl" class="jc-list">
      <p v-if="!msgs.length && !streamText" class="foot jc-hint">
        对这条流程直接提问或下指令，比如「现在卡在哪一步」「为什么失败了」「把开头写得更口语一点」。
        每次发送都会带上它此刻的真实状态和日志。
      </p>
      <div v-for="mm in msgs" :key="mm.id" class="jc-msg" :class="mm.role">
        <div class="jc-bubble">{{ mm.content }}</div>
      </div>
      <div v-if="sending" class="jc-msg assistant">
        <div class="jc-bubble">
          <template v-if="streamText">{{ streamText }}</template>
          <span v-else class="foot"><span class="spin">◔</span> 思考中…</span>
          <div v-if="streamTool" class="foot jc-tool">🔧 {{ streamTool }}</div>
        </div>
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
.jc-list { flex: 1; min-height: 0; overflow-y: auto; display: flex; flex-direction: column; gap: 10px; padding-bottom: 8px; }
.jc-hint { margin: auto 12px; text-align: center; }
.jc-msg { display: flex; }
.jc-msg.user { justify-content: flex-end; }
.jc-bubble {
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
.jc-msg.user .jc-bubble { background: var(--accent-soft); border-color: var(--line-2); }
.jc-tool { margin-top: 6px; }
.jc-input { flex: none; display: flex; align-items: flex-end; gap: 8px; padding-top: 10px; border-top: 1px solid var(--line); }
.jc-input textarea {
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
.jc-input textarea:focus { outline: 1px solid var(--line-2); }
</style>
