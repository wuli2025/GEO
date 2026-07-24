<script setup lang="ts">
import { computed } from "vue";
import { vGateHtml } from "./render";
defineProps<{ sub?: string; platform: string }>();
const html = computed(() => vGateHtml());
</script>
<template><div v-html="html"></div></template>
