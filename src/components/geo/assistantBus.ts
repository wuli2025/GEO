/**
 * 助手外壳总线 —— 助手与外壳之间的那点共享状态，只剩三件事：
 *
 *  1) 助手栏开合（顶栏「对话列表」键与助手自己的收起键共用一个开关）；
 *  2) 壳层记录流：跳转、排产、告警这类**不是对话**但该留痕的事，
 *     和会话消息、job 生成记录一起在助手里按时间汇成一条流；
 *  3) 助手 → 外壳的导航请求（「打开账号矩阵」「切到头条」）。
 *
 * 输入框已经长在助手自己身上了，所以这里不再有「投一句话给助手」的通道。
 */
import { ref } from "vue";

/* ── 助手栏开合 ─────────────────────────────────────────────────── */

const LS_OPEN = "geo.assistant.open";
/** 默认打开：助手是这套界面的主入口，不该藏起来等人去找。 */
export const dockOpen = ref(localStorage.getItem(LS_OPEN) !== "0");
export function setDock(v: boolean) {
  dockOpen.value = v;
  localStorage.setItem(LS_OPEN, v ? "1" : "0");
}
export function toggleDock() {
  setDock(!dockOpen.value);
}

/* ── 记录流（壳层发生的事：跳转、排产、告警） ─────────────────────── */

export type RecordKind = "sys" | "warn";
export interface ShellRecord {
  id: string;
  /** unix 秒 */
  ts: number;
  kind: RecordKind;
  text: string;
}

const LS_RECORDS = "geo.assistant.records";
/** 多泳道时代的分桶记录，首次加载时并成一条流。 */
const LS_RECORDS_LEGACY = "geo.lane.records";
const MAX_RECORDS = 120;

function loadRecords(): ShellRecord[] {
  try {
    const own = JSON.parse(localStorage.getItem(LS_RECORDS) || "null");
    if (Array.isArray(own) && own.length) return (own as ShellRecord[]).slice(-MAX_RECORDS);
  } catch { /* 坏数据当没有 */ }

  // 迁移：老的 { lane: rows[] } 分桶按时间并成一条，别让人的历史凭空少一截。
  try {
    const old = JSON.parse(localStorage.getItem(LS_RECORDS_LEGACY) || "null");
    if (old && typeof old === "object" && !Array.isArray(old)) {
      const merged = Object.values(old as Record<string, ShellRecord[]>)
        .filter(Array.isArray)
        .flat()
        .sort((a, b) => (a.ts || 0) - (b.ts || 0))
        .slice(-MAX_RECORDS);
      localStorage.removeItem(LS_RECORDS_LEGACY);
      return merged;
    }
  } catch { /* 同上 */ }
  return [];
}

export const records = ref<ShellRecord[]>(loadRecords());

let persistTimer: ReturnType<typeof setTimeout> | null = null;
function persist() {
  // 攒一拍再写：连发指令时不至于每条都撞一次 localStorage
  if (persistTimer) return;
  persistTimer = setTimeout(() => {
    persistTimer = null;
    try { localStorage.setItem(LS_RECORDS, JSON.stringify(records.value)); } catch { /* 配额满则丢弃持久化，内存里仍在 */ }
  }, 300);
}

let recSeq = 0;
export function pushRecord(kind: RecordKind, text: string): ShellRecord {
  recSeq += 1;
  const rec: ShellRecord = {
    id: `r${Date.now().toString(36)}-${recSeq}`,
    kind, text,
    ts: Math.floor(Date.now() / 1000),
  };
  records.value.push(rec);
  if (records.value.length > MAX_RECORDS) records.value.splice(0, records.value.length - MAX_RECORDS);
  persist();
  return rec;
}

/* ── 助手 → 外壳：导航请求 ─────────────────────────────────────── */

export interface NavReq { id: string; view: string; platform?: string }
export const navRequest = ref<NavReq | null>(null);
let navSeq = 0;
export function goTo(view: string, platform?: string) {
  navSeq += 1;
  navRequest.value = { id: `n-${navSeq}`, view, platform };
}
export function consumeNav(id: string) {
  if (navRequest.value?.id === id) navRequest.value = null;
}
