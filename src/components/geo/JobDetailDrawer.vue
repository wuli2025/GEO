<script setup lang="ts">
/**
 * 流程详情抽屉：任何一条投递流水线 job 点进来，看得到它的完整生成流程。
 * 布局是三栏——
 *  - 右栏：常驻对话框（JobChatPanel）——对这条流程直接提问/下指令，自动带实时状态
 * 左中两栏——
 *  - 左栏：基本信息 + 步骤时间线（每一步都能点）
 *  - 右栏：选中步骤的「思路与对话」面板——这一步谁在干（专家/技能/模型）、
 *    喂进去的提示词全文（对话输入）、以及按时间窗切出来的该步骤运行日志（过程自述）。
 *    右栏还有「全部日志」「正文产物」两个页签。
 *  - 就地改：任一步点「改提示词」直接开专家提示词抽屉，改完存即产生新版本、可回滚
 *  - 实时刷新（跑着时 2s 轮询，自动跟随当前进行中的步骤）
 *  - 操作：取消（在跑）/ 重跑（终态）
 */
import { ref, computed, watch, onMounted, onBeforeUnmount, nextTick } from "vue";
import { mediaJob, type MediaJob, type MediaJobStep } from "../../tauri";
import { toast } from "../../composables/useToast";
import { P } from "./data";
import ExpertPromptDrawer from "./ExpertPromptDrawer.vue";
import JobChatPanel from "./JobChatPanel.vue";

const props = defineProps<{ jobId: string }>();
const emit = defineEmits<{ (e: "close"): void; (e: "rerun", job: MediaJob): void }>();

const job = ref<MediaJob | null>(null);
const log = ref("");
const logEl = ref<HTMLPreElement | null>(null);
const article = ref<string | null>(null);
const articleErr = ref("");
const busy = ref(false);
let timer: ReturnType<typeof setInterval> | null = null;

const isLive = computed(() => job.value?.status === "running" || job.value?.status === "pending");
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
  return `${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

function fmtDur(ms: number): string {
  if (!ms || ms <= 0) return "";
  if (ms < 1000) return `${ms}ms`;
  const s = Math.round(ms / 1000);
  return s < 60 ? `${s}s` : `${Math.floor(s / 60)}m${String(s % 60).padStart(2, "0")}s`;
}

// ── 右栏：选中步骤 + 页签 ──
type Pane = "step" | "log" | "article";
const pane = ref<Pane>("step");
/** 选中的步骤 key。null = 还没手动点过 → 自动跟随（进行中的那步 / 最后一步）。 */
const pickedKey = ref<string | null>(null);
const selectedStep = computed<MediaJobStep | null>(() => {
  const steps = job.value?.steps ?? [];
  if (!steps.length) return null;
  if (pickedKey.value) {
    const hit = steps.find((s) => s.key === pickedKey.value);
    if (hit) return hit;
  }
  return steps.find((s) => s.status === "run") ?? steps[steps.length - 1];
});
function pickStep(s: MediaJobStep) {
  pickedKey.value = s.key;
  pane.value = "step";
}

/** 该步骤的日志时间窗切片：日志每行是 [HH:MM:SS] 前缀（本地时区），
 *  按 [本步开始, 下一步开始) 的秒数窗口切出属于这一步的过程自述。 */
const stepLog = computed<string>(() => {
  const s = selectedStep.value;
  if (!s || !log.value) return "";
  const steps = job.value?.steps ?? [];
  const idx = steps.findIndex((x) => x.key === s.key);
  const begin = s.startedAt || s.at;
  // 下一个「开始时间更晚」的步骤作为窗口右界（同秒并行步骤会互相包含，可接受）
  let end = Infinity;
  for (let i = idx + 1; i < steps.length; i++) {
    const t = steps[i].startedAt || steps[i].at;
    if (t > begin) { end = t; break; }
  }
  const sod = (epoch: number) => {
    const d = new Date(epoch * 1000);
    return d.getHours() * 3600 + d.getMinutes() * 60 + d.getSeconds();
  };
  const b = sod(begin), e = end === Infinity ? Infinity : sod(end);
  const out: string[] = [];
  let inWin = false;
  for (const line of log.value.split("\n")) {
    const m = /^\[(\d{2}):(\d{2}):(\d{2})\]/.exec(line);
    if (m) {
      const t = +m[1] * 3600 + +m[2] * 60 + +m[3];
      inWin = t >= b && t < e;
    }
    // 无时间戳的续行（py> 输出等）跟随上一行的归属
    if (inWin) out.push(line);
  }
  return out.join("\n").trim();
});

/** 这一步有没有留痕可看（老 job 快照全空则归因区不渲染）。 */
function hasTrace(s: MediaJobStep): boolean {
  return !!(s.expertId || s.skillId || s.skillScript || s.prompt || s.modelHint || s.durationMs);
}

const showFullPrompt = ref(false);
watch(selectedStep, () => { showFullPrompt.value = false; });

// ── 就地改提示词 ──
const promptEdit = ref<{ expertId: string; platform: string } | null>(null);
function editPrompt(s: MediaJobStep) {
  if (!job.value || !s.expertId) return;
  promptEdit.value = { expertId: s.expertId, platform: job.value.platform };
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
    const atBottom = !logEl.value ||
      logEl.value.scrollTop + logEl.value.clientHeight >= logEl.value.scrollHeight - 24;
    log.value = await mediaJob.log(props.jobId);
    if (atBottom) {
      await nextTick();
      if (logEl.value) logEl.value.scrollTop = logEl.value.scrollHeight;
    }
  } catch { /* 日志未产生（pending）时静默 */ }
  // 跑着才轮询；到终态自动停表。
  if (isLive.value && !timer) timer = setInterval(refresh, 2000);
  if (!isLive.value && timer) { clearInterval(timer); timer = null; }
}

async function loadArticle() {
  pane.value = "article";
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

async function copyText(t?: string) {
  if (!t) return;
  try { await navigator.clipboard.writeText(t); toast.success("已复制路径"); } catch { toast.error("复制失败"); }
}

watch(() => props.jobId, () => {
  job.value = null; log.value = ""; article.value = null;
  pickedKey.value = null; pane.value = "step";
  refresh();
});
onMounted(refresh);
onBeforeUnmount(() => { if (timer) clearInterval(timer); });
</script>

<template>
  <div class="gd-mask" @click.self="emit('close')">
    <div class="gd gd-wide">
      <div class="gd-h">
        <span v-if="job">
          流程详情 · {{ platName }} ·《{{ job.title }}》
          <span class="sline" style="margin-left:8px;font-weight:400">
            <span class="sdot" :class="STATUS_DOT[job.status] || 'idle'"></span>{{ STATUS_TEXT[job.status] || job.status }}
          </span>
        </span>
        <span v-else>流程详情</span>
        <button class="xbtn" title="关闭" @click="emit('close')">✕</button>
      </div>

      <div class="gd-cols" v-if="job">
        <!-- ══ 左栏：基本信息 + 可点的步骤时间线 ══ -->
        <div class="gd-left">
          <div class="card">
            <h3>基本信息</h3>
            <div class="tbl-wrap"><table>
              <tr><th style="width:90px">job</th><td style="font-variant-numeric:tabular-nums">{{ job.id }}</td></tr>
              <tr><th>阶段编排</th><td>{{ job.stages.join(" → ") }}</td></tr>
              <tr v-if="job.topic"><th>选题方向</th><td>{{ job.topic }}</td></tr>
              <tr><th>发起时间</th><td>{{ fmtTime(job.createdAt) }}<span style="color:var(--muted)">（最后更新 {{ fmtTime(job.updatedAt) }}）</span></td></tr>
              <tr v-if="job.error"><th>失败原因</th><td style="color:var(--bad)">{{ job.error }}</td></tr>
            </table></div>
            <div style="display:flex;gap:8px;margin-top:10px;flex-wrap:wrap">
              <button v-if="isLive" class="btn sm danger" :disabled="busy" @click="cancel">取消 job</button>
              <button v-if="!isLive && job.status !== 'done'" class="btn sm" :disabled="busy" @click="resume" title="从断点续跑：已完成且产物还在的阶段自动跳过">继续</button>
              <button v-if="!isLive" class="btn sm ghost" :disabled="busy" @click="rerun">按同参数重跑</button>
              <button v-if="job.articlePath" class="btn sm ghost" @click="copyText(job.articlePath)">复制产物路径</button>
              <button class="btn sm ghost" @click="copyText(job.logPath)">复制日志路径</button>
            </div>
          </div>

          <div class="card">
            <h3>生成流程<span v-if="isLive" style="color:var(--warn);font-weight:400;font-size:var(--text-xs)">（进行中，实时刷新）</span></h3>
            <div v-if="!job.steps.length" class="foot">还没有步骤记录{{ job.status === "pending" ? "，排队等待开跑…" : "（老 job 只有日志，点右上「全部日志」看）" }}。</div>
            <div v-else class="tl" style="margin-top:8px">
              <div
                v-for="s in job.steps"
                :key="s.key"
                class="tlitem tl-click"
                :class="{ picked: pane === 'step' && selectedStep?.key === s.key }"
                role="button"
                tabindex="0"
                :title="'点开右侧看这一步的思路与对话'"
                @click="pickStep(s)"
                @keydown.enter="pickStep(s)"
              >
                <div class="when">
                  {{ fmtTime(s.at) }} · <span class="sline"><span class="sdot" :class="STEP_DOT[s.status] || 'idle'"></span>{{ STEP_TEXT[s.status] || s.status }}</span>
                  <template v-if="fmtDur(s.durationMs)"> · 耗时 {{ fmtDur(s.durationMs) }}</template>
                </div>
                <div class="what">
                  <b>{{ s.label }}</b>
                  <span v-if="s.expertName" class="tchip" style="margin-left:6px">👤 {{ s.expertName }}</span>
                  <span class="tl-more">查看 ›</span>
                </div>
              </div>
            </div>
          </div>
        </div>

        <!-- ══ 右栏：选中步骤的思路与对话 / 全部日志 / 正文产物 ══ -->
        <div class="gd-right">
          <div class="gd-tabs">
            <button class="stab" :class="{ active: pane === 'step' }" @click="pane = 'step'">思路与对话</button>
            <button class="stab" :class="{ active: pane === 'log' }" @click="pane = 'log'">全部日志</button>
            <button class="stab" :class="{ active: pane === 'article' }" :disabled="!job.articlePath" @click="loadArticle">正文产物</button>
          </div>

          <!-- 步骤面板 -->
          <div v-if="pane === 'step'" class="gd-pane">
            <template v-if="selectedStep">
              <div class="sp-head">
                <b>{{ selectedStep.label }}</b>
                <span class="sline"><span class="sdot" :class="STEP_DOT[selectedStep.status] || 'idle'"></span>{{ STEP_TEXT[selectedStep.status] || selectedStep.status }}</span>
              </div>
              <div class="sp-meta">
                {{ fmtTime(selectedStep.startedAt || selectedStep.at) }}
                <template v-if="fmtDur(selectedStep.durationMs)"> · 耗时 {{ fmtDur(selectedStep.durationMs) }}</template>
              </div>
              <p v-if="selectedStep.detail" class="sp-detail">{{ selectedStep.detail }}</p>

              <div v-if="hasTrace(selectedStep)" class="trace">
                <span v-if="selectedStep.expertName" class="tchip" :title="`专家 id：${selectedStep.expertId}`">👤 {{ selectedStep.expertName }}</span>
                <span v-if="selectedStep.skillId" class="tchip" :title="selectedStep.skillScript || '编排里配置的技能 id'">🧩 {{ selectedStep.skillId }}</span>
                <span v-if="selectedStep.modelHint" class="tchip" title="专家卡上的推荐模型档；生成实际走 claude CLI 默认模型，未显式下发 --model">🤖 建议 {{ selectedStep.modelHint }}</span>
                <span v-if="selectedStep.promptVersionId" class="tchip" title="生成时该专家在本平台补丁的生效版本">🕓 补丁版本 {{ selectedStep.promptVersionId.slice(0, 8) }}</span>
                <button v-if="selectedStep.expertId" class="btn sm ghost" @click="editPrompt(selectedStep)">改提示词</button>
              </div>
              <p v-if="selectedStep.skillScript" class="foot" style="word-break:break-all">脚本：{{ selectedStep.skillScript }}</p>

              <!-- 对话输入：喂给模型的提示词全文 -->
              <template v-if="selectedStep.prompt">
                <h4 class="sp-sub">🗣 喂给模型的提示词（对话输入 · {{ selectedStep.prompt.length }} 字）</h4>
                <pre class="gd-doc sp-doc" :class="{ clamp: !showFullPrompt }">{{ selectedStep.prompt }}</pre>
                <button class="btn sm ghost" @click="showFullPrompt = !showFullPrompt">{{ showFullPrompt ? "收起" : "展开全文" }}</button>
              </template>

              <!-- 过程自述：这一步时间窗内的运行日志 -->
              <h4 class="sp-sub">💭 这一步的过程记录<span v-if="isLive && selectedStep.status === 'run'" style="color:var(--warn);font-weight:400">（实时）</span></h4>
              <pre v-if="stepLog" class="gd-doc sp-doc tall">{{ stepLog }}</pre>
              <p v-else class="foot">这一步没有独立的日志片段（瞬时步骤或老 job）——可切「全部日志」看完整过程。</p>
            </template>
            <p v-else class="foot">左侧点任意一步，这里会显示这一步的思路与对话。</p>
          </div>

          <!-- 全部日志 -->
          <div v-else-if="pane === 'log'" class="gd-pane">
            <p class="foot" style="margin:0 0 8px;word-break:break-all">{{ job.logPath }}</p>
            <pre ref="logEl" class="gd-doc sp-doc grow">{{ log || "（暂无日志）" }}</pre>
          </div>

          <!-- 正文产物 -->
          <div v-else class="gd-pane">
            <p v-if="articleErr" class="foot" style="color:var(--bad)">{{ articleErr }}</p>
            <pre v-else class="gd-doc sp-doc grow">{{ article ?? "读取中…" }}</pre>
          </div>
        </div>

        <!-- ══ 右栏：常驻对话框——对这条泳道直接下指令/提问 ══ -->
        <div class="gd-chat">
          <div class="gd-chat-h">💬 对话操控<span class="foot" style="font-weight:400">（带实时状态）</span></div>
          <div class="gd-pane">
            <JobChatPanel :job="job" :log="log" />
          </div>
        </div>
      </div>
      <div class="gd-body" v-else><div class="foot"><span class="spin">◔</span> 读取 job…</div></div>
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
.gd-mask {
  background: var(--overlay);
  justify-content: center;
  align-items: center;
  padding: 18px;
  box-sizing: border-box;
}

.gd.gd-wide {
  width: min(1480px, 97vw);
  height: 100%;
  max-height: calc(100vh - 36px);
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: 22px;
  box-shadow: var(--shadow-lg);
  overflow: hidden;
}

.gd-h {
  border-bottom: 1px solid var(--border-soft);
  background: var(--bg-soft);
  letter-spacing: .01em;
}
.gd-h .xbtn {
  width: 28px;
  height: 28px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 50%;
  background: var(--bg-soft);
  border: 1px solid var(--border-soft);
}
.gd-h .xbtn:hover { background: var(--selection-bg); }

.gd-left .card,
.gd-right,
.gd-chat {
  background: var(--panel);
  border: 1px solid var(--border-soft);
  border-radius: 16px;
  box-shadow: var(--shadow-sm);
}

.gd-left :deep(.tbl-wrap) { border-radius: 12px; overflow: hidden; border: 1px solid var(--border-soft); }
.gd-left :deep(table) { border-collapse: collapse; }
.gd-left :deep(th) { background: var(--bg-soft); border: none; border-bottom: 1px solid var(--border-soft); }
.gd-left :deep(td) { background: var(--panel); border: none; border-bottom: 1px solid var(--border-soft); }
.gd-left :deep(tr:last-child th), .gd-left :deep(tr:last-child td) { border-bottom: none; }
.gd-cols {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: minmax(280px, 4fr) minmax(340px, 5fr) minmax(300px, 4fr);
  gap: 14px;
  padding: 16px;
  overflow: hidden;
}
.gd-chat {
  min-width: 0;
  display: flex;
  flex-direction: column;
  background: var(--card);
  border: 1px solid var(--line);
  border-radius: var(--radius-card);
  overflow: hidden;
}
.gd-chat-h {
  flex: none;
  display: flex;
  align-items: baseline;
  gap: 6px;
  padding: 11px 14px;
  border-bottom: 1px solid rgba(255, 255, 255, .6);
  background: linear-gradient(180deg, rgba(255, 255, 255, .5), rgba(255, 255, 255, .1));
  font-size: var(--text-s);
  font-weight: 600;
}
.gd-left { overflow-y: auto; display: flex; flex-direction: column; gap: 14px; min-width: 0; }
.gd-right {
  min-width: 0;
  display: flex;
  flex-direction: column;
  background: var(--card);
  border: 1px solid var(--line);
  border-radius: var(--radius-card);
  overflow: hidden;
}
/* iOS 分段式页签:容器凹槽 + 选中白色浮起小药丸 */
.gd-tabs {
  display: flex;
  gap: 2px;
  margin: 10px 12px 0;
  padding: 3px;
  border: none;
  flex: none;
  background: rgba(118, 128, 160, .12);
  border-radius: 10px;
  box-shadow: inset 0 1px 2px rgba(30, 40, 80, .06);
}
.gd-tabs .stab {
  flex: 1;
  border: none;
  border-radius: 8px;
  background: transparent;
  transition: background-color var(--dur-fast) var(--ease-out), color var(--dur-fast) var(--ease-out), box-shadow var(--dur-fast) var(--ease-out);
}
.gd-tabs .stab:hover { background: rgba(255, 255, 255, .45); }
.gd-tabs .stab.active {
  background: rgba(255, 255, 255, .95);
  color: var(--ink);
  box-shadow: 0 1px 3px rgba(30, 40, 80, .14), 0 3px 8px rgba(30, 40, 80, .07);
}
.gd-pane { flex: 1; min-height: 0; overflow-y: auto; padding: 12px 16px 16px; display: flex; flex-direction: column; }

/* 左栏时间线：可点(玻璃小卡) */
.tl-click { cursor: pointer; border-radius: 10px; padding: 8px 10px; margin-left: -8px; border: 1px solid transparent; transition: background-color var(--dur-fast) var(--ease-out), border-color var(--dur-fast) var(--ease-out), box-shadow var(--dur-fast) var(--ease-out); }
.tl-click:hover { background: rgba(255, 255, 255, .75); border-color: rgba(255, 255, 255, .9); box-shadow: 0 2px 8px rgba(30, 40, 80, .07); }
.tl-click.picked {
  background: linear-gradient(150deg, rgba(255, 255, 255, .92), rgba(238, 242, 255, .8));
  border-color: rgba(255, 255, 255, .95);
  outline: none;
  box-shadow: 0 2px 6px rgba(66, 99, 235, .12), 0 6px 18px rgba(30, 40, 80, .08), inset 0 1px 0 rgba(255, 255, 255, 1);
}
.tl-click .tl-more { float: right; color: var(--muted); font-size: var(--text-2xs); opacity: 0; transition: opacity var(--dur-fast) var(--ease-out); }
.tl-click:hover .tl-more, .tl-click.picked .tl-more { opacity: 1; }

/* 右栏步骤面板 */
.sp-head { display: flex; align-items: center; justify-content: space-between; gap: 8px; font-size: var(--text-m); }
.sp-meta { color: var(--muted); font-size: var(--text-2xs); margin-top: 2px; }
.sp-detail { color: var(--ink2); font-size: var(--text-s); margin-top: 6px; word-break: break-word; }
.sp-sub { font-size: var(--text-xs); color: var(--dim); font-weight: 600; margin: 14px 0 6px; }
.sp-doc { max-height: none; }
.sp-doc.clamp { max-height: 180px; overflow: hidden; -webkit-mask-image: linear-gradient(#000 60%, transparent); mask-image: linear-gradient(#000 60%, transparent); }
.sp-doc.tall { max-height: 340px; overflow: auto; }
.sp-doc.grow { flex: 1; min-height: 0; overflow: auto; }
.sp-doc + .btn { align-self: flex-start; margin-top: 6px; }

.trace { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; margin-top: 10px; }
.tchip {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  border: 1px solid rgba(255, 255, 255, .85);
  background: rgba(255, 255, 255, .6);
  box-shadow: 0 1px 2px rgba(30, 40, 80, .06), inset 0 1px 0 rgba(255, 255, 255, .9);
  color: var(--ink2);
  font-size: var(--text-2xs);
  padding: 3px 9px;
  border-radius: var(--radius-pill, 999px);
  white-space: nowrap;
}

/* 文档/日志块:内嵌玻璃凹槽 */
.gd-right :deep(.gd-doc), .gd-chat :deep(.gd-doc) {
  background: rgba(246, 248, 253, .72);
  border: 1px solid rgba(255, 255, 255, .85);
  border-radius: 12px;
  box-shadow: inset 0 1px 3px rgba(30, 40, 80, .05);
}

/* 右栏对话气泡与输入区顺色 */
.gd-chat :deep(.jc-bubble) {
  box-shadow: 0 1px 3px rgba(30, 40, 80, .07);
}
.gd-chat :deep(.jc-input) {
  border-top: 1px solid rgba(255, 255, 255, .6);
  background: linear-gradient(0deg, rgba(255, 255, 255, .5), rgba(255, 255, 255, .12));
}
.gd-chat :deep(.jc-input .ta),
.gd-chat :deep(.jc-input textarea) {
  background: rgba(255, 255, 255, .75);
  border: 1px solid rgba(255, 255, 255, .9);
  border-radius: 12px;
  box-shadow: inset 0 1px 3px rgba(30, 40, 80, .05);
}

@media (max-width: 1180px) {
  .gd-cols { grid-template-columns: minmax(300px, 1fr) minmax(320px, 1fr); }
  .gd-chat { grid-column: 1 / -1; min-height: 320px; }
}
@media (max-width: 900px) {
  .gd-cols { grid-template-columns: 1fr; overflow-y: auto; }
  .gd-right { min-height: 420px; }
  .gd-chat { min-height: 380px; }
}
</style>
