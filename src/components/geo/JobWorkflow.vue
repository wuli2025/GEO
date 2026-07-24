<script setup lang="ts">
/**
 * 后端工作流的结构化排版：把 media_engine 的动作流画成一条能读的流程，
 * 而不是一段等宽字堆。工具调用是一颗药丸，阶段自述是一句话，脚本输出折起来。
 */
import { ref } from "vue";
import type { WfEvent, WfOutLine } from "./jobLog";

defineProps<{ events: WfEvent[] }>();

/** 脚本输出默认折着；报错的、短的（≤4 行）自动摊开。 */
const opened = ref<Record<number, boolean>>({});
function isOpen(i: number, bad: boolean, n: number): boolean {
  return i in opened.value ? opened.value[i] : bad || n <= 4;
}
function toggle(i: number, bad: boolean, n: number) {
  opened.value = { ...opened.value, [i]: !isOpen(i, bad, n) };
}

/** 折起来时给一行末尾预览。 */
function preview(l?: WfOutLine): string {
  if (!l) return "";
  return [l.label, l.detail].filter(Boolean).join(" · ") || l.raw;
}
</script>

<template>
  <div class="wf">
    <template v-for="(e, i) in events" :key="i">
      <!-- claude CLI 的一次工具调用 -->
      <div v-if="e.t === 'tool'" class="wf-r">
        <span class="wf-ts">{{ e.ts }}</span>
        <span class="wf-tool">{{ e.name }}<i v-if="e.n > 1">×{{ e.n }}</i></span>
      </div>

      <!-- 外挂脚本输出 -->
      <div v-else-if="e.t === 'out'" class="wf-r">
        <span class="wf-ts">{{ e.ts }}</span>
        <div class="wf-out" :class="{ bad: e.bad }">
          <button class="wf-out-h" @click="toggle(i, e.bad, e.lines.length)">
            {{ e.kind }}输出 {{ e.lines.length }} 行
            <i>{{ isOpen(i, e.bad, e.lines.length) ? "收起" : "展开" }}</i>
          </button>
          <div v-if="isOpen(i, e.bad, e.lines.length)" class="wf-out-b">
            <p v-for="(l, j) in e.lines" :key="j" class="wf-out-l" :class="{ bad: l.bad }">
              <template v-if="l.label || l.detail">
                <span v-if="l.label" class="wf-out-k">{{ l.label }}</span>{{ l.detail }}
              </template>
              <span v-else class="wf-raw">{{ l.raw }}</span>
            </p>
          </div>
          <p v-else class="wf-out-p">{{ preview(e.lines[e.lines.length - 1]) }}</p>
        </div>
      </div>

      <!-- 阶段自述：太长的（生图画面描述那种）先折三行，点开看全 -->
      <div v-else class="wf-r" :class="e.tone">
        <span class="wf-ts">{{ e.ts }}</span>
        <p
          class="wf-n"
          :class="{ clamp: e.text.length > 80 && !opened[i], long: e.text.length > 80 }"
          @click="e.text.length > 80 && (opened = { ...opened, [i]: !opened[i] })"
        >
          <span v-if="e.scope" class="wf-sc">{{ e.scope }}</span>{{ e.text }}
        </p>
      </div>
    </template>
  </div>
</template>

<style scoped>
.wf { display: flex; flex-direction: column; gap: 5px; padding: 2px 0 4px; }
.wf-r { display: grid; grid-template-columns: 36px minmax(0, 1fr); gap: 8px; align-items: baseline; }
.wf-ts {
  font-family: Consolas, ui-monospace, monospace;
  font-size: 11px;
  color: var(--muted);
  font-variant-numeric: tabular-nums;
  user-select: none;
}

/* 工具调用 */
.wf-tool {
  justify-self: start;
  display: inline-flex;
  align-items: baseline;
  gap: 5px;
  padding: 2px 9px;
  border-radius: var(--radius-pill, 999px);
  border: 1px solid rgba(255, 255, 255, .9);
  background: rgba(255, 255, 255, .72);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, .9);
  font-family: Consolas, ui-monospace, monospace;
  font-size: 11.5px;
  color: var(--dim);
}
.wf-tool i { font-style: normal; color: var(--muted); }

/* 阶段自述 */
.wf-n {
  margin: 0;
  font-size: var(--text-xs);
  line-height: 1.7;
  color: var(--ink2);
  white-space: pre-wrap;
  word-break: break-word;
}
.wf-n.long { cursor: pointer; }
.wf-n.clamp {
  display: -webkit-box;
  -webkit-line-clamp: 3;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
.wf-sc {
  display: inline-block;
  margin-right: 7px;
  color: var(--muted);
  font-size: var(--text-2xs);
}
.wf-r.ok .wf-n { color: var(--ink2); }
.wf-r.warn .wf-n { color: var(--tag-warn-ink, var(--warn)); }
.wf-r.bad .wf-n { color: var(--bad); }

/* 脚本输出 */
.wf-out { min-width: 0; }
.wf-out-h {
  border: none;
  background: transparent;
  padding: 0;
  font-family: inherit;
  font-size: var(--text-2xs);
  color: var(--muted);
  cursor: pointer;
}
.wf-out-h i { font-style: normal; margin-left: 6px; color: var(--dim); text-decoration: underline; text-underline-offset: 2px; }
.wf-out-h:hover { color: var(--ink2); }
.wf-out.bad .wf-out-h { color: var(--bad); }
.wf-out-b {
  margin: 4px 0 0;
  padding: 7px 10px;
  max-height: 220px;
  overflow: auto;
  border-radius: 9px;
  border: 1px solid rgba(255, 255, 255, .8);
  background: rgba(246, 248, 253, .68);
  box-shadow: inset 0 1px 3px rgba(20, 30, 62, .05);
}
.wf-out.bad .wf-out-b { border-color: rgba(208, 59, 59, .18); background: rgba(208, 59, 59, .05); }
.wf-out-l {
  margin: 0;
  font-size: var(--text-2xs);
  line-height: 1.75;
  color: var(--dim);
  word-break: break-word;
}
.wf-out-l.bad { color: var(--bad); }
.wf-out-k {
  display: inline-block;
  margin-right: 7px;
  font-family: Consolas, ui-monospace, monospace;
  color: var(--ink2);
}
.wf-out-l.bad .wf-out-k { color: var(--bad); }
.wf-raw {
  font-family: Consolas, ui-monospace, monospace;
  font-size: 11px;
  color: var(--muted);
  white-space: pre-wrap;
}
.wf-out-p {
  margin: 2px 0 0;
  font-size: var(--text-2xs);
  color: var(--muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
