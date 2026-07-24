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
import { dockOpen, setDock, toggleDock, navRequest, consumeNav } from "./geo/assistantBus";
import JobDetailDrawer from "./geo/JobDetailDrawer.vue";
import Assistant from "./geo/Assistant.vue";

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
// 现在只有「知识库」「设置」还有子标签，两者都短，无需折叠——留着这张表是为了
// 以后哪个视图再长出一排子页时有地方按。
const SUBTAB_PRIMARY: Record<string, number> = {};
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
const EXPERT_KEYS = ["experts", "brain", "promo", "kb", "questions", "engine", "gate", "layout"];
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

// ── 助手（右侧那块板，输入框长在它自己身上） ──
// 开合状态放在 assistantBus：顶栏那枚键与助手自己的收起键共用一份真源；默认展开。
// 选题投来规划请求时自动展开，好让规划卡有处可落。
watch(planRequest, (req) => { if (req) setDock(true); });
const VIEW_LABEL: Record<string, string> = (() => {
  const m: Record<string, string> = {};
  ZONES.forEach((z) => z.keys.forEach((k) => { m[k[0]] = k[2]; }));
  return m;
})();
// 助手只有一条，平台不再是泳道而是**上下文**：人站在哪个门户上，
// 这个平台就作为默认对象注进提示词；不在门户上则为 null（跨平台事务）。
const activePlatform = computed(() => (view.value === "portal" ? platform.value : null));

// 注入模型的「人此刻站在哪」——平台上下文见 activePlatform，这里只报界面位置。
const viewLabel = computed(() => {
  if (openJobId.value) return "流程详情";
  if (view.value === "portal") return `${P(platform.value)?.name ?? platform.value}门户`;
  return VIEW_LABEL[view.value] ?? view.value;
});
const viewCtx = computed(() => {
  const parts: string[] = [];
  if (currentSub.value) parts.push(`子标签：${currentSub.value}`);
  if (openJobId.value) parts.push(`打开着的流程 job：${openJobId.value}`);
  return parts.join("；");
});

// 命令坞解析出的导航指令（「打开账号矩阵」「切到头条」）→ 真正跳转在这里做。
watch(navRequest, (r) => {
  if (!r) return;
  consumeNav(r.id);
  go(r.view, r.platform);
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
  if (!rootEl.value || rootEl.value.offsetParent === null) return; // 不可见（被切走）则不响应
  // Ctrl/⌘+K：唤起助手。输入框已经长在助手身上了，助手收着时组件根本没挂载，
  // 这个快捷键只能由外壳来接。
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
    e.preventDefault();
    setDock(true);
    return;
  }
  if (tag === "INPUT" || tag === "TEXTAREA" || e.metaKey || e.ctrlKey || e.altKey) return;
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
          :class="{ active: dockOpen }"
          title="对话列表 · 助手的对话与生成记录（输入在底部那块玻璃）"
          @click="toggleDock"
        ><span class="ic" v-html="ico('chat')"></span>对话列表</button>
      </div>

      <!-- bar2：媒体门户切换器 + 健康条 -->
      <!-- 「媒体门户」这个 zlab 去掉了：每枚键自带平台徽标，一眼就知道这排是什么 -->
      <div class="bar2">
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

    <!-- 视图区 + 右侧泳道终端（玻璃板）+ 底部总控命令坞（常驻悬浮） -->
    <div class="geo-workarea" :class="{ docked: dockOpen }">
      <div class="geo-main" ref="main" @click="onDelegate" @mousemove="onMove" @mouseleave="hideTip">
        <component :is="currentComp" :sub="currentSub" :platform="platform" />
      </div>
      <Assistant
        v-if="dockOpen"
        :view-label="viewLabel"
        :view-ctx="viewCtx"
        :platform="activePlatform"
        @close="setDock(false)"
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

<style scoped>
/* 「对话列表」这枚键是右侧那块板的总开关，全顶栏用得最勤的一个 —— 别和旁边
   十几个功能键长得一样大。给它一圈边框、放大字号，并压低两像素与下一排对齐。 */
.geo-ops .chatkey {
  margin-left: auto;
  margin-top: 3px;
  padding: 9px 16px;
  gap: 8px;
  font-size: var(--text-m);
  border: 1px solid var(--line-2);
  border-radius: var(--radius-ctl);
}
.geo-ops .chatkey :deep(.i) { width: 17px; height: 17px; }
.geo-ops .chatkey.active { border-color: transparent; }

/* 泳道栏与命令坞浮在内容之上 → 工作区当它们的定位参照系。
   （这两条写在组件里而不是 geo.css：它们只服务于本组件模板里的这层布局） */
.geo-workarea { position: relative; }
/* 终端展开时给内容让位。只比玻璃板窄 8px：卡片的留白边缘从玻璃底下穿过去，
   backdrop-filter 才有东西可折射（玻璃有厚度），但正文一个字都不被压住。 */
.geo-workarea.docked .geo-main { padding-right: 396px; }
@media (max-width: 1180px) {
  .geo-workarea.docked .geo-main { padding-right: 346px; }
}
</style>
