<script setup lang="ts">
/**
 * 流程详情卡：一条投递流水线 job 的全貌——
 *  - 顶：**工作流轨道**。总规划 → 逐格步骤横着一条流铺开（原来它缩在左下角一列，
 *    既看不出先后关系，也占着最好的一块地）。点哪一格，下面中栏就换成哪一格的提示词。
 *    轨道上面一行是这条 job 的基本信息与产物入口。
 *  - 左（宽）：**只放提示词**——喂给模型的原文，按段落/要点结构化排出来，字号放大。
 *    打开即是**整篇文章的总规划提示词**（写作专家画像 + 本平台补丁 + 写作任务 + 品牌契约），
 *    不必先去点某个人格才看得到提示词；点轨道上某一格才切成那一步的局部提示词。
 *  - 右：对话操控——步骤节点、后端（claude CLI + 脚本）跑的每一个动作、模型回复
 *    汇成一条流，像正常聊天一样往下淌
 * 失败/中断的 job，「继续」直接摆在卡片最上面的操作条上。
 */
import { ref, computed, watch, nextTick, onMounted, onBeforeUnmount } from "vue";
import { mediaJob, type MediaJob, type MediaJobStep, type MediaJobPlan } from "../../tauri";
import { toast } from "../../composables/useToast";
import { P } from "./data";
import ExpertPromptDrawer from "./ExpertPromptDrawer.vue";
import JobChatPanel from "./JobChatPanel.vue";

const props = defineProps<{ jobId: string }>();
const emit = defineEmits<{ (e: "close"): void; (e: "rerun", job: MediaJob): void }>();

const job = ref<MediaJob | null>(null);
const log = ref("");
const article = ref<string | null>(null);
const articleErr = ref("");
const busy = ref(false);
let timer: ReturnType<typeof setInterval> | null = null;

const isLive = computed(() => job.value?.status === "running" || job.value?.status === "pending");
/** 终态但没跑完 —— 这类 job 才给「继续」。 */
const canResume = computed(() => !!job.value && !isLive.value && job.value.status !== "done");
const platName = computed(() => (job.value ? P(job.value.platform)?.name ?? job.value.platform : ""));

const STATUS_TEXT: Record<string, string> = {
  pending: "排队中", running: "进行中", done: "完成", failed: "失败", canceled: "已取消",
};
const STATUS_DOT: Record<string, string> = {
  pending: "idle", running: "warn", done: "ok", failed: "bad", canceled: "idle",
};
const STEP_DOT: Record<string, string> = { run: "warn", ok: "ok", fail: "bad", skip: "idle" };
const STEP_TEXT: Record<string, string> = { run: "进行中", ok: "完成", fail: "失败", skip: "跳过" };

function fmtTime(sec: number): string {
  const d = new Date(sec * 1000);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}
function fmtDur(ms: number): string {
  if (!ms || ms <= 0) return "";
  if (ms < 1000) return `${ms}ms`;
  const s = Math.round(ms / 1000);
  return s < 60 ? `${s}s` : `${Math.floor(s / 60)}m${String(s % 60).padStart(2, "0")}s`;
}

/* ── 选中步骤：没手点过就自动跟着进行中的那步走 ── */
const pickedKey = ref<string | null>(null);
const selectedStep = computed<MediaJobStep | null>(() => {
  const steps = job.value?.steps ?? [];
  if (!steps.length) return null;
  if (pickedKey.value) {
    const hit = steps.find((s) => s.key === pickedKey.value);
    if (hit) return hit;
  }
  // 没手点过：跟着在跑的那步；都跑完了就落到最后一个**有提示词**的步骤上——
  // 否则中栏经常停在「编辑页已打开」这种瞬时步骤上，一片空。
  const running = steps.find((s) => s.status === "run");
  if (running) return running;
  for (let i = steps.length - 1; i >= 0; i--) if (steps[i].prompt) return steps[i];
  return steps[steps.length - 1];
});
function pickStep(s: MediaJobStep) {
  pickedKey.value = s.key;
  mid.value = "prompt";
}

/* 轨道横着排，跑到后面几格时选中的那格会滑出视野——自动把它带回来。 */
const railEl = ref<HTMLDivElement | null>(null);
async function keepActiveInView() {
  await nextTick();
  railEl.value?.querySelector<HTMLElement>(".wfn.on")
    ?.scrollIntoView({ behavior: "smooth", inline: "nearest", block: "nearest" });
}

/**
 * 工作流日志每行只有 [HH:MM:SS]，没有日期。给右栏一个基准零点——
 * 这条 job 开跑那天的 00:00——它就能把日志行还原成绝对时间，
 * 和步骤、对话消息排进同一条时间轴。
 */
const dayStart = computed(() => {
  const j = job.value;
  if (!j) return 0;
  const first = j.steps?.[0]?.startedAt || j.steps?.[0]?.at || j.createdAt;
  const d = new Date(first * 1000);
  return Math.floor(new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime() / 1000);
});

/* ── 中栏：提示词结构化 ────────────────────────────────────────────
   提示词是一整块纯文本，直接摊出来是一堵墙。按行认几种常见形态：
   标题（【…】/ 短句加冒号 / # 开头）、要点（-、•、数字、①②③）、分隔线，
   其余按空行聚成段落。认不出来也不丢内容，一律落到段落里。            */
type Blk =
  | { t: "head"; s: string }
  | { t: "list"; items: string[] }
  | { t: "text"; s: string }
  | { t: "rule" };

const RULE = /^(?:[─—–\-*=_]{3,})$/;
const HEAD_WRAP = /^【(.+)】$/;
const HEAD_HASH = /^#{1,4}\s+(.+)$/;
/** 整行加粗的也是小标题 */
const HEAD_BOLD = /^\*\*([^*]+)\*\*[：:]?$/;
/** 列表符后面必须有空格——否则 **加粗** 开头的行会被当成列表，星号被啃掉一个 */
const BULLET = /^(?:[-•·*]\s+|\d{1,2}[.、)]\s*|[①-⑳]\s*|[一二三四五六七八九十]、\s*)(.+)$/;

function parsePrompt(src: string): Blk[] {
  const out: Blk[] = [];
  let para: string[] = [];
  let list: string[] = [];
  const flushPara = () => { if (para.length) { out.push({ t: "text", s: para.join("\n") }); para = []; } };
  const flushList = () => { if (list.length) { out.push({ t: "list", items: list }); list = []; } };
  const flush = () => { flushList(); flushPara(); };

  for (const raw of src.split("\n")) {
    const line = raw.trimEnd();
    const t = line.trim();
    if (!t) { flush(); continue; }
    if (RULE.test(t)) { flush(); out.push({ t: "rule" }); continue; }

    const hw = HEAD_WRAP.exec(t) ?? HEAD_HASH.exec(t) ?? HEAD_BOLD.exec(t);
    if (hw) { flush(); out.push({ t: "head", s: hw[1].trim() }); continue; }
    // 「要求：」这种独立成行的短句也当小标题
    if (t.length <= 26 && /[:：]$/.test(t) && !BULLET.test(t)) {
      flush(); out.push({ t: "head", s: t.replace(/[:：]$/, "") }); continue;
    }
    const b = BULLET.exec(t);
    if (b) { flushPara(); list.push(b[1].trim() || t); continue; }
    flushList();
    para.push(line);
  }
  flush();
  // 结尾若是分隔线，去掉，免得留个空尾巴
  while (out.length && out[out.length - 1].t === "rule") out.pop();
  return out;
}

/** 行内 **加粗** 与 `代码`：切成片段渲染，不走 v-html。 */
type Seg = { t: "b" | "c" | "p"; s: string };
const INLINE = /\*\*([^*]+)\*\*|`([^`]+)`/g;
function segs(s: string): Seg[] {
  const out: Seg[] = [];
  let last = 0;
  let m: RegExpExecArray | null;
  INLINE.lastIndex = 0;
  while ((m = INLINE.exec(s))) {
    if (m.index > last) out.push({ t: "p", s: s.slice(last, m.index) });
    out.push(m[1] ? { t: "b", s: m[1] } : { t: "c", s: m[2] });
    last = INLINE.lastIndex;
  }
  if (last < s.length) out.push({ t: "p", s: s.slice(last) });
  return out;
}

/* ── 中栏模式：总规划提示词（默认） / 某一步的提示词 / 正文产物 ── */
const mid = ref<"plan" | "prompt" | "article">("plan");

/**
 * 总规划提示词：整篇文章的总纲——写作专家画像 + 本平台补丁 + insight 卡 + 写作任务 + 品牌契约。
 * 后端一并管两种来源：generate 跑过就回放当时的原文快照，还没跑到就按当前配置现拼预览，
 * 所以排队中的 job 打开也有东西看，不用干等某一步留痕。
 */
const plan = ref<MediaJobPlan | null>(null);
const planErr = ref("");
async function loadPlan() {
  try {
    plan.value = await mediaJob.planPrompt(props.jobId);
    planErr.value = "";
  } catch (e: any) {
    plan.value = null;
    planErr.value = e?.message ?? String(e);
  }
}

/** 中栏当前该摊开哪段提示词 */
const midPrompt = computed(() =>
  mid.value === "plan" ? plan.value?.prompt ?? "" : selectedStep.value?.prompt ?? "",
);
const promptBlocks = computed<Blk[]>(() => (midPrompt.value ? parsePrompt(midPrompt.value) : []));

/* ── 就地改提示词 ── */
const promptEdit = ref<{ expertId: string; platform: string } | null>(null);
function editPrompt(expertId?: string) {
  if (!job.value || !expertId) return;
  promptEdit.value = { expertId, platform: job.value.platform };
}
function closePromptEdit() {
  promptEdit.value = null;
  refresh(); // 改完回来刷一把：版本号等留痕可能已变
}

async function refresh() {
  try {
    job.value = await mediaJob.status(props.jobId);
  } catch (e: any) {
    toast.error(e?.message ?? String(e));
    return;
  }
  try {
    log.value = await mediaJob.log(props.jobId);
  } catch { /* 日志未产生（pending）时静默 */ }
  // 总纲还是「现拼的预览」就跟着轮询要一次：generate 一开跑就换成当时落盘的真快照。
  if (!plan.value?.recorded) await loadPlan();
  // 跑着才轮询；到终态自动停表。
  if (isLive.value && !timer) timer = setInterval(refresh, 2000);
  if (!isLive.value && timer) { clearInterval(timer); timer = null; }
}

/** 看完正文产物按原路返回：从总规划来就回总规划，从某一步来就回那一步。 */
const beforeArticle = ref<"plan" | "prompt">("plan");
async function openArticle() {
  if (mid.value !== "article") beforeArticle.value = mid.value;
  mid.value = "article";
  if (article.value !== null) return;
  articleErr.value = "";
  try {
    article.value = await mediaJob.article(props.jobId);
  } catch (e: any) {
    articleErr.value = e?.message ?? String(e);
  }
}

async function cancel() {
  if (!job.value) return;
  busy.value = true;
  try {
    await mediaJob.cancel(job.value.id);
    toast.info("已发出取消，正在杀进程树");
    await refresh();
  } catch (e: any) {
    toast.error(e?.message ?? String(e));
  } finally { busy.value = false; }
}

async function resume() {
  if (!job.value) return;
  busy.value = true;
  try {
    await mediaJob.resume(job.value.id);
    toast.success("已从断点续跑：已完成的阶段自动跳过");
    await refresh();
  } catch (e: any) {
    toast.error(e?.message ?? String(e));
  } finally { busy.value = false; }
}

async function rerun() {
  const j = job.value;
  if (!j) return;
  busy.value = true;
  try {
    const nj = await mediaJob.start({
      queueId: j.queueId, platform: j.platform, title: j.title, topic: j.topic,
      stages: j.stages, articlePath: j.articlePath,
    });
    toast.success(`已按同参数重跑，新 job ${nj.id.slice(0, 8)}`);
    emit("rerun", nj);
  } catch (e: any) {
    toast.error(e?.message ?? String(e));
  } finally { busy.value = false; }
}

async function copyText(t?: string, what = "路径") {
  if (!t) return;
  try { await navigator.clipboard.writeText(t); toast.success(`已复制${what}`); } catch { toast.error("复制失败"); }
}

watch([() => selectedStep.value?.key, mid], keepActiveInView);

watch(() => props.jobId, () => {
  job.value = null; log.value = ""; article.value = null;
  plan.value = null; planErr.value = "";
  pickedKey.value = null; mid.value = "plan";
  refresh();
});
onMounted(refresh);
onBeforeUnmount(() => { if (timer) clearInterval(timer); });
</script>

<template>
  <div class="gd-mask" @click.self="emit('close')">
    <div class="gd gd-wide">
      <!-- ══ 顶条：标题 + 操作（失败/中断时「继续」就在这儿） ══ -->
      <div class="gd-h">
        <template v-if="job">
          <span class="sline hstat"><span class="sdot" :class="STATUS_DOT[job.status] || 'idle'"></span>{{ STATUS_TEXT[job.status] || job.status }}</span>
          <b class="htitle">《{{ job.title }}》</b>
          <span class="hsub">{{ platName }}</span>
          <span class="hsp"></span>
          <button v-if="canResume" class="btn sm hgo" :disabled="busy" @click="resume" title="从断点续跑：已完成且产物还在的阶段自动跳过">继续</button>
          <button v-if="isLive" class="btn sm danger" :disabled="busy" @click="cancel">取消</button>
          <button v-if="!isLive" class="btn sm ghost" :disabled="busy" @click="rerun">重跑</button>
        </template>
        <span v-else>流程详情</span>
        <button class="xbtn" title="关闭" @click="emit('close')">✕</button>
      </div>

      <!-- ══ 工作流轨道：整条流水线横着铺在卡片最上面，点哪一格下面就换哪一格的提示词 ══ -->
      <div class="gd-flow" v-if="job">
        <div class="gd-meta">
          <span>编号 <b class="mono">{{ job.id.slice(0, 8) }}</b></span>
          <span v-if="job.topic" class="mtopic">选题 <b>{{ job.topic }}</b></span>
          <span>发起 <b>{{ fmtTime(job.createdAt) }}</b></span>
          <span>更新 <b>{{ fmtTime(job.updatedAt) }}</b></span>
          <span v-if="isLive" class="live">实时</span>
          <span class="hsp"></span>
          <button v-if="job.articlePath" class="lnk" @click="openArticle">正文产物</button>
          <button v-if="job.articlePath" class="lnk" @click="copyText(job.articlePath)">产物路径</button>
          <button class="lnk" @click="copyText(job.logPath)">日志路径</button>
        </div>

        <div ref="railEl" class="rail">
          <!-- 总规划钉在轨道最前面：它管的是整篇文章，不属于任何一格步骤 -->
          <button class="wfn plan" :class="{ on: mid === 'plan' }" @click="mid = 'plan'">
            <span class="wfn-h">
              <span class="sdot" :class="plan?.recorded ? 'ok' : 'idle'"></span>
              <span class="wfn-i">总纲</span>
            </span>
            <span class="wfn-l">总规划提示词</span>
            <span class="wfn-m">
              整篇怎么写
              <template v-if="plan?.expertName"> · {{ plan.expertName }}</template>
              <template v-if="plan?.prompt"> · {{ plan.prompt.length }} 字</template>
            </span>
          </button>

          <p v-if="!job.steps.length" class="rail-hint">
            还没有步骤记录{{ job.status === "pending" ? "，排队等待开跑。" : "。" }}
          </p>
          <template v-for="(s, i) in job.steps" :key="s.key">
            <span class="arr" :class="s.status"></span>
            <button
              class="wfn"
              :class="[s.status, { on: mid === 'prompt' && selectedStep?.key === s.key }]"
              :title="s.label"
              @click="pickStep(s)"
            >
              <span class="wfn-h">
                <span class="sdot" :class="STEP_DOT[s.status] || 'idle'"></span>
                <span class="wfn-i">{{ i + 1 }}</span>
                <span class="wfn-s">{{ STEP_TEXT[s.status] || s.status }}</span>
              </span>
              <span class="wfn-l">{{ s.label }}</span>
              <span class="wfn-m">
                <template v-if="fmtDur(s.durationMs)">{{ fmtDur(s.durationMs) }}</template>
                <template v-if="s.expertName">{{ fmtDur(s.durationMs) ? " · " : "" }}{{ s.expertName }}</template>
              </span>
            </button>
          </template>
        </div>

        <p v-if="job.error" class="errline">{{ job.error }}</p>
      </div>

      <div class="gd-cols" v-if="job">
        <!-- ══ 左：提示词——打开即整篇文章的总规划，点轨道某一格才切成那步的局部提示词 ══ -->
        <div class="col-m pane">
          <div class="mid-t">
            <template v-if="mid === 'plan'">
              <b>总规划提示词 · 整篇文章</b>
              <span v-if="plan?.prompt" class="mid-n">{{ plan.prompt.length }} 字</span>
              <span class="mid-tag" :class="{ pre: !plan?.recorded }">
                {{ plan?.recorded ? "已喂给模型的原文" : "按当前配置预览" }}
              </span>
              <span class="hsp"></span>
              <button v-if="plan?.prompt" class="lnk" @click="copyText(plan.prompt, '总规划提示词')">复制</button>
              <button v-if="plan?.expertId" class="lnk" @click="editPrompt(plan.expertId)">改提示词</button>
            </template>
            <template v-else-if="mid === 'prompt'">
              <b>{{ selectedStep?.label ?? "提示词" }}</b>
              <span v-if="selectedStep?.prompt" class="mid-n">{{ selectedStep.prompt.length }} 字</span>
              <span class="hsp"></span>
              <button class="lnk" @click="mid = 'plan'">回到总规划</button>
              <button v-if="selectedStep?.prompt" class="lnk" @click="copyText(selectedStep.prompt, '提示词')">复制</button>
              <button v-if="selectedStep?.expertId" class="lnk" @click="editPrompt(selectedStep.expertId)">改提示词</button>
            </template>
            <template v-else>
              <b>正文产物</b>
              <span class="hsp"></span>
              <button class="lnk" @click="mid = beforeArticle">{{ beforeArticle === "plan" ? "回到总规划" : "回到提示词" }}</button>
            </template>
          </div>

          <div class="mid-b">
            <template v-if="mid !== 'article'">
              <template v-if="promptBlocks.length">
                <template v-for="(b, i) in promptBlocks" :key="i">
                  <h4 v-if="b.t === 'head'" class="pb-h">{{ b.s.replace(/\*\*/g, "") }}</h4>
                  <hr v-else-if="b.t === 'rule'" class="pb-r" />
                  <ul v-else-if="b.t === 'list'" class="pb-l">
                    <li v-for="(it, j) in b.items" :key="j">
                      <template v-for="(g, k) in segs(it)" :key="k">
                        <strong v-if="g.t === 'b'">{{ g.s }}</strong>
                        <code v-else-if="g.t === 'c'">{{ g.s }}</code>
                        <template v-else>{{ g.s }}</template>
                      </template>
                    </li>
                  </ul>
                  <p v-else class="pb-p">
                    <template v-for="(g, k) in segs(b.s)" :key="k">
                      <strong v-if="g.t === 'b'">{{ g.s }}</strong>
                      <code v-else-if="g.t === 'c'">{{ g.s }}</code>
                      <template v-else>{{ g.s }}</template>
                    </template>
                  </p>
                </template>
              </template>
              <template v-else-if="mid === 'plan'">
                <p v-if="planErr" class="errline">读取总规划失败：{{ planErr }}</p>
                <p v-else-if="plan" class="hint">
                  这条 job 不含生成阶段（直接拿现成正文排版投递），没有总规划提示词。
                </p>
                <p v-else class="hint"><span class="spin">◔</span> 读取总规划…</p>
              </template>
              <p v-else-if="selectedStep" class="hint">这一步没有留下提示词（瞬时步骤，或早期版本跑的 job）。</p>
              <p v-else class="hint">上面轨道点任意一格，这里显示喂给模型的提示词。</p>
            </template>
            <template v-else>
              <p v-if="articleErr" class="errline">{{ articleErr }}</p>
              <pre v-else class="art">{{ article ?? "读取中…" }}</pre>
            </template>
          </div>
        </div>

        <!-- ══ 右：对话操控（步骤事件 + 日志 + 模型回复汇成一条流） ══ -->
        <div class="col-r pane">
          <div class="mid-t"><b>对话操控</b></div>
          <JobChatPanel :job="job" :log="log" :day-start="dayStart" />
        </div>
      </div>
      <div class="gd-body" v-else><div class="hint"><span class="spin">◔</span> 读取 job…</div></div>
    </div>
  </div>

  <!-- 就地改：叠在流程详情之上，改完关掉即回到原处（并刷新留痕） -->
  <ExpertPromptDrawer
    v-if="promptEdit"
    :expert-id="promptEdit.expertId"
    :platform="promptEdit.platform"
    lock-platform
    @close="closePromptEdit"
  />
</template>

<style scoped>
/* ═══ 卡片：不透明 ═══
   原来整张卡是半透明玻璃，后面的看板隐约透上来 —— 读提示词、读日志时那层底纹一直在
   跟文字抢眼睛。风格照旧（同样的圆角、同样的冷白渐层、同样的投影），只是把透明度收掉：
   打开就是一张实心纸，后面什么都看不见。 */
.gd-mask {
  background: rgba(18, 26, 52, .3);
  justify-content: center;
  align-items: center;
  padding: 20px;
  box-sizing: border-box;
}

.gd.gd-wide {
  width: min(1520px, 97vw);
  height: 100%;
  max-height: calc(100vh - 40px);
  border-radius: 22px;
  border: 1px solid var(--line-2);
  background: linear-gradient(158deg, #ffffff 0%, #f3f6fd 48%, #fbfcff 100%);
  box-shadow:
    0 32px 80px rgba(20, 30, 62, .28),
    0 2px 10px rgba(20, 30, 62, .10);
  overflow: hidden;
}

/* ── 顶条 ── */
.gd-h {
  flex: none;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 12px 16px;
  border-bottom: 1px solid var(--line);
  background: var(--panel);
  font-size: var(--text-s);
  font-weight: 400;
}
.hstat { font-size: var(--text-xs); color: var(--dim); }
.htitle { font-size: var(--text-m); color: var(--ink); font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.hsub { font-size: var(--text-xs); color: var(--muted); white-space: nowrap; }
.hsp { flex: 1; }
.gd-h .hgo {
  background: var(--accent);
  border-color: transparent;
  color: #fff;
  font-size: var(--text-xs);
  padding: 4px 14px;
}
.gd-h .hgo:hover { background: var(--accent-ink); }
.gd-h .xbtn {
  width: 26px; height: 26px;
  display: inline-flex; align-items: center; justify-content: center;
  border-radius: 50%;
  border: 1px solid var(--line);
  background: var(--panel);
  color: var(--dim);
  cursor: pointer;
  font-family: inherit;
}
.gd-h .xbtn:hover { background: var(--card2); color: var(--ink); }

/* ═══ 顶部工作流轨道 ═══
   原来这条流程是左下角一列竖着的按钮：既看不出「先后」，又把卡片最好的一块地占成一条窄条。
   现在整条流水线横着铺在顶条底下——总纲 → ① → ② → …，中间用箭头连起来，
   一眼看得出走到哪、哪一格红了。点任意一格，下面的提示词栏跟着换。 */
.gd-flow {
  flex: none;
  padding: 10px 16px 12px;
  border-bottom: 1px solid var(--line);
  background: linear-gradient(180deg, var(--panel), color-mix(in srgb, var(--panel) 82%, transparent));
}

/* 基本信息收成轨道上面一行：编号/选题/时间 + 产物入口 */
.gd-meta {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 4px 16px;
  margin-bottom: 10px;
  font-size: var(--text-2xs);
  color: var(--muted);
}
.gd-meta b { font-weight: 500; color: var(--ink2); }
.gd-meta .mono { font-family: Consolas, ui-monospace, monospace; font-variant-numeric: tabular-nums; }
.gd-meta .mtopic { max-width: 46%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.gd-meta .live { color: var(--warn); }

.rail {
  display: flex;
  align-items: stretch;
  overflow-x: auto;
  padding-bottom: 4px;
  scrollbar-width: thin;
}
.wfn {
  /* 格子平分轨道宽度：步骤少时铺满（中文步骤名长，宽一点才不被截成「配图（读文出画面…」），
     步骤多时缩到 min-width 后整条轨道横向滚动。 */
  flex: 1 1 0;
  min-width: 156px;
  max-width: 300px;
  display: flex;
  flex-direction: column;
  gap: 3px;
  padding: 8px 11px 9px;
  border: 1px solid var(--line);
  border-radius: 12px;
  background: var(--card);
  text-align: left;
  cursor: pointer;
  font-family: inherit;
  transition:
    background-color var(--dur-fast) var(--ease-out),
    border-color var(--dur-fast) var(--ease-out),
    box-shadow var(--dur-fast) var(--ease-out);
}
.wfn:hover { background: var(--card2); border-color: var(--line-2); }
.wfn.on {
  background: var(--card2);
  border-color: color-mix(in srgb, var(--accent) 55%, transparent);
  box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 12%, transparent);
}
.wfn.fail { border-color: color-mix(in srgb, var(--bad) 32%, transparent); }
.wfn.run { border-color: color-mix(in srgb, var(--warn) 42%, transparent); }
.wfn-h { display: flex; align-items: center; gap: 6px; }
.wfn-i {
  font-size: var(--text-2xs);
  color: var(--muted);
  font-variant-numeric: tabular-nums;
}
.wfn-s { margin-left: auto; font-size: var(--text-2xs); color: var(--muted); white-space: nowrap; }
.wfn.fail .wfn-s { color: var(--bad); }
.wfn.run .wfn-s { color: var(--warn); }
/* 步骤名最多两行，第二行还放不下才省略号——比一行截断能读出多得多 */
.wfn-l {
  font-size: var(--text-s);
  line-height: 1.45;
  color: var(--ink2);
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  word-break: break-word;
}
.wfn.on .wfn-l { color: var(--ink); font-weight: 600; }
.wfn.plan .wfn-l { font-weight: 600; }
.wfn-m {
  margin-top: auto; /* 名字占一行还是两行，脚注都贴在格子底边，一排对齐 */
  padding-top: 2px;
  min-height: 1.4em;
  font-size: var(--text-2xs);
  color: var(--muted);
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
}

/* 格与格之间的连线：跑过的实线，没跑到的虚线 */
.arr {
  flex: 0 0 auto;
  align-self: center;
  position: relative;
  width: 20px;
  margin: 0 3px;
  border-top: 1px dashed var(--line-2);
}
.arr.ok, .arr.run, .arr.fail { border-top-style: solid; }
.arr.fail { border-top-color: color-mix(in srgb, var(--bad) 45%, transparent); }
.arr::after {
  content: "";
  position: absolute;
  right: -1px; top: -4px;
  border-left: 5px solid var(--line-2);
  border-top: 4px solid transparent;
  border-bottom: 4px solid transparent;
}
.arr.fail::after { border-left-color: color-mix(in srgb, var(--bad) 45%, transparent); }
.rail-hint { margin: 0; align-self: center; padding-left: 14px; color: var(--muted); font-size: var(--text-xs); }

.gd-flow .errline { margin: 10px 0 0; }

/* ── 两栏正文：提示词（宽） + 对话操控 ── */
.gd-cols {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: minmax(420px, 1fr) minmax(340px, 400px);
  gap: 14px;
  padding: 14px 16px 16px;
  overflow: hidden;
}
.col-m, .col-r { min-width: 0; display: flex; flex-direction: column; }

.pane {
  border: 1px solid var(--line);
  background: var(--panel);
  border-radius: 16px;
  box-shadow: 0 2px 10px rgba(20, 30, 62, .05);
  overflow: hidden;
}
.errline {
  margin: 10px 14px 0;
  padding: 8px 10px;
  border-radius: 10px;
  background: rgba(208, 59, 59, .08);
  border: 1px solid rgba(208, 59, 59, .18);
  color: var(--tag-bad-ink, var(--bad));
  font-size: var(--text-xs);
  line-height: 1.6;
  word-break: break-word;
}
.lnk {
  border: none; background: transparent; padding: 0;
  color: var(--dim); font-family: inherit; font-size: var(--text-xs);
  cursor: pointer; text-decoration: underline; text-decoration-color: rgba(120, 130, 160, .35);
  text-underline-offset: 3px;
}
.lnk:hover { color: var(--accent-ink); text-decoration-color: currentColor; }

/* ── 提示词栏 ── */
.mid-t {
  flex: none;
  display: flex; align-items: baseline; gap: 10px;
  padding: 11px 16px 10px;
  border-bottom: 1px solid var(--line);
  font-size: var(--text-s);
  color: var(--ink);
}
.mid-t b { font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.mid-n { font-size: var(--text-2xs); color: var(--muted); font-variant-numeric: tabular-nums; }
/* 「原文」还是「预览」——这两件事不能含混，故给个明确小标 */
.mid-tag {
  flex: none;
  font-size: var(--text-2xs);
  padding: 1px 7px;
  border-radius: 9px;
  border: 1px solid color-mix(in srgb, var(--ok) 35%, transparent);
  color: var(--ok);
  white-space: nowrap;
}
.mid-tag.pre { border-color: var(--line-2); color: var(--muted); }
.mid-b { flex: 1; min-height: 0; overflow-y: auto; padding: 14px 22px 22px; }

.pb-h {
  font-size: var(--text-m);
  font-weight: 600;
  color: var(--ink);
  margin: 20px 0 8px;
  padding-left: 10px;
  border-left: 3px solid color-mix(in srgb, var(--accent) 55%, transparent);
  line-height: 1.5;
}
.pb-h:first-child { margin-top: 0; }
.pb-p {
  font-size: var(--text-s);
  line-height: 1.85;
  color: var(--ink2);
  margin: 0 0 10px;
  white-space: pre-wrap;
  word-break: break-word;
}
.pb-l { margin: 0 0 12px; padding-left: 4px; list-style: none; display: flex; flex-direction: column; gap: 6px; }
.pb-l li {
  position: relative;
  padding-left: 18px;
  font-size: var(--text-s);
  line-height: 1.8;
  color: var(--ink2);
  word-break: break-word;
}
.pb-l li::before {
  content: "";
  position: absolute;
  left: 3px; top: .72em;
  width: 5px; height: 5px;
  border-radius: 50%;
  background: color-mix(in srgb, var(--accent) 60%, transparent);
}
.pb-r { border: none; border-top: 1px solid rgba(120, 130, 165, .18); margin: 16px 0; }
.mid-b :deep(strong) { font-weight: 600; color: var(--ink); }
.mid-b :deep(code) {
  font-family: Consolas, ui-monospace, monospace;
  font-size: .92em;
  padding: 1px 5px;
  border-radius: 5px;
  background: var(--code-bg);
  border: 1px solid var(--line);
  color: var(--dim);
}
.art {
  margin: 0;
  font-size: var(--text-s);
  line-height: 1.85;
  color: var(--ink2);
  white-space: pre-wrap;
  word-break: break-word;
  background: transparent;
  border: none;
  padding: 0;
  font-family: inherit;
}

.hint { color: var(--muted); font-size: var(--text-xs); padding: 2px 14px 12px; line-height: 1.7; }
.mid-b .hint { padding: 0; }
.gd-body { flex: 1; display: flex; align-items: center; justify-content: center; }

@media (max-width: 1100px) {
  .gd-cols { grid-template-columns: 1fr; overflow-y: auto; }
  .col-m { min-height: 380px; }
  .col-r { min-height: 340px; }
  .wfn { width: 150px; }
}
</style>
