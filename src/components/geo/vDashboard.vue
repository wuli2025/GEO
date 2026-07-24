<script setup lang="ts">
import { computed, ref, onMounted, watch } from "vue";
import { vDashboardHtml, mergeKpi } from "./render";
import { MOCK } from "./data";
import { mediaOps, mediaJob, type MediaJob } from "../../tauri";
import { openJobId } from "./jobsBus";

defineProps<{ sub?: string; platform: string }>();

// KPI 卡带能接真的接真：mediaops_metrics_summary 覆盖发布数/成功率/token/成本，
// 取不到（后端未就绪 / 浏览器预览）沿用设计稿 mock。
const kpi = ref(MOCK.kpi);
// 「最近投递」接真实流水线 job：行带 data-job，壳层事件委托点开生成流程详情。
const jobs = ref<MediaJob[]>([]);
async function load() {
  try {
    const s = await mediaOps.metricsSummary();
    if (s) kpi.value = mergeKpi(MOCK.kpi, s.d7, s.d30);
  } catch {
    /* 后端未就绪：沿用 mock */
  }
  try { jobs.value = await mediaJob.list(); } catch { jobs.value = []; }
}
onMounted(load);
// 详情抽屉关掉时刷一遍（里面可能取消/重跑了 job）
watch(openJobId, (v) => { if (!v) load(); });

const html = computed(() => vDashboardHtml(kpi.value, jobs.value));
</script>
<template><div v-html="html"></div></template>
