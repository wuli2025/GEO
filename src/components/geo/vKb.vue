<script setup lang="ts">
import { computed } from "vue";
import { vKbHtml } from "./render";
const props = defineProps<{ sub: string; platform: string }>();
const html = computed(() => vKbHtml(props.sub));
</script>
<template><div v-html="html"></div></template>
