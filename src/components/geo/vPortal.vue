<script setup lang="ts">
/**
 * 平台门户：一页到底，只有两块——
 *   ① 工作流：本平台流水线编排（环节→专家→技能，mediaOps 真编排）+ 选题/队列/job 看板泳道；
 *   ② 专家团补丁：工作流上站着的那几位专家，每位可当场改本平台补丁（ExpertPromptDrawer，锁定本平台）。
 * 原来的 选题·题库 / 规划队列 / 账号·发送方式 / 卡点档案 / 文风宪法 五个子页已撤销：
 * 队列与选题在看板泳道里就能看能点，文风宪法 = 主笔的平台补丁（在专家团里改），
 * 账号与发送方式收成工作流顶上的一行小控件（平台级开关，全应用没别处可设）。
 */
import { ref, computed, watch, onMounted, onBeforeUnmount } from "vue";
import { portalTitleHtml, portalHeaderHtml } from "./render";
import { P } from "./data";
import {
  mediaOps, mediaAccounts, mediaJob, expertMedia, chat, listen, MEDIA_PLATFORMS,
  type MediaTopic, type MediaQueueItem, type MediaPlatformSettings, type MediaAccountStatus, type MediaPlatform, type MediaJob,
  type MediaWorkflowStep, type ChatStreamEvent,
} from "../../tauri";
import { toast } from "../../composables/useToast";
import ExpertPromptDrawer from "./ExpertPromptDrawer.vue";
import { openJobDetail, openJobId } from "./jobsBus";
import { requestPlan } from "./planBus";

// sub 由壳层统一传入；本门户已取消子标签（bar3 不再为 portal 出现），故不使用。
const props = defineProps<{ sub?: string; platform: string }>();

const REAL = MEDIA_PLATFORMS.map((p) => p.id) as string[];
const isReal = computed(() => REAL.includes(props.platform));
const plat = computed(() => props.platform as MediaPlatform);
const pname = computed(() => P(props.platform)?.name ?? props.platform);

const titleHtml = computed(() => portalTitleHtml(props.platform));
const head = computed(() => portalHeaderHtml(props.platform));

// ── 真数据 ──
const topics = ref<MediaTopic[]>([]);
const queue = ref<MediaQueueItem[]>([]);
const settings = ref<MediaPlatformSettings[]>([]);
const accts = ref<MediaAccountStatus[]>([]);
const newTopic = ref({ title: "", angle: "", keywords: "" });

async function loadState() {
  if (!isReal.value) { topics.value = []; queue.value = []; settings.value = []; return; }
  try {
    const s = await mediaOps.state();
    topics.value = (s.topics ?? []).filter((t) => t.platform === plat.value);
    queue.value = (s.queue ?? []).filter((q) => q.platform === plat.value);
    settings.value = s.settings ?? [];
  } catch {
    topics.value = []; queue.value = []; settings.value = [];
  }
}
async function loadAccts() {
  try { accts.value = await mediaAccounts.status(); } catch { accts.value = []; }
}
async function loadJobs() {
  try { jobs.value = await mediaJob.list(); } catch { jobs.value = []; }
}
/** 自媒体专家名册（真 id：media-writer / media-typesetter…），专家团表拿它显示中文名。 */
const experts = ref<{ id: string; name?: string; role?: string }[]>([]);
async function loadExperts() {
  try { experts.value = (await expertMedia.list()) as { id: string; name?: string; role?: string }[]; }
  catch { experts.value = []; }
}
onMounted(async () => {
  loadState(); loadAccts(); loadJobs(); loadExperts();
  // 深度搜索的流事件：整个门户共用一个订阅，按 reqId 认领（对话坞同样监听这条流，各认各的）。
  try {
    dsUnlisten = await listen<ChatStreamEvent>("chat:stream", onDsStream);
  } catch { dsUnlisten = null; }
});
onBeforeUnmount(() => {
  dsSeq += 1;
  if (dsReqId) chat.cancel(dsReqId).catch(() => {});
  dsUnlisten?.();
});
watch(() => props.platform, () => {
  loadState(); loadJobs();
  newTopic.value = { title: "", angle: "", keywords: "" };
  // 换平台 = 换泳道：两颗主功能的临时状态一并清掉，别把 A 平台的题带到 B 平台。
  pubOpen.value = false;
  if (dsOpen.value) dsClose();
  dsText.value = ""; dsItems.value = []; dsErr.value = ""; dsPhase.value = "idle";
});
// 详情抽屉里可能取消/重跑——关抽屉时刷新队列与 job 映射
watch(openJobId, (v) => { if (!v) { loadState(); loadJobs(); } });

// ── 流程打通：选题 → 队列 → 流水线 job → 点进生成流程 ──
const jobs = ref<MediaJob[]>([]);
/** 每个队列项对应的最新 job（详情入口） */
const jobByQueue = computed(() => {
  const m: Record<string, MediaJob> = {};
  for (const j of [...jobs.value].sort((a, b) => a.createdAt - b.createdAt)) {
    if (j.queueId) m[j.queueId] = j;
  }
  return m;
});
const producing = ref<string | null>(null); // 正在排产/启动的 topic 或 queue id

/**
 * 选题点「生成→投递」：不再立刻排产，先把规划请求投给对话框——
 * 对话坞里流式出一份撰写规划，人看完点「开始」才真跑，点「否决」就作罢。
 */
function produceTopic(t: MediaTopic) {
  if (producing.value) return;
  requestPlan({
    platform: plat.value,
    platformName: pname.value,
    title: t.title,
    angle: t.angle || undefined,
    keywords: t.keywords && t.keywords.length ? t.keywords : undefined,
    onApprove: () => doProduce(t),
  });
}

/** 真正排产：入队 → 标记 picked → 启动全链路 job。记录落在对话框，不再自动弹抽屉。 */
async function doProduce(t: MediaTopic): Promise<{ jobId: string }> {
  producing.value = t.id;
  try {
    const q = await mediaOps.queueAdd(plat.value, t.title, t.id);
    await mediaOps.topicUpdate(t.id, { status: "picked" }).catch(() => {});
    const j = await mediaJob.start({ queueId: q.id, topic: t.angle || undefined });
    toast.success(`已排产并启动流水线（job ${j.id.slice(0, 8)}）`);
    await loadState(); await loadJobs();
    return { jobId: j.id };
  } finally { producing.value = null; }
}

/** 队列项手动跑一条全链路 job */
async function runQueueItem(q: MediaQueueItem) {
  if (producing.value) return;
  producing.value = q.id;
  try {
    const j = await mediaJob.start({ queueId: q.id });
    toast.success(`流水线已启动（job ${j.id.slice(0, 8)}）`);
    openJobDetail(j.id);
    await loadState(); await loadJobs();
  } catch (e: any) {
    toast.error(e?.message ?? String(e));
  } finally { producing.value = null; }
}

// ══════════════════════════════════════════════════════════════
// 门户两大主功能（右上角大按钮）
//   ① 立即发一篇 —— 给个标题就直接开跑本平台全链路（生成→配图→排版→投草稿箱）
//   ② 立即深度搜索选题 —— 起一次联网深挖，收敛成能写的题，一键入池 / 直接开跑
// ══════════════════════════════════════════════════════════════

/** 排产一条临时稿：先登记进队列（看板与规划队列同步可见），再启动全链路 job。 */
async function startAdhoc(title: string, angle: string): Promise<string> {
  const q = await mediaOps.queueAdd(plat.value, title).catch(() => null);
  const j = q
    ? await mediaJob.start({ queueId: q.id, topic: angle || undefined })
    : await mediaJob.start({ platform: plat.value, title, topic: angle || undefined });
  await loadState(); await loadJobs();
  return j.id;
}

// ── ① 立即发一篇 ──
const pubOpen = ref(false);
const pubTitle = ref("");
const pubAngle = ref("");
const pubBusy = ref(false);
/** 选题池里还没被挑走的题，开写弹窗里当快捷填充。 */
const poolPick = computed(() => topics.value.filter((t) => t.status !== "picked").slice(0, 8));

function openPub(seed?: { title: string; angle?: string }) {
  if (!isReal.value) return;
  pubTitle.value = seed?.title ?? "";
  pubAngle.value = seed?.angle ?? "";
  pubOpen.value = true;
}

async function pubRunNow() {
  const t = pubTitle.value.trim();
  if (!t || pubBusy.value) return;
  pubBusy.value = true;
  try {
    const jobId = await startAdhoc(t, pubAngle.value.trim());
    toast.success(`已开跑：${pname.value}《${t}》（job ${jobId.slice(0, 8)}）`);
    pubOpen.value = false;
    openJobDetail(jobId);
  } catch (e: any) {
    toast.error(e?.message ?? String(e));
  } finally { pubBusy.value = false; }
}

/** 不放心就先看规划：投给对话坞出一份撰写规划，点「开始」才真排产。 */
function pubPlanFirst() {
  const t = pubTitle.value.trim();
  if (!t) return;
  const angle = pubAngle.value.trim();
  pubOpen.value = false;
  requestPlan({
    platform: plat.value,
    platformName: pname.value,
    title: t,
    angle: angle || undefined,
    onApprove: async () => ({ jobId: await startAdhoc(t, angle) }),
  });
}

// ── ② 立即深度搜索选题 ──
interface DsItem { title: string; angle: string; keywords: string[]; why: string; sources: string[]; added: boolean }
const dsOpen = ref(false);
const dsDir = ref("");
const dsCount = ref(5);
const dsPhase = ref<"idle" | "running" | "done" | "failed">("idle");
const dsText = ref("");
const dsTool = ref("");
const dsErr = ref("");
const dsItems = ref<DsItem[]>([]);
const dsShowRaw = ref(false);
/** 当前这轮深搜的 chat 请求 id（流事件按它认领）。 */
let dsReqId: string | null = null;
let dsUnlisten: (() => void) | null = null;
/** 使较早、较慢返回的 chat_send 不能夺走新一轮深搜的流归属。 */
let dsSeq = 0;

function dsPrompt(): string {
  const p = P(props.platform);
  const n = Math.min(10, Math.max(3, Number(dsCount.value) || 5));
  const pool = topics.value.slice(0, 20).map((t) => `《${t.title}》`).join("、");
  return [
    `你是 GEO 自媒体运营中心「${pname.value}」这条泳道的选题雷达。做一次深度联网搜索，给出 ${n} 条能写、能带量的具体选题。`,
    p ? `平台画像：主打 AI 引擎 ${p.ai}；文体规范：${p.style}；红线：${p.redline}` : "",
    dsDir.value.trim()
      ? `本次搜索方向（用户指定）：${dsDir.value.trim()}`
      : "本次搜索方向：按本平台既有定位，找最近的热点、争议与对标爆文。",
    pool ? `选题池里已有这些题，别重复：${pool}` : "",
    "────",
    "要求：",
    "1. 真的联网搜（WebSearch / WebFetch），覆盖最近 7–30 天的新闻、动态、高赞讨论与对标账号爆文；",
    "2. 每条选题都要具体到能直接开写，不要「XX 行业趋势」这类空题；",
    "3. 讲清切入角度 + 为什么这题值得写（读者痛点 / 传播势能 / 与本平台主打 AI 引擎的契合），并附信息来源；",
    "4. 不要写正文，不要改动任何文件。",
    "────",
    "最后必须、也只能以一个 JSON 代码块收尾（```json 开头、``` 结尾），数组元素字段：",
    "title（标题）/ angle（切入角度）/ keywords（关键词数组）/ why（为什么值得写）/ sources（来源链接或出处数组）。JSON 之后不要再写任何字。",
  ].filter(Boolean).join("\n");
}

/** 从模型输出里抠出选题数组：优先最后一个代码围栏，兜底抓最后一段中括号。 */
function parseTopics(raw: string): DsItem[] {
  const cands = [...raw.matchAll(/```(?:json)?\s*([\s\S]*?)```/g)].map((m) => m[1].trim()).reverse();
  if (!cands.length) {
    const s = raw.lastIndexOf("["), e = raw.lastIndexOf("]");
    if (s >= 0 && e > s) cands.push(raw.slice(s, e + 1));
  }
  for (const c of cands) {
    let v: any;
    try { v = JSON.parse(c); } catch { continue; }
    const arr = Array.isArray(v) ? v : Array.isArray(v?.topics) ? v.topics : null;
    if (!arr) continue;
    const items: DsItem[] = arr
      .filter((x: any) => x && typeof x.title === "string" && x.title.trim())
      .map((x: any) => ({
        title: String(x.title).trim(),
        angle: String(x.angle ?? "").trim(),
        keywords: Array.isArray(x.keywords)
          ? x.keywords.map((k: any) => String(k).trim()).filter(Boolean)
          : String(x.keywords ?? "").split(/[,，、\s]+/).map((k) => k.trim()).filter(Boolean),
        why: String(x.why ?? "").trim(),
        sources: Array.isArray(x.sources) ? x.sources.map((s: any) => String(s).trim()).filter(Boolean) : [],
        added: false,
      }));
    if (items.length) return items;
  }
  return [];
}

function onDsStream(ev: ChatStreamEvent) {
  if (!dsReqId || ev.reqId !== dsReqId) return;
  if (ev.kind === "delta" && ev.text) dsText.value += ev.text;
  else if (ev.kind === "tool") dsTool.value = ev.tool || ev.text || "";
  else if (ev.kind === "error") {
    dsReqId = null; dsPhase.value = "failed"; dsErr.value = ev.text || "深度搜索出错";
  } else if (ev.kind === "done") {
    dsReqId = null;
    dsItems.value = parseTopics(dsText.value);
    dsPhase.value = "done";
    if (!dsItems.value.length) {
      dsErr.value = "没能从结果里解析出选题清单——可展开原文自己挑，或重搜一次。";
      dsShowRaw.value = true;
    }
  }
}

async function dsStart() {
  if (dsPhase.value === "running" || !isReal.value) return;
  const seq = ++dsSeq;
  dsText.value = ""; dsTool.value = ""; dsErr.value = ""; dsItems.value = [];
  dsShowRaw.value = false; dsPhase.value = "running";
  try {
    const id = await chat.send({
      prompt: dsPrompt(),
      permissionMode: "auto_current",
      workMode: "office",
      skillIds: ["hot-topic-radar"],
    });
    // 期间用户已重搜/关窗 → 这条流没人要了，直接掐掉。
    if (seq !== dsSeq) { chat.cancel(id).catch(() => {}); return; }
    dsReqId = id;
  } catch (e: any) {
    dsPhase.value = "failed";
    dsErr.value = e?.message ?? String(e);
  }
}

function dsStop() {
  dsSeq += 1;
  if (dsReqId) { chat.cancel(dsReqId).catch(() => {}); dsReqId = null; }
  // 停在半路也把已经吐出来的部分尽量解析出来，不白跑。
  dsItems.value = dsText.value ? parseTopics(dsText.value) : [];
  dsPhase.value = dsText.value ? "done" : "idle";
}

function dsClose() {
  if (dsPhase.value === "running") dsStop();
  dsOpen.value = false;
}

async function dsAdd(it: DsItem) {
  if (it.added) return;
  try {
    const created = await mediaOps.topicAdd(plat.value, it.title, it.angle, it.keywords, "deep-search");
    topics.value = [created, ...topics.value];
    it.added = true;
    toast.success("已加入选题池");
  } catch (e: any) { toast.error(e?.message ?? String(e)); }
}
async function dsAddAll() {
  for (const it of dsItems.value) await dsAdd(it);
}
/** 深搜结果直接开写：带着标题/角度跳到「立即发一篇」。 */
function dsPublish(it: DsItem) {
  dsClose();
  openPub({ title: it.title, angle: it.angle });
}

const platSettings = computed(() =>
  settings.value.find((s) => s.platform === plat.value)
);
const sendMode = computed<"ai" | "manual">(() => {
  const s = platSettings.value?.sendMode;
  if (s) return s;
  return P(props.platform)?.sendMode === "manual" ? "manual" : "ai";
});

async function addTopic() {
  const t = newTopic.value.title.trim();
  if (!t || !isReal.value) return;
  const kws = newTopic.value.keywords.split(/[,，、\s]+/).map((s) => s.trim()).filter(Boolean);
  try {
    const created = await mediaOps.topicAdd(plat.value, t, newTopic.value.angle.trim(), kws, "manual");
    topics.value = [created, ...topics.value];
    newTopic.value = { title: "", angle: "", keywords: "" };
    toast.success("已加入选题池");
  } catch (e: any) {
    toast.error(e?.message ?? String(e));
  }
}
async function toggleSend() {
  if (!isReal.value) return;
  const next: "ai" | "manual" = sendMode.value === "ai" ? "manual" : "ai";
  try {
    const updated = await mediaOps.settingsSet(plat.value, { sendMode: next });
    const exists = settings.value.some((x) => x.platform === updated.platform);
    settings.value = exists ? settings.value.map((x) => (x.platform === updated.platform ? updated : x)) : [...settings.value, updated];
    toast.info(next === "ai" ? "已切到 AI 直传草稿箱" : "已切到手动辅助");
  } catch (e: any) {
    toast.error(e?.message ?? String(e));
  }
}
const acctBusy = ref(false);
async function openLogin() {
  if (!isReal.value) return;
  acctBusy.value = true;
  try {
    const r = await mediaAccounts.open(plat.value, "login");
    toast.info(r?.message ?? "已打开登录窗口，扫码后关闭即可");
    setTimeout(loadAccts, 800);
  } catch (e: any) { toast.error(e?.message ?? String(e)); } finally { acctBusy.value = false; }
}
const platAcct = computed(() => accts.value.find((a) => a.platform === plat.value));

// ── 工作流编排（真编排：mediaops settings.workflow，执行面按它取人）──
// 后端没给（平台未接入 / 命令不可用）时用与 mediaops.rs default_workflow 一致的兜底，
// 让门户至少能说清「这条流水线由谁站哪一格」。
const FALLBACK_WORKFLOW: MediaWorkflowStep[] = [
  { step: "选题", expertId: "media-strategist", skillId: "hot-topic-radar", note: "" },
  { step: "调研", expertId: "media-researcher", skillId: "deep-research", note: "" },
  { step: "写作", expertId: "media-writer", skillId: "", note: "" },
  { step: "质检", expertId: "media-reviewer", skillId: "", note: "" },
  { step: "AI痕迹优化", expertId: "media-deaiflavor", skillId: "", note: "" },
  { step: "配图", expertId: "media-imagedirector", skillId: "", note: "" },
  { step: "排版", expertId: "media-typesetter", skillId: "", note: "" },
  { step: "投递", expertId: "media-publisher", skillId: "media-publisher", note: "" },
];
const workflow = computed<MediaWorkflowStep[]>(() => {
  const w = platSettings.value?.workflow;
  return w && w.length ? w : FALLBACK_WORKFLOW;
});
function expertName(id: string): string {
  const e = experts.value.find((x) => x.id === id);
  return e?.name || id;
}
function expertRole(id: string): string {
  return experts.value.find((x) => x.id === id)?.role || "";
}
/** 专家团 = 工作流上站着的人（按出场顺序去重），每位一行、可改本平台补丁。 */
const team = computed(() => {
  const seen = new Map<string, string[]>();
  for (const w of workflow.value) {
    if (!w.expertId) continue;
    const steps = seen.get(w.expertId);
    if (steps) steps.push(w.step);
    else seen.set(w.expertId, [w.step]);
  }
  return [...seen].map(([id, steps]) => ({ id, steps }));
});

// ── 看板真实泳道：选题池 + 队列 + job 按状态分列，每张卡点开真实生成流程 ──
type LaneCard = { key: string; title: string; jobId?: string; note?: string; topic?: MediaTopic; hot?: boolean };
const lanes = computed<{ name: string; cards: LaneCard[] }[]>(() => {
  const jb = jobByQueue.value;
  const qCard = (q: MediaQueueItem, hot = false): LaneCard => ({
    key: q.id, title: q.title, jobId: jb[q.id]?.id, note: q.note || undefined, hot,
  });
  const byStatus = (st: MediaQueueItem["status"]) => queue.value.filter((q) => q.status === st);
  const running = queue.value.filter((q) => q.status === "running"
    || (jb[q.id] && (jb[q.id].status === "running" || jb[q.id].status === "pending") && q.status === "queued"));
  const runningIds = new Set(running.map((q) => q.id));
  return [
    { name: "选题池", cards: topics.value.filter((t) => t.status !== "picked").map((t) => ({ key: t.id, title: t.title, topic: t })) },
    { name: "排队中", cards: byStatus("queued").filter((q) => !runningIds.has(q.id)).map((q) => qCard(q)) },
    { name: "流水线在跑", cards: running.map((q) => qCard(q, true)) },
    { name: "草稿已投·待人预览", cards: byStatus("draft_uploaded").map((q) => qCard(q, true)) },
    { name: "完成", cards: byStatus("done").map((q) => qCard(q)) },
    { name: "失败", cards: byStatus("failed").map((q) => qCard(q)) },
  ];
});
function laneCardClick(c: LaneCard) {
  if (c.jobId) { openJobDetail(c.jobId); return; }
  if (c.topic) { produceTopic(c.topic); return; }
  const q = queue.value.find((x) => x.id === c.key);
  if (q) runQueueItem(q);
}

// 门户「编辑补丁」→ 抽屉（锁定本平台）
const editingExpert = ref<string | null>(null);
</script>

<template>
  <div>
    <!-- 门户抬头：左标题，右上角两颗主功能大按钮 -->
    <div class="portal-hero">
      <div class="hero-l" v-html="titleHtml"></div>
      <div class="hero-acts">
        <button
          class="bigbtn pri"
          :disabled="!isReal"
          :title="isReal ? `给个标题就跑完 ${pname} 全链路：生成→配图→排版→投草稿箱` : '该平台尚未接入'"
          @click="openPub()"
        >
          <span class="bb-ic">⚡</span>
          <span class="bb-t">立即发一篇{{ pname }}<small>全链路一把梭 · 只投草稿箱</small></span>
        </button>
        <button
          class="bigbtn"
          :disabled="!isReal"
          :title="isReal ? '联网深挖最近热点与对标爆文，收敛成能直接开写的题' : '该平台尚未接入'"
          @click="dsOpen = true"
        >
          <span class="bb-ic">🔍</span>
          <span class="bb-t">立即深度搜索选题<small>{{ dsPhase === "running" ? "搜索中…" : "联网深挖 · 一键入池" }}</small></span>
        </button>
      </div>
    </div>
    <div v-html="head"></div>

    <!-- ══ ① 工作流 ══ -->
    <section>
      <div class="card">
        <h3>工作流 · {{ pname }} 流水线编排</h3>
        <p class="foot flush">
          执行面按这张编排取人：每一格由哪位专家、挂哪个技能。改专家的<b>本平台补丁</b>就是改这条流水线的产出
          —— 补丁表在下面「专家团」里，改完下一条 job 立即生效。
        </p>
        <div class="wf-flow">
          <template v-for="(w, i) in workflow" :key="w.step">
            <button
              class="wf-step"
              :title="`${w.step}：${expertName(w.expertId)}${w.skillId ? ' · 技能 ' + w.skillId : ''}——点开改本平台补丁`"
              @click="editingExpert = w.expertId"
            >
              <b>{{ w.step }}</b>
              <small>{{ expertName(w.expertId) }}</small>
            </button>
            <span v-if="i < workflow.length - 1" class="arr">→</span>
          </template>
        </div>
        <!-- 账号与发送方式：平台级两个开关，收成一行，别再单开一页 -->
        <div class="wf-ops">
          <span class="sline">
            <span class="sdot" :class="platAcct?.bound ? 'ok' : 'idle'"></span>
            {{ platAcct?.bound ? "登录态已绑定" : "未登录" }}
          </span>
          <button class="btn sm" :disabled="acctBusy || !isReal" @click="openLogin">
            <span v-if="acctBusy" class="spin" style="margin-right: 4px">◔</span>扫码登录 / 续期
          </button>
          <span class="wf-sep"></span>
          <span class="foot flush">发送方式</span>
          <div class="switch">
            <button :class="{ on: sendMode === 'ai' }" :disabled="!isReal" @click="sendMode !== 'ai' && toggleSend()">AI 直传草稿箱</button>
            <button :class="{ on: sendMode === 'manual' }" :disabled="!isReal" @click="sendMode !== 'manual' && toggleSend()">手动辅助</button>
          </div>
          <span class="foot flush">任何一步失败自动降级手动辅助；只投草稿箱，永不自动对外发布。</span>
        </div>
      </div>
    </section>

    <!-- 选题快速入池：选题池就是看板第一列，加题的口留在这里 -->
    <section v-if="isReal">
      <div class="card">
        <h3>加一条选题（直接进下面「选题池」）</h3>
        <div style="display: flex; gap: 8px; flex-wrap: wrap">
          <input v-model="newTopic.title" class="inp" style="flex: 2; min-width: 180px" placeholder="选题标题" @keydown.enter="addTopic" />
          <input v-model="newTopic.angle" class="inp" style="flex: 1; min-width: 120px" placeholder="切入角度（可选）" />
          <input v-model="newTopic.keywords" class="inp" style="flex: 1; min-width: 120px" placeholder="关键词，逗号分隔" />
          <button class="btn" @click="addTopic">＋ 加入</button>
        </div>
      </div>
    </section>

    <!-- 看板泳道：选题池 → 排队 → 在跑 → 草稿待预览 → 完成/失败，每张卡点开真实生成流程 -->
    <section>
      <div class="lanes">
        <div v-for="l in lanes" :key="l.name" class="lane">
          <h5>{{ l.name }}<span class="cnt">{{ l.cards.length }}</span></h5>
          <div
            v-for="c in l.cards"
            :key="c.key"
            class="draft"
            :title="c.jobId ? '点击查看这条流程的生成过程' : c.topic ? '点击：先在对话框出规划，再定夺开始/否决' : '点击：启动全链路流水线'"
            @click="laneCardClick(c)"
          >
            {{ c.title }}
            <div class="tags">
              <span class="tag">{{ pname }}</span>
              <span v-if="c.hot" class="tag hot">{{ c.jobId ? "点开看进度" : "等点头" }}</span>
              <span v-else-if="c.topic" class="tag">▶ 生成→投递</span>
              <span v-if="c.note" class="tag">{{ c.note }}</span>
            </div>
          </div>
          <div v-if="!l.cards.length" style="color: var(--muted); font-size: var(--text-2xs); padding: 5px 2px">（空）</div>
        </div>
      </div>
    </section>

    <!-- ══ ② 专家团补丁（工作流上站着的那几位，逐位可改） ══ -->
    <section>
      <div class="card">
        <h3>专家团 · 本平台补丁（{{ team.length }} 位）</h3>
        <p class="foot flush">
          一套专家团 + 每平台一段提示词补丁：<code>系统提示 = 基础画像 + 本平台补丁 + 闸门A注入</code>。
          文风宪法、标题公式、排版规范、红线全在补丁里改——点「编辑补丁」当场改、当场存，保存即记一版可回滚。
        </p>
        <div class="tbl-wrap">
          <table>
            <tr><th style="width:150px">负责环节</th><th>专家</th><th>职责</th><th></th></tr>
            <tr v-for="m in team" :key="m.id">
              <td>{{ m.steps.join(" / ") }}</td>
              <td><b>{{ expertName(m.id) }}</b><br><code>{{ m.id }}</code></td>
              <td class="foot flush">{{ expertRole(m.id) || "—" }}</td>
              <td style="white-space: nowrap">
                <button class="btn sm" @click="editingExpert = m.id">编辑补丁</button>
              </td>
            </tr>
          </table>
        </div>
        <div style="margin-top: 8px">
          <button class="btn sm ghost" data-go="experts">看完整专家阵容 →</button>
        </div>
      </div>
    </section>

    <!-- 主功能弹窗①：立即发一篇 —— 给标题（可带角度）→ 直接开跑，或先出规划再跑 -->
    <div v-if="pubOpen" class="gm-mask" @click.self="pubOpen = false">
      <div class="gm">
        <div class="gm-h">
          <span>⚡ 立即发一篇{{ pname }}</span>
          <button class="xbtn" title="关闭" @click="pubOpen = false">✕</button>
        </div>
        <div class="gm-body">
          <div class="fld">
            <span>标题（必填）</span>
            <input v-model="pubTitle" class="inp" placeholder="这篇叫什么" @keydown.enter="pubRunNow" />
          </div>
          <div class="fld">
            <span>切入角度 / 交代给主笔的话（可选）</span>
            <input v-model="pubAngle" class="inp" placeholder="怎么切、写给谁看、要立什么观点" @keydown.enter="pubRunNow" />
          </div>
          <div v-if="poolPick.length">
            <div class="foot flush">或从选题池挑一条：</div>
            <div class="bd-chips" style="margin-top: 6px">
              <button
                v-for="t in poolPick"
                :key="t.id"
                class="btn sm ghost"
                @click="pubTitle = t.title; pubAngle = t.angle || ''"
              >{{ t.title }}</button>
            </div>
          </div>
          <p class="foot">
            走的是本平台完整流水线：生成 → 配图 → 排版 → 投递。<b>只投草稿箱，永不自动对外发布。</b>
            开跑后自动打开生成流程，可随时中断。
          </p>
        </div>
        <div class="gm-foot">
          <button class="btn ghost" :disabled="!pubTitle.trim() || pubBusy" @click="pubPlanFirst">先出规划再跑</button>
          <span class="grow"></span>
          <button class="btn ghost" @click="pubOpen = false">取消</button>
          <button class="btn" :disabled="!pubTitle.trim() || pubBusy" @click="pubRunNow">
            <span v-if="pubBusy" class="spin" style="margin-right: 4px">◔</span>▶ 立即开跑
          </button>
        </div>
      </div>
    </div>

    <!-- 主功能弹窗②：深度搜索选题 —— 联网深挖 → 解析成选题卡 → 入池 / 直接开写 -->
    <div v-if="dsOpen" class="gm-mask" @click.self="dsClose">
      <div class="gm wide">
        <div class="gm-h">
          <span>🔍 深度搜索选题 · {{ pname }}</span>
          <button class="xbtn" title="关闭" @click="dsClose">✕</button>
        </div>
        <div class="gm-body">
          <div class="ds-bar">
            <div class="fld grow" style="min-width: 220px">
              <span>搜索方向（可选，留空＝按本平台定位找热点）</span>
              <input v-model="dsDir" class="inp" placeholder="例：AI 搜索优化、行业新政、竞品动态…" @keydown.enter="dsStart" />
            </div>
            <div class="fld" style="width: 92px">
              <span>要几条</span>
              <input v-model.number="dsCount" class="inp" type="number" min="3" max="10" />
            </div>
            <button v-if="dsPhase === 'running'" class="btn danger" @click="dsStop">停止</button>
            <button v-else class="btn" @click="dsStart">{{ dsText ? "重搜一次" : "开始深度搜索" }}</button>
          </div>

          <div v-if="dsErr" class="err" style="margin: 12px 0 0">{{ dsErr }}</div>
          <p v-if="dsPhase === 'running'" class="foot">
            <span class="spin">◔</span> 正在联网深挖…{{ dsTool ? `（${dsTool}）` : "" }}
          </p>
          <div v-if="dsPhase === 'running' && dsText" class="pre-box sm">{{ dsText }}</div>

          <div v-if="dsItems.length" class="ds-list">
            <div v-for="(it, i) in dsItems" :key="i" class="ds-item">
              <div class="ds-t">{{ i + 1 }}. {{ it.title }}</div>
              <div v-if="it.angle" class="ds-k"><b>角度</b>{{ it.angle }}</div>
              <div v-if="it.why" class="ds-k"><b>为什么值得写</b>{{ it.why }}</div>
              <div v-if="it.keywords.length" class="bd-chips" style="margin-top: 6px">
                <span v-for="k in it.keywords" :key="k" class="badge b-ghost">{{ k }}</span>
              </div>
              <div v-if="it.sources.length" class="foot">来源：{{ it.sources.join(" · ") }}</div>
              <div class="ds-act">
                <button class="btn sm" :disabled="it.added" @click="dsAdd(it)">{{ it.added ? "✓ 已入池" : "＋ 加入选题池" }}</button>
                <button class="btn sm ghost" @click="dsPublish(it)">⚡ 直接发这篇</button>
              </div>
            </div>
          </div>

          <p v-if="dsPhase === 'idle' && !dsText" class="foot">
            起一次联网深挖：扒最近 7–30 天的热点、讨论与对标爆文，收敛成能直接开写的题。
            结果可一键入池，也可直接开跑全链路。跑一轮通常一两分钟。
          </p>
          <div v-if="dsText && dsPhase !== 'running'" style="margin-top: 10px">
            <button class="btn sm ghost" @click="dsShowRaw = !dsShowRaw">{{ dsShowRaw ? "收起" : "展开" }}搜索过程原文</button>
            <div v-if="dsShowRaw" class="pre-box sm">{{ dsText }}</div>
          </div>
        </div>
        <div class="gm-foot">
          <button v-if="dsItems.length" class="btn ghost" @click="dsAddAll">全部加入选题池</button>
          <span class="grow"></span>
          <button class="btn ghost" @click="dsClose">关闭</button>
        </div>
      </div>
    </div>

    <ExpertPromptDrawer
      v-if="editingExpert"
      :expert-id="editingExpert"
      :platform="props.platform"
      lock-platform
      @close="editingExpert = null"
    />
  </div>
</template>

<style scoped>
/* 门户抬头：标题与右上角主功能大按钮同排（窄屏时按钮整排落到标题下面）。
   样式放组件里而不是 geo.css——这两颗按钮只服务门户，且 geo.css 正被别处改着。 */
.portal-hero {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--space-m);
  flex-wrap: wrap;
}
.hero-l { min-width: 0; }
.hero-acts { display: flex; gap: 10px; flex: none; flex-wrap: wrap; margin-left: auto; }

.bigbtn {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  padding: 12px 20px;
  border-radius: 12px;
  border: 1px solid var(--line-2);
  background: var(--card);
  color: var(--ink);
  font-family: inherit;
  font-size: var(--text-m);
  font-weight: 600;
  line-height: 1.25;
  text-align: left;
  cursor: pointer;
  box-shadow: 0 1px 2px rgba(30, 40, 80, .06), 0 6px 18px rgba(30, 40, 80, .06);
  transition: transform var(--dur-fast) var(--ease-out), box-shadow var(--dur-fast) var(--ease-out), background-color var(--dur-fast) var(--ease-out);
}
.bigbtn:hover { transform: translateY(-1px); box-shadow: 0 2px 4px rgba(30, 40, 80, .08), 0 10px 26px rgba(30, 40, 80, .10); }
.bigbtn:active { transform: none; }
.bigbtn:disabled { opacity: .5; cursor: not-allowed; transform: none; box-shadow: none; }
.bigbtn.pri {
  background: linear-gradient(180deg, #5a7af0, var(--accent));
  border-color: var(--accent);
  color: #fff;
  box-shadow: 0 2px 4px rgba(66, 99, 235, .18), 0 10px 26px rgba(66, 99, 235, .26);
}
.bigbtn.pri:hover { box-shadow: 0 3px 6px rgba(66, 99, 235, .22), 0 14px 32px rgba(66, 99, 235, .32); }
.bb-ic { font-size: 20px; line-height: 1; flex: none; }
.bb-t { display: flex; flex-direction: column; gap: 3px; white-space: nowrap; }
.bb-t small { font-size: var(--text-2xs); font-weight: 400; color: var(--dim); }
.bigbtn.pri .bb-t small { color: rgba(255, 255, 255, .82); }

/* 主功能弹窗（居中模态，抽屉留给专家提示词） */
.gm-mask {
  position: fixed;
  inset: 0;
  z-index: 70;
  background: rgba(20, 28, 55, .38);
  display: flex;
  align-items: center;
  justify-content: center;
  padding: var(--space-xl);
}
.gm {
  width: 620px;
  max-width: 96vw;
  max-height: 88vh;
  display: flex;
  flex-direction: column;
  background: var(--bg);
  border: 1px solid var(--line-2);
  border-radius: 14px;
  box-shadow: 0 24px 70px rgba(30, 40, 80, .3);
  overflow: hidden;
}
.gm.wide { width: 820px; }
.gm-h {
  flex: none;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-xs);
  padding: 14px 18px;
  border-bottom: 1px solid var(--line);
  font-size: var(--text-m);
  font-weight: 600;
  color: var(--ink);
}
.gm-body { overflow: auto; padding: 16px 18px; display: flex; flex-direction: column; gap: 12px; }
.gm-foot {
  flex: none;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 12px 18px;
  border-top: 1px solid var(--line);
  background: var(--panel);
}

/* 工作流编排条：一格一个环节，点开就是那位专家的本平台补丁 */
.wf-flow { display: flex; align-items: stretch; gap: 6px; flex-wrap: wrap; margin-top: 10px; }
.wf-flow .arr { align-self: center; color: var(--muted); font-size: var(--text-xs); }
.wf-step {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 8px 12px;
  border-radius: 10px;
  border: 1px solid var(--line);
  background: var(--card);
  color: var(--ink);
  font-family: inherit;
  text-align: left;
  cursor: pointer;
  transition: border-color var(--dur-fast) var(--ease-out), transform var(--dur-fast) var(--ease-out);
}
.wf-step:hover { border-color: var(--accent); transform: translateY(-1px); }
.wf-step b { font-size: var(--text-xs); font-weight: 600; }
.wf-step small { font-size: var(--text-2xs); color: var(--dim); }

.wf-ops {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
  margin-top: 12px;
  padding-top: 10px;
  border-top: 1px solid var(--line);
}
.wf-sep { width: 1px; align-self: stretch; background: var(--line); }

.ds-bar { display: flex; gap: 8px; flex-wrap: wrap; align-items: flex-end; }
.ds-list { display: flex; flex-direction: column; gap: 10px; }
.ds-item { border: 1px solid var(--line); border-radius: 10px; padding: 10px 13px; background: var(--card); }
.ds-t { font-size: var(--text-s); font-weight: 600; color: var(--ink); }
.ds-k { font-size: var(--text-xs); color: var(--ink2); margin-top: 4px; }
.ds-k b { color: var(--dim); font-weight: 500; margin-right: 6px; }
.ds-act { display: flex; gap: 6px; margin-top: 8px; }
</style>
