<script setup lang="ts">
import { computed } from "vue";
import { vGateHtml } from "./render";
const props = defineProps<{ sub: string; platform: string }>();
const html = computed(() => vGateHtml(props.sub));
</script>
<template><div v-html="html"></div></template>
