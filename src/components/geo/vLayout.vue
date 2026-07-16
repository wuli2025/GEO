<script setup lang="ts">
import { computed } from "vue";
import { vLayoutHtml } from "./render";
const props = defineProps<{ sub: string; platform: string }>();
const html = computed(() => vLayoutHtml(props.sub));
</script>
<template><div v-html="html"></div></template>
