<script setup lang="ts">
import { ref, computed, onMounted, watch } from "vue";
import {
  Megaphone,
  Radar,
  Workflow as WorkflowIcon,
  ListChecks,
  KeyRound,
  Users,
  Cpu,
  BookOpen,
  Plus,
  Trash2,
  Pencil,
  Check,
  X,
  Loader,
  Send,
  RefreshCw,
  LogOut,
  ExternalLink,
  FileText,
  Sparkles,
  Eye,
  EyeOff,
  Zap,
  ChevronRight,
  Save,
} from "@lucide/vue";
import { useAppStore } from "../stores/app";
import { useChatStore } from "../stores/chat";
import {
  mediaAccounts,
  mediaOps,
  ark,
  expertMedia,
  MEDIA_PLATFORMS,
  type MediaPlatform,
  type MediaAccountStatus,
  type MediaTopic,
  type MediaQueueItem,
  type MediaPlatformSettings,
  type MediaWorkflowStep,
  type ArkConfig,
} from "../tauri";
import { toast } from "../composables/useToast";

// KeepAlive include 按组件 name 匹配 → 必须显式命名 "MediaOps"（切走再回来不丢状态）
defineOptions({ name: "MediaOps" });

const app = useAppStore();
const chat = useChatStore();
const PROJECT_NAME = "自媒体运营";

// ───────── 区块 / 平台 ─────────
type Zone = "topics" | "workflow" | "queue" | "accounts" | "experts" | "api" | "kb";
const zone = ref<Zone>("topics");
const platform = ref<MediaPlatform>("wechat");
const error = ref<string | null>(null);

const ZONES: { id: Zone; label: string; icon: any }[] = [
  { id: "topics", label: "选题·题库", icon: Radar },
  { id: "workflow", label: "工作流", icon: WorkflowIcon },
  { id: "queue", label: "规划队列", icon: ListChecks },
  { id: "accounts", label: "账号", icon: KeyRound },
  { id: "experts", label: "专家团", icon: Users },
  { id: "api", label: "API 中心", icon: Cpu },
  { id: "kb", label: "知识库", icon: BookOpen },
];

const platformName = computed(
  () => MEDIA_PLATFORMS.find((p) => p.id === platform.value)?.name ?? platform.value
);

// 每平台的「全链路」pipeline 技能（后端可能尚未就绪，仅作 skillId 提示传入）
const PIPELINE_SKILL: Record<MediaPlatform, string> = {
  wechat: "wechat-pipeline",
  xhs: "xiaohongshu-pipeline",
  zhihu: "zhihu-pipeline",
  toutiao: "toutiao-pipeline",
  baijia: "baijia-pipeline",
  bilibili: "bilibili-pipeline",
  douyin: "douyin-pipeline",
};

// ───────── 账号登录态 ─────────
const accounts = ref<MediaAccountStatus[]>([]);
const accBusy = ref<string | null>(null);
const accMsg = ref<string | null>(null);
async function loadAccounts() {
  try {
    accounts.value = await mediaAccounts.status();
  } catch {
    accounts.value = [];
  }
}
function acctFor(p: MediaPlatform): MediaAccountStatus | undefined {
  return accounts.value.find((a) => a.platform === p);
}
const currentAccount = computed(() => acctFor(platform.value));

function fmtLastActive(secs: number | null | undefined): string {
  if (!secs) return "";
  const diff = Date.now() / 1000 - secs;
  if (diff < 3600) return `${Math.max(1, Math.floor(diff / 60))} 分钟前`;
  if (diff < 86400) return `${Math.floor(diff / 3600)} 小时前`;
  return `${Math.floor(diff / 86400)} 天前`;
}

// ───────── 运营中心状态（题库 / 队列 / 平台设置）─────────
const topics = ref<MediaTopic[]>([]);
const queue = ref<MediaQueueItem[]>([]);
const settings = ref<MediaPlatformSettings[]>([]);
const stateLoading = ref(false);

async function loadState() {
  stateLoading.value = true;
  try {
    const s = await mediaOps.state();
    topics.value = s.topics ?? [];
    queue.value = s.queue ?? [];
    settings.value = s.settings ?? [];
  } catch {
    topics.value = [];
    queue.value = [];
    settings.value = [];
  } finally {
    stateLoading.value = false;
  }
}

const platformTopics = computed(() => topics.value.filter((t) => t.platform === platform.value));
const platformQueue = computed(() => queue.value.filter((q) => q.platform === platform.value));

// 8 步默认工作流（平台设置为空时兜底）
const DEFAULT_WORKFLOW: MediaWorkflowStep[] = [
  { step: "选题", expertId: "", skillId: "hot-topic-radar", note: "联网抓热点 + 对标爆文" },
  { step: "调研", expertId: "", skillId: "deep-research", note: "多源查证事实与数据" },
  { step: "写作", expertId: "", skillId: "", note: "按平台文风成稿" },
  { step: "质检", expertId: "", skillId: "", note: "事实 / 合规 / 错别字核查" },
  { step: "AI痕迹优化", expertId: "", skillId: "", note: "去机翻腔、长短句交错、口语化" },
  { step: "配图", expertId: "", skillId: "media-publisher", note: "ark_image.py 生成封面/插图" },
  { step: "排版", expertId: "", skillId: "wechat-md-typesetter", note: "套主题渲染成品" },
  { step: "投递", expertId: "", skillId: "media-publisher", note: "draft_uploader.py 存草稿箱" },
];

const currentSettings = computed<MediaPlatformSettings>(
  () =>
    settings.value.find((s) => s.platform === platform.value) ?? {
      platform: platform.value,
      enabled: true,
      sendMode: "ai",
      weeklyQuota: 3,
      workflow: [],
    }
);
const workflowSteps = computed<MediaWorkflowStep[]>(() =>
  currentSettings.value.workflow?.length ? currentSettings.value.workflow : DEFAULT_WORKFLOW
);
const sendMode = computed<"ai" | "manual">(() => currentSettings.value.sendMode ?? "ai");

// ───────── 状态 pill 文案 ─────────
const TOPIC_STATUS: Record<string, string> = {
  pool: "选题池", picked: "已选用", drafted: "已成稿", published: "已发布", rejected: "已弃用",
};
const TOPIC_STATUS_LIST = ["pool", "picked", "drafted", "published", "rejected"];
const QUEUE_STATUS: Record<string, string> = {
  queued: "排队中", running: "生产中", draft_uploaded: "草稿已传", done: "已完成", failed: "失败",
};
const QUEUE_STATUS_LIST = ["queued", "running", "draft_uploaded", "done", "failed"];

// ═══════════ 题库 CRUD ═══════════
const newTopic = ref({ title: "", angle: "", keywords: "" });
const editing = ref<string | null>(null);
const editBuf = ref({ title: "", angle: "", note: "" });

async function addTopic() {
  const t = newTopic.value.title.trim();
  if (!t) return;
  error.value = null;
  const kws = newTopic.value.keywords.split(/[,，、\s]+/).map((s) => s.trim()).filter(Boolean);
  try {
    const created = await mediaOps.topicAdd(platform.value, t, newTopic.value.angle.trim(), kws, "manual");
    topics.value = [created, ...topics.value];
  } catch (e: any) {
    error.value = e?.message ?? String(e);
  }
  newTopic.value = { title: "", angle: "", keywords: "" };
}
function startEdit(t: MediaTopic) {
  editing.value = t.id;
  editBuf.value = { title: t.title, angle: t.angle, note: t.note };
}
async function saveEdit(t: MediaTopic) {
  try {
    const updated = await mediaOps.topicUpdate(t.id, {
      title: editBuf.value.title.trim() || t.title,
      angle: editBuf.value.angle,
      note: editBuf.value.note,
    });
    topics.value = topics.value.map((x) => (x.id === t.id ? updated : x));
  } catch (e: any) {
    error.value = e?.message ?? String(e);
  }
  editing.value = null;
}
async function delTopic(id: string) {
  try {
    await mediaOps.topicDelete(id);
    topics.value = topics.value.filter((t) => t.id !== id);
  } catch (e: any) {
    error.value = e?.message ?? String(e);
  }
}
async function setTopicStatus(t: MediaTopic, status: string) {
  try {
    const updated = await mediaOps.topicUpdate(t.id, { status });
    topics.value = topics.value.map((x) => (x.id === t.id ? updated : x));
  } catch (e: any) {
    error.value = e?.message ?? String(e);
  }
}

// ═══════════ 队列 CRUD ═══════════
async function setQueueStatus(q: MediaQueueItem, status: string) {
  try {
    const updated = await mediaOps.queueUpdate(q.id, { status });
    queue.value = queue.value.map((x) => (x.id === q.id ? updated : x));
  } catch (e: any) {
    error.value = e?.message ?? String(e);
  }
}
async function delQueue(id: string) {
  try {
    await mediaOps.queueDelete(id);
    queue.value = queue.value.filter((q) => q.id !== id);
  } catch (e: any) {
    error.value = e?.message ?? String(e);
  }
}

// ───────── 拼 prompt → 交给对话执行 ─────────
async function ensureConv(): Promise<string> {
  let project = app.projects.find((p) => p.name === PROJECT_NAME);
  let projectId: string | null = project?.id ?? null;
  if (!projectId) {
    await app.createProject(PROJECT_NAME);
    projectId = app.currentProjectId;
    if (!projectId) throw new Error("创建自媒体运营项目失败");
  }
  const conv = await app.createConversation(projectId);
  return conv.id;
}

const launching = ref(false);

// AI 补充选题：hot-topic-radar + 平台名 + 现有题目避重
function suggestPrompt(): string {
  const existing = platformTopics.value.map((t) => `- ${t.title}`).join("\n");
  const lines = [
    `请用「hot-topic-radar」（选题雷达）技能为【${platformName.value}】平台联网抓取近期热点，给我 5 个适合该平台的具体选题。`,
    "每个选题给出：① 一句话标题　② 切入角度　③ 为什么现在值得写（时效/流量点）。编号列出。",
  ];
  if (existing) {
    lines.push("", "【已有选题，请避免重复】", existing);
  }
  lines.push(
    "",
    "我挑中编号后，你再帮我把它们落进题库（说明它们各自的关键词）。此刻先只给候选，不要成稿。"
  );
  return lines.join("\n");
}
async function aiSuggestTopics() {
  if (launching.value) return;
  error.value = null;
  launching.value = true;
  try {
    const id = await ensureConv();
    app.setView("chat");
    await chat.send(id, suggestPrompt(), `📡 ${platformName.value}·AI 补充选题`, [], {
      permissionMode: "auto_current",
      skillIds: ["hot-topic-radar", "deep-research"],
      useKb: true,
    });
  } catch (e: any) {
    error.value = e?.message ?? String(e);
    app.setView("media_ops");
  } finally {
    launching.value = false;
  }
}

// 开始生产：完整生产 prompt
const prodTopicId = ref<string>("");
const producibleTopics = computed(() =>
  platformTopics.value.filter((t) => t.status === "pool" || t.status === "picked")
);

function productionPrompt(topic: MediaTopic): string {
  const plat = platformName.value;
  const mode = sendMode.value;
  const lines: string[] = [];
  lines.push(
    `我要在【${plat}】平台生产并投递一篇内容。请严格按下面的运营工作流一步步执行，除「选题」外全程自行拍板做到底，把关键判断写出来供我复盘。`
  );

  lines.push("", "【选题 · 已锁定，直接用，不要再改】");
  lines.push(`标题：${topic.title}`);
  if (topic.angle) lines.push(`角度：${topic.angle}`);
  if (topic.keywords?.length) lines.push(`关键词：${topic.keywords.join("、")}`);

  lines.push("", "【工作流 · 按序执行】");
  workflowSteps.value.forEach((s, i) => {
    const bits: string[] = [];
    if (s.skillId) bits.push(`技能=${s.skillId}`);
    if (s.expertId) bits.push(`专家=${s.expertId}`);
    if (s.note) bits.push(s.note);
    lines.push(`${i + 1}. ${s.step}${bits.length ? "：" + bits.join("；") : ""}`);
  });

  lines.push("", "【配图 · 统一走 media-publisher 技能的 ark_image.py】");
  lines.push(
    "所有封面/插图统一用「media-publisher」技能的 ark_image.py 生成：",
    '`python <media-publisher>/scripts/ark_image.py --prompt "<画面描述>" --size 1024x1024 --out <落盘路径>`',
    "封面 1 张 + 关键节点插图若干；生图失败时降级为 emoji + 卡片占位，不要卡住流程。"
  );

  lines.push("", `【交付 · 当前为「${mode === "ai" ? "AI 直传草稿箱" : "手动辅助"}」模式】`);
  if (mode === "ai") {
    lines.push(
      "成稿并排版后，调用「media-publisher」技能的 draft_uploader.py 把成品直传到平台草稿箱（**只存草稿，绝不自动发布**）：",
      `\`python <media-publisher>/scripts/draft_uploader.py --platform ${platform.value} --title "<标题>" --content <正文文件> --images <图片目录>\``
    );
    if (platform.value === "wechat")
      lines.push("公众号：先用「wechat-md-typesetter」把正文排成微信兼容 HTML，再交给 draft_uploader.py 传草稿箱。");
    else if (platform.value === "xhs")
      lines.push("小红书：走「post-to-xhs」把图卡 PNG + 文案填进创作页存草稿（只填不发）。");
    else
      lines.push(`${plat}：draft_uploader.py 会用该平台已保存的登录态直传草稿箱。`);
  } else {
    lines.push(
      "先产出干净的 markdown 正文 + 配好的图片，再调用 draft_uploader.py 的**手动辅助模式**：",
      `\`python <media-publisher>/scripts/draft_uploader.py --platform ${platform.value} --mode manual --title "<标题>" --content <正文文件>\``,
      "它会打开该平台的编辑页并把正文复制进剪贴板，我自己粘贴、核对后手动发布。"
    );
  }

  lines.push(
    "",
    "【完成后】把成品文件的绝对路径、草稿箱状态、每步的关键判断汇总汇报给我。"
  );
  return lines.join("\n");
}

function prodSkillIds(): string[] {
  const ids = new Set<string>();
  ids.add(PIPELINE_SKILL[platform.value]);
  ids.add("media-publisher");
  ids.add("hot-topic-radar");
  if (platform.value === "wechat") ids.add("wechat-md-typesetter");
  if (platform.value === "xhs") ids.add("post-to-xhs");
  workflowSteps.value.forEach((s) => {
    if (s.skillId && !s.skillId.startsWith("__")) ids.add(s.skillId);
  });
  return Array.from(ids);
}

async function startProduction() {
  if (launching.value) return;
  const topic = producibleTopics.value.find((t) => t.id === prodTopicId.value) ?? producibleTopics.value[0];
  if (!topic) {
    error.value = "题库里还没有可生产的选题，先在「选题·题库」加一个。";
    return;
  }
  error.value = null;
  launching.value = true;
  try {
    const id = await ensureConv();
    const prompt = productionPrompt(topic);
    const display = `🚀 ${platformName.value}·开始生产：${topic.title.slice(0, 20)}`;
    app.setView("chat");
    await chat.send(id, prompt, display, [], {
      permissionMode: "auto_current",
      skillIds: prodSkillIds(),
      useKb: true,
      goal: `把这条${platformName.value}选题从成稿→配图→排版→存草稿一路做完`,
    });
    // 登记队列 + 选题置为「已选用」（失败不阻断已跳转的对话）
    try {
      const qi = await mediaOps.queueAdd(platform.value, topic.title, topic.id);
      queue.value = [qi, ...queue.value];
    } catch {
      /* 后端未就绪：忽略 */
    }
    try {
      const updated = await mediaOps.topicUpdate(topic.id, { status: "picked" });
      topics.value = topics.value.map((x) => (x.id === topic.id ? updated : x));
    } catch {
      /* 忽略 */
    }
  } catch (e: any) {
    error.value = e?.message ?? String(e);
    app.setView("media_ops");
  } finally {
    launching.value = false;
  }
}

// ───────── 账号动作 ─────────
async function openAccount(p: MediaPlatform, target: "login" | "draft") {
  if (accBusy.value) return;
  accBusy.value = p + target;
  accMsg.value = null;
  error.value = null;
  try {
    const res = await mediaAccounts.open(p, target);
    accMsg.value = res?.message ?? (target === "login" ? "已打开登录窗口" : "已打开草稿箱");
    if (target === "login") toast.info("窗口会一直保持，登录后直接关掉即可，登录态永久保留");
    setTimeout(loadAccounts, 800);
  } catch (e: any) {
    error.value = e?.message ?? String(e);
  } finally {
    accBusy.value = null;
  }
}
async function forgetAccount(p: MediaPlatform) {
  if (accBusy.value) return;
  accBusy.value = p + "forget";
  accMsg.value = null;
  error.value = null;
  try {
    accMsg.value = await mediaAccounts.forget(p);
    await loadAccounts();
  } catch (e: any) {
    error.value = e?.message ?? String(e);
  } finally {
    accBusy.value = null;
  }
}
async function toggleSendMode() {
  const next: "ai" | "manual" = sendMode.value === "ai" ? "manual" : "ai";
  try {
    const updated = await mediaOps.settingsSet(platform.value, { sendMode: next });
    settings.value = upsertSettings(updated);
    toast.info(next === "ai" ? "已切到 AI 直传草稿箱" : "已切到手动辅助模式");
  } catch (e: any) {
    error.value = e?.message ?? String(e);
  }
}
function upsertSettings(s: MediaPlatformSettings): MediaPlatformSettings[] {
  const exists = settings.value.some((x) => x.platform === s.platform);
  return exists ? settings.value.map((x) => (x.platform === s.platform ? s : x)) : [...settings.value, s];
}

// ───────── 专家团 / 工作流步骤 抽屉 ─────────
interface ExpertLite {
  id: string;
  name: string;
  icon?: string;
  role?: string;
  enabled?: boolean;
}
const experts = ref<ExpertLite[]>([]);
async function loadExperts() {
  try {
    experts.value = (await expertMedia.list()) as ExpertLite[];
  } catch {
    experts.value = [];
  }
}
function expertName(id: string): string {
  return experts.value.find((e) => e.id === id)?.name ?? id;
}

type Drawer =
  | { kind: "step"; index: number; expertId: string }
  | { kind: "expert"; expertId: string };
const drawer = ref<Drawer | null>(null);
const docText = ref("");
const docLoading = ref(false);
const docErr = ref<string | null>(null);
const overlayText = ref("");
const overlaySource = ref<string>("");
const savingOverlay = ref(false);
// 步骤编辑缓冲
const stepBuf = ref({ skillId: "", note: "" });
const savingStep = ref(false);

async function loadDoc(expertId: string) {
  docText.value = "";
  overlayText.value = "";
  overlaySource.value = "";
  docErr.value = null;
  if (!expertId) {
    docErr.value = "该步骤未指派专家，无提示词可看。";
    return;
  }
  docLoading.value = true;
  try {
    docText.value = await expertMedia.doc(expertId, platform.value);
  } catch (e: any) {
    docErr.value = "拼接提示词读取失败（后端开发中）：" + (e?.message ?? String(e));
  }
  try {
    const o = await expertMedia.overlayGet(platform.value, expertId);
    overlayText.value = o?.content ?? "";
    overlaySource.value = o?.source ?? "none";
  } catch {
    overlayText.value = "";
    overlaySource.value = "none";
  } finally {
    docLoading.value = false;
  }
}
async function openStepDrawer(i: number) {
  const s = workflowSteps.value[i];
  drawer.value = { kind: "step", index: i, expertId: s.expertId };
  stepBuf.value = { skillId: s.skillId, note: s.note };
  await loadDoc(s.expertId);
}
async function openExpertDrawer(id: string) {
  drawer.value = { kind: "expert", expertId: id };
  await loadDoc(id);
}
function closeDrawer() {
  drawer.value = null;
}
async function saveOverlay() {
  if (!drawer.value) return;
  savingOverlay.value = true;
  error.value = null;
  try {
    await expertMedia.overlaySet(platform.value, drawer.value.expertId, overlayText.value);
    overlaySource.value = "runtime";
    toast.success("已保存该专家的本平台补丁");
  } catch (e: any) {
    error.value = e?.message ?? String(e);
  } finally {
    savingOverlay.value = false;
  }
}
async function saveStep() {
  if (!drawer.value || drawer.value.kind !== "step") return;
  savingStep.value = true;
  error.value = null;
  const idx = drawer.value.index;
  const wf = workflowSteps.value.map((s, i) =>
    i === idx ? { ...s, skillId: stepBuf.value.skillId, note: stepBuf.value.note } : { ...s }
  );
  try {
    const updated = await mediaOps.settingsSet(platform.value, { workflow: wf });
    settings.value = upsertSettings(updated);
    toast.success("已保存步骤配置");
  } catch (e: any) {
    error.value = e?.message ?? String(e);
  } finally {
    savingStep.value = false;
  }
}

// ───────── API 中心（火山方舟 / ark）─────────
const arkCfg = ref<ArkConfig>({ apiKey: "", baseUrl: "", imageModel: "", chatModel: "" });
const showKey = ref(false);
const arkSaving = ref(false);
const arkTest = ref<{ kind: "conn" | "image" | "chat"; ok: boolean; text: string } | null>(null);
const arkTesting = ref<string | null>(null);

async function loadArk() {
  try {
    arkCfg.value = await ark.configGet();
  } catch {
    /* 后端未就绪 */
  }
}
async function saveArk() {
  arkSaving.value = true;
  error.value = null;
  try {
    arkCfg.value = await ark.configSet({ ...arkCfg.value });
    toast.success("已保存 API 配置");
  } catch (e: any) {
    error.value = e?.message ?? String(e);
  } finally {
    arkSaving.value = false;
  }
}
async function testConn() {
  arkTesting.value = "conn";
  arkTest.value = null;
  try {
    const r = await ark.test();
    arkTest.value = { kind: "conn", ok: r.ok, text: r.ok ? `连通 · 延迟 ${r.latencyMs}ms` : r.message };
  } catch (e: any) {
    arkTest.value = { kind: "conn", ok: false, text: e?.message ?? String(e) };
  } finally {
    arkTesting.value = null;
  }
}
async function testImage() {
  arkTesting.value = "image";
  arkTest.value = null;
  try {
    const r = await ark.imageGenerate("一只可爱的猫", "1024x1024");
    arkTest.value = { kind: "image", ok: true, text: `生图成功 · ${r.model} · 落盘：${r.path}` };
  } catch (e: any) {
    arkTest.value = { kind: "image", ok: false, text: e?.message ?? String(e) };
  } finally {
    arkTesting.value = null;
  }
}
async function testChat() {
  arkTesting.value = "chat";
  arkTest.value = null;
  try {
    const r = await ark.chatTest("你好");
    arkTest.value = { kind: "chat", ok: r.ok, text: `${r.content.slice(0, 60)}（${r.latencyMs}ms）` };
  } catch (e: any) {
    arkTest.value = { kind: "chat", ok: false, text: e?.message ?? String(e) };
  } finally {
    arkTesting.value = null;
  }
}

// ───────── 生命周期 ─────────
watch(platform, () => {
  prodTopicId.value = "";
  closeDrawer();
});
onMounted(async () => {
  app.refreshProjects?.();
  loadAccounts();
  loadState();
  loadExperts();
  loadArk();
});
</script>

<template>
  <div class="mo">
    <!-- 顶栏 + 平台切换器 -->
    <header class="mo-head">
      <div class="mo-head-top">
        <Megaphone :size="20" :stroke-width="1.7" class="mo-icon" />
        <h1 class="mo-title">运营中心</h1>
        <span class="mo-sub">题库 → 工作流 → 生产 → 投递，一处调度全平台</span>
      </div>
      <div class="mo-plats">
        <button
          v-for="p in MEDIA_PLATFORMS"
          :key="p.id"
          class="mo-plat-tab"
          :class="{ active: platform === p.id }"
          @click="platform = p.id"
        >
          <span class="mo-plat-dot" :class="{ on: acctFor(p.id)?.bound }" />
          <span>{{ p.name }}</span>
        </button>
      </div>
    </header>

    <div class="mo-body">
      <!-- 左：区块导航 -->
      <nav class="mo-nav">
        <button
          v-for="z in ZONES"
          :key="z.id"
          class="mo-nav-item"
          :class="{ active: zone === z.id }"
          @click="zone = z.id"
        >
          <component :is="z.icon" :size="16" /><span>{{ z.label }}</span>
        </button>
        <div class="mo-nav-foot">
          <p>当前平台：<b>{{ platformName }}</b><br />切换顶部平台，下方全部区块随之过滤。</p>
        </div>
      </nav>

      <!-- 右：工作区 -->
      <div class="mo-work">
        <div v-if="error" class="mo-error">{{ error }}</div>

        <!-- ════════ 1 选题·题库 ════════ -->
        <section v-if="zone === 'topics'" class="mo-sec">
          <div class="mo-block">
            <div class="mo-block-h">
              <Plus :size="15" /> 手动加选题
              <button class="mo-primary sm ml-auto" :disabled="launching" @click="aiSuggestTopics">
                <Loader v-if="launching" :size="14" class="spin" /><Sparkles v-else :size="14" />
                <span>AI 补充选题</span>
              </button>
            </div>
            <input v-model="newTopic.title" class="mo-input" placeholder="选题标题" @keydown.enter="addTopic" />
            <div class="mo-row">
              <input v-model="newTopic.angle" class="mo-input flex" placeholder="切入角度（可选）" />
              <input v-model="newTopic.keywords" class="mo-input flex" placeholder="关键词，逗号分隔（可选）" />
              <button class="mo-primary sm" @click="addTopic"><Plus :size="14" /> 加入</button>
            </div>
          </div>

          <div class="mo-block">
            <div class="mo-block-h">
              <Radar :size="15" /> {{ platformName }}·题库（{{ platformTopics.length }}）
              <button class="mo-ghost ml-auto" :disabled="stateLoading" @click="loadState">
                <RefreshCw :size="13" /><span>刷新</span>
              </button>
            </div>
            <div v-if="!platformTopics.length" class="mo-empty">
              该平台还没有选题。手动加一个，或点「AI 补充选题」让选题雷达抓几个给你。
            </div>
            <div v-else class="mo-topic-list">
              <div v-for="t in platformTopics" :key="t.id" class="mo-topic">
                <template v-if="editing === t.id">
                  <div class="mo-topic-edit">
                    <input v-model="editBuf.title" class="mo-input" placeholder="标题" />
                    <input v-model="editBuf.angle" class="mo-input" placeholder="角度" />
                    <input v-model="editBuf.note" class="mo-input" placeholder="备注" />
                    <div class="mo-row">
                      <button class="mo-primary sm" @click="saveEdit(t)"><Check :size="13" /> 保存</button>
                      <button class="mo-ghost" @click="editing = null"><X :size="13" /> 取消</button>
                    </div>
                  </div>
                </template>
                <template v-else>
                  <div class="mo-topic-main">
                    <div class="mo-topic-title">{{ t.title }}</div>
                    <div v-if="t.angle" class="mo-topic-angle">{{ t.angle }}</div>
                    <div v-if="t.keywords?.length" class="mo-topic-kws">
                      <span v-for="k in t.keywords" :key="k" class="mo-kw">{{ k }}</span>
                    </div>
                  </div>
                  <select
                    class="mo-pill-select"
                    :class="'st-' + t.status"
                    :value="t.status"
                    @change="setTopicStatus(t, ($event.target as HTMLSelectElement).value)"
                  >
                    <option v-for="s in TOPIC_STATUS_LIST" :key="s" :value="s">{{ TOPIC_STATUS[s] }}</option>
                  </select>
                  <button class="mo-icon-btn" title="编辑" @click="startEdit(t)"><Pencil :size="14" /></button>
                  <button class="mo-icon-btn danger" title="删除" @click="delTopic(t.id)"><Trash2 :size="14" /></button>
                </template>
              </div>
            </div>
          </div>
        </section>

        <!-- ════════ 2 工作流 ════════ -->
        <section v-else-if="zone === 'workflow'" class="mo-sec">
          <div class="mo-block">
            <div class="mo-block-h"><WorkflowIcon :size="15" /> {{ platformName }}·生产工作流</div>
            <p class="mo-desc">点开任一步骤看该专家在本平台的拼接提示词、编辑本平台补丁，或改这一步的技能 / 备注。</p>
            <div class="mo-steps">
              <div v-for="(s, i) in workflowSteps" :key="i" class="mo-stepcard" @click="openStepDrawer(i)">
                <span class="mo-step-idx">{{ i + 1 }}</span>
                <div class="mo-step-b">
                  <div class="mo-step-name">{{ s.step }}</div>
                  <div class="mo-step-meta">
                    <span v-if="s.expertId" class="mo-step-tag">👤 {{ expertName(s.expertId) }}</span>
                    <span v-if="s.skillId" class="mo-step-tag">🧩 {{ s.skillId }}</span>
                    <span v-if="!s.expertId && !s.skillId" class="mo-muted">未指派</span>
                  </div>
                  <div v-if="s.note" class="mo-step-note">{{ s.note }}</div>
                </div>
                <ChevronRight :size="15" class="mo-muted" />
              </div>
            </div>
          </div>

          <div class="mo-block mo-prod">
            <div class="mo-block-h"><Zap :size="15" /> 开始生产</div>
            <p class="mo-desc">
              从题库挑一个选题，按上面的工作流一口气跑到存草稿。当前交付模式：
              <b>{{ sendMode === "ai" ? "AI 直传草稿箱" : "手动辅助" }}</b>（去「账号」区块切换）。
            </p>
            <div class="mo-row">
              <select v-model="prodTopicId" class="mo-input flex">
                <option value="">— 挑一个选题（{{ producibleTopics.length }} 个可用）—</option>
                <option v-for="t in producibleTopics" :key="t.id" :value="t.id">{{ t.title }}</option>
              </select>
              <button class="mo-primary" :disabled="launching || !producibleTopics.length" @click="startProduction">
                <Loader v-if="launching" :size="15" class="spin" /><Send v-else :size="15" />
                <span>开始生产</span>
              </button>
            </div>
          </div>
        </section>

        <!-- ════════ 3 规划队列 ════════ -->
        <section v-else-if="zone === 'queue'" class="mo-sec">
          <div class="mo-block">
            <div class="mo-block-h">
              <ListChecks :size="15" /> {{ platformName }}·规划队列（{{ platformQueue.length }}）
              <button class="mo-ghost ml-auto" :disabled="stateLoading" @click="loadState">
                <RefreshCw :size="13" /><span>刷新</span>
              </button>
            </div>
            <div v-if="!platformQueue.length" class="mo-empty">队列为空。去「工作流」点「开始生产」，任务会登记到这里。</div>
            <div v-else class="mo-queue-list">
              <div v-for="q in platformQueue" :key="q.id" class="mo-queue">
                <div class="mo-queue-main">
                  <div class="mo-queue-title">{{ q.title }}</div>
                  <div class="mo-queue-meta">
                    <span v-if="q.scheduledAt">🕒 {{ q.scheduledAt }}</span>
                    <span v-if="q.note" class="mo-muted">{{ q.note }}</span>
                    <span v-if="q.articlePath" class="mo-muted">📄 已产出</span>
                  </div>
                </div>
                <select
                  class="mo-pill-select"
                  :class="'qs-' + q.status"
                  :value="q.status"
                  @change="setQueueStatus(q, ($event.target as HTMLSelectElement).value)"
                >
                  <option v-for="s in QUEUE_STATUS_LIST" :key="s" :value="s">{{ QUEUE_STATUS[s] }}</option>
                </select>
                <button class="mo-icon-btn danger" title="删除" @click="delQueue(q.id)"><Trash2 :size="14" /></button>
              </div>
            </div>
          </div>
        </section>

        <!-- ════════ 4 账号 ════════ -->
        <section v-else-if="zone === 'accounts'" class="mo-sec">
          <div v-if="accMsg" class="mo-acct-msg">{{ accMsg }}</div>

          <!-- 发送方式大开关 -->
          <div class="mo-block mo-sendmode">
            <div class="mo-block-h"><Send :size="15" /> {{ platformName }}·发送方式</div>
            <div class="mo-mode-switch">
              <button class="mo-mode" :class="{ on: sendMode === 'ai' }" @click="sendMode !== 'ai' && toggleSendMode()">
                <div class="mo-mode-t">AI 直传草稿箱</div>
                <div class="mo-mode-d">成稿排版后自动调 draft_uploader.py 直接把草稿传进平台后台，你只需最后点发布。</div>
              </button>
              <button class="mo-mode" :class="{ on: sendMode === 'manual' }" @click="sendMode !== 'manual' && toggleSendMode()">
                <div class="mo-mode-t">手动辅助</div>
                <div class="mo-mode-d">产出 md + 图后，脚本打开编辑页并把正文复制到剪贴板，你自己粘贴、核对、发布。</div>
              </button>
            </div>
          </div>

          <!-- 当前平台账号卡 -->
          <div class="mo-block">
            <div class="mo-block-h">
              <KeyRound :size="15" /> {{ platformName }}·账号
              <button class="mo-ghost ml-auto" @click="loadAccounts"><RefreshCw :size="13" /><span>刷新状态</span></button>
            </div>
            <div v-if="currentAccount" class="mo-acct-card">
              <div class="mo-acct-head">
                <span class="mo-acct-name">{{ currentAccount.label }}</span>
                <span class="mo-acct-badge" :class="{ on: currentAccount.bound }">
                  <Check v-if="currentAccount.bound" :size="12" />{{ currentAccount.bound ? " 已登录" : "未登录" }}
                </span>
              </div>
              <div v-if="currentAccount.bound && currentAccount.lastActive" class="mo-acct-meta">
                最近活动：{{ fmtLastActive(currentAccount.lastActive) }}
              </div>
              <p class="mo-acct-detail">{{ currentAccount.detail }}</p>
              <div class="mo-acct-path" :title="currentAccount.profileDir">登录态目录：{{ currentAccount.profileDir }}</div>
              <div class="mo-acct-actions">
                <button class="mo-primary sm" :disabled="!!accBusy" @click="openAccount(platform, 'login')">
                  <Loader v-if="accBusy === platform + 'login'" :size="14" class="spin" /><KeyRound v-else :size="14" />
                  <span>打开登录窗口</span>
                </button>
                <button class="mo-ghost" :disabled="!!accBusy" @click="openAccount(platform, 'draft')">
                  <Loader v-if="accBusy === platform + 'draft'" :size="13" class="spin" /><FileText v-else :size="13" />
                  <span>打开草稿箱</span>
                </button>
                <button v-if="currentAccount.bound" class="mo-ghost danger" :disabled="!!accBusy" @click="forgetAccount(platform)">
                  <LogOut :size="13" /><span>解绑</span>
                </button>
              </div>
              <p class="mo-hint">窗口会一直保持，登录后直接关掉即可，登录态永久保留；下次发文自动复用、不用重扫。</p>
            </div>
            <div v-else class="mo-empty">读取账号状态中…（后端开发中时此处为空）</div>
          </div>
        </section>

        <!-- ════════ 5 专家团 ════════ -->
        <section v-else-if="zone === 'experts'" class="mo-sec">
          <div class="mo-block">
            <div class="mo-block-h">
              <Users :size="15" /> 自媒体专家团（{{ experts.length }}）· {{ platformName }} 视角
              <button class="mo-ghost ml-auto" @click="loadExperts"><RefreshCw :size="13" /><span>刷新</span></button>
            </div>
            <div v-if="!experts.length" class="mo-empty">专家团读取中…（后端开发中时此处为空）</div>
            <div v-else class="mo-expert-wall">
              <button v-for="e in experts" :key="e.id" class="mo-expert" @click="openExpertDrawer(e.id)">
                <span class="mo-expert-ico">{{ e.icon || "👤" }}</span>
                <div class="mo-expert-b">
                  <div class="mo-expert-n">{{ e.name }}</div>
                  <div class="mo-expert-r">{{ e.role || "自媒体专家" }}</div>
                </div>
                <span class="mo-expert-badge" :class="{ off: e.enabled === false }">
                  {{ e.enabled === false ? "停用" : "启用" }}
                </span>
              </button>
            </div>
          </div>
        </section>

        <!-- ════════ 6 API 中心 ════════ -->
        <section v-else-if="zone === 'api'" class="mo-sec">
          <div class="mo-block">
            <div class="mo-block-h"><Cpu :size="15" /> 火山方舟 API</div>
            <div class="mo-form">
              <label class="mo-field">
                <span>API Key</span>
                <div class="mo-key-row">
                  <input :type="showKey ? 'text' : 'password'" v-model="arkCfg.apiKey" class="mo-input flex" placeholder="ark api key" />
                  <button class="mo-icon-btn" @click="showKey = !showKey" :title="showKey ? '隐藏' : '明文'">
                    <EyeOff v-if="showKey" :size="15" /><Eye v-else :size="15" />
                  </button>
                </div>
              </label>
              <label class="mo-field"><span>Base URL</span><input v-model="arkCfg.baseUrl" class="mo-input" placeholder="https://ark.cn-beijing.volces.com/api/v3" /></label>
              <label class="mo-field"><span>生图模型</span><input v-model="arkCfg.imageModel" class="mo-input" placeholder="doubao-seedream…" /></label>
              <label class="mo-field"><span>对话模型</span><input v-model="arkCfg.chatModel" class="mo-input" placeholder="doubao-…" /></label>
            </div>
            <div class="mo-row">
              <button class="mo-primary sm" :disabled="arkSaving" @click="saveArk">
                <Loader v-if="arkSaving" :size="14" class="spin" /><Save v-else :size="14" /><span>保存</span>
              </button>
              <button class="mo-ghost" :disabled="!!arkTesting" @click="testConn">
                <Loader v-if="arkTesting === 'conn'" :size="13" class="spin" /><Zap v-else :size="13" /><span>测试连接</span>
              </button>
              <button class="mo-ghost" :disabled="!!arkTesting" @click="testImage">
                <Loader v-if="arkTesting === 'image'" :size="13" class="spin" /><Sparkles v-else :size="13" /><span>测试生图</span>
              </button>
              <button class="mo-ghost" :disabled="!!arkTesting" @click="testChat">
                <Loader v-if="arkTesting === 'chat'" :size="13" class="spin" /><Send v-else :size="13" /><span>测试对话</span>
              </button>
            </div>
            <div v-if="arkTest" class="mo-test" :class="{ ok: arkTest.ok, bad: !arkTest.ok }">
              <Check v-if="arkTest.ok" :size="14" /><X v-else :size="14" /><span>{{ arkTest.text }}</span>
            </div>
          </div>

          <div class="mo-block mo-minimax">
            <div class="mo-block-h">🎁 MiniMax 通道</div>
            <p class="mo-desc">
              内置粉丝福利 <b>MiniMax-M3</b> 通道，无需自备 key 即可跑对话 / 生成。想切换时在左下角
              <b>「API 供应商」</b>坞里选择该通道即可，本运营中心的生产链路会自动沿用当前对话供应商。
            </p>
          </div>
        </section>

        <!-- ════════ 7 知识库 ════════ -->
        <section v-else class="mo-sec">
          <div class="mo-block mo-kb">
            <div class="mo-kb-ico"><BookOpen :size="26" /></div>
            <div class="mo-kb-b">
              <div class="mo-kb-t">llmwiki 知识库</div>
              <div class="mo-kb-d">选题避重、事实补料、风格沉淀都靠它。生产链路开「知识库严格搜索」时会读取这里的结构化 wiki。</div>
            </div>
            <button class="mo-primary" @click="app.setView('wiki')">
              <ExternalLink :size="15" /><span>打开知识库</span>
            </button>
          </div>
        </section>
      </div>
    </div>

    <!-- ════════ 抽屉：专家提示词 / 步骤配置 ════════ -->
    <div v-if="drawer" class="mo-drawer-mask" @click.self="closeDrawer">
      <div class="mo-drawer">
        <div class="mo-drawer-h">
          <span v-if="drawer.kind === 'step'">
            步骤 {{ drawer.index + 1 }}· {{ workflowSteps[drawer.index].step }}
          </span>
          <span v-else>专家 · {{ expertName(drawer.expertId) }}</span>
          <button class="mo-icon-btn" @click="closeDrawer"><X :size="16" /></button>
        </div>
        <div class="mo-drawer-body">
          <!-- 步骤专属：改技能 / 备注 -->
          <div v-if="drawer.kind === 'step'" class="mo-block">
            <div class="mo-block-h">这一步的配置</div>
            <label class="mo-field"><span>技能 ID（skillId）</span><input v-model="stepBuf.skillId" class="mo-input" placeholder="如 deep-research" /></label>
            <label class="mo-field"><span>备注</span><input v-model="stepBuf.note" class="mo-input" placeholder="这一步要做什么" /></label>
            <button class="mo-primary sm" :disabled="savingStep" @click="saveStep">
              <Loader v-if="savingStep" :size="14" class="spin" /><Save v-else :size="14" /><span>保存步骤</span>
            </button>
          </div>

          <!-- 拼接后的提示词 -->
          <div class="mo-block">
            <div class="mo-block-h"><FileText :size="14" /> 该专家在 {{ platformName }} 的拼接提示词</div>
            <div v-if="docLoading" class="mo-empty"><Loader :size="14" class="spin" /> 读取中…</div>
            <div v-else-if="docErr" class="mo-empty">{{ docErr }}</div>
            <pre v-else class="mo-doc">{{ docText || "（空）" }}</pre>
          </div>

          <!-- 本平台补丁 overlay -->
          <div v-if="drawer.expertId" class="mo-block">
            <div class="mo-block-h">
              <Pencil :size="14" /> 本平台补丁（overlay）
              <span class="mo-muted" style="margin-left: 6px; font-size: 11px">来源：{{ overlaySource || "none" }}</span>
            </div>
            <textarea v-model="overlayText" class="mo-textarea" rows="6" placeholder="在这里为该专家写一段只在本平台生效的补充提示词…" />
            <button class="mo-primary sm" :disabled="savingOverlay" @click="saveOverlay">
              <Loader v-if="savingOverlay" :size="14" class="spin" /><Save v-else :size="14" /><span>保存补丁</span>
            </button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.mo { height: 100%; display: flex; flex-direction: column; overflow: hidden; background: var(--bg); }

/* 顶栏 */
.mo-head { border-bottom: 1px solid var(--border-soft); background: var(--panel); }
.mo-head-top { display: flex; align-items: center; gap: 10px; padding: 13px 22px 8px; }
.mo-icon { color: var(--primary); }
.mo-title { font-family: var(--serif); font-size: 17px; font-weight: 600; color: var(--text); }
.mo-sub { font-size: 12.5px; color: var(--muted); margin-left: 6px; }
.mo-plats { display: flex; gap: 6px; padding: 0 20px 11px; flex-wrap: wrap; }
.mo-plat-tab {
  display: inline-flex; align-items: center; gap: 7px;
  padding: 6px 13px; border: 1px solid var(--border); border-radius: 8px;
  background: var(--bg); color: var(--text-2); font-size: 13px; font-weight: 500; cursor: pointer;
  transition: border-color 0.15s, background 0.15s, color 0.15s;
}
.mo-plat-tab:hover { border-color: var(--primary); }
.mo-plat-tab.active { border-color: var(--primary); background: var(--primary-soft); color: var(--primary-deep); font-weight: 600; }
.mo-plat-dot { width: 7px; height: 7px; border-radius: 50%; background: var(--border-strong); flex: 0 0 auto; }
.mo-plat-dot.on { background: #39d09a; box-shadow: 0 0 0 3px rgba(57, 208, 154, 0.2); }

.mo-body { flex: 1; display: grid; grid-template-columns: 176px 1fr; overflow: hidden; }

/* 左导航 */
.mo-nav {
  border-right: 1px solid var(--border-soft); background: var(--bg-soft);
  padding: 14px 10px; display: flex; flex-direction: column; gap: 4px;
}
.mo-nav-item {
  display: flex; align-items: center; gap: 9px;
  padding: 10px 12px; border: none; border-radius: 9px; background: transparent;
  color: var(--text-2); font-size: 13.5px; font-weight: 500; cursor: pointer;
  transition: background 0.15s, color 0.15s; text-align: left;
}
.mo-nav-item:hover { background: var(--panel); color: var(--text); }
.mo-nav-item.active { background: var(--primary-soft); color: var(--primary-deep); font-weight: 600; }
.mo-nav-foot { margin-top: auto; padding: 10px 12px; }
.mo-nav-foot p { font-size: 11.5px; color: var(--muted); line-height: 1.7; margin: 0; }
.mo-nav-foot b { color: var(--primary-deep); }

/* 工作区 */
.mo-work { overflow: auto; padding: 18px 24px; }
.mo-sec { display: flex; flex-direction: column; gap: 16px; max-width: 860px; }

.mo-block {
  border: 1px solid var(--border-soft); border-radius: 12px; background: var(--panel);
  padding: 15px 17px; display: flex; flex-direction: column; gap: 12px;
}
.mo-block-h { display: flex; align-items: center; gap: 8px; font-size: 13.5px; font-weight: 600; color: var(--text); }
.ml-auto { margin-left: auto; }
.mo-desc { font-size: 12.5px; color: var(--text-2); line-height: 1.7; margin: 0; }
.mo-muted { color: var(--muted); }
.mo-hint { font-size: 11.5px; color: var(--muted); line-height: 1.6; margin: 0; }
.mo-empty { font-size: 12.5px; color: var(--muted); padding: 10px 2px; line-height: 1.7; display: flex; align-items: center; gap: 6px; }

/* 输入 */
.mo-input {
  width: 100%; padding: 9px 12px; border: 1px solid var(--border); border-radius: 8px;
  background: var(--bg); color: var(--text); font-size: 13px; box-sizing: border-box;
}
.mo-input:focus { outline: none; border-color: var(--primary); }
.mo-input.flex { flex: 1; min-width: 0; }
.mo-row { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
.mo-textarea {
  width: 100%; resize: vertical; min-height: 90px; box-sizing: border-box;
  padding: 10px 12px; border: 1px solid var(--border); border-radius: 8px;
  background: var(--bg); color: var(--text); font-size: 13px; line-height: 1.7;
}
.mo-textarea:focus { outline: none; border-color: var(--primary); }

/* 按钮 */
.mo-primary {
  display: inline-flex; align-items: center; justify-content: center; gap: 8px;
  padding: 10px 20px; border: none; border-radius: 10px;
  background: var(--primary); color: #fff; font-size: 13.5px; font-weight: 600;
  cursor: pointer; transition: filter 0.15s; white-space: nowrap;
}
.mo-primary.sm { padding: 8px 14px; font-size: 12.5px; }
.mo-primary:hover:not(:disabled) { filter: brightness(1.07); }
.mo-primary:disabled { opacity: 0.55; cursor: default; }
.mo-ghost {
  display: inline-flex; align-items: center; gap: 5px;
  padding: 7px 12px; border: 1px solid var(--border); border-radius: 8px;
  background: transparent; color: var(--text-2); font-size: 12.5px; cursor: pointer;
  transition: border-color 0.15s, color 0.15s; white-space: nowrap;
}
.mo-ghost:hover:not(:disabled) { border-color: var(--primary); color: var(--primary); }
.mo-ghost:disabled { opacity: 0.5; cursor: default; }
.mo-ghost.danger { color: var(--vermilion); }
.mo-ghost.danger:hover:not(:disabled) { border-color: var(--vermilion); color: var(--vermilion); }
.mo-icon-btn {
  display: inline-flex; padding: 6px; border: 1px solid transparent; border-radius: 7px;
  background: transparent; color: var(--muted); cursor: pointer; transition: color 0.15s, border-color 0.15s;
}
.mo-icon-btn:hover { color: var(--primary); border-color: var(--border); }
.mo-icon-btn.danger:hover { color: var(--vermilion); border-color: var(--vermilion); }

.mo-error { padding: 9px 12px; border-radius: 8px; background: var(--vermilion-soft); color: var(--vermilion); font-size: 12.5px; margin-bottom: 14px; }

/* 题库 */
.mo-topic-list { display: flex; flex-direction: column; gap: 8px; }
.mo-topic {
  display: flex; align-items: center; gap: 10px;
  padding: 11px 13px; border: 1px solid var(--border-soft); border-radius: 10px; background: var(--bg);
}
.mo-topic-main { flex: 1; min-width: 0; }
.mo-topic-title { font-size: 13.5px; font-weight: 600; color: var(--text); }
.mo-topic-angle { font-size: 12px; color: var(--text-2); margin-top: 2px; }
.mo-topic-kws { display: flex; flex-wrap: wrap; gap: 4px; margin-top: 5px; }
.mo-kw { font-size: 10.5px; color: var(--muted); background: var(--bg-soft); padding: 1px 7px; border-radius: 999px; }
.mo-topic-edit { flex: 1; display: flex; flex-direction: column; gap: 7px; }

/* pill select */
.mo-pill-select {
  flex: 0 0 auto; font-size: 11px; font-weight: 600; padding: 4px 9px; border-radius: 999px;
  border: 1px solid var(--border); background: var(--bg-soft); color: var(--text-2); cursor: pointer;
}
.mo-pill-select:focus { outline: none; }
.mo-pill-select.st-pool { color: #9db4d0; }
.mo-pill-select.st-picked { color: #6fb6ff; border-color: rgba(111,176,255,.4); }
.mo-pill-select.st-drafted { color: #e6b873; border-color: rgba(230,184,115,.4); }
.mo-pill-select.st-published { color: #6fe0b6; border-color: rgba(57,208,154,.4); }
.mo-pill-select.st-rejected { color: var(--muted); }
.mo-pill-select.qs-queued { color: #9db4d0; }
.mo-pill-select.qs-running { color: #6fb6ff; border-color: rgba(111,176,255,.4); }
.mo-pill-select.qs-draft_uploaded { color: #e6b873; border-color: rgba(230,184,115,.4); }
.mo-pill-select.qs-done { color: #6fe0b6; border-color: rgba(57,208,154,.4); }
.mo-pill-select.qs-failed { color: #ff8f8f; border-color: rgba(255,143,143,.4); }

/* 工作流步骤 */
.mo-steps { display: flex; flex-direction: column; gap: 8px; }
.mo-stepcard {
  display: flex; align-items: center; gap: 12px;
  padding: 11px 13px; border: 1px solid var(--border-soft); border-radius: 10px; background: var(--bg);
  cursor: pointer; transition: border-color 0.15s;
}
.mo-stepcard:hover { border-color: var(--primary); }
.mo-step-idx {
  flex: 0 0 auto; width: 22px; height: 22px; border-radius: 7px;
  display: flex; align-items: center; justify-content: center;
  background: var(--primary); color: #fff; font-size: 12px; font-weight: 700;
}
.mo-step-b { flex: 1; min-width: 0; }
.mo-step-name { font-size: 13.5px; font-weight: 600; color: var(--text); }
.mo-step-meta { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 3px; }
.mo-step-tag { font-size: 11px; color: var(--text-2); background: var(--bg-soft); padding: 1px 8px; border-radius: 6px; }
.mo-step-note { font-size: 11.5px; color: var(--muted); margin-top: 3px; }

.mo-prod .mo-row { margin-top: 2px; }

/* 队列 */
.mo-queue-list { display: flex; flex-direction: column; gap: 8px; }
.mo-queue {
  display: flex; align-items: center; gap: 10px;
  padding: 11px 13px; border: 1px solid var(--border-soft); border-radius: 10px; background: var(--bg);
}
.mo-queue-main { flex: 1; min-width: 0; }
.mo-queue-title { font-size: 13px; font-weight: 600; color: var(--text); }
.mo-queue-meta { display: flex; flex-wrap: wrap; gap: 8px; font-size: 11.5px; color: var(--text-2); margin-top: 3px; }

/* 账号 */
.mo-acct-msg { padding: 9px 12px; border-radius: 8px; font-size: 12.5px; background: rgba(57,208,154,.1); color: #6fe0b6; border: 1px solid rgba(57,208,154,.28); }
.mo-sendmode .mo-mode-switch { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
.mo-mode {
  text-align: left; padding: 13px 15px; border-radius: 11px; border: 1.5px solid var(--border);
  background: var(--bg); cursor: pointer; transition: border-color 0.15s, background 0.15s;
}
.mo-mode:hover { border-color: var(--primary); }
.mo-mode.on { border-color: var(--primary); background: var(--primary-soft); }
.mo-mode-t { font-size: 14px; font-weight: 600; color: var(--text); }
.mo-mode.on .mo-mode-t { color: var(--primary-deep); }
.mo-mode-d { font-size: 11.5px; color: var(--muted); line-height: 1.6; margin-top: 5px; }

.mo-acct-card { display: flex; flex-direction: column; gap: 8px; padding: 15px 16px; border-radius: 12px; border: 1px solid var(--border-soft); background: var(--bg); }
.mo-acct-head { display: flex; align-items: center; gap: 8px; }
.mo-acct-name { font-size: 14.5px; font-weight: 600; color: var(--text); }
.mo-acct-badge {
  margin-left: auto; display: inline-flex; align-items: center; gap: 2px;
  font-size: 11px; font-weight: 600; padding: 2px 9px; border-radius: 999px;
  background: var(--bg-soft); color: var(--muted);
}
.mo-acct-badge.on { background: rgba(57,208,154,.16); color: #6fe0b6; }
.mo-acct-meta { font-size: 11.5px; color: var(--muted); }
.mo-acct-detail { font-size: 12px; color: var(--text-2); line-height: 1.6; margin: 0; }
.mo-acct-path {
  font-size: 10.5px; color: var(--muted); font-family: var(--mono, monospace);
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  padding: 5px 8px; border-radius: 6px; background: var(--bg-soft);
}
.mo-acct-actions { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 4px; }

/* 专家墙 */
.mo-expert-wall { display: grid; grid-template-columns: repeat(auto-fill, minmax(220px, 1fr)); gap: 10px; }
.mo-expert {
  display: flex; align-items: center; gap: 11px; text-align: left;
  padding: 12px 13px; border: 1px solid var(--border); border-radius: 11px; background: var(--bg);
  cursor: pointer; transition: border-color 0.15s;
}
.mo-expert:hover { border-color: var(--primary); }
.mo-expert-ico {
  width: 34px; height: 34px; border-radius: 9px; flex: 0 0 auto;
  display: flex; align-items: center; justify-content: center; font-size: 18px;
  background: rgba(255,255,255,.04);
}
.mo-expert-b { flex: 1; min-width: 0; }
.mo-expert-n { font-size: 13.5px; font-weight: 600; color: var(--text); }
.mo-expert-r { font-size: 11px; color: var(--muted); line-height: 1.4; margin-top: 1px; }
.mo-expert-badge { font-size: 10px; font-weight: 700; padding: 2px 8px; border-radius: 6px; background: rgba(57,208,154,.14); color: #6fe0b6; }
.mo-expert-badge.off { background: var(--bg-soft); color: var(--muted); }

/* API 表单 */
.mo-form { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
.mo-field { display: flex; flex-direction: column; gap: 5px; }
.mo-field > span { font-size: 11.5px; color: var(--text-2); font-weight: 500; }
.mo-key-row { display: flex; gap: 6px; align-items: center; }
.mo-test { display: inline-flex; align-items: center; gap: 6px; font-size: 12px; padding: 8px 12px; border-radius: 8px; }
.mo-test.ok { background: rgba(57,208,154,.1); color: #6fe0b6; }
.mo-test.bad { background: var(--vermilion-soft); color: var(--vermilion); }
.mo-minimax { border-color: rgba(230,184,115,.3); background: linear-gradient(100deg, rgba(230,184,115,.08), transparent); }

/* 知识库跳转卡 */
.mo-kb { flex-direction: row; align-items: center; gap: 14px; }
.mo-kb-ico {
  width: 48px; height: 48px; border-radius: 12px; flex: 0 0 auto;
  display: flex; align-items: center; justify-content: center;
  background: var(--primary-soft); color: var(--primary-deep);
}
.mo-kb-b { flex: 1; min-width: 0; }
.mo-kb-t { font-size: 14.5px; font-weight: 600; color: var(--text); }
.mo-kb-d { font-size: 12px; color: var(--muted); line-height: 1.6; margin-top: 2px; }

/* 抽屉 */
.mo-drawer-mask { position: fixed; inset: 0; z-index: 60; background: rgba(0,0,0,.5); display: flex; justify-content: flex-end; }
.mo-drawer {
  width: 560px; max-width: 92vw; height: 100%; background: var(--panel);
  border-left: 1px solid var(--border-strong); display: flex; flex-direction: column;
  box-shadow: -20px 0 60px rgba(0,0,0,.4);
}
.mo-drawer-h {
  display: flex; align-items: center; justify-content: space-between; gap: 8px;
  padding: 14px 18px; border-bottom: 1px solid var(--border-soft);
  font-size: 14px; font-weight: 600; color: var(--text);
}
.mo-drawer-body { overflow: auto; padding: 16px; display: flex; flex-direction: column; gap: 14px; }
.mo-doc {
  margin: 0; padding: 12px; border-radius: 8px; background: var(--bg); border: 1px solid var(--border-soft);
  font-size: 12px; line-height: 1.7; color: var(--text-2); white-space: pre-wrap; word-break: break-word;
  max-height: 340px; overflow: auto; font-family: var(--mono, monospace);
}

.spin { animation: mo-spin 0.9s linear infinite; }
@keyframes mo-spin { to { transform: rotate(360deg); } }
</style>
