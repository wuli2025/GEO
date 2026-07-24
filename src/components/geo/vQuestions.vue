<script setup lang="ts">
import { computed } from "vue";
import { vQuestionsHtml } from "./render";
defineProps<{ sub?: string; platform: string }>();
const html = computed(() => vQuestionsHtml());
</script>
<template><div v-html="html"></div></template>
