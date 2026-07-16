<script setup lang="ts">
import { vApprovalsHtml } from "./render";
defineProps<{ sub: string; platform: string }>();
</script>
<template><div v-html="vApprovalsHtml()"></div></template>
