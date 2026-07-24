<script setup lang="ts">
/**
 * 自动规划（一页到底，原子标签已撤）：定时任务表接真（mediaops 发文排期，默认每平台
 * 3 天/篇，大脑可经 apihub 调参）；其下依次是设计稿的 Policy / 分发回路 / 决策回路 /
 * 风险分级 / 触发式调配示例。
 */
import { ref, onMounted } from "vue";
import { vAutopilotHtml, cronDesignHtml, title } from "./render";
import { mediaOps, MEDIA_PLATFORMS, type MediaSchedule, type MediaPlatform } from "../../tauri";
import { toast } from "../../composables/useToast";

defineProps<{ sub?: string; platform: string }>();
const html = vAutopilotHtml();
const head = title("自动规划", "总控 / 自治调度");
const designHtml = cronDesignHtml();

const schedules = ref<MediaSchedule[]>([]);
const drafts = ref<Record<string, number>>({});
const ctxOpen = ref<string | null>(null);
const ctxDrafts = ref<Record<string, string>>({});
const busy = ref<string | null>(null);

function pname(pid: string): string {
  return MEDIA_PLATFORMS.find((p) => p.id === pid)?.name ?? pid;
}

async function load() {
  try {
    schedules.value = await mediaOps.scheduleList();
    for (const s of schedules.value) {
      drafts.value[s.platform] = s.intervalDays;
      ctxDrafts.value[s.platform] = s.context;
    }
  } catch (e: any) {
    toast.error(e?.message ?? String(e));
  }
}
onMounted(load);

function fmtAgo(ts: number | null): string {
  if (!ts) return "—";
  const diff = Math.floor(Date.now() / 1000) - ts;
  if (diff < 3600) return `${Math.max(1, Math.floor(diff / 60))} 分钟前`;
  if (diff < 86400) return `${Math.floor(diff / 3600)} 小时前`;
  return `${Math.floor(diff / 86400)} 天前`;
}
function nextDue(s: MediaSchedule): { txt: string; due: boolean } {
  if (!s.enabled) return { txt: "已停用", due: false };
  if (!s.lastFiredAt) return { txt: "下轮巡检", due: true };
  const left = s.lastFiredAt + s.intervalDays * 86400 - Math.floor(Date.now() / 1000);
  if (left <= 0) return { txt: "已到期 · 下轮巡检入队", due: true };
  const d = Math.floor(left / 86400), h = Math.floor((left % 86400) / 3600);
  return { txt: d > 0 ? `${d} 天 ${h} 小时后` : h > 0 ? `${h} 小时后` : `${Math.max(1, Math.floor(left / 60))} 分钟后`, due: false };
}
const srcLabel: Record<string, string> = { seed: "默认", human: "人工", brain: "大脑" };

async function save(s: MediaSchedule) {
  const days = drafts.value[s.platform];
  if (!days || days < 1 || days > 60) { toast.error("周期须在 1–60 天之间"); return; }
  busy.value = s.platform;
  try {
    await mediaOps.scheduleSet(s.platform as MediaPlatform, { intervalDays: days, source: "human" });
    toast.info(`${pname(s.platform)}：周期已调为每 ${days} 天一篇（已留进化卡）`);
    await load();
  } catch (e: any) {
    toast.error(e?.message ?? String(e));
  } finally {
    busy.value = null;
  }
}
async function toggle(s: MediaSchedule) {
  busy.value = s.platform;
  try {
    await mediaOps.scheduleSet(s.platform as MediaPlatform, { enabled: !s.enabled, source: "human" });
    await load();
  } catch (e: any) {
    toast.error(e?.message ?? String(e));
  } finally {
    busy.value = null;
  }
}
function toggleCtx(s: MediaSchedule) {
  ctxOpen.value = ctxOpen.value === s.platform ? null : s.platform;
  if (ctxOpen.value) ctxDrafts.value[s.platform] = s.context;
}
async function saveCtx(s: MediaSchedule) {
  busy.value = s.platform;
  try {
    await mediaOps.scheduleSet(s.platform as MediaPlatform, { context: ctxDrafts.value[s.platform] ?? "", source: "human" });
    toast.info(`${pname(s.platform)}：派发上下文已更新（已留进化卡）`);
    ctxOpen.value = null;
    await load();
  } catch (e: any) {
    toast.error(e?.message ?? String(e));
  } finally {
    busy.value = null;
  }
}
async function tickNow() {
  busy.value = "__tick";
  try {
    const fired = await mediaOps.scheduleTick();
    toast.info(fired.length
      ? `本轮入队 ${fired.length} 条例行任务：${fired.map((f) => pname(f.platform)).join("、")}`
      : "本轮无到期平台（有未完成任务的平台会跳过）");
    await load();
  } catch (e: any) {
    toast.error(e?.message ?? String(e));
  } finally {
    busy.value = null;
  }
}
</script>

<template>
  <div>
    <div v-html="head"></div>
    <div class="callout y">
      <b>真实排期已接通</b>：到期只向「规划队列」塞一条例行任务（该平台还有未完成任务则本轮跳过，防堆积），
      后续照常走流水线与人工审批——<b>绝不自动发布</b>。每行「上下文」可编辑该平台的<b>派发上下文</b>，
      到期入队时附在任务上，主 agent 领任务即按此执行。大脑（autopilot）经 apihub 用同一接口调参
      （<code>source=brain</code>）；周期/启停/上下文的每次实际变更都会自动在「大脑·进化 / 时间线」留一张调度卡。
    </div>
    <section>
      <div class="card">
        <h3>各平台发文排期（默认每 3 天一篇 · 后端每 30 分钟巡检）</h3>
        <div class="tbl-wrap">
          <table>
            <tr>
              <th>平台</th><th>排期</th><th>周期（天/篇）</th><th>上次触发</th><th>下次到期</th><th>最近调参</th><th>操作</th>
            </tr>
            <tr v-if="!schedules.length">
              <td colspan="7" style="color: var(--muted)">读取排期中…（若持续为空，后端命令不可用）</td>
            </tr>
            <template v-for="s in schedules" :key="s.platform">
              <tr>
                <td><b>{{ pname(s.platform) }}</b></td>
                <td>
                  <button class="btn sm" :class="{ ghost: !s.enabled }" :disabled="busy === s.platform" @click="toggle(s)">
                    {{ s.enabled ? "已启用" : "已停用" }}
                  </button>
                </td>
                <td class="num-cell">
                  <input v-model.number="drafts[s.platform]" type="number" min="1" max="60" class="inp" style="width: 64px; padding: 4px 8px" />
                </td>
                <td>{{ fmtAgo(s.lastFiredAt) }}</td>
                <td :style="nextDue(s).due ? 'color: var(--warn, #b58900)' : ''">{{ nextDue(s).txt }}</td>
                <td><span class="badge" :class="s.source === 'brain' ? 'b-full' : 'b-ghost'">{{ srcLabel[s.source] ?? s.source }}</span></td>
                <td style="white-space: nowrap">
                  <button class="btn sm" :disabled="busy === s.platform || drafts[s.platform] === s.intervalDays" @click="save(s)">保存周期</button>
                  <button class="btn sm ghost" style="margin-left: 6px" @click="toggleCtx(s)">{{ ctxOpen === s.platform ? "收起上下文" : "上下文" }}</button>
                </td>
              </tr>
              <tr v-if="ctxOpen === s.platform">
                <td colspan="7" style="background: var(--bg2, transparent)">
                  <div style="display: flex; flex-direction: column; gap: 6px; padding: 4px 0">
                    <div style="color: var(--muted); font-size: var(--text-2xs)">
                      派发上下文——到期入队时附在任务上，主 agent 领任务即按此执行（大脑调参也会改这里，改动自动留进化卡）：
                    </div>
                    <textarea v-model="ctxDrafts[s.platform]" class="inp" rows="4" style="width: 100%; resize: vertical; font-size: var(--text-xs)"></textarea>
                    <div style="display: flex; gap: 8px">
                      <button class="btn sm" :disabled="busy === s.platform || ctxDrafts[s.platform] === s.context" @click="saveCtx(s)">保存上下文</button>
                      <button class="btn sm ghost" @click="ctxOpen = null">取消</button>
                    </div>
                  </div>
                </td>
              </tr>
            </template>
          </table>
        </div>
        <div style="margin-top: var(--space-xs); display: flex; gap: 8px; flex-wrap: wrap">
          <button class="btn sm" :disabled="busy === '__tick'" @click="tickNow">立即巡检</button>
          <button class="btn sm ghost" @click="load">刷新</button>
        </div>
        <p class="foot">巡检到期 → 队列出现「【例行】某平台：选题待定（每 N 天一篇）」任务；平台在「平台设置」里被停用时同样跳过。调控历史见「大脑·进化 / 进化时间线」的调度类卡片。</p>
      </div>
    </section>
    <div v-html="designHtml"></div>
    <div v-html="html"></div>
  </div>
</template>
