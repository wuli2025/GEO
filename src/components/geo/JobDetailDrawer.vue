<script setup lang="ts">
/**
 * 流程详情抽屉：任何一条投递流水线 job 点进来，看得到它的完整生成流程——
 *  - 步骤时间线（MediaJob.steps：三大阶段 + upload 脚本每步回执）
 *  - 实时日志（media_job_log，跑着时 2s 轮询自动滚底）
 *  - 正文产物预览（media_job_article，按需加载）
 *  - 操作：取消（在跑）/ 重跑（终态）
 */
import { ref, computed, watch, onMounted, onBeforeUnmount, nextTick } from "vue";
import { mediaJob, type MediaJob } from "../../tauri";
import { toast } from "../../composables/useToast";
import { P } from "./data";

const props = defineProps<{ jobId: string }>();
const emit = defineEmits<{ (e: "close"): void; (e: "rerun", job: MediaJob): void }>();

const job = ref<MediaJob | null>(null);
const log = ref("");
const logEl = ref<HTMLPreElement | null>(null);
const article = ref<string | null>(null);
const articleErr = ref("");
const showArticle = ref(false);
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
  showArticle.value = !showArticle.value;
  if (!showArticle.value || article.value !== null) return;
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
  job.value = null; log.value = ""; article.value = null; showArticle.value = false;
  refresh();
});
onMounted(refresh);
onBeforeUnmount(() => { if (timer) clearInterval(timer); });
</script>

<template>
  <div class="gd-mask" @click.self="emit('close')">
    <div class="gd">
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
      <div class="gd-body" v-if="job">
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
            <button v-else class="btn sm" :disabled="busy" @click="rerun">按同参数重跑</button>
            <button v-if="job.articlePath" class="btn sm ghost" @click="copyText(job.articlePath)">复制产物路径</button>
            <button class="btn sm ghost" @click="copyText(job.logPath)">复制日志路径</button>
          </div>
        </div>

        <div class="card">
          <h3>生成流程<span v-if="isLive" style="color:var(--warn);font-weight:400;font-size:var(--text-xs)">（进行中，实时刷新）</span></h3>
          <div v-if="!job.steps.length" class="foot">还没有步骤记录{{ job.status === "pending" ? "，排队等待开跑…" : "（老 job 只有日志，见下方）" }}。</div>
          <div v-else class="tl" style="margin-top:8px">
            <div v-for="s in job.steps" :key="s.key" class="tlitem">
              <div class="when">{{ fmtTime(s.at) }} · <span class="sline"><span class="sdot" :class="STEP_DOT[s.status] || 'idle'"></span>{{ STEP_TEXT[s.status] || s.status }}</span></div>
              <div class="what"><b>{{ s.label }}</b><template v-if="s.detail"><br /><span style="color:var(--dim);word-break:break-all">{{ s.detail }}</span></template></div>
            </div>
          </div>
        </div>

        <div class="card">
          <h3>运行日志<span style="color:var(--muted);font-weight:400;font-size:var(--text-2xs)">（{{ job.logPath }}）</span></h3>
          <pre ref="logEl" class="gd-doc" style="max-height:260px">{{ log || "（暂无日志）" }}</pre>
        </div>

        <div class="card">
          <h3 style="display:flex;justify-content:space-between;align-items:center">
            正文产物预览
            <button class="btn sm ghost" :disabled="!job.articlePath" @click="loadArticle">{{ showArticle ? "收起" : job.articlePath ? "展开预览" : "尚无产物" }}</button>
          </h3>
          <template v-if="showArticle">
            <p v-if="articleErr" class="foot" style="color:var(--bad)">{{ articleErr }}</p>
            <pre v-else class="gd-doc" style="max-height:320px">{{ article ?? "读取中…" }}</pre>
          </template>
        </div>
      </div>
      <div class="gd-body" v-else><div class="foot"><span class="spin">◔</span> 读取 job…</div></div>
    </div>
  </div>
</template>
