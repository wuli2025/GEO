<script setup lang="ts">
import { computed, ref } from "vue";
import { vBrainHtml, title } from "./render";
import { useEvolution } from "../../composables/useEvolution";
defineProps<{ sub?: string; platform: string }>();
// 大脑·进化数据抽在可替换 composable 里；已接真实后端（evolution.rs），空账本回落示例。
const { data, source, liveTimeline, liveVersions, refresh, addCard, addEntry, decideEntry, rollbackPrompt } = useEvolution();
const head = title("大脑 · 进化", "总控 / 循环工程");
// 子标签已撤：时间线 / 卡库 / 版本树 / 飞轮 / 双环一页到底。
const html = computed(() => vBrainHtml(data.value));

// ── 手写一张卡 ──
const showCardForm = ref(false);
const cardKind = ref("rule");
const cardTitle = ref("");
const cardScope = ref("全局");
const cardContent = ref("");
const busy = ref(false);
const msg = ref("");
async function submitCard() {
  if (!cardTitle.value.trim() || !cardContent.value.trim()) { msg.value = "标题与内容必填"; return; }
  busy.value = true; msg.value = "";
  try {
    await addCard(cardKind.value, cardTitle.value, cardContent.value, cardScope.value);
    showCardForm.value = false; cardTitle.value = ""; cardContent.value = "";
    msg.value = "卡已入库";
  } catch (e) { msg.value = String(e); } finally { busy.value = false; }
}

// ── 登记一次进化 ──
const showEvoForm = ref(false);
const evoKind = ref("prompt");
const evoTitle = ref("");
const evoDetail = ref("");
const evoExpect = ref("");
async function submitEvo() {
  if (!evoTitle.value.trim()) { msg.value = "进化标题必填"; return; }
  busy.value = true; msg.value = "";
  try {
    await addEntry(evoKind.value, evoTitle.value, { detail: evoDetail.value, expect: evoExpect.value });
    showEvoForm.value = false; evoTitle.value = ""; evoDetail.value = ""; evoExpect.value = "";
    msg.value = "已登记，进入观察期";
  } catch (e) { msg.value = String(e); } finally { busy.value = false; }
}

// ── 观察期裁决 / prompt 回滚（仅实时账本） ──
const observing = computed(() => liveTimeline.value.filter((e) => e.status === "观察中"));
const rollables = computed(() => liveVersions.value.filter((v) => v.status !== "active"));
async function decide(id: string, status: "已固化" | "已回滚") {
  busy.value = true; msg.value = "";
  try {
    await decideEntry(id, status);
    msg.value = status === "已固化" ? "已固化，关联卡功劳分 +1" : "已回滚，已自动沉淀 anti_pattern 卡";
  } catch (e) { msg.value = String(e); } finally { busy.value = false; }
}
async function rollback(id: string) {
  busy.value = true; msg.value = "";
  try {
    const v = await rollbackPrompt(id);
    msg.value = `已回滚到 ${v.expertId}·${v.anchor} v${v.version}（内容已置为 active，可在专家阵容里写回补丁）`;
  } catch (e) { msg.value = String(e); } finally { busy.value = false; }
}
</script>
<template>
  <div>
    <div v-html="head"></div>
    <div class="card" style="margin-bottom:12px">
      <h3>
        进化账本
        <span class="badge" :class="source === 'live' ? 'b-play' : 'b-ghost'">{{ source === "live" ? "实时账本" : "示例数据（账本为空）" }}</span>
      </h3>
      <div style="display:flex;gap:8px;flex-wrap:wrap;margin-top:6px">
        <button class="btn sm" @click="showCardForm = !showCardForm">＋ 手写一张卡</button>
        <button class="btn sm ghost" @click="showEvoForm = !showEvoForm">登记一次进化</button>
        <button class="btn sm ghost" :disabled="busy" @click="refresh()">刷新</button>
        <span v-if="msg" style="font-size:13px;color:var(--dim);align-self:center">{{ msg }}</span>
      </div>
      <div v-if="showCardForm" style="margin-top:10px;display:grid;gap:8px;max-width:640px">
        <div style="display:flex;gap:8px">
          <select v-model="cardKind" class="geo-input"><option value="anti_pattern">anti_pattern 教训</option><option value="rule">rule 规则</option><option value="playbook">playbook 打法</option></select>
          <input v-model="cardScope" class="geo-input" placeholder="范围（全局/平台/专家）" style="flex:1" />
        </div>
        <input v-model="cardTitle" class="geo-input" placeholder="卡标题" />
        <textarea v-model="cardContent" class="geo-input" rows="3" placeholder="内容：一条可直接教给主 Agent 的经验"></textarea>
        <div><button class="btn sm" :disabled="busy" @click="submitCard">入库</button></div>
      </div>
      <div v-if="showEvoForm" style="margin-top:10px;display:grid;gap:8px;max-width:640px">
        <div style="display:flex;gap:8px">
          <select v-model="evoKind" class="geo-input"><option value="prompt">Prompt</option><option value="skill">Skill</option><option value="expert">专家团</option><option value="schedule">调度</option></select>
          <input v-model="evoTitle" class="geo-input" placeholder="变更标题（改了什么）" style="flex:1" />
        </div>
        <input v-model="evoDetail" class="geo-input" placeholder="变更明细 / diff 摘要" />
        <input v-model="evoExpect" class="geo-input" placeholder="预期效果（观察期结束对照）" />
        <div><button class="btn sm" :disabled="busy" @click="submitEvo">登记（进入观察期）</button></div>
      </div>
      <div v-if="source === 'live' && observing.length" style="margin-top:10px">
        <h3 style="margin-bottom:6px">观察期裁决（{{ observing.length }} 项进行中）</h3>
        <div v-for="e in observing" :key="e.id" style="display:flex;gap:8px;align-items:center;margin:4px 0;font-size:14px">
          <span style="flex:1;color:var(--ink2)">{{ e.title }}<span v-if="e.expect" style="color:var(--muted)">（预期：{{ e.expect }}）</span></span>
          <button class="btn sm" :disabled="busy" @click="decide(e.id, '已固化')">固化</button>
          <button class="btn sm danger" :disabled="busy" @click="decide(e.id, '已回滚')">回滚</button>
        </div>
      </div>
      <div v-if="source === 'live' && rollables.length" style="margin-top:10px">
        <h3 style="margin-bottom:6px">历史版本一键回滚</h3>
        <div v-for="v in rollables" :key="v.id" style="display:flex;gap:8px;align-items:center;margin:4px 0;font-size:14px">
          <span style="flex:1;color:var(--ink2)">{{ v.expertId }}（{{ v.platform || "基础" }}·{{ v.anchor }}）v{{ v.version }} · {{ v.status === "rolled_back" ? "已回滚" : "已归档" }}</span>
          <button class="btn sm ghost" :disabled="busy" @click="rollback(v.id)">回滚到此版</button>
        </div>
      </div>
    </div>
    <div v-html="html"></div>
  </div>
</template>
<style scoped>
.geo-input {
  background: var(--code-bg, #0b0e1a);
  border: 1px solid var(--line, #2a3050);
  color: var(--ink, #1c2233);
  border-radius: 8px;
  padding: 6px 10px;
  font-size: 14px;
  font-family: inherit;
}
.geo-input:focus-visible { outline: 2px solid var(--focus, #8fa6ff); outline-offset: 1px; }
</style>
