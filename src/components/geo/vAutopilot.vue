<script setup lang="ts">
import { computed } from "vue";
import { vAutopilotHtml } from "./render";
const props = defineProps<{ sub: string; platform: string }>();
const html = computed(() => vAutopilotHtml(props.sub));
</script>
<template><div v-html="html"></div></template>
