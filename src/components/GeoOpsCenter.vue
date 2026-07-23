<script setup lang="ts">
/**
 * GEO 自媒体运营中心 —— 应用唯一界面（旧的 Polaris 通用外壳与 MediaOps 系组件均已删）。
 *
 * 严格按设计稿 v2：顶栏三排（bar1 三板块功能键 + 自建 SVG 图标；bar2 十平台门户切换器 + 健康条；
 * bar3 当前视图子标签）。深色控制台主题的全部 CSS 变量 scope 在 .geo-ops 下（geo/geo.css），
 * 不污染全局 app 样式。12 视图 + portal 门户视图为 geo/ 下子组件；能接真的接真后端。
 */
import { ref, computed, watch, onMounted, onBeforeUnmount, useTemplateRef } from "vue";
import "./geo/geo.css";
import {
  PLATFORMS, ZONES, SUBTABS, KEYMAP, ico, pico, P,
} from "./geo/data";
import { chartTip } from "./geo/charts";
import { openJobId, openJobDetail, closeJobDetail } from "./geo/jobsBus";
import { planRequest } from "./geo/planBus";
import JobDetailDrawer from "./geo/JobDetailDrawer.vue";
import GlobalChatDock from "./geo/GlobalChatDock.vue";

import vDashboard from "./geo/vDashboard.vue";
import vApprovals from "./geo/vApprovals.vue";
import vAutopilot from "./geo/vAutopilot.vue";
import vBrain from "./geo/vBrain.vue";
import vAccounts from "./geo/vAccounts.vue";
import vExperts from "./geo/vExperts.vue";
import vBrand from "./geo/vBrand.vue";
import vPromo from "./geo/vPromo.vue";
import vKb from "./geo/vKb.vue";
import vQuestions from "./geo/vQuestions.vue";
import vEngine from "./geo/vEngine.vue";
import vGate from "./geo/vGate.vue";
import vLayout from "./geo/vLayout.vue";
import vSettings from "./geo/vSettings.vue";
import vPortal from "./geo/vPortal.vue";

// KeepAlive include 按组件 name 匹配 → 必须显式命名（切走再回来不丢状态）
defineOptions({ name: "GeoOpsCenter" });

const VIEW_COMPONENTS: Record<string, any> = {
  dashboard: vDashboard, approvals: vApprovals, autopilot: vAutopilot, brain: vBrain,
  accounts: vAccounts, experts: vExperts, brand: vBrand, promo: vPromo, kb: vKb, questions: vQuestions,
  engine: vEngine, gate: vGate, layout: vLayout, settings: vSettings, portal: vPortal,
};

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

// ── bar3 子标签「更多」折叠：每视图保留前 N 个内联，其余收进下拉 ──
const SUBTAB_PRIMARY: Record<string, number> = { dashboard: 1, autopilot: 1 };
const primarySubs = computed(() => {
  const n = SUBTAB_PRIMARY[curSubKey.value];
  return n ? subtabs.value.slice(0, n) : subtabs.value;
});
const moreSubs = computed(() => {
  const n = SUBTAB_PRIMARY[curSubKey.value];
  return n ? subtabs.value.slice(n) : [];
});
const moreSubActive = computed(() => moreSubs.value.some((s) => s[0] === currentSub.value));

// ── bar1「专家模式」折叠：资源/系统两区除账号矩阵、设置外全部收进下拉 ──
const KEY_BY_ID: Record<string, [string, string, string, string]> = (() => {
  const m: Record<string, [string, string, string, string]> = {};
  ZONES.forEach((z) => z.keys.forEach((k) => { m[k[0]] = k; }));
  return m;
})();
const mainZone = ZONES[0]; // 总控
const EXPERT_KEYS = ["experts", "brand", "promo", "kb", "questions", "engine", "gate", "layout"];
const expertKeys = EXPERT_KEYS.map((id) => KEY_BY_ID[id]).filter(Boolean);
const expertActive = computed(() => EXPERT_KEYS.includes(view.value));
const accountsKey = KEY_BY_ID["accounts"];
const settingsKey = KEY_BY_ID["settings"];

// ── 下拉菜单开合（点选后 / 点外部关闭）──
const openMenu = ref<null | "expert" | "more">(null);
function toggleMenu(m: "expert" | "more") {
  openMenu.value = openMenu.value === m ? null : m;
}

function go(v: string, p?: string) {
  view.value = v;
  if (p !== undefined) platform.value = p;
  openMenu.value = null;
}
function goSub(k: string) {
  sub.value = { ...sub.value, [curSubKey.value]: k };
  openMenu.value = null;
}

// ── 全局 AI 对话坞（右侧常驻，可锚定当前泳道） ──
const chatOpen = ref(localStorage.getItem("geo.globalChat.open") !== "0");
function toggleChat() {
  chatOpen.value = !chatOpen.value;
  localStorage.setItem("geo.globalChat.open", chatOpen.value ? "1" : "0");
}
// 选题投来规划请求时，对话坞若收着就自动展开，好让规划卡有处可落。
watch(planRequest, (req) => { if (req) chatOpen.value = true; });
const VIEW_LABEL: Record<string, string> = (() => {
  const m: Record<string, string> = {};
  ZONES.forEach((z) => z.keys.forEach((k) => { m[k[0]] = k[2]; }));
  return m;
})();
// 一个媒体门户一条泳道，各自独立会话；非门户视图共用「总控」泳道。
const laneKey = computed(() =>
  view.value === "portal" ? `portal:${platform.value}` : "hub",
);
const anchorLabel = computed(() => {
  if (openJobId.value) return "流程详情";
  if (view.value === "portal") return `${P(platform.value)?.name ?? platform.value}门户`;
  return VIEW_LABEL[view.value] ?? view.value;
});
const anchorCtx = computed(() => {
  const parts = [`当前视图：${anchorLabel.value}`];
  if (view.value === "portal") parts.push(`门户平台：${P(platform.value)?.name ?? platform.value}（id=${platform.value}）`);
  if (currentSub.value) parts.push(`子标签：${currentSub.value}`);
  if (openJobId.value) parts.push(`打开着的流程 job：${openJobId.value}`);
  return parts.join("；");
});

// ── 顶栏派生 ──
const pendTotal = computed(() => PLATFORMS.reduce((s, p) => s + p.pending, 0));
function chipTitle(p: (typeof PLATFORMS)[number]): string {
  const st = p.login === "ok" ? "登录态正常" : p.login === "none" ? "尚未接入" : "账号/网络异常 — " + p.loginNote;
  return `${p.name}：${st}`;
}

// ── 事件委托（v-html 内容的导航 / job 详情） ──
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
// 点击下拉菜单之外 → 关闭
function onDocClick(e: MouseEvent) {
  if (!openMenu.value) return;
  if (!(e.target as HTMLElement)?.closest?.(".menu-wrap")) openMenu.value = null;
}
onMounted(() => {
  window.addEventListener("keydown", onKey);
  window.addEventListener("click", onDocClick);
});
onBeforeUnmount(() => {
  window.removeEventListener("keydown", onKey);
  window.removeEventListener("click", onDocClick);
});
</script>

<template>
  <div class="geo-ops" ref="root">
    <div class="geo-header">
      <!-- bar1：三板块功能键 -->
      <div class="bar1">
        <div class="brand"><b>Polaris × GEO</b><small>自媒体运营中心</small></div>
        <!-- 总控区 -->
        <div class="zone">
          <span class="zlab">{{ mainZone.label }}</span>
          <button
            v-for="k in mainZone.keys"
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
        <!-- 账号矩阵（常驻） + 专家模式（更多） + 设置 -->
        <div class="zone">
          <button
            class="fkey"
            :class="{ active: view === accountsKey[0] }"
            :title="`${accountsKey[2]} · 快捷键 ${accountsKey[3]}`"
            @click="go(accountsKey[0])"
          >
            <span class="ic" v-html="ico(accountsKey[1])"></span>{{ accountsKey[2] }}
          </button>
          <div class="menu-wrap">
            <button
              class="fkey"
              :class="{ active: expertActive || openMenu === 'expert' }"
              title="专家模式 · 更多功能"
              @click.stop="toggleMenu('expert')"
            >
              <span class="ic" v-html="ico('experts')"></span>专家模式
              <span class="caret" :class="{ up: openMenu === 'expert' }">▾</span>
            </button>
            <div v-if="openMenu === 'expert'" class="menu menu-r">
              <button
                v-for="k in expertKeys"
                :key="k[0]"
                class="mitem"
                :class="{ active: view === k[0] }"
                @click="go(k[0])"
              >
                <span class="ic" v-html="ico(k[1])"></span>{{ k[2] }}
                <span class="hk">{{ k[3] }}</span>
              </button>
            </div>
          </div>
          <button
            class="fkey"
            :class="{ active: view === settingsKey[0] }"
            :title="`${settingsKey[2]} · 快捷键 ${settingsKey[3]}`"
            @click="go(settingsKey[0])"
          >
            <span class="ic" v-html="ico(settingsKey[1])"></span>{{ settingsKey[2] }}
          </button>
        </div>
        <button
          class="fkey chatkey"
          :class="{ active: chatOpen }"
          title="运营助手 · 全局 AI 对话（锚定当前泳道）"
          @click="toggleChat"
        >💬 助手</button>
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
          <span class="pic" v-html="pico(p.id)"></span>{{ p.name }}<span v-if="p.pending" class="n">{{ p.pending }}</span>
        </button>
        <button class="pchip ghost" title="接入新平台：先到账号矩阵扫码建登录态" @click="go('accounts')">＋ 新建平台</button>
      </div>

      <!-- bar3：当前视图子标签 -->
      <div class="bar3" v-if="subtabs.length">
        <button
          v-for="s in primarySubs"
          :key="s[0]"
          class="stab"
          :class="{ active: currentSub === s[0] }"
          @click="goSub(s[0])"
        >{{ s[1] }}</button>
        <div v-if="moreSubs.length" class="menu-wrap">
          <button
            class="stab"
            :class="{ active: moreSubActive || openMenu === 'more' }"
            @click.stop="toggleMenu('more')"
          >更多<span class="caret" :class="{ up: openMenu === 'more' }">▾</span></button>
          <div v-if="openMenu === 'more'" class="menu">
            <button
              v-for="s in moreSubs"
              :key="s[0]"
              class="mitem"
              :class="{ active: currentSub === s[0] }"
              @click="goSub(s[0])"
            >{{ s[1] }}</button>
          </div>
        </div>
      </div>
    </div>

    <!-- 视图区 + 右侧全局对话坞 -->
    <div class="geo-workarea">
      <div class="geo-main" ref="main" @click="onDelegate" @mousemove="onMove" @mouseleave="hideTip">
        <component :is="currentComp" :sub="currentSub" :platform="platform" />
      </div>
      <GlobalChatDock
        v-if="chatOpen"
        :lane-key="laneKey"
        :anchor-label="anchorLabel"
        :anchor-ctx="anchorCtx"
        @close="toggleChat"
      />
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
