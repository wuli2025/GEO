<script setup lang="ts">
import { computed, ref, onMounted, watch } from "vue";
import { vDashboardHtml, mergeKpi } from "./render";
import { MOCK } from "./data";
import { mediaOps, mediaJob, type MediaJob, type MediaCrawlSnapshot } from "../../tauri";
import { openJobId } from "./jobsBus";

defineProps<{ sub?: string; platform: string }>();

// KPI 卡带能接真的接真：mediaops_metrics_summary 覆盖发布数/成功率/token/成本，
// 取不到（后端未就绪 / 浏览器预览）沿用设计稿 mock。
const kpi = ref(MOCK.kpi);
// 「最近投递」接真实流水线 job：行带 data-job，壳层事件委托点开生成流程详情。
const jobs = ref<MediaJob[]>([]);
// 「阅读/点击」与「平台×指标热力表」的数据源：各平台创作者后台抓取快照。
// 本地度量事件只知道我们发了什么，不知道发出去之后有多少人看——那些数字只有平台后台有。
const crawls = ref<MediaCrawlSnapshot[]>([]);
const crawling = ref(false);
const crawlErr = ref("");

async function load() {
  try {
    const s = await mediaOps.metricsSummary();
    if (s) kpi.value = mergeKpi(MOCK.kpi, s.d7, s.d30);
  } catch {
    /* 后端未就绪：沿用 mock */
  }
  try { jobs.value = await mediaJob.list(); } catch { jobs.value = []; }
  // 只读快照，绝不在加载时自动开浏览器抓取——那会在用户没点任何东西时弹出 Chrome 窗口。
  try { crawls.value = await mediaOps.crawlSnapshots(); } catch { crawls.value = []; }
}
onMounted(load);
// 详情抽屉关掉时刷一遍（里面可能取消/重跑了 job）
watch(openJobId, (v) => { if (!v) load(); });

/** 「立即抓取」：慢操作（每平台约 25 秒 × N），期间按钮置灰、错误就地显示。 */
async function runCrawl() {
  if (crawling.value) return;
  crawling.value = true;
  crawlErr.value = "";
  try {
    crawls.value = await mediaOps.crawlRun();
  } catch (e: any) {
    crawlErr.value = String(e?.message || e);
  } finally {
    crawling.value = false;
  }
}

// 壳层的全局事件委托只认 data-go / data-job，这个按钮是看板独有的，就地委托。
function onClick(ev: MouseEvent) {
  const el = (ev.target as HTMLElement)?.closest?.("[data-crawl]");
  if (el) { ev.stopPropagation(); void runCrawl(); }
}

const html = computed(() => vDashboardHtml(kpi.value, jobs.value, crawls.value, crawling.value));
</script>
<template>
  <div>
    <div v-html="html" @click="onClick"></div>
    <p v-if="crawlErr" class="empty" style="color:var(--bad)">抓取失败：{{ crawlErr }}</p>
  </div>
</template>
