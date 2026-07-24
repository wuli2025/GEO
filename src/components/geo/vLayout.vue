<script setup lang="ts">
import { computed } from "vue";
import { vLayoutHtml } from "./render";
defineProps<{ sub?: string; platform: string }>();
const html = computed(() => vLayoutHtml());
</script>
<template><div v-html="html"></div></template>
