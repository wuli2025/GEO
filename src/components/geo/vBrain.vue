<script setup lang="ts">
import { computed } from "vue";
import { vBrainHtml } from "./render";
import { useEvolution } from "../../composables/useEvolution";
const props = defineProps<{ sub: string; platform: string }>();
// 大脑·进化数据抽在可替换 composable 里（现为设计稿 mock，接真时零改视图）。
const { data } = useEvolution();
const html = computed(() => vBrainHtml(props.sub, data.value));
</script>
<template><div v-html="html"></div></template>
