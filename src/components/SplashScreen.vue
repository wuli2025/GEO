<script setup lang="ts">
import { onMounted, onBeforeUnmount, watch } from "vue";

const emit = defineEmits<{ (e: "done"): void }>();
// 父级（App）在应用外壳挂载就绪后把它置 true → 开屏「就绪即放行」，
// 不再无条件硬等固定时长。仍尊重下面的最短展示时间，避免一闪而过。
const props = defineProps<{ ready?: boolean }>();

// 开屏只是启动占位（真正的重活——扫盘/建库/PATH 预热——早已在后台线程，不被它挡）：
//   · 最短 MIN_MS 防止一闪而过；
//   · 一旦父级 ready 且过了最短时间即放行；
//   · CAP_MS 作上限兜底（ready 信号万一没来也不会卡）。
const MIN_MS = 400;
const CAP_MS = 1800;
const mountedAt = Date.now();

let capTimer: number | undefined;
let minTimer: number | undefined;
let finished = false;

function finish() {
  if (finished) return;
  finished = true;
  emit("done");
}

/** 就绪且已过最短展示时间才放行；否则等最短时间到点再判一次。 */
function maybeFinish() {
  if (finished) return;
  const elapsed = Date.now() - mountedAt;
  if (props.ready && elapsed >= MIN_MS) finish();
}

function onKey() {
  finish();
}

// 父级 ready 翻转时尝试放行（通常最短时间一到就走）。
watch(
  () => props.ready,
  () => maybeFinish()
);

onMounted(() => {
  // 最短时间到点后再判一次（此刻 ready 多半已 true → 立即走）。
  minTimer = window.setTimeout(maybeFinish, MIN_MS);
  // 上限兜底：ready 信号异常缺失也不至于卡在开屏。
  capTimer = window.setTimeout(finish, CAP_MS);
  window.addEventListener("keydown", onKey);
});

onBeforeUnmount(() => {
  if (capTimer) window.clearTimeout(capTimer);
  if (minTimer) window.clearTimeout(minTimer);
  window.removeEventListener("keydown", onKey);
});
</script>

<template>
  <div class="splash" @click="finish" title="点击进入">
    <div class="wordmark">北极星 · GEO</div>
  </div>
</template>

<style scoped>
.splash {
  position: fixed;
  inset: 0;
  z-index: 9999;
  background: #0c1320;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  user-select: none;
}
.wordmark {
  font-family: var(--serif);
  font-size: 15px;
  letter-spacing: 0.5em;
  text-indent: 0.5em;
  color: rgba(200, 212, 230, 0.85);
}
</style>
