<script setup lang="ts">
import { ref, computed, onMounted } from "vue";
import {
  LayoutDashboard,
  RefreshCw,
  Loader,
  FileText,
  Layers,
  Activity,
  DollarSign,
  Database,
  Radar,
  KeyRound,
  Zap,
  Check,
} from "@lucide/vue";
import { useAppStore } from "../stores/app";
import {
  MEDIA_PLATFORMS,
  mediaOps,
  mediaAccounts,
  ark,
  provider,
  type MediaKpi,
  type MediaAccountStatus,
  type MediaQueueItem,
  type MediaPlatformSettings,
  type MediaPlatform,
  type ArkConfig,
  type UsageSummary,
} from "../tauri";

// KeepAlive 按 name 匹配 → 显式命名，切走再回来不丢已加载数据
defineOptions({ name: "MediaDashboard" });

const app = useAppStore();

// 全局刷新态（各卡独立 try/catch，互不拖累）
const loading = ref(false);

// ───────── 数据源（每张卡一份，各自 err 标志） ─────────
type Summary = { d7: MediaKpi; d30: MediaKpi; perPlatform: Record<string, MediaKpi> };
const summary = ref<Summary | null>(null);
const summaryErr = ref(false);

type OpsState = {
  queue: MediaQueueItem[];
  settings: MediaPlatformSettings[];
};
const opsState = ref<OpsState | null>(null);
const stateErr = ref(false);

const accounts = ref<MediaAccountStatus[]>([]);
const accountsErr = ref(false);

const usage = ref<UsageSummary | null>(null);
const usageErr = ref(false);

const arkCfg = ref<ArkConfig | null>(null);
const arkCfgErr = ref(false);
const arkProbe = ref<{ ok: boolean; latencyMs: number; message: string } | null>(null);
const arkProbing = ref(false);

// ───────── 各卡独立加载器（绝不因某个后端命令未就绪拖垮其它卡） ─────────
async function loadMetrics() {
  summaryErr.value = false;
  try {
    summary.value = await mediaOps.metricsSummary();
  } catch {
    summaryErr.value = true;
  }
}
async function loadState() {
  stateErr.value = false;
  try {
    const s = await mediaOps.state();
    opsState.value = { queue: s.queue ?? [], settings: s.settings ?? [] };
  } catch {
    stateErr.value = true;
  }
}
async function loadAccounts() {
  accountsErr.value = false;
  try {
    accounts.value = await mediaAccounts.status();
  } catch {
    accountsErr.value = true;
  }
}
async function loadUsage() {
  usageErr.value = false;
  try {
    usage.value = await provider.usage();
  } catch {
    usageErr.value = true;
  }
}
async function loadArkCfg() {
  arkCfgErr.value = false;
  try {
    arkCfg.value = await ark.configGet();
  } catch {
    arkCfgErr.value = true;
  }
}
async function probeArk() {
  if (arkProbing.value) return;
  arkProbing.value = true;
  try {
    arkProbe.value = await ark.test();
  } catch (e: any) {
    arkProbe.value = { ok: false, latencyMs: 0, message: e?.message ?? "探活失败" };
  } finally {
    arkProbing.value = false;
  }
}

async function refreshAll() {
  loading.value = true;
  await Promise.allSettled([
    loadMetrics(),
    loadState(),
    loadAccounts(),
    loadUsage(),
    loadArkCfg(),
    probeArk(),
  ]);
  loading.value = false;
}

onMounted(refreshAll);

// ───────── 格式化 ─────────
function fmtInt(n: number): string {
  return Math.round(n).toLocaleString("en-US");
}
function fmtK(n: number): string {
  if (n >= 1e9) return (n / 1e9).toFixed(1) + "B";
  if (n >= 1e6) return (n / 1e6).toFixed(1) + "M";
  if (n >= 1e3) return (n / 1e3).toFixed(1) + "k";
  return String(Math.round(n));
}
function fmtCost(n: number): string {
  return "$" + (n || 0).toFixed(2);
}
function fmtPct(rate: number, runs: number): string {
  if (!runs) return "未采集";
  const p = rate <= 1 ? rate * 100 : rate;
  return p.toFixed(1) + "%";
}

// ───────── KPI 计算 ─────────
const d7 = computed<MediaKpi | undefined>(() => summary.value?.d7);
const d30 = computed<MediaKpi | undefined>(() => summary.value?.d30);

// token / 成本：以自媒体埋点为主，可用时叠加真实 LLM 用量（近 30 天）
const realTokens = computed<number | null>(() =>
  usage.value?.available ? usage.value.month.total : null
);
const realCost = computed<number | null>(() =>
  usage.value?.available ? usage.value.month.cost : null
);

const tokenMain = computed<string>(() => {
  const m = d30.value?.tokens ?? 0;
  if (m > 0) return fmtInt(m);
  if (realTokens.value != null && realTokens.value > 0) return fmtK(realTokens.value);
  return summary.value ? "未采集" : "0";
});
const costMain = computed<string>(() => {
  const m = d30.value?.cost ?? 0;
  if (m > 0) return fmtCost(m);
  if (realCost.value != null && realCost.value > 0) return fmtCost(realCost.value);
  return summary.value ? "未采集" : "$0.00";
});

// ───────── 平台×指标热力表 ─────────
const platformRows = computed(() =>
  MEDIA_PLATFORMS.map((p) => {
    const k = summary.value?.perPlatform?.[p.id];
    const s = opsState.value?.settings?.find((x) => x.platform === p.id);
    const inQueue =
      opsState.value?.queue?.filter(
        (q) => q.platform === p.id && (q.status === "queued" || q.status === "running")
      ).length ?? 0;
    return {
      id: p.id as MediaPlatform,
      name: p.name,
      enabled: s?.enabled ?? false,
      hasSettings: !!s,
      weeklyQuota: s?.weeklyQuota ?? 0,
      inQueue,
      drafts: k?.drafts ?? 0,
      published: k?.published ?? 0,
      failed: k?.failed ?? 0,
      cost: k?.cost ?? 0,
    };
  })
);
const maxDraft = computed(() => Math.max(1, ...platformRows.value.map((r) => r.drafts)));
const maxPub = computed(() => Math.max(1, ...platformRows.value.map((r) => r.published)));
const maxFail = computed(() => Math.max(1, ...platformRows.value.map((r) => r.failed)));
const maxCost = computed(() => Math.max(0.01, ...platformRows.value.map((r) => r.cost)));

// 热力底色：数值越大底色越浓；0 无底色
function heat(v: number, max: number, rgb: string): Record<string, string> {
  if (!v || v <= 0) return {};
  const a = 0.1 + 0.42 * Math.min(1, v / max);
  return { background: `rgba(${rgb},${a.toFixed(3)})` };
}
const RGB_DRAFT = "111,176,255";
const RGB_PUB = "57,208,154";
const RGB_FAIL = "233,90,90";
const RGB_COST = "230,184,115";

// ───────── 账号健康度 ─────────
const SEVEN_DAYS = 7 * 86400;
function acctFor(id: MediaPlatform): MediaAccountStatus | undefined {
  return accounts.value.find((a) => a.platform === id);
}
type Health = "green" | "yellow" | "gray";
function healthOf(id: MediaPlatform): Health {
  const a = acctFor(id);
  if (!a || !a.bound) return "gray";
  if (a.lastActive && Date.now() / 1000 - a.lastActive > SEVEN_DAYS) return "yellow";
  return "green";
}
function healthLabel(h: Health): string {
  return h === "green" ? "活跃" : h === "yellow" ? "超 7 天未活动" : "未绑定";
}
function gotoOps() {
  app.setView("media_ops");
}

// ───────── 队列概览 ─────────
const recentQueue = computed<MediaQueueItem[]>(() =>
  [...(opsState.value?.queue ?? [])].sort((a, b) => b.updatedAt - a.updatedAt).slice(0, 12)
);
const QUEUE_META: Record<string, { label: string; cls: string }> = {
  queued: { label: "排队中", cls: "q-blue" },
  running: { label: "运行中", cls: "q-amber" },
  draft_uploaded: { label: "已存草稿", cls: "q-green" },
  done: { label: "已完成", cls: "q-teal" },
  failed: { label: "失败", cls: "q-red" },
};
function qMeta(s: string) {
  return QUEUE_META[s] ?? { label: s, cls: "q-gray" };
}
function platName(id: MediaPlatform): string {
  return MEDIA_PLATFORMS.find((m) => m.id === id)?.name ?? id;
}
</script>

<template>
  <div class="md">
    <!-- 顶栏 -->
    <header class="md-head">
      <LayoutDashboard :size="20" :stroke-width="1.7" class="md-icon" />
      <h1 class="md-title">数据看板</h1>
      <span class="md-sub">自媒体运营驾驶舱 · 全链路度量一屏总览</span>
      <button class="md-refresh" :disabled="loading" @click="refreshAll">
        <Loader v-if="loading" :size="14" class="spin" /><RefreshCw v-else :size="14" />
        <span>刷新</span>
      </button>
    </header>

    <div class="md-body">
      <!-- 1 · KPI 卡带 -->
      <section class="md-kpis">
        <div class="md-kpi">
          <div class="k-head"><span class="k-lab">近 7 天草稿</span><span class="k-ic blue"><FileText :size="15" /></span></div>
          <div class="k-num" :class="{ dim: summaryErr }">{{ summaryErr ? "加载失败" : fmtInt(d7?.drafts ?? 0) }}</div>
          <div class="k-sub" v-if="!summaryErr">发布 {{ d7?.published ?? 0 }} · 失败 {{ d7?.failed ?? 0 }}</div>
        </div>

        <div class="md-kpi">
          <div class="k-head"><span class="k-lab">近 30 天草稿</span><span class="k-ic purple"><Layers :size="15" /></span></div>
          <div class="k-num" :class="{ dim: summaryErr }">{{ summaryErr ? "加载失败" : fmtInt(d30?.drafts ?? 0) }}</div>
          <div class="k-sub" v-if="!summaryErr">运行 {{ d30?.runs ?? 0 }} 次 · 发布 {{ d30?.published ?? 0 }}</div>
        </div>

        <div class="md-kpi">
          <div class="k-head"><span class="k-lab">Run 成功率<span class="k-tag">30天</span></span><span class="k-ic green"><Activity :size="15" /></span></div>
          <div class="k-num" :class="{ dim: summaryErr }">{{ summaryErr ? "加载失败" : fmtPct(d30?.successRate ?? 0, d30?.runs ?? 0) }}</div>
          <div class="k-sub" v-if="!summaryErr">失败 {{ d30?.failed ?? 0 }} / 运行 {{ d30?.runs ?? 0 }}</div>
        </div>

        <div class="md-kpi">
          <div class="k-head"><span class="k-lab">Token 用量<span class="k-tag">30天</span></span><span class="k-ic amber"><Database :size="15" /></span></div>
          <div class="k-num" :class="{ dim: summaryErr }">{{ summaryErr ? "加载失败" : tokenMain }}</div>
          <div class="k-sub" v-if="!summaryErr">
            <template v-if="realTokens != null">LLM 实测 {{ fmtK(realTokens) }}</template>
            <template v-else-if="usageErr">LLM 用量加载失败</template>
            <template v-else>暂无 LLM 实测</template>
          </div>
        </div>

        <div class="md-kpi">
          <div class="k-head"><span class="k-lab">成本<span class="k-tag">30天</span></span><span class="k-ic red"><DollarSign :size="15" /></span></div>
          <div class="k-num" :class="{ dim: summaryErr }">{{ summaryErr ? "加载失败" : costMain }}</div>
          <div class="k-sub" v-if="!summaryErr">
            <template v-if="realCost != null">LLM 实测 {{ fmtCost(realCost) }}</template>
            <template v-else>估算 · 未含 LLM 实测</template>
          </div>
        </div>
      </section>

      <!-- 2 · 平台×指标热力表 -->
      <section class="md-card">
        <div class="md-card-h">
          <Radar :size="15" /><span>平台 × 指标热力表</span>
          <span class="md-h-note">近 30 天埋点 · 底色越浓量越大</span>
        </div>
        <div v-if="stateErr && summaryErr" class="md-fail">加载失败 · 后端度量命令未就绪</div>
        <div v-else class="md-heat-wrap">
          <table class="md-heat">
            <thead>
              <tr>
                <th class="th-plat">平台</th>
                <th>启用</th>
                <th>周配额</th>
                <th>队列中</th>
                <th>草稿</th>
                <th>发布</th>
                <th>失败</th>
                <th>成本</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="r in platformRows" :key="r.id">
                <td class="td-plat">{{ r.name }}</td>
                <td>
                  <span class="dot-status" :class="r.enabled ? 'on' : 'off'" />
                  <span class="td-sm">{{ r.hasSettings ? (r.enabled ? "开" : "关") : "未配" }}</span>
                </td>
                <td class="td-num">{{ r.hasSettings ? r.weeklyQuota : "—" }}</td>
                <td class="td-num">{{ r.inQueue || "·" }}</td>
                <td class="td-num" :style="heat(r.drafts, maxDraft, RGB_DRAFT)">{{ r.drafts || "·" }}</td>
                <td class="td-num" :style="heat(r.published, maxPub, RGB_PUB)">{{ r.published || "·" }}</td>
                <td class="td-num" :style="heat(r.failed, maxFail, RGB_FAIL)">{{ r.failed || "·" }}</td>
                <td class="td-num" :style="heat(r.cost, maxCost, RGB_COST)">{{ r.cost > 0 ? fmtCost(r.cost) : "·" }}</td>
              </tr>
            </tbody>
          </table>
        </div>
      </section>

      <!-- 3 · 账号健康度 -->
      <section class="md-card">
        <div class="md-card-h">
          <KeyRound :size="15" /><span>账号健康度</span>
          <span class="md-h-note">点灯跳运营中心管理登录态</span>
        </div>
        <div v-if="accountsErr" class="md-fail">加载失败 · 账号状态命令未就绪</div>
        <div v-else class="md-health">
          <button
            v-for="p in MEDIA_PLATFORMS"
            :key="p.id"
            class="md-light"
            :title="`${p.name} · ${healthLabel(healthOf(p.id))} · 点击去运营中心`"
            @click="gotoOps"
          >
            <span class="light-dot" :class="healthOf(p.id)" />
            <span class="light-name">{{ p.name }}</span>
            <span class="light-state" :class="healthOf(p.id)">{{ healthLabel(healthOf(p.id)) }}</span>
          </button>
        </div>
      </section>

      <div class="md-2col">
        <!-- 4 · 队列概览 -->
        <section class="md-card">
          <div class="md-card-h">
            <Layers :size="15" /><span>队列概览</span>
            <span class="md-h-note">最近 12 条</span>
          </div>
          <div v-if="stateErr" class="md-fail">加载失败 · 队列命令未就绪</div>
          <div v-else-if="!recentQueue.length" class="md-empty">队列为空 · 去运营中心排入选题即可开跑</div>
          <div v-else class="md-queue">
            <div v-for="q in recentQueue" :key="q.id" class="q-row">
              <span class="q-plat">{{ platName(q.platform) }}</span>
              <span class="q-title" :title="q.title">{{ q.title || "（未命名）" }}</span>
              <span class="q-pill" :class="qMeta(q.status).cls">{{ qMeta(q.status).label }}</span>
            </div>
          </div>
        </section>

        <!-- 5 · API 通道状态 -->
        <section class="md-card">
          <div class="md-card-h">
            <Zap :size="15" /><span>API 通道状态</span>
            <span class="md-h-note">火山方舟 · 仅展示，配置在运营中心</span>
          </div>
          <div class="md-api">
            <div class="api-row">
              <span class="api-lab">连通</span>
              <span
                v-if="arkProbe"
                class="api-val"
                :class="arkProbe.ok ? 'ok' : 'err'"
              >
                <span class="light-dot" :class="arkProbe.ok ? 'green' : 'gray'" />
                {{ arkProbe.ok ? "正常" : "异常" }}
                <b v-if="arkProbe.ok && arkProbe.latencyMs"> · {{ arkProbe.latencyMs }}ms</b>
              </span>
              <span v-else class="api-val muted">{{ arkProbing ? "探测中…" : "未探测" }}</span>
            </div>
            <div class="api-row">
              <span class="api-lab">生图模型</span>
              <span class="api-val mono">{{ arkCfgErr ? "加载失败" : (arkCfg?.imageModel || "未配置") }}</span>
            </div>
            <div class="api-row" v-if="arkProbe && !arkProbe.ok">
              <span class="api-lab">信息</span>
              <span class="api-val err-msg">{{ arkProbe.message }}</span>
            </div>
            <button class="md-btn" :disabled="arkProbing" @click="probeArk">
              <Loader v-if="arkProbing" :size="13" class="spin" /><Zap v-else :size="13" />
              <span>探活测速</span>
            </button>
          </div>
        </section>
      </div>

      <p class="md-foot">
        数据只反映已埋点 / 已采集部分，<b>没有的显示 0 或「未采集」，绝不编造</b>。配置与详细操作在
        <button class="md-link" @click="gotoOps">自媒体运营中心</button>。
      </p>
    </div>
  </div>
</template>

<style scoped>
.md { height: 100%; display: flex; flex-direction: column; overflow: hidden; background: var(--bg); }

/* 顶栏 */
.md-head {
  display: flex; align-items: center; gap: 10px;
  padding: 14px 22px; border-bottom: 1px solid var(--border-soft); background: var(--panel);
}
.md-icon { color: var(--primary); }
.md-title { font-family: var(--serif); font-size: 17px; font-weight: 600; color: var(--text); }
.md-sub { font-size: 12.5px; color: var(--muted); margin-left: 6px; }
.md-refresh {
  margin-left: auto; display: inline-flex; align-items: center; gap: 6px;
  padding: 7px 14px; border: 1px solid var(--border); border-radius: 8px;
  background: var(--panel); color: var(--text-2); font-size: 12.5px; cursor: pointer;
  transition: border-color 0.15s, color 0.15s;
}
.md-refresh:hover:not(:disabled) { border-color: var(--primary); color: var(--primary); }
.md-refresh:disabled { opacity: 0.55; cursor: default; }

.md-body { flex: 1; overflow: auto; padding: 18px 24px; display: flex; flex-direction: column; gap: 16px; }

/* KPI 卡带 */
.md-kpis { display: grid; grid-template-columns: repeat(5, 1fr); gap: 12px; }
.md-kpi {
  background: linear-gradient(165deg, var(--panel) 0%, var(--bg-soft) 100%);
  border: 1px solid var(--border-soft); border-radius: 13px;
  padding: 14px 15px 13px; box-shadow: var(--shadow-sm);
  min-height: 104px; display: flex; flex-direction: column;
}
.k-head { display: flex; align-items: center; justify-content: space-between; margin-bottom: 10px; }
.k-lab { font-size: 12px; color: var(--text-2); display: inline-flex; align-items: center; gap: 5px; }
.k-tag { font-size: 9px; color: var(--dim); border: 1px solid var(--border-strong); border-radius: 4px; padding: 0 4px; }
.k-ic { width: 28px; height: 28px; border-radius: 8px; display: inline-flex; align-items: center; justify-content: center; }
.k-ic.blue { background: #2c6fff1a; color: #2c6fff; }
.k-ic.green { background: #16a34a1a; color: #16a34a; }
.k-ic.purple { background: #7c5cff1a; color: #7c5cff; }
.k-ic.amber { background: #e8833a1a; color: #e8833a; }
.k-ic.red { background: #e9545a1a; color: #e9545a; }
.k-num {
  font-family: var(--mono); font-size: 25px; font-weight: 700; color: var(--ink);
  letter-spacing: -0.5px; line-height: 1.1;
}
.k-num.dim { font-size: 15px; color: var(--dim); font-weight: 500; }
.k-sub { margin-top: auto; padding-top: 8px; font-size: 10.5px; color: var(--muted); }

/* 通用卡 */
.md-card {
  border: 1px solid var(--border-soft); border-radius: 12px; background: var(--panel);
  padding: 15px 17px; display: flex; flex-direction: column; gap: 12px;
}
.md-card-h {
  display: flex; align-items: center; gap: 8px;
  font-size: 13.5px; font-weight: 600; color: var(--text);
}
.md-card-h > svg { color: var(--primary); }
.md-h-note { margin-left: auto; font-size: 11px; font-weight: 400; color: var(--muted); }
.md-fail { font-size: 12.5px; color: var(--vermilion); padding: 10px 2px; }
.md-empty { font-size: 12.5px; color: var(--muted); padding: 10px 2px; line-height: 1.7; }

/* 热力表 */
.md-heat-wrap { overflow-x: auto; }
.md-heat { width: 100%; border-collapse: collapse; font-size: 12.5px; min-width: 560px; }
.md-heat th {
  text-align: center; font-weight: 600; color: var(--muted); font-size: 11px;
  padding: 6px 8px; border-bottom: 1px solid var(--border-soft); white-space: nowrap;
}
.md-heat th.th-plat { text-align: left; }
.md-heat td {
  text-align: center; padding: 8px; border-bottom: 1px solid var(--border-soft);
  color: var(--text-2); white-space: nowrap;
}
.md-heat tbody tr:last-child td { border-bottom: none; }
.td-plat { text-align: left; font-weight: 600; color: var(--text); }
.td-num { font-family: var(--mono); border-radius: 6px; }
.td-sm { font-size: 11px; color: var(--muted); margin-left: 3px; }
.dot-status { display: inline-block; width: 8px; height: 8px; border-radius: 50%; vertical-align: middle; }
.dot-status.on { background: #39d09a; }
.dot-status.off { background: var(--border-strong); }

/* 账号健康度 */
.md-health { display: flex; flex-wrap: wrap; gap: 10px; }
.md-light {
  display: flex; align-items: center; gap: 8px;
  padding: 9px 13px; border: 1px solid var(--border); border-radius: 10px;
  background: var(--bg); cursor: pointer; transition: border-color 0.15s, background 0.15s;
}
.md-light:hover { border-color: var(--primary); background: var(--primary-soft); }
.light-dot { width: 9px; height: 9px; border-radius: 50%; flex-shrink: 0; }
.light-dot.green { background: #39d09a; box-shadow: 0 0 0 3px #39d09a26; }
.light-dot.yellow { background: #e6b873; box-shadow: 0 0 0 3px #e6b87326; }
.light-dot.gray { background: var(--border-strong); }
.light-name { font-size: 12.5px; font-weight: 600; color: var(--text); }
.light-state { font-size: 10.5px; }
.light-state.green { color: #2aa87a; }
.light-state.yellow { color: #c9902f; }
.light-state.gray { color: var(--dim); }

/* 双列 */
.md-2col { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; align-items: start; }

/* 队列 */
.md-queue { display: flex; flex-direction: column; gap: 6px; max-height: 320px; overflow-y: auto; }
.q-row { display: flex; align-items: center; gap: 9px; padding: 7px 4px; border-bottom: 1px solid var(--border-soft); }
.q-row:last-child { border-bottom: none; }
.q-plat {
  flex-shrink: 0; font-size: 11px; color: var(--text-2);
  background: var(--bg-soft); border-radius: 5px; padding: 2px 7px;
}
.q-title { flex: 1; min-width: 0; font-size: 12.5px; color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.q-pill { flex-shrink: 0; font-size: 10.5px; font-weight: 600; border-radius: 999px; padding: 2px 9px; }
.q-blue { background: #2c6fff1f; color: #2c6fff; }
.q-amber { background: #e8833a24; color: #d47a2a; }
.q-green { background: #39d09a26; color: #229a70; }
.q-teal { background: #14b8a622; color: #0f9a8c; }
.q-red { background: #e9545a24; color: #d8434a; }
.q-gray { background: var(--bg-soft); color: var(--muted); }

/* API 通道 */
.md-api { display: flex; flex-direction: column; gap: 10px; }
.api-row { display: flex; align-items: center; gap: 10px; }
.api-lab { width: 64px; flex-shrink: 0; font-size: 12px; color: var(--muted); }
.api-val { font-size: 13px; color: var(--text); display: inline-flex; align-items: center; gap: 6px; }
.api-val.ok { color: #229a70; font-weight: 600; }
.api-val.err { color: var(--vermilion); font-weight: 600; }
.api-val.muted { color: var(--dim); }
.api-val.mono { font-family: var(--mono); font-size: 12px; }
.api-val b { font-family: var(--mono); font-weight: 600; }
.err-msg { font-size: 11.5px; color: var(--vermilion); }
.md-btn {
  align-self: flex-start; margin-top: 2px;
  display: inline-flex; align-items: center; gap: 6px;
  padding: 7px 14px; border: 1px solid var(--border); border-radius: 8px;
  background: transparent; color: var(--text-2); font-size: 12.5px; cursor: pointer;
  transition: border-color 0.15s, color 0.15s;
}
.md-btn:hover:not(:disabled) { border-color: var(--primary); color: var(--primary); }
.md-btn:disabled { opacity: 0.55; cursor: default; }

/* 脚注 */
.md-foot { font-size: 11.5px; color: var(--muted); line-height: 1.7; margin: 0; }
.md-foot b { color: var(--text-2); }
.md-link {
  border: none; background: transparent; color: var(--primary); cursor: pointer;
  font-size: 11.5px; padding: 0; text-decoration: underline;
}

.spin { animation: md-spin 0.9s linear infinite; }
@keyframes md-spin { to { transform: rotate(360deg); } }

@media (max-width: 900px) {
  .md-kpis { grid-template-columns: repeat(2, 1fr); }
  .md-2col { grid-template-columns: 1fr; }
}
</style>
