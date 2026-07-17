/**
 * 跨视图打开「流程详情」的极简总线：
 * 仪表盘「最近投递」/ 门户规划队列点某条流程 → 置 openJobId →
 * 挂在 GeoOpsCenter 壳层的 JobDetailDrawer 弹出对应 job 的生成流程。
 */
import { ref } from "vue";

export const openJobId = ref<string | null>(null);

export function openJobDetail(id: string) {
  openJobId.value = id;
}
export function closeJobDetail() {
  openJobId.value = null;
}
