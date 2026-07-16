<script setup lang="ts">
import { computed, ref, onMounted } from "vue";
import { vDashboardHtml, mergeKpi } from "./render";
import { MOCK } from "./data";
import { mediaOps } from "../../tauri";

const props = defineProps<{ sub: string; platform: string }>();

// KPI 卡带能接真的接真：mediaops_metrics_summary 覆盖发布数/成功率/token/成本，
// 取不到（后端未就绪 / 浏览器预览）沿用设计稿 mock。
const kpi = ref(MOCK.kpi);
onMounted(async () => {
  try {
    const s = await mediaOps.metricsSummary();
    if (s) kpi.value = mergeKpi(MOCK.kpi, s.d7, s.d30);
  } catch {
    /* 后端未就绪：沿用 mock */
  }
});

const html = computed(() => vDashboardHtml(props.sub, kpi.value));
</script>
<template><div v-html="html"></div></template>
