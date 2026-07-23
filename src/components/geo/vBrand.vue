<script setup lang="ts">
/**
 * 品牌档案 —— 推广主体的「切入点库」，单页搞定。
 *
 * 两条路，任选或混用：
 *   ① **上传资料**：把公司介绍/产品文档/FAQ 丢进来（.md/.txt/.json/.csv），原件存
 *      `~/PolarisGEO/data/brand-docs/`，勾选后随写作提示词喂给模型，让它自己找切入点；
 *   ② **手填字段**：把内涵拆成结构化落点（痛点/场景/短板/术语…），模型挑得更准。
 *
 * 填的不是广告词，而是**供模型取用的落点**：落点越多，越不必硬提品牌——
 * 夹带得住的私货，都是长在论证链里的。
 *
 * 一切以文件保管：档案本体 `~/PolarisGEO/data/brand.json`，资料原件在 brand-docs/ 目录，
 * 都可直接用编辑器改、可备份、可随仓库迁移。保存即生效，下一条 generate job 就按新档案织入。
 *
 * 写法上的两条纪律（踩过坑）：
 *   - 数组字段在 UI 上是 textarea 文本（`buf`），**任何写盘前都必须先 `syncLists()` 回灌**，
 *     否则会拿旧数组盖掉用户刚敲的内容；
 *   - 资料勾选/删除只改 `docs` 一个字段，走 `persistDocs()` 读-改-写，
 *     既不吞掉未保存的表单编辑，也不偷偷把它们存盘。
 */
import { ref, computed, onMounted, onBeforeUnmount, watch } from "vue";
import { title } from "./render";
import { PLATFORMS } from "./data";
import { brand, type BrandProfile, type BrandTactic, type BrandDoc } from "../../tauri";
import { toast } from "../../composables/useToast";

const head = computed(() =>
  title("品牌档案", "资源 / 推广主体的切入点库 —— 上传资料或手填，落点越多，私货夹带得越自然")
);

// ── 字段表：声明式渲染，免得 20 个字段写 20 段几乎一样的模板 ──
type Kind = "line" | "text" | "list";
interface Field {
  key: keyof BrandProfile;
  label: string;
  kind: Kind;
  ph?: string;
  hint?: string;
  rows?: number;
}
interface Group { id: string; title: string; intro: string; fields: Field[] }

const GROUPS: Group[] = [
  {
    id: "identity",
    title: "① 身份三件套",
    intro: "最低限度：只填这一组也能跑，但模型只能干巴巴地提品牌名。",
    fields: [
      { key: "name", label: "品牌 / 应用名", kind: "line", ph: "如：llmwiki",
        hint: "空着 = 档案视同未启用，generate 一字不加。" },
      { key: "domain", label: "推广域名", kind: "line", ph: "如：llmwiki.cloud",
        hint: "只有强植入平台允许出现在正文；弱/零平台由硬广守卫拦死。" },
      { key: "tagline", label: "一句话定位", kind: "line", ph: "一句话说清这是什么、为谁做的" },
      { key: "industry", label: "行业 / 赛道", kind: "line", ph: "如：AI 写作与多平台分发工具",
        hint: "决定模型把你归到哪个词条下——AI 检索时的实体类别。" },
      { key: "tone", label: "提及口吻", kind: "line", ph: "如：克制、工程师味、不喊口号",
        hint: "约束品牌出现那几句的语气，避免整篇干货里突然冒出一句广告腔。" },
    ],
  },
  {
    id: "geo",
    title: "② 关键词与手法",
    intro: "「填几个关键词就能被搜到」的入口，加上「用什么姿势不经意地带出来」。",
    fields: [
      { key: "keywords", label: "GEO 关键词", kind: "list", rows: 4,
        ph: "一行一个，或用逗号分隔。写成读者会怎么问的样子：\nAI 写作工具怎么选\n自媒体多平台分发怎么做",
        hint: "契约会要求正文自然覆盖（≥2 个进小标题或首段），并让锚词与品牌名同段共现一次——AI 检索这个主题时才会把你一起召回。同时压了密度红线：禁堆砌、禁为塞词造句。" },
    ],
  },
  {
    id: "depth",
    title: "③ 品牌内涵",
    intro: "理念与故事是「叙事型植入」的弹药：读者记住的是故事，品牌只是故事的主语。",
    fields: [
      { key: "philosophy", label: "品牌理念 / 价值主张", kind: "text", rows: 3,
        ph: "你们相信什么、反对什么。有立场的段落最容易被引用。" },
      { key: "story", label: "品牌故事 / 起源", kind: "text", rows: 3,
        ph: "为什么做这件事、最早是为了解决谁的什么问题" },
      { key: "business", label: "核心业务 / 产品线", kind: "list", rows: 4,
        ph: "一行一条：\n多平台一稿多发\n发布前 GEO 质检",
        hint: "写清「做什么」，模型才知道哪类选题跟你有关、能不能自然带到。" },
      { key: "terms", label: "专有名词 / 术语", kind: "list", rows: 3,
        ph: "一行一个，如：\nGEO 质检门禁\n七维评审官",
        hint: "契约要求原词照抄不改写——这是让 AI 学会该词条、把它跟你绑定的关键。" },
      { key: "faq", label: "常见问题", kind: "list", rows: 4,
        ph: "一行一条，用 || 分隔问与答：\n要付费吗||基础功能免费，团队版按席位",
        hint: "会被改写成「问句小标题 + 紧跟直答段落」——AI 抽答案片段最爱吃这个结构。" },
    ],
  },
  {
    id: "audience",
    title: "④ 人群 · 痛点 · 场景",
    intro: "痛点是最好用的切入口：先把问题讲透、给出通用解法，品牌只作为其中一个具体落点出现在末尾——读者当干货收，平台判不了硬广。",
    fields: [
      { key: "audience", label: "目标人群", kind: "list", rows: 3,
        ph: "一行一条：\n一人运营多个账号的自媒体作者" },
      { key: "painPoints", label: "解决的痛点", kind: "list", rows: 4,
        ph: "一行一条：\n同一篇稿子在七个平台重复排版\n发出去才发现踩了平台红线",
        hint: "填得越具体，「痛点方案」手法越有东西可写。" },
      { key: "scenarios", label: "典型使用场景", kind: "list", rows: 3,
        ph: "一行一条：\n热点当天两小时内多平台跟发" },
    ],
  },
  {
    id: "trust",
    title: "⑤ 可信度弹药",
    intro: "GEO 的胜负手不在夸得多好，而在可核验。事实、背书、以及敢写的短板，决定 AI 会不会把你当信源。",
    fields: [
      { key: "founder", label: "创始人 / 团队背景", kind: "text", rows: 2,
        ph: "谁做的、有什么相关经历",
        hint: "E-E-A-T 里「经验 + 权威」的来源，也是「案例引用」手法的落点。" },
      { key: "facts", label: "事实库", kind: "list", rows: 4,
        ph: "一行一条可引用的数据/案例（3~5 条）",
        hint: "强植入平台以「据 {品牌名}…」标注出处；弱平台化用成个人体验。" },
      { key: "endorsements", label: "权威背书", kind: "list", rows: 2,
        ph: "一行一条：媒体报道 / 开源仓库 / 行业引用…" },
      { key: "differentiators", label: "差异化优势", kind: "list", rows: 3,
        ph: "别写形容词，写「别人做不到的具体事」",
        hint: "只许在对比/选型语境里用，且必须同时给出下面的短板。" },
      { key: "weaknesses", label: "已知短板 / 不适用场景", kind: "list", rows: 3,
        ph: "一行一条：\n重排版需求还得手动调\n不支持企业微信",
        hint: "反直觉但关键：敢写短板才可信，可信才被 AI 采信、才不被平台判硬广。凡提差异化必配一条。" },
      { key: "competitors", label: "同类对照对象", kind: "list", rows: 2,
        ph: "一行一个同类产品名",
        hint: "「横向对比」手法用；契约要求客观并列、不得贬低他方。" },
      { key: "bannedWords", label: "表述红线", kind: "list", rows: 2,
        ph: "一行一个不许出现的词：\n最好\n第一\n唯一\n国家级",
        hint: "任何强度下都不得出现——广告法风险词先在这里堵死。" },
    ],
  },
];

const LIST_KEYS = GROUPS.flatMap((g) => g.fields).filter((f) => f.kind === "list").map((f) => f.key);

// ── 档案读写 ──
const profile = ref<BrandProfile | null>(null);
const buf = ref<Record<string, string>>({}); // 数组字段的 textarea 缓冲（一行一条）
const saving = ref(false);
const tactics = ref<BrandTactic[]>([]);
const docs = ref<BrandDoc[]>([]);
const paths = ref<[string, string] | null>(null);
// 头两组默认展开：先让人填得动最低限度的东西，长表单不劈头盖脸砸过来
const openGroups = ref<Record<string, boolean>>({ identity: true, geo: true });

function fillBuf(p: BrandProfile) {
  const b: Record<string, string> = {};
  for (const k of LIST_KEYS) b[k as string] = (((p as any)[k] as string[]) || []).join("\n");
  buf.value = b;
}

/** 把 textarea 缓冲回灌进 profile 的数组字段。**任何写盘前必须先调**。 */
function syncLists() {
  const p = profile.value;
  if (!p) return;
  for (const k of LIST_KEYS) {
    // 关键词额外容忍逗号/顿号写法；其余字段严格一行一条（正文里本就可能带逗号）
    const raw = buf.value[k as string] || "";
    (p as any)[k] = (k === "keywords" ? raw.split(/[\n,，、]/) : raw.split("\n"))
      .map((s) => s.trim())
      .filter(Boolean);
  }
}

function normalize(p: BrandProfile): BrandProfile {
  for (const k of LIST_KEYS) if (!Array.isArray((p as any)[k])) (p as any)[k] = [];
  if (!Array.isArray(p.tactics)) p.tactics = [];
  if (!Array.isArray(p.docs)) p.docs = [];
  return p;
}

// ── 写盘串行化 ──
// save / persistDocs / 上传 三条路径都能写 brand.json，交错就是 last-write-wins。
// 排成一条链，谁先来谁先写，后来的等着。
let writeChain: Promise<unknown> = Promise.resolve();
function withWriteLock<T>(fn: () => Promise<T>): Promise<T> {
  const run = writeChain.then(fn, fn);
  writeChain = run.catch(() => {}); // 失败不能卡死后面的写
  return run;
}

// ── 未保存提醒：长表单最怕默默丢改动 ──
const saved = ref(""); // 上次落盘时的指纹
/** 指纹**不含 docs**：资料勾选/删除是即时落盘的，不该让「未保存」灯常亮。 */
function fingerprint(): string {
  const p = profile.value;
  if (!p) return "";
  const { docs: _docs, ...rest } = p;
  return JSON.stringify([rest, buf.value]);
}
const dirty = computed(() => !!profile.value && fingerprint() !== saved.value);

// ── 草稿兜底 ──
// 本页没有被 KeepAlive 罩住：顶栏切个视图组件就销毁了。所以离开前把未保存的内容
// 存进 localStorage，回来再问要不要恢复——比弹「确定离开吗」体验好，也不用改父组件。
const DRAFT_KEY = "geo.brand.draft";
const draftRestored = ref(false);
function stashDraft() {
  if (dirty.value) {
    try {
      localStorage.setItem(DRAFT_KEY, JSON.stringify({ p: profile.value, b: buf.value }));
    } catch { /* 配额满了就算了，不能因为存草稿把页面搞崩 */ }
  }
}
function dropDraft() {
  localStorage.removeItem(DRAFT_KEY);
  draftRestored.value = false;
}
async function discardDraft() {
  dropDraft();
  await load();
}

const loading = ref(true);

async function load() {
  loading.value = true;
  try {
    const p = normalize(await brand.get());
    profile.value = p;
    fillBuf(p);
    saved.value = fingerprint();
  } catch {
    profile.value = null; // 后端不可用（纯前端 dev）→ 降级为只读说明
  } finally {
    loading.value = false;
  }
  try { tactics.value = await brand.tactics(); } catch { tactics.value = []; }
  try { paths.value = await brand.paths(); } catch { paths.value = null; }
  await loadDocs();
}

onMounted(async () => {
  await load();
  // 有草稿且确实与盘上不同 → 恢复，并在页首明示（可一键放弃）
  const raw = localStorage.getItem(DRAFT_KEY);
  if (!raw || !profile.value) return;
  try {
    const d = JSON.parse(raw) as { p: BrandProfile; b: Record<string, string> };
    if (!d?.p || !d?.b) return dropDraft();
    profile.value = normalize(d.p);
    buf.value = d.b;
    if (dirty.value) draftRestored.value = true;
    else dropDraft();
  } catch {
    dropDraft();
  }
});

async function loadDocs() {
  try { docs.value = await brand.docList(); } catch { docs.value = []; }
}

async function save() {
  if (!profile.value || saving.value) return;
  saving.value = true;
  try {
    syncLists();
    await withWriteLock(() => brand.set(profile.value!));
    saved.value = fingerprint();
    dropDraft();
    toast.info("已保存到 ~/PolarisGEO/data/brand.json——下一条 generate 即生效");
    refreshPreview();
  } catch (e: any) {
    toast.error(`保存失败：${e?.message ?? e}`);
  } finally {
    saving.value = false;
  }
}

/**
 * 只更新 `docs` 一个字段：从盘上读最新档案 → 换掉 docs → 写回。
 * 这样勾选/删除资料既不会拿内存里的旧数组盖掉盘上的东西，
 * 也不会把用户还没决定要不要保存的表单编辑偷偷存进去。
 */
async function persistDocs() {
  if (!profile.value) return;
  await withWriteLock(async () => {
    const fresh = normalize(await brand.get());
    fresh.docs = [...profile.value!.docs];
    await brand.set(fresh);
  });
}

// ── 品牌资料上传（纯文本类；原件落 brand-docs/）──
const TEXT_EXT = [".md", ".txt", ".json", ".csv", ".markdown", ".yml", ".yaml", ".html"];
/** 单文件上限：资料是给模型挑切入点的，写作时本就只喂前 4000 字，没必要收一份几十兆的东西进内存。 */
const MAX_DOC_BYTES = 2 * 1024 * 1024;

/** 是不是一份真的 brand.json？光看字段名出现过太松，会把普通 json 当档案吞掉。 */
function looksLikeProfile(o: any): o is BrandProfile {
  return (
    !!o && typeof o === "object" && !Array.isArray(o) &&
    typeof o.name === "string" && typeof o.enabled === "boolean" &&
    !!o.strength && typeof o.strength === "object"
  );
}
const fileInput = ref<HTMLInputElement | null>(null);
const dragging = ref(false);
const busyDoc = ref("");
const uploading = ref(false);

async function ingest(files: FileList | File[]) {
  if (!profile.value || uploading.value) return;
  uploading.value = true;
  const list = Array.from(files);
  let added = 0;
  try {
    for (const f of list) {
      if (!TEXT_EXT.some((e) => f.name.toLowerCase().endsWith(e))) {
        toast.error(`「${f.name}」不是纯文本——PDF/Word 请先另存为 .md 或 .txt 再传`);
        continue;
      }
      if (f.size > MAX_DOC_BYTES) {
        toast.error(`「${f.name}」超过 2MB——请先裁成要点再传（写作时本就只喂前 4000 字）`);
        continue;
      }
      try {
        const text = await f.text();
        // 是一份完整的 brand.json → 当档案导入，不进资料库；覆盖整页表单前先问一句
        if (f.name.toLowerCase().endsWith(".json")) {
          let parsed: unknown = null;
          try { parsed = JSON.parse(text); } catch { /* 不是 json 就按普通资料收 */ }
          if (looksLikeProfile(parsed)) {
            if (!confirm(`「${f.name}」是一份完整的品牌档案。导入会覆盖当前页面上的全部内容，继续？`)) continue;
            const p = normalize(parsed);
            profile.value = p;
            fillBuf(p);
            toast.info(`已从「${f.name}」导入档案——核对无误后点「保存档案」才会落盘`);
            continue;
          }
        }
        await brand.docSave(f.name, text);
        if (!profile.value.docs.includes(f.name)) profile.value.docs.push(f.name); // 传了就默认启用
        added++;
      } catch (e: any) {
        toast.error(`「${f.name}」上传失败：${e?.message ?? e}`);
      }
    }
    if (added) {
      try {
        await persistDocs();
        toast.info(`已收下 ${added} 份资料，默认启用——写作时模型会从里面找切入点`);
      } catch (e: any) {
        toast.error(`资料已存盘，但启用清单没写进档案：${e?.message ?? e}`);
      }
      await loadDocs();
      refreshPreview();
    }
  } finally {
    uploading.value = false;
  }
}

function onDrop(e: DragEvent) {
  dragging.value = false;
  if (e.dataTransfer?.files?.length) ingest(e.dataTransfer.files);
}
function onPick(e: Event) {
  const t = e.target as HTMLInputElement;
  if (t.files?.length) ingest(t.files);
  t.value = ""; // 允许重复选同一个文件
}

async function toggleDoc(d: BrandDoc) {
  if (!profile.value || busyDoc.value) return;
  const i = profile.value.docs.indexOf(d.name);
  i >= 0 ? profile.value.docs.splice(i, 1) : profile.value.docs.push(d.name);
  busyDoc.value = d.name;
  try {
    await persistDocs();
    await loadDocs();
    refreshPreview();
  } catch (e: any) {
    // 写盘失败就把 UI 状态回滚，别让复选框显示一个没生效的结果
    const j = profile.value.docs.indexOf(d.name);
    j >= 0 ? profile.value.docs.splice(j, 1) : profile.value.docs.push(d.name);
    toast.error(e?.message ?? String(e));
  } finally {
    busyDoc.value = "";
  }
}

async function delDoc(d: BrandDoc) {
  if (busyDoc.value) return;
  if (!confirm(`删除资料「${d.name}」？原文件会从 brand-docs/ 目录移除。`)) return;
  busyDoc.value = d.name;
  try {
    await brand.docDelete(d.name); // 后端会顺手把它从启用清单里摘掉
    if (profile.value) {
      const i = profile.value.docs.indexOf(d.name);
      if (i >= 0) profile.value.docs.splice(i, 1);
    }
    if (viewDoc.value?.name === d.name) viewDoc.value = null;
    await loadDocs();
    refreshPreview();
  } catch (e: any) {
    toast.error(e?.message ?? String(e));
  } finally {
    busyDoc.value = "";
  }
}

const viewDoc = ref<{ name: string; text: string } | null>(null);
async function openDoc(d: BrandDoc) {
  if (viewDoc.value?.name === d.name) { viewDoc.value = null; return; } // 再点一次收起
  try {
    viewDoc.value = { name: d.name, text: await brand.docRead(d.name) };
  } catch (e: any) {
    toast.error(e?.message ?? String(e));
  }
}

/** 导出：把当前档案下载成 brand.json，方便备份/换机/进版本库。导出的是**眼前所见**，故先回灌。 */
function exportJson() {
  if (!profile.value) return;
  syncLists();
  const blob = new Blob([JSON.stringify(profile.value, null, 2)], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = "brand.json";
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000); // 立刻 revoke 会赶在下载真正开始之前
}

// ── 切入点计数：只作丰满度提示，不是打分 ──
const anchorCount = computed(() => {
  const p = profile.value;
  if (!p) return 0;
  let n = 0;
  for (const k of ["name", "domain", "tagline", "industry", "tone", "philosophy", "story", "founder"] as const) {
    if (String(p[k] || "").trim()) n++;
  }
  // 拆分规则要与 syncLists 一致，否则关键词那栏的计数会失真
  for (const k of LIST_KEYS) {
    const raw = buf.value[k as string] || "";
    n += (k === "keywords" ? raw.split(/[\n,，、]/) : raw.split("\n")).filter((s) => s.trim()).length;
  }
  return n + docs.value.filter((d) => d.enabled).length * 3;
});

// ── 植入手法多选 ──
function toggleTactic(id: string) {
  if (!profile.value) return;
  const cur = profile.value.tactics;
  const i = cur.indexOf(id);
  i >= 0 ? cur.splice(i, 1) : cur.push(id);
}
const hasTactic = (id: string) => !!profile.value?.tactics?.includes(id);

// ── 契约预览（所见即所喂）──
const previewPid = ref("zhihu");
const preview = ref<[string, string] | null>(null);
const previewOpen = ref(false);
async function refreshPreview() {
  if (!previewOpen.value) return; // 收起时不白跑一趟后端
  try { preview.value = await brand.contractPreview(previewPid.value); } catch { preview.value = null; }
}
watch([previewPid, previewOpen], refreshPreview);

// 未保存就关窗 → 浏览器/WebView 层拦一下
function onBeforeUnload(e: BeforeUnloadEvent) {
  if (!dirty.value) return;
  stashDraft();
  e.preventDefault();
  e.returnValue = ""; // 部分内核只认这个
}
onMounted(() => window.addEventListener("beforeunload", onBeforeUnload));
// 切视图会直接销毁本组件 → 把未保存内容存成草稿，下次进来可一键恢复
onBeforeUnmount(() => {
  window.removeEventListener("beforeunload", onBeforeUnload);
  if (dirty.value) {
    stashDraft();
    toast.info("品牌档案有未保存的改动，已存为草稿——回到该页可继续编辑");
  }
});
</script>

<template>
  <div>
    <div v-html="head"></div>

    <div v-if="loading" class="callout">正在读取档案…</div>
    <div v-else-if="!profile" class="callout r">后端命令不可用——请在桌面应用内打开本页。</div>

    <template v-else>
      <div v-if="draftRestored" class="callout bd-bar">
        <b>已恢复上次未保存的草稿</b>
        <span class="foot flush">离开本页时自动存的，尚未落盘。核对后点「保存档案」才会生效。</span>
        <span class="grow"></span>
        <button class="btn sm ghost" @click="discardDraft">放弃草稿，读回已保存的档案</button>
      </div>

      <!-- 状态条：开关 + 丰满度 + 保存（常驻页首） -->
      <div class="callout y bd-bar">
        <label style="display:flex;gap:6px;align-items:center">
          <input type="checkbox" v-model="profile.enabled" />
          <b>{{ profile.enabled ? "已启用织入" : "未启用（不织入，硬广守卫仍守底线）" }}</b>
        </label>
        <span class="foot flush">
          切入点 <b>{{ anchorCount }}</b> 处 —— 越多，模型越能挑到贴合选题的角度，越不必硬提品牌。
        </span>
        <span v-if="dirty" class="badge b-partial">● 有未保存的改动</span>
        <span class="grow"></span>
        <button class="btn sm ghost" @click="exportJson">导出 brand.json</button>
        <button class="btn sm" :disabled="saving || !dirty" @click="save">
          {{ saving ? "保存中…" : dirty ? "保存档案" : "已保存" }}
        </button>
      </div>

      <!-- 上传区：不想手填就丢文件 -->
      <section>
        <div class="card">
          <h3>上传品牌资料（不想手填就走这条）</h3>
          <p class="foot" style="margin:-4px 0 10px">
            把公司介绍、产品文档、FAQ、过往稿件丢进来即可——原件存进
            <code>brand-docs/</code>，勾选的会随写作提示词喂给模型，让它<b>自己从里面提炼切入点</b>。
            契约里写明「这是素材不是提纲」：只许提炼 1~2 个与选题相关的点，禁止整段搬运或复述宣传语。
          </p>
          <!-- 用 button 而不是 div：键盘可聚焦、可回车触发，读屏也报得出来 -->
          <button
            type="button" class="drop" :class="{ on: dragging }" :disabled="uploading"
            @dragover.prevent="dragging = true" @dragleave="dragging = false" @drop.prevent="onDrop"
            @click="fileInput?.click()"
          >
            <b>{{ uploading ? "读取中…" : "把文件拖到这里，或点击选择" }}</b>
            <span class="foot" style="display:block;margin-top:6px">
              支持 .md / .txt / .json / .csv / .yml / .html —— PDF、Word 请先另存为纯文本。
              直接拖入一份 <code>brand.json</code> 则视为<b>导入档案</b>，会填满下面的表单。
            </span>
          </button>
          <input ref="fileInput" type="file" multiple hidden
            accept=".md,.txt,.json,.csv,.markdown,.yml,.yaml,.html" @change="onPick" />

          <div v-if="docs.length" class="tbl-wrap" style="margin-top:12px"><table>
            <tr><th style="width:44px">喂</th><th>文件</th><th style="width:80px">字数</th><th>开头</th><th style="width:130px">操作</th></tr>
            <tr v-for="d in docs" :key="d.name">
              <td>
                <input :id="`doc-${d.name}`" type="checkbox" :checked="d.enabled"
                  :disabled="busyDoc === d.name" @change="toggleDoc(d)" />
              </td>
              <td>
                <label :for="`doc-${d.name}`"><b>{{ d.name }}</b></label>
                <span v-if="d.truncated" class="foot" style="display:block;margin:0">写作时只喂前 4000 字</span>
              </td>
              <td>{{ d.chars }}</td>
              <td class="foot flush">{{ d.excerpt }}…</td>
              <td>
                <button class="btn sm ghost" @click="openDoc(d)">{{ viewDoc?.name === d.name ? "收起" : "查看" }}</button>
                <button class="btn sm danger" :disabled="busyDoc === d.name" @click="delDoc(d)">删除</button>
              </td>
            </tr>
          </table></div>
          <p v-else class="foot">还没有资料。完全不传也行——下面手填同样有效，两条路可以混用。</p>

          <pre v-if="viewDoc" class="pre-box sm">{{ viewDoc.text }}</pre>

          <p v-if="paths" class="foot">
            档案：<code>{{ paths[0] }}</code> ｜ 资料目录：<code>{{ paths[1] }}</code>
            —— 都是普通文件，可直接用编辑器改、可备份、可随仓库迁移。
          </p>
        </div>
      </section>

      <!-- 手填区：分组折叠，一页到底 -->
      <section v-for="g in GROUPS" :key="g.id">
        <div class="card">
          <button type="button" class="bd-head" :aria-expanded="!!openGroups[g.id]"
            @click="openGroups[g.id] = !openGroups[g.id]">
            <h3 class="flush">{{ g.title }}</h3>
            <span class="grow"></span>
            <span class="foot flush">{{ openGroups[g.id] ? "收起 ▴" : "展开 ▾" }}</span>
          </button>
          <template v-if="openGroups[g.id]">
            <p class="foot" style="margin:8px 0 12px">{{ g.intro }}</p>
            <div class="bd-form">
              <template v-for="f in g.fields" :key="f.key">
                <b><label :for="`f-${f.key}`">{{ f.label }}</label></b>
                <div>
                  <input v-if="f.kind === 'line'" :id="`f-${f.key}`" class="inp"
                    :placeholder="f.ph" v-model="(profile as any)[f.key]" />
                  <textarea v-else-if="f.kind === 'text'" :id="`f-${f.key}`" class="inp"
                    :rows="f.rows || 3" :placeholder="f.ph" v-model="(profile as any)[f.key]"></textarea>
                  <textarea v-else :id="`f-${f.key}`" class="inp"
                    :rows="f.rows || 4" :placeholder="f.ph" v-model="buf[f.key as string]"></textarea>
                  <p v-if="f.hint" class="foot">{{ f.hint }}</p>
                </div>
              </template>

              <!-- 手法多选挂在「关键词与手法」组里 -->
              <template v-if="g.id === 'geo'">
                <b>植入手法</b>
                <div>
                  <div class="bd-chips">
                    <button v-for="t in tactics" :key="t[0]" type="button" class="btn sm"
                      :class="hasTactic(t[0]) ? '' : 'ghost'" :title="t[2]"
                      :aria-pressed="hasTactic(t[0])" @click="toggleTactic(t[0])">
                      {{ hasTactic(t[0]) ? "✓ " : "" }}{{ t[1] }}
                    </button>
                  </div>
                  <p class="foot">选「用什么姿势不经意地带出品牌」，不选＝模型按语境自选。共同点：<b>品牌只作为论证的副产品出现</b>，把品牌名换成「某厂商」后段落仍成立。</p>
                  <ul v-if="tactics.some((t) => hasTactic(t[0]))" class="foot" style="padding-left:18px">
                    <li v-for="t in tactics.filter((x) => hasTactic(x[0]))" :key="t[0]"><b>{{ t[1] }}</b>：{{ t[2] }}</li>
                  </ul>
                </div>
              </template>
            </div>
          </template>
        </div>
      </section>

      <!-- 契约预览：所见即所喂 -->
      <section>
        <div class="card">
          <div class="bd-bar">
            <h3 class="flush">契约预览（所见即所喂）</h3>
            <select v-model="previewPid" class="inp auto"
              aria-label="预览平台">
              <option v-for="p in PLATFORMS" :key="p.id" :value="p.id">{{ p.name }}</option>
            </select>
            <span v-if="previewOpen && preview" class="badge b-full">{{ preview[0] }}</span>
            <span class="grow"></span>
            <button class="btn sm ghost" :aria-expanded="previewOpen" @click="previewOpen = !previewOpen">
              {{ previewOpen ? "收起 ▴" : "展开 ▾" }}
            </button>
          </div>
          <template v-if="previewOpen">
            <p class="foot">
              下面这段会<b>原样追加</b>到 generate 的写作提示词末尾，并随流程详情的「提示词快照」全程留痕。
              空字段整节不出现，不会拿空话占提示词。强度由「推广植入 → 强度矩阵」决定。
              <b v-if="dirty">注意：预览读的是已存盘的档案，未保存的改动不在其中。</b>
            </p>
            <pre v-if="preview" class="pre-box">{{ preview[1] }}</pre>
            <p v-else class="foot">档案未启用或品牌名为空——generate 一字不加。</p>
          </template>
        </div>
      </section>
    </template>
  </div>
</template>

