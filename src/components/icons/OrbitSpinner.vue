<template>
  <span class="orbit" :style="{ '--sz': size + 'px' }" aria-hidden="true">
    <span class="ring"></span>
  </span>
</template>

<script setup lang="ts">
// 简单的匀速加载环, 纯 CSS, 跟随 --primary 主题色。
withDefaults(defineProps<{ size?: number }>(), { size: 14 });
</script>

<style scoped>
.orbit {
  position: relative;
  display: inline-block;
  width: var(--sz);
  height: var(--sz);
  flex-shrink: 0;
}
.ring {
  position: absolute;
  inset: 0;
  border-radius: 50%;
  border: 2px solid var(--primary-soft);
  border-top-color: var(--primary);
  animation: orbit-turn 0.8s linear infinite;
}
@keyframes orbit-turn {
  to {
    transform: rotate(360deg);
  }
}
</style>
