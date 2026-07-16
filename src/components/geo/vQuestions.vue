<script setup lang="ts">
import { computed } from "vue";
import { vQuestionsHtml } from "./render";
const props = defineProps<{ sub: string; platform: string }>();
const html = computed(() => vQuestionsHtml(props.sub));
</script>
<template><div v-html="html"></div></template>
