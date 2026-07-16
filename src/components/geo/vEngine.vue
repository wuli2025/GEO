<script setup lang="ts">
import { computed } from "vue";
import { vEngineHtml } from "./render";
const props = defineProps<{ sub: string; platform: string }>();
const html = computed(() => vEngineHtml(props.sub));
</script>
<template><div v-html="html"></div></template>
