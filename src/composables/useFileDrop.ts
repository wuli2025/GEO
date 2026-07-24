/**
 * 拖入上传 —— 全应用统一的那一条路。
 *
 * 桌面（Tauri）与浏览器（Docker/Web）在拖放这件事上**根本不是同一套机制**，
 * 各处各写一遍必然走样，所以收进这一个组合式：
 *
 *  - **桌面**：HTML5 的 `drop` 事件**收不到文件**——webview 把原生拖放整个截走了，
 *    只有 `webview.onDragDropEvent` 能拿到磁盘绝对路径。而那个事件是**整窗口**的、
 *    不认 DOM 元素，所以这里维护一张拖拽区注册表：谁挂了拖拽区就登记自己的元素，
 *    事件来了按指针坐标命中**最后注册（视觉最上层）**的那个区。
 *    ——「拖进去没反应」这类问题九成出在这里：桌面端别指望 `@drop` 里的 dataTransfer。
 *  - **浏览器**：走标准 HTML5，文件先 POST 给后端换成服务端路径。
 *
 * 两条路的出口统一成 `{ paths, files }`：
 *  - `paths` 有值 → 后端能直接按路径读（`brand_doc_import` / `chat_attach_files` …）；
 *  - 只有 `files` → 纯前端预览态（没有后端可上传），调用方自己 `f.text()` 兜底。
 */
import { ref, watch, onMounted, onBeforeUnmount, type Ref } from "vue";
import { isTauri, uploadToBackend } from "../tauri";

export interface DropPayload {
  /** 后端可直接读的绝对路径（桌面=原始路径；Docker=上传后的服务端路径） */
  paths: string[];
  /** 浏览器拿到的文件对象（桌面端恒为空） */
  files: File[];
}

interface Zone {
  el: Ref<HTMLElement | null>;
  onDrop: (p: DropPayload) => void | Promise<void>;
  disabled?: Ref<boolean>;
  over: Ref<boolean>;
  busy: Ref<boolean>;
}

/** 注册顺序即层级：后注册的（浮层、抽屉、后弹出的面板）先命中。 */
const zones: Zone[] = [];
let nativeStarted = false;

function clearOver() {
  for (const z of zones) z.over.value = false;
}

/** 指针坐标（物理像素）落在哪个拖拽区上。 */
function hit(pos: { x: number; y: number } | undefined): Zone | null {
  if (!pos) return null;
  const r = window.devicePixelRatio || 1;
  const x = pos.x / r;
  const y = pos.y / r;
  for (let i = zones.length - 1; i >= 0; i--) {
    const z = zones[i];
    if (z.disabled?.value || z.busy.value) continue;
    const el = z.el.value;
    if (!el) continue;
    const b = el.getBoundingClientRect();
    if (x >= b.left && x <= b.right && y >= b.top && y <= b.bottom) return z;
  }
  return null;
}

/** 整窗口只挂一个原生监听，进程内长驻——拖拽区来来去去，监听不必跟着拆装。 */
async function startNative() {
  if (nativeStarted || !isTauri) return;
  nativeStarted = true;
  try {
    const { getCurrentWebview } = await import("@tauri-apps/api/webview");
    await getCurrentWebview().onDragDropEvent((ev: any) => {
      const t = ev.payload?.type;
      if (t === "drop") {
        clearOver();
        const z = hit(ev.payload?.position);
        if (z) deliver(z, [], ev.payload?.paths ?? []);
      } else if (t === "over" || t === "enter") {
        const z = hit(ev.payload?.position);
        clearOver();
        if (z) z.over.value = true;
      } else {
        clearOver();
      }
    });
  } catch {
    // 拿不到 webview API（老版本/非桌面）：HTML5 那条路仍在，不影响浏览器形态。
    nativeStarted = false;
  }
}

async function deliver(z: Zone, files: File[], paths: string[]) {
  if (z.disabled?.value || z.busy.value) return;
  if (!files.length && !paths.length) return;
  z.busy.value = true;
  try {
    let p = paths;
    // 浏览器：文件本体先落到后端，换成服务端路径，后面的命令与桌面走同一条。
    if (!p.length && files.length && !isTauri) {
      try {
        p = (await uploadToBackend(files)).map((u) => u.path).filter(Boolean);
      } catch {
        p = []; // 纯前端预览没有后端可传 → 交给调用方按 files 兜底
      }
    }
    await z.onDrop({ paths: p, files });
  } finally {
    z.busy.value = false;
  }
}

/**
 * 把一个元素变成拖拽区。
 *
 * @param el      拖拽区元素（`ref="xxx"` 绑上即可，无需在模板里写 @dragover/@drop）
 * @param onDrop  收到文件时调用；期间 `busy` 为 true，重复投放会被忽略
 */
export function useFileDrop(
  el: Ref<HTMLElement | null>,
  onDrop: (p: DropPayload) => void | Promise<void>,
  opts: { disabled?: Ref<boolean> } = {}
): { over: Ref<boolean>; busy: Ref<boolean> } {
  const over = ref(false);
  const busy = ref(false);
  const zone: Zone = { el, onDrop, disabled: opts.disabled, over, busy };

  // 浏览器形态才绑 HTML5：桌面端这些事件要么不来，要么来了也没有文件。
  const onOver = (e: DragEvent) => {
    e.preventDefault();
    if (!zone.disabled?.value) over.value = true;
  };
  const onLeave = () => { over.value = false; };
  const onDropEv = (e: DragEvent) => {
    e.preventDefault();
    over.value = false;
    const fl = e.dataTransfer?.files;
    if (fl?.length) deliver(zone, Array.from(fl), []);
  };

  function bind(node: HTMLElement | null, off: HTMLElement | null) {
    if (off) {
      off.removeEventListener("dragover", onOver);
      off.removeEventListener("dragleave", onLeave);
      off.removeEventListener("drop", onDropEv);
    }
    if (node) {
      node.addEventListener("dragover", onOver);
      node.addEventListener("dragleave", onLeave);
      node.addEventListener("drop", onDropEv);
    }
  }

  let stopWatch: (() => void) | null = null;

  onMounted(() => {
    zones.push(zone);
    startNative();
    if (!isTauri) stopWatch = watch(el, (node, old) => bind(node, old ?? null), { immediate: true });
  });

  onBeforeUnmount(() => {
    const i = zones.indexOf(zone);
    if (i >= 0) zones.splice(i, 1);
    stopWatch?.();
    if (!isTauri) bind(null, el.value);
  });

  return { over, busy };
}
