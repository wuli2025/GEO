<script setup lang="ts">
/**
 * GEO 自媒体运营中心 · media_ops 视图的唯一渲染组件（旧的 MediaOps/MediaOpsCenter/MediaDashboard 已删）。
 * 本视图独占全窗：App.vue 的 .shell.geo-full 会把侧栏收成零宽，返回 Polaris 走 bar1 左侧的返回键。
 *
 * 严格按设计稿 v2：顶栏三排（bar1 三板块功能键 + 自建 SVG 图标；bar2 十平台门户切换器 + 健康条；
 * bar3 当前视图子标签）。深色控制台主题的全部 CSS 变量 scope 在 .geo-ops 下（geo/geo.css），
 * 不污染全局 app 样式。12 视图 + portal 门户视图为 geo/ 下子组件；能接真的接真后端。
 */
import { ref, computed, onMounted, onBeforeUnmount, useTemplateRef } from "vue";
import "./geo/geo.css";
import {
  PLATFORMS, ZONES, SUBTABS, KEYMAP, ico, sdot,
} from "./geo/data";
import { chartTip } from "./geo/charts";
import { toast } from "../composables/useToast";
import { useAppStore } from "../stores/app";
import { openJobId, openJobDetail, closeJobDetail } from "./geo/jobsBus";
import JobDetailDrawer from "./geo/JobDetailDrawer.vue";

import vDashboard from "./geo/vDashboard.vue";
import vApprovals from "./geo/vApprovals.vue";
import vAutopilot from "./geo/vAutopilot.vue";
import vBrain from "./geo/vBrain.vue";
import vAccounts from "./geo/vAccounts.vue";
import vExperts from "./geo/vExperts.vue";
import vKb from "./geo/vKb.vue";
import vQuestions from "./geo/vQuestions.vue";
import vEngine from "./geo/vEngine.vue";
import vGate from "./geo/vGate.vue";
import vLayout from "./geo/vLayout.vue";
import vApi from "./geo/vApi.vue";
import vPortal from "./geo/vPortal.vue";

// KeepAlive include 按组件 name 匹配 → 必须显式命名（切走再回来不丢状态）
defineOptions({ name: "GeoOpsCenter" });

const VIEW_COMPONENTS: Record<string, any> = {
  dashboard: vDashboard, approvals: vApprovals, autopilot: vAutopilot, brain: vBrain,
  accounts: vAccounts, experts: vExperts, kb: vKb, questions: vQuestions,
  engine: vEngine, gate: vGate, layout: vLayout, api: vApi, portal: vPortal,
};

// 本视图独占全窗（App.vue 的 .shell.geo-full 收起侧栏），Polaris 的其余功能
// 靠品牌区这个返回键回去 —— 设计稿没有侧栏，但老视图不能因此够不着。
const app = useAppStore();

// ── 状态 ──
const view = ref("dashboard");
const platform = ref("wechat");
const sub = ref<Record<string, string>>({});

const curSubKey = computed(() => (view.value === "portal" ? "portal" : view.value));
function curSub(v: string): string {
  return sub.value[v] || (SUBTABS[v] ? SUBTABS[v][0][0] : "");
}
const currentSub = computed(() => curSub(curSubKey.value));
const currentComp = computed(() => VIEW_COMPONENTS[view.value] || vDashboard);
const subtabs = computed(() => SUBTABS[curSubKey.value] || []);

function go(v: string, p?: string) {
  view.value = v;
  if (p !== undefined) platform.value = p;
}
function goSub(k: string) {
  sub.value = { ...sub.value, [curSubKey.value]: k };
}

// ── 顶栏派生 ──
const pendTotal = computed(() => PLATFORMS.reduce((s, p) => s + p.pending, 0));
const badCount = computed(() => PLATFORMS.filter((p) => p.login === "warn" || p.login === "bad").length);
const todoCount = computed(() => PLATFORMS.filter((p) => p.login === "none").length);
const healthInner = computed(() => {
  let h = `<span title="日健康度：探测线正常、门禁拦截 2 篇、配额未超发">${sdot("ok", "日健康度 3/4")}</span>`;
  h += `<span title="account-keeper 每周一 08:00 全平台体检；点进对应门户看明细">${badCount.value ? sdot("warn", badCount.value + " 个账号异常") : sdot("ok", "账号全绿")}</span>`;
  if (todoCount.value) h += `<span title="已排期接入，适配未完成">${sdot("idle", todoCount.value + " 个待接入")}</span>`;
  h += `<span title="token 日预算硬顶（超限走 L3 代码级拦截）">${sdot("ok", "token 6.2M / 8M")}</span>`;
  return h;
});
function chipTitle(p: (typeof PLATFORMS)[number]): string {
  const st = p.login === "ok" ? "登录态正常" : p.login === "none" ? "尚未接入" : "账号/网络异常 — " + p.loginNote;
  return `${p.name}：${st}`;
}

// ── 事件委托（v-html 内容的导航 / 演示 toast） ──
function onDelegate(e: MouseEvent) {
  const target = e.target as HTMLElement;
  // 任意 v-html 内容里带 data-job 的元素 → 打开该条流程的生成详情
  const jobEl = target.closest?.("[data-job]") as HTMLElement | null;
  if (jobEl && jobEl.dataset.job) {
    e.preventDefault();
    openJobDetail(jobEl.dataset.job);
    return;
  }
  const navEl = target.closest?.("[data-go],[data-gosub]") as HTMLElement | null;
  if (navEl) {
    e.preventDefault();
    const gv = navEl.dataset.go;
    const gs = navEl.dataset.gosub;
    if (gv) go(gv, navEl.dataset.portal);
    if (gs) goSub(gs);
    return;
  }
  const tEl = target.closest?.("[data-toast]") as HTMLElement | null;
  if (tEl && tEl.dataset.toast) toast.info("原型演示：" + tEl.dataset.toast);
}

// ── 图表悬停（十字准线 + tooltip） ──
const tipEl = useTemplateRef<HTMLDivElement>("tip");
const mainEl = useTemplateRef<HTMLDivElement>("main");
let lastCross: Element | null = null;
function showTip(e: MouseEvent, html: string) {
  const tip = tipEl.value;
  if (!tip) return;
  tip.innerHTML = html;
  tip.style.opacity = "1";
  const r = tip.getBoundingClientRect();
  let x = e.clientX + 14, y = e.clientY + 14;
  if (x + r.width > window.innerWidth - 8) x = e.clientX - r.width - 14;
  if (y + r.height > window.innerHeight - 8) y = e.clientY - r.height - 14;
  tip.style.left = x + "px";
  tip.style.top = y + "px";
}
function hideTip() {
  if (tipEl.value) tipEl.value.style.opacity = "0";
  if (lastCross) { (lastCross as HTMLElement).style.opacity = "0"; lastCross = null; }
}
function onMove(e: MouseEvent) {
  const cell = (e.target as HTMLElement).closest?.("[data-chart]") as HTMLElement | null;
  if (!cell) { hideTip(); return; }
  const t = chartTip(cell.dataset.chart || "", Number(cell.dataset.i));
  if (!t) { hideTip(); return; }
  showTip(e, t.html);
  const cross = mainEl.value?.querySelector("#" + CSS.escape(t.crossId));
  if (cross) {
    cross.setAttribute("x1", String(t.x));
    cross.setAttribute("x2", String(t.x));
    (cross as HTMLElement).style.opacity = "0.6";
    lastCross = cross;
  }
}

// ── 快捷键（仅本视图可见时生效，避免隐藏在 KeepAlive 里劫持全局按键） ──
const rootEl = useTemplateRef<HTMLDivElement>("root");
function onKey(e: KeyboardEvent) {
  const tag = (e.target as HTMLElement)?.tagName;
  if (tag === "INPUT" || tag === "TEXTAREA" || e.metaKey || e.ctrlKey || e.altKey) return;
  if (!rootEl.value || rootEl.value.offsetParent === null) return; // 不可见（被切走）则不响应
  const k = e.key.toUpperCase();
  if (KEYMAP[k]) { go(KEYMAP[k]); return; }
  if (/^[1-9]$/.test(k)) go("portal", PLATFORMS[+k - 1].id);
  if (k === "0" && PLATFORMS[9]) go("portal", PLATFORMS[9].id);
}
onMounted(() => window.addEventListener("keydown", onKey));
onBeforeUnmount(() => window.removeEventListener("keydown", onKey));
</script>

<template>
  <div class="geo-ops" ref="root">
    <div class="geo-header">
      <!-- bar1：三板块功能键 -->
      <div class="bar1">
        <button class="backkey" title="返回 Polaris（对话 / 图谱 / 工坊等）" @click="app.setView('chat')">
          <span class="ic" v-html="ico('back')"></span>
        </button>
        <div class="brand"><b>Polaris × GEO</b><small>自媒体运营中心</small></div>
        <div class="zone" v-for="z in ZONES" :key="z.label">
          <span class="zlab">{{ z.label }}</span>
          <button
            v-for="k in z.keys"
            :key="k[0]"
            class="fkey"
            :class="{ active: view === k[0] }"
            :title="`${k[2]} · 快捷键 ${k[3]}`"
            @click="go(k[0])"
          >
            <span class="ic" v-html="ico(k[1])"></span>{{ k[2] }}
            <span v-if="k[0] === 'approvals' && pendTotal" class="pip">{{ pendTotal }}</span>
          </button>
        </div>
      </div>

      <!-- bar2：媒体门户切换器 + 健康条 -->
      <div class="bar2">
        <span class="zlab">媒体门户</span>
        <button
          v-for="p in PLATFORMS"
          :key="p.id"
          class="pchip"
          :class="{ active: view === 'portal' && platform === p.id, ghost: p.login === 'none' }"
          :title="chipTitle(p)"
          @click="go('portal', p.id)"
        >
          {{ p.name }}<span v-if="p.pending" class="n">{{ p.pending }}</span>
        </button>
        <button class="pchip ghost" @click="toast.info('原型演示：新建平台项目 ≤3 步（选平台→绑 conv 项目→套文风宪法模板）')">＋ 新建平台</button>
        <div class="barhealth" v-html="healthInner"></div>
      </div>

      <!-- bar3：当前视图子标签 -->
      <div class="bar3" v-if="subtabs.length">
        <button
          v-for="s in subtabs"
          :key="s[0]"
          class="stab"
          :class="{ active: currentSub === s[0] }"
          @click="goSub(s[0])"
        >{{ s[1] }}</button>
      </div>
    </div>

    <!-- 视图区（事件委托 + 图表悬停） -->
    <div class="geo-main" ref="main" @click="onDelegate" @mousemove="onMove" @mouseleave="hideTip">
      <component :is="currentComp" :sub="currentSub" :platform="platform" />
    </div>

    <div class="geo-tip" ref="tip"></div>

    <!-- 流程详情抽屉（全局挂壳层：仪表盘/引擎/门户任意入口都能点开） -->
    <JobDetailDrawer
      v-if="openJobId"
      :job-id="openJobId"
      @close="closeJobDetail()"
      @rerun="(j) => openJobDetail(j.id)"
    />
  </div>
</template>
