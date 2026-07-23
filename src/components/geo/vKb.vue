<script setup lang="ts">
/**
 * 知识库：资料文件清单（kb_list 实时）+ 星图（kb_graph 力导向内联渲染，Obsidian 风）。
 * 支持把文件（建议 Markdown / 文本）拖入本页入库；旧的静态说明页与「打开」跳转均已删。
 */
import { ref, shallowRef, computed, onMounted, onBeforeUnmount } from "vue";
import { title } from "./render";
import { kb, isTauri, uploadToBackend, type KbGraph, type KbNode } from "../../tauri";
import { toast } from "../../composables/useToast";

const props = defineProps<{ sub: string; platform: string }>();
const head = computed(() => title("知识库", "资源 / 资料文件 + 语义星图 · 可拖入文件入库"));

// ── 资料文件清单 ──
const files = ref<string[]>([]);
const filesLoaded = ref(false);
const filter = ref("");
async function loadFiles() {
  try {
    files.value = await kb.list(null);
  } catch {
    files.value = [];
  } finally {
    filesLoaded.value = true;
  }
}
const shownFiles = computed(() => {
  const q = filter.value.trim().toLowerCase();
  return q ? files.value.filter((f) => f.toLowerCase().includes(q)) : files.value;
});
async function removeFile(rel: string) {
  if (!confirm(`删除「${rel}」？模型将不再读到这份资料。`)) return;
  try {
    const left = await kb.delete(rel);
    toast.info(`已删除，剩余 ${left} 份`);
    await refreshAll();
  } catch (e: any) {
    toast.error(e?.message ?? String(e));
  }
}

// ── 拖拽入库（Tauri 走原生路径事件；浏览器/Docker 走 HTML5 + /api/upload）──
const dragOver = ref(false);
const uploading = ref(false);
let unlistenDrop: (() => void) | null = null;

async function ingestPaths(paths: string[]) {
  if (!paths?.length) return;
  uploading.value = true;
  try {
    const r = await kb.uploadFiles(paths);
    const ok = r.filter((x) => x.ok).length;
    const failed = r.filter((x) => !x.ok);
    toast.info(`已入库 ${ok}/${r.length} 份${failed.length ? "，" + failed.length + " 份失败" : ""}`);
    if (failed.length) toast.error(failed.map((f) => `${f.name}: ${f.message}`).join("；"));
    await refreshAll();
  } catch (e: any) {
    toast.error(e?.message ?? String(e));
  } finally {
    uploading.value = false;
  }
}

async function onHtmlDrop(e: DragEvent) {
  e.preventDefault();
  dragOver.value = false;
  if (isTauri) return; // 桌面端由原生事件处理，HTML drop 不带文件
  const fl = e.dataTransfer?.files;
  if (!fl?.length) return;
  uploading.value = true;
  try {
    const up = await uploadToBackend(fl);
    const paths = up.map((u) => u.path).filter(Boolean);
    if (!paths.length) { toast.error("上传失败：后端未返回文件路径"); return; }
    await ingestPaths(paths);
  } catch (e: any) {
    toast.error(e?.message ?? String(e));
  } finally {
    uploading.value = false;
  }
}
function onDragOver(e: DragEvent) { e.preventDefault(); dragOver.value = true; }
function onDragLeave() { dragOver.value = false; }

// ── 星图：kb_graph → 力导向布局（自写轻量物理，无外部依赖）──
const graph = ref<KbGraph | null>(null);
const graphLoaded = ref(false);

interface PNode { id: string; x: number; y: number; vx: number; vy: number; fx: number | null; fy: number | null; n: KbNode; deg: number }
let pnodes: PNode[] = [];
let idIndex = new Map<string, number>();
const eIdx = ref<[number, number][]>([]);
const nodes = shallowRef<PNode[]>([]); // 每帧重新赋值触发渲染（对象复用，n 小）

const CAT_COLORS = ["var(--s1)", "var(--s2)", "var(--s3)", "var(--s4)", "var(--s5)"];
const catColor = computed(() => {
  const m = new Map<string, string>();
  (graph.value?.nodes ?? []).forEach((n) => {
    if (!m.has(n.category)) m.set(n.category, CAT_COLORS[m.size % CAT_COLORS.length]);
  });
  return m;
});

const VB = 1000;
const C = VB / 2;
const view = ref({ x: 0, y: 0, w: VB, h: VB });
const viewBox = computed(() => `${view.value.x} ${view.value.y} ${view.value.w} ${view.value.h}`);
const hoverId = ref<string | null>(null);

function buildSim() {
  const g = graph.value;
  pnodes = [];
  idIndex = new Map();
  const deg = new Map<string, number>();
  (g?.edges ?? []).forEach((e) => {
    deg.set(e.source, (deg.get(e.source) ?? 0) + 1);
    deg.set(e.target, (deg.get(e.target) ?? 0) + 1);
  });
  (g?.nodes ?? []).forEach((n, i) => {
    idIndex.set(n.id, i);
    // 确定性初值（免物理抖动 + 不用 Math.random 也可复现）：黄金角螺旋撒开
    const a = i * 2.399963;
    const r = 40 + Math.sqrt(i) * 26;
    pnodes.push({
      id: n.id, x: C + Math.cos(a) * r, y: C + Math.sin(a) * r,
      vx: 0, vy: 0, fx: n.kind === "root" ? C : null, fy: n.kind === "root" ? C : null,
      n, deg: deg.get(n.id) ?? 0,
    });
  });
  eIdx.value = (g?.edges ?? [])
    .map((e) => [idIndex.get(e.source), idIndex.get(e.target)] as [number | undefined, number | undefined])
    .filter((p): p is [number, number] => p[0] !== undefined && p[1] !== undefined);
  nodes.value = pnodes.slice();
  reheat();
}

const edgeLines = computed(() =>
  eIdx.value.map(([s, t]) => ({ a: nodes.value[s], b: nodes.value[t], s, t }))
    .filter((e) => e.a && e.b),
);

function nodeR(p: PNode): number {
  return p.n.kind === "root" ? 13 : p.n.kind === "folder" ? 8 + Math.min(p.deg, 6) : 4 + Math.min(p.deg, 5);
}
function labelVisible(p: PNode): boolean {
  return p.n.kind !== "doc" || hoverId.value === p.id;
}
function dim(p: PNode): boolean {
  if (!hoverId.value) return false;
  if (p.id === hoverId.value) return false;
  return !eIdx.value.some(([s, t]) =>
    (pnodes[s].id === hoverId.value && pnodes[t].id === p.id) ||
    (pnodes[t].id === hoverId.value && pnodes[s].id === p.id));
}

// 物理：库仑斥力 + 边弹簧 + 向心，速度阻尼；alpha 退火，拖动时重加热
let alpha = 0;
let raf = 0;
function reheat() { alpha = Math.max(alpha, 0.9); if (!raf) raf = requestAnimationFrame(step); }
function step() {
  raf = 0;
  const ns = pnodes;
  const rep = 1600, spring = 0.03, rest = 70, center = 0.006, damp = 0.86;
  for (const a of ns) { if (a.fx == null) { a.vx += (C - a.x) * center; a.vy += (C - a.y) * center; } }
  for (let i = 0; i < ns.length; i++) {
    for (let j = i + 1; j < ns.length; j++) {
      const a = ns[i], b = ns[j];
      let dx = a.x - b.x, dy = a.y - b.y; let d2 = dx * dx + dy * dy || 0.01;
      const f = rep / d2; const d = Math.sqrt(d2); const fx = (dx / d) * f, fy = (dy / d) * f;
      if (a.fx == null) { a.vx += fx; a.vy += fy; }
      if (b.fx == null) { b.vx -= fx; b.vy -= fy; }
    }
  }
  for (const [s, t] of eIdx.value) {
    const a = ns[s], b = ns[t];
    let dx = b.x - a.x, dy = b.y - a.y; const d = Math.hypot(dx, dy) || 0.01;
    const f = (d - rest) * spring; const fx = (dx / d) * f, fy = (dy / d) * f;
    if (a.fx == null) { a.vx += fx; a.vy += fy; }
    if (b.fx == null) { b.vx -= fx; b.vy -= fy; }
  }
  let energy = 0;
  for (const a of ns) {
    if (a.fx != null) { a.x = a.fx; a.y = a.fy!; a.vx = 0; a.vy = 0; continue; }
    a.vx *= damp; a.vy *= damp; a.x += a.vx * alpha; a.y += a.vy * alpha;
    energy += a.vx * a.vx + a.vy * a.vy;
  }
  nodes.value = ns.slice();
  alpha *= 0.985;
  if (alpha > 0.02 && energy > 0.05 || draggingId) raf = requestAnimationFrame(step);
  else alpha = 0;
}

// ── 交互：节点拖动 / 背景平移 / 滚轮缩放 ──
const svgEl = ref<SVGSVGElement | null>(null);
let draggingId: string | null = null;
let panStart: { mx: number; my: number; vx: number; vy: number } | null = null;

function toSvg(clientX: number, clientY: number) {
  const svg = svgEl.value; if (!svg) return { x: 0, y: 0 };
  const pt = svg.createSVGPoint(); pt.x = clientX; pt.y = clientY;
  const m = svg.getScreenCTM(); if (!m) return { x: 0, y: 0 };
  const p = pt.matrixTransform(m.inverse()); return { x: p.x, y: p.y };
}
function onNodeDown(e: PointerEvent, p: PNode) {
  e.stopPropagation();
  draggingId = p.id;
  const s = toSvg(e.clientX, e.clientY); p.fx = s.x; p.fy = s.y;
  reheat();
  window.addEventListener("pointermove", onNodeMove);
  window.addEventListener("pointerup", onNodeUp);
}
function onNodeMove(e: PointerEvent) {
  if (!draggingId) return;
  const i = idIndex.get(draggingId); if (i === undefined) return;
  const s = toSvg(e.clientX, e.clientY); pnodes[i].fx = s.x; pnodes[i].fy = s.y;
  reheat();
}
function onNodeUp() {
  if (draggingId) { const i = idIndex.get(draggingId); if (i !== undefined && pnodes[i].n.kind !== "root") { pnodes[i].fx = null; pnodes[i].fy = null; } }
  draggingId = null;
  window.removeEventListener("pointermove", onNodeMove);
  window.removeEventListener("pointerup", onNodeUp);
  reheat();
}
function onBgDown(e: PointerEvent) {
  panStart = { mx: e.clientX, my: e.clientY, vx: view.value.x, vy: view.value.y };
  window.addEventListener("pointermove", onBgMove);
  window.addEventListener("pointerup", onBgUp);
}
function onBgMove(e: PointerEvent) {
  if (!panStart || !svgEl.value) return;
  const scale = view.value.w / svgEl.value.clientWidth;
  view.value = { ...view.value, x: panStart.vx - (e.clientX - panStart.mx) * scale, y: panStart.vy - (e.clientY - panStart.my) * scale };
}
function onBgUp() {
  panStart = null;
  window.removeEventListener("pointermove", onBgMove);
  window.removeEventListener("pointerup", onBgUp);
}
function onWheel(e: WheelEvent) {
  e.preventDefault();
  const p = toSvg(e.clientX, e.clientY);
  const f = e.deltaY > 0 ? 1.12 : 0.89;
  const w = Math.min(VB * 3, Math.max(VB * 0.2, view.value.w * f));
  const h = w; // 正方形
  view.value = { w, h, x: p.x - (p.x - view.value.x) * (w / view.value.w), y: p.y - (p.y - view.value.y) * (h / view.value.h) };
}
function resetView() { view.value = { x: 0, y: 0, w: VB, h: VB }; }

async function loadGraph() {
  try {
    graph.value = await kb.graph();
  } catch {
    graph.value = null;
  } finally {
    graphLoaded.value = true;
    buildSim();
  }
}

async function refreshAll() { await Promise.all([loadFiles(), loadGraph()]); }

onMounted(async () => {
  refreshAll();
  if (isTauri) {
    try {
      const { getCurrentWebview } = await import("@tauri-apps/api/webview");
      unlistenDrop = await getCurrentWebview().onDragDropEvent((ev: any) => {
        const t = ev.payload?.type;
        if (t === "over" || t === "enter") dragOver.value = true;
        else if (t === "drop") { dragOver.value = false; ingestPaths(ev.payload.paths ?? []); }
        else dragOver.value = false;
      });
    } catch { /* 原生拖拽不可用则仅浏览器路径生效 */ }
  }
});
onBeforeUnmount(() => {
  // 视图在拖动中被切走时，window 级监听不会随 DOM 自动移除。
  // 显式收尾，避免之后在别的页面移动鼠标仍触发已卸载星图的回调。
  window.removeEventListener("pointermove", onNodeMove);
  window.removeEventListener("pointerup", onNodeUp);
  window.removeEventListener("pointermove", onBgMove);
  window.removeEventListener("pointerup", onBgUp);
  draggingId = null;
  panStart = null;
  if (raf) cancelAnimationFrame(raf);
  raf = 0;
  alpha = 0;
  unlistenDrop?.();
});
</script>

<template>
  <div class="kb-root" :class="{ 'drag-over': dragOver }"
    @dragover="onDragOver" @dragleave="onDragLeave" @drop="onHtmlDrop">
    <div v-html="head"></div>

    <div v-if="dragOver" class="kb-dropveil">松手即入库 · 建议 Markdown / 文本（其它格式自动转 md）</div>
    <div v-if="uploading" class="callout">正在入库并重建知识网…</div>

    <!-- 资料文件清单 -->
    <template v-if="props.sub !== 'graph'">
      <section>
        <div class="card">
          <h3>资料文件（{{ files.length }} 份）</h3>
          <div style="margin:8px 0; display:flex; gap:8px; align-items:center; flex-wrap:wrap">
            <input v-model="filter" class="inp" placeholder="按路径 / 文件名筛选…" style="max-width:320px" />
            <button class="btn sm ghost" @click="refreshAll">刷新</button>
            <span style="color:var(--muted); font-size:var(--text-xs)">把文件拖到本页任意处即可入库</span>
          </div>
          <div class="tbl-wrap">
            <table>
              <tr><th>文件（相对 KB 根）</th><th style="width:80px">操作</th></tr>
              <tr v-if="!filesLoaded"><td colspan="2" style="color:var(--muted)">读取中…</td></tr>
              <tr v-else-if="!shownFiles.length">
                <td colspan="2"><p class="empty">{{ files.length ? "无匹配文件" : "知识库还是空的——拖入 Markdown / 文本文件即可在此看到清单" }}</p></td>
              </tr>
              <tr v-for="f in shownFiles" :key="f">
                <td><code :title="f">{{ f }}</code></td>
                <td><button class="btn sm ghost" @click="removeFile(f)">删除</button></td>
              </tr>
            </table>
          </div>
        </div>
      </section>
    </template>

    <!-- 星图（力导向内联渲染） -->
    <template v-else>
      <section>
        <div class="card">
          <div style="display:flex; justify-content:space-between; align-items:center; gap:8px; flex-wrap:wrap">
            <h3>知识库星图（{{ graph?.nodes.length ?? 0 }} 节点 · {{ graph?.edges.length ?? 0 }} 关联）</h3>
            <div style="display:flex; gap:6px">
              <button class="btn sm ghost" @click="resetView">复位</button>
              <button class="btn sm ghost" @click="refreshAll">刷新</button>
            </div>
          </div>
          <p v-if="!graphLoaded" style="color:var(--muted); margin-top:8px">构建中…</p>
          <p v-else-if="!graph?.nodes.length" class="empty" style="margin-top:8px">
            暂无星图——拖入资料并「构建知识网」后，节点与双链会显示在这里。
          </p>
          <template v-else>
            <p style="color:var(--muted); font-size:var(--text-xs); margin-top:4px">拖动节点重排 · 拖背景平移 · 滚轮缩放 · 悬停高亮邻居</p>
            <div class="kb-stage">
              <svg ref="svgEl" :viewBox="viewBox" @pointerdown="onBgDown" @wheel="onWheel">
                <line
                  v-for="(e, i) in edgeLines" :key="'e'+i"
                  :x1="e.a.x" :y1="e.a.y" :x2="e.b.x" :y2="e.b.y"
                  stroke="var(--line-2)" stroke-width="1"
                  :opacity="hoverId ? ((pnodes[e.s].id === hoverId || pnodes[e.t].id === hoverId) ? 0.85 : 0.12) : 0.5"
                />
                <g v-for="p in nodes" :key="p.id"
                  :opacity="dim(p) ? 0.25 : 1"
                  style="cursor:grab"
                  @pointerdown="onNodeDown($event, p)"
                  @pointerenter="hoverId = p.id" @pointerleave="hoverId === p.id && (hoverId = null)">
                  <circle
                    :cx="p.x" :cy="p.y" :r="nodeR(p)"
                    :fill="catColor.get(p.n.category)"
                    :stroke="p.id === hoverId ? 'var(--ink)' : (p.n.kind === 'root' ? 'var(--ink2)' : 'var(--panel)')"
                    :stroke-width="p.id === hoverId ? 2 : 1"
                  >
                    <title>{{ p.n.title }} · {{ p.n.category }}{{ p.n.summary ? ' — ' + p.n.summary : '' }}</title>
                  </circle>
                  <text v-if="labelVisible(p)"
                    :x="p.x" :y="p.y - nodeR(p) - 4"
                    text-anchor="middle" font-size="12" fill="var(--dim)"
                    style="pointer-events:none; user-select:none">{{ p.n.title }}</text>
                </g>
              </svg>
            </div>
            <div style="display:flex; gap:14px; flex-wrap:wrap; margin-top:10px">
              <span v-for="[cat, col] in catColor" :key="cat" class="sline">
                <span class="sdot" :style="{ background: col }"></span>{{ cat }}
              </span>
            </div>
          </template>
        </div>
      </section>
    </template>
  </div>
</template>

<style scoped>
.kb-root { position: relative; }
.kb-root.drag-over { outline: 2px dashed var(--accent); outline-offset: -6px; border-radius: var(--radius-card); }
.kb-dropveil {
  position: sticky; top: 0; z-index: 5; margin: 0 0 var(--space-s);
  padding: var(--space-s) var(--space-m); border: 1px dashed var(--accent);
  border-radius: var(--radius-ctl); background: var(--accent-soft); color: var(--accent-ink);
  font-size: var(--text-s); text-align: center;
}
.kb-stage { margin-top: 8px; border: 1px solid var(--line); border-radius: var(--radius-ctl); background: var(--wash); overflow: hidden; }
.kb-stage svg { width: 100%; height: min(62vh, 640px); display: block; touch-action: none; cursor: grab; }
.kb-stage svg:active { cursor: grabbing; }
</style>
