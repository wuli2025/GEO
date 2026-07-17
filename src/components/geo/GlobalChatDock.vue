<script setup lang="ts">
/**
 * 全局 AI 对话坞：常驻运营中心右侧，不再只在流程抽屉里。
 * - 一条常驻会话（jobId 无关），历史续聊，存 localStorage。
 * - 泳道锚点：每次发送都带上「当前锚定的泳道」上下文（当前视图 / 门户平台 /
 *   打开着的流程 job）。顶部锚点条常显，让模型每次都知道你此刻站在哪条泳道上。
 */
import { ref, watch, nextTick, onMounted, onBeforeUnmount } from "vue";
import {
  chat, convApi, listen,
  type Message, type ChatStreamEvent,
} from "../../tauri";
import { toast } from "../../composables/useToast";

const props = defineProps<{
  /** 顶部锚点条展示文案，如「知乎门户」「数据看板」。 */
  anchorLabel: string;
  /** 发送时注入模型的锚点上下文（当前视图/平台/job 的简述）。 */
  anchorCtx: string;
}>();
const emit = defineEmits<{ (e: "close"): void }>();

const PROJECT_NAME = "GEO 全局助手";
const LS_KEY = "geo.globalChat.conv";

const msgs = ref<Message[]>([]);
const draft = ref("");
const sending = ref(false);
const streamText = ref("");
const streamTool = ref("");
const convId = ref<string | null>(localStorage.getItem(LS_KEY));
const listEl = ref<HTMLDivElement | null>(null);
let reqId: string | null = null;
let unlisten: (() => void) | null = null;

async function ensureConv(): Promise<string> {
  if (convId.value) return convId.value;
  const projects = await convApi.listProjects();
  const proj = projects.find((p) => p.name === PROJECT_NAME && !p.archived)
    ?? (await convApi.createProject(PROJECT_NAME));
  const conv = await convApi.createConversation(proj.id);
  await convApi.renameConversation(conv.id, "GEO 运营助手").catch(() => {});
  localStorage.setItem(LS_KEY, conv.id);
  convId.value = conv.id;
  return conv.id;
}

async function loadHistory() {
  if (!convId.value) { msgs.value = []; return; }
  try {
    msgs.value = (await convApi.getMessages(convId.value)).filter((m) => m.role !== "tool");
    scrollBottom();
  } catch { msgs.value = []; }
}

async function scrollBottom() {
  await nextTick();
  if (listEl.value) listEl.value.scrollTop = listEl.value.scrollHeight;
}

function ctxBlock(): string {
  return [
    "【泳道锚点 · 系统自动注入，无需向用户复述】",
    `你现在站在：${props.anchorLabel}`,
    props.anchorCtx,
    "你是 GEO 自媒体运营中心的运营助手。基于用户当前所在的泳道/视图回答与协作。",
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

function newChat() {
  if (sending.value) return;
  localStorage.removeItem(LS_KEY);
  convId.value = null;
  msgs.value = [];
  streamText.value = "";
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
      <p v-if="!msgs.length && !streamText" class="foot gchat-hint">
        随时问我这条泳道上的事——每次发送都会带上你此刻所在的视图/平台/流程作为锚点。
      </p>
      <div v-for="mm in msgs" :key="mm.id" class="gchat-msg" :class="mm.role">
        <div class="gchat-bubble">{{ mm.content }}</div>
      </div>
      <div v-if="sending" class="gchat-msg assistant">
        <div class="gchat-bubble">
          <template v-if="streamText">{{ streamText }}</template>
          <span v-else class="foot"><span class="spin">◔</span> 思考中…</span>
          <div v-if="streamTool" class="foot gchat-tool">🔧 {{ streamTool }}</div>
        </div>
      </div>
    </div>
    <div class="gchat-input">
      <textarea
        v-model="draft"
        rows="2"
        placeholder="说点什么…（Enter 发送，Shift+Enter 换行）"
        @keydown.enter.exact.prevent="send"
      ></textarea>
      <button v-if="sending" class="btn sm danger" @click="stop">停止</button>
      <button v-else class="btn sm" :disabled="!draft.trim()" @click="send">发送</button>
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
