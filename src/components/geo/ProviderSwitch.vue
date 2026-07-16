<script setup lang="ts">
/**
 * 切换中心（cc-switch 复刻的供应商坞·内联版）—— 嵌进运营中心「API 中心 / 模型通道」下方。
 * 与 Sidebar 左下角的 ProviderDock 共用同一个 providers store，状态天然同步。
 * 功能齐平：点选切换供应商、联动/隔离开关、本地路由热切换开关、添加供应商入口、Codex/Claude 授权入口。
 */
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import {
  Plus, Check, RefreshCw, Pencil, Trash2, ExternalLink, Search,
  LogIn, ShieldCheck, KeyRound, CircleAlert, Zap,
} from "@lucide/vue";
import { useProvidersStore } from "../../stores/providers";
import type { ProviderView, CodexDeviceLogin, ClaudeLoginStart } from "../../tauri";

const store = useProvidersStore();
const filter = ref("");

// Codex（ChatGPT）授权
const codexOpen = ref(false);
const codexDevice = ref<CodexDeviceLogin | null>(null);
const codexBusy = ref(false);
const codexErr = ref<string | null>(null);
let codexTimer: number | null = null;
let codexExpireAt = 0;

// Claude 官方订阅授权
const claudeOpen = ref(false);
const claudeLogin = ref<ClaudeLoginStart | null>(null);
const claudePasted = ref("");
const claudeBusy = ref(false);
const claudeErr = ref<string | null>(null);
let claudeTimer: number | null = null;
let claudeExpireAt = 0;

onMounted(() => {
  store.refresh();
  store.refreshCodex();
  store.refreshCodexProxy();
  store.refreshClaudeAuth();
});
onBeforeUnmount(() => {
  stopCodexPoll();
  stopClaudePoll();
});

const current = computed(() => store.current);
function hostOf(url: string): string {
  if (!url) return "本地 / 订阅";
  try {
    return new URL(url).host;
  } catch {
    return url.replace(/^https?:\/\//, "");
  }
}
const currentModel = computed(() => {
  const env = current.value?.settingsConfig?.env ?? {};
  return env.ANTHROPIC_MODEL || env.ANTHROPIC_DEFAULT_SONNET_MODEL || "";
});

const filtered = computed(() => {
  const q = filter.value.trim().toLowerCase();
  if (!q) return store.providers;
  return store.providers.filter(
    (p) =>
      p.name.toLowerCase().includes(q) ||
      hostOf(p.baseUrl).toLowerCase().includes(q) ||
      p.id.toLowerCase().includes(q)
  );
});

/** 联动/隔离切换 */
async function toggleLink() {
  await store.setLinkMode(!store.linkGlobal);
  await store.refresh();
}
/** 本地路由 · 热切换总开关 */
async function toggleRoute() {
  await store.setRouteMode(!store.routeLocal);
  await store.refresh();
  await store.refreshCodexProxy();
}

async function onRowClick(p: ProviderView) {
  if (p.kind === "codex") {
    await store.refreshCodex();
    store.refreshCodexProxy();
    if (store.codex?.loggedIn && p.id !== store.currentId) {
      await store.switchTo("codex");
    } else {
      codexOpen.value = true;
    }
    return;
  }
  if (p.kind === "copilot") {
    store.openAdd(p);
    return;
  }
  if (p.id === store.currentId) return;
  if (!p.hasKey) {
    store.openAdd(p);
    return;
  }
  await store.switchTo(p.id);
}
function editProvider(p: ProviderView) {
  store.openAdd(p);
}
function addCustom() {
  store.openAdd(null);
}
async function removeProvider(p: ProviderView) {
  const verb = p.isPreset ? "清除配置" : "删除";
  if (!confirm(`${verb}「${p.name}」?`)) return;
  await store.remove(p.id);
}
function openSite(url: string) {
  if (url) window.open(url, "_blank");
}
function subtitleOf(p: ProviderView): string {
  if (p.kind === "codex") return store.codex?.loggedIn ? "ChatGPT · 已授权 · 点即用" : "ChatGPT · 需先授权";
  if (p.kind === "copilot") return "需 OAuth · 代理";
  if (p.protocol === "openai") return `${hostOf(p.baseUrl)} · OpenAI 协议`;
  return hostOf(p.baseUrl);
}

// ── Codex 授权 ──
async function startCodexAuth() {
  codexErr.value = null;
  codexBusy.value = true;
  const dev = await store.codexStartLogin();
  codexBusy.value = false;
  if (!dev) {
    codexErr.value = store.error || "发起授权失败";
    return;
  }
  codexDevice.value = dev;
  codexExpireAt = Date.now() + dev.expiresIn * 1000;
  if (dev.mode === "auto") startCodexAutoPoll();
  else startCodexPoll(dev);
}
function startCodexPoll(dev: CodexDeviceLogin) {
  stopCodexPoll();
  const intervalMs = Math.max(2, dev.interval) * 1000;
  codexTimer = window.setInterval(async () => {
    if (Date.now() > codexExpireAt) {
      resetCodexAuth();
      codexErr.value = "授权超时, 请重试";
      return;
    }
    try {
      const st = await store.codexPollLogin(dev.deviceCode, dev.userCode);
      if (st === "ok") {
        stopCodexPoll();
        codexDevice.value = null;
        await store.refreshCodex();
      }
    } catch (e) {
      stopCodexPoll();
      codexDevice.value = null;
      codexErr.value = String(e);
    }
  }, intervalMs);
}
function startCodexAutoPoll() {
  stopCodexPoll();
  codexTimer = window.setInterval(async () => {
    if (Date.now() > codexExpireAt) {
      resetCodexAuth();
      codexErr.value = "授权超时, 请重试";
      return;
    }
    try {
      const r = await store.codexLoginPoll();
      if (r.status === "pending") return;
      stopCodexPoll();
      codexDevice.value = null;
      if (r.status !== "ok") codexErr.value = r.message || "授权未完成, 请重试";
    } catch (e) {
      stopCodexPoll();
      codexDevice.value = null;
      codexErr.value = String(e);
    }
  }, 1500);
}
function stopCodexPoll() {
  if (codexTimer !== null) {
    clearInterval(codexTimer);
    codexTimer = null;
  }
}
function resetCodexAuth() {
  stopCodexPoll();
  if (codexDevice.value?.mode === "auto") store.codexLoginCancel();
  codexDevice.value = null;
  codexBusy.value = false;
}
async function routeCodex() {
  codexErr.value = null;
  const ok = await store.switchTo("codex");
  await store.refreshCodexProxy();
  if (ok) codexOpen.value = false;
  else codexErr.value = store.error || "切换失败";
}

// ── Claude 官方订阅授权 ──
async function startClaudeAuth(forceManual = false) {
  stopClaudePoll();
  if (forceManual) store.claudeLoginCancel();
  claudeErr.value = null;
  claudePasted.value = "";
  claudeBusy.value = true;
  const login = await store.claudeStartLogin(forceManual);
  claudeBusy.value = false;
  if (!login) {
    claudeErr.value = store.error || "发起授权失败";
    return;
  }
  claudeLogin.value = login;
  if (login.mode === "auto") startClaudeAutoPoll();
}
function startClaudeAutoPoll() {
  stopClaudePoll();
  claudeExpireAt = Date.now() + 10 * 60 * 1000;
  claudeTimer = window.setInterval(async () => {
    if (Date.now() > claudeExpireAt) {
      resetClaudeAuth();
      claudeErr.value = "授权超时, 请重试";
      return;
    }
    try {
      const r = await store.claudeLoginPoll();
      if (r.status === "pending") return;
      stopClaudePoll();
      claudeLogin.value = null;
      if (r.status !== "ok") claudeErr.value = r.message || "授权未完成, 请重试";
    } catch (e) {
      stopClaudePoll();
      claudeLogin.value = null;
      claudeErr.value = String(e);
    }
  }, 1500);
}
function stopClaudePoll() {
  if (claudeTimer !== null) {
    clearInterval(claudeTimer);
    claudeTimer = null;
  }
}
function openClaudeAuthPage() {
  if (claudeLogin.value) window.open(claudeLogin.value.authorizeUrl, "_blank");
}
async function submitClaudeCode() {
  if (!claudeLogin.value || !claudePasted.value.trim()) return;
  claudeErr.value = null;
  claudeBusy.value = true;
  try {
    const ok = await store.claudeFinishLogin(claudePasted.value, claudeLogin.value.verifier, claudeLogin.value.state);
    if (ok) {
      claudeLogin.value = null;
      claudePasted.value = "";
    } else {
      claudeErr.value = "授权未完成,请确认授权码完整";
    }
  } catch (e) {
    claudeErr.value = String(e);
  } finally {
    claudeBusy.value = false;
  }
}
function resetClaudeAuth() {
  stopClaudePoll();
  if (claudeLogin.value?.mode === "auto") store.claudeLoginCancel();
  claudeLogin.value = null;
  claudePasted.value = "";
  claudeBusy.value = false;
}
</script>

<template>
  <div class="psw card">
    <div class="psw-head">
      <h3>切换中心<span class="ccnote">cc-switch 复刻 · 与侧栏坞共用状态</span></h3>
      <div class="psw-head-act">
        <button class="btn ghost sm" title="刷新" @click="store.refresh(); store.refreshCodex(); store.refreshClaudeAuth()">
          <RefreshCw :size="13" :stroke-width="1.8" />
        </button>
        <button class="btn sm" @click="addCustom"><Plus :size="13" :stroke-width="2.2" /> 添加供应商</button>
      </div>
    </div>

    <!-- 当前供应商 -->
    <div v-if="current" class="psw-now">
      <span class="dot" :style="{ background: current.color, boxShadow: `0 0 0 3px ${current.color}29` }" />
      <div class="now-info">
        <div class="now-name">{{ current.name }}
          <span v-if="current.kind === 'codex'" class="tag gpt">GPT</span>
          <span v-else-if="current.protocol === 'openai'" class="tag oa">OpenAI</span>
        </div>
        <div class="now-host">
          <template v-if="current.kind === 'codex'">
            <span v-if="store.codex?.loggedIn">ChatGPT 已授权 · 经本地路由</span>
            <span v-else class="warn">⚠ 需先授权 ChatGPT</span>
          </template>
          <template v-else-if="current.kind === 'official'">
            <span v-if="store.claudeAuth?.loggedIn">Claude 订阅 · 已登录</span>
            <span v-else class="warn">未登录订阅 · 可用 API Key 或点下方授权</span>
          </template>
          <template v-else>{{ hostOf(current.baseUrl) }}<span v-if="currentModel"> · {{ currentModel }}</span></template>
        </div>
      </div>
      <span class="now-using"><Check :size="12" :stroke-width="2.6" /> 使用中</span>
    </div>

    <!-- 联动/隔离 + 本地路由 两档开关 -->
    <div class="psw-switches">
      <div class="sw-row">
        <div class="sw-info">
          <span class="sw-title">联动系统 CLI</span>
          <span class="sw-desc">{{ store.linkGlobal ? "切换写入 ~/.claude/settings.json,终端 claude 跟着变" : "已隔离:仅 Polaris 自用,终端与监控不受影响" }}</span>
        </div>
        <button class="toggle" :class="{ on: store.linkGlobal }" role="switch" :aria-checked="store.linkGlobal" @click="toggleLink"><span class="knob" /></button>
      </div>
      <div class="sw-row">
        <div class="sw-info">
          <span class="sw-title">本地路由 · 热切换</span>
          <span class="sw-desc">{{
            store.routeLocal
              ? (store.codexProxy?.running
                  ? `全部请求经 127.0.0.1:${store.codexProxy.port} 转发,改 Key 即刻生效`
                  : "全部请求经本地路由转发,改 Key 即刻生效")
              : "直连:切换时把地址/Key 注入环境;GPT/Codex 仍自动走本地路由"
          }}</span>
        </div>
        <button class="toggle" :class="{ on: store.routeLocal }" role="switch" :aria-checked="store.routeLocal" @click="toggleRoute"><span class="knob" /></button>
      </div>
    </div>

    <!-- 搜索 -->
    <div class="psw-search">
      <Search :size="13" :stroke-width="1.8" class="s-ic" />
      <input v-model="filter" placeholder="搜索供应商 / 主机名…" />
      <button v-if="filter" class="btn ghost sm" @click="filter = ''">清空</button>
    </div>

    <!-- 供应商列表 -->
    <div class="psw-list">
      <div
        v-for="p in filtered"
        :key="p.id"
        class="prov-row"
        :class="{ on: p.id === store.currentId, pending: store.switching === p.id }"
        @click="onRowClick(p)"
      >
        <span class="prov-dot" :style="{ background: p.color }" />
        <span class="prov-info">
          <span class="prov-name">{{ p.name }}
            <span v-if="p.kind === 'codex'" class="tag gpt">GPT</span>
            <span v-else-if="p.protocol === 'openai'" class="tag oa">OpenAI</span>
          </span>
          <span class="prov-host">{{ subtitleOf(p) }}</span>
        </span>
        <span class="prov-tail">
          <span v-if="store.switching === p.id" class="spinner" />
          <span v-else-if="p.id === store.currentId" class="badge-on"><Check :size="11" :stroke-width="2.6" /> 使用中</span>
          <span v-else-if="p.kind === 'codex' || p.kind === 'copilot'" class="badge-oauth">授权</span>
          <span v-else-if="!p.hasKey" class="badge-need">配置</span>
          <span class="row-actions">
            <button v-if="p.websiteUrl" class="mini" title="官网" @click.stop="openSite(p.websiteUrl)"><ExternalLink :size="12" :stroke-width="1.8" /></button>
            <button v-if="p.kind !== 'codex' && p.kind !== 'copilot'" class="mini" :title="p.isPreset ? '配置' : '编辑'" @click.stop="editProvider(p)"><Pencil :size="12" :stroke-width="1.8" /></button>
            <button v-if="(p.isPreset && p.hasKey && p.kind === 'key') || p.kind === 'custom'" class="mini danger" :title="p.isPreset ? '清除配置' : '删除'" @click.stop="removeProvider(p)"><Trash2 :size="12" :stroke-width="1.8" /></button>
          </span>
        </span>
      </div>
      <div v-if="filtered.length === 0" class="list-empty">无匹配供应商</div>
    </div>

    <!-- 授权入口 -->
    <div class="psw-auth-entries">
      <button class="auth-entry gpt" @click="codexOpen = !codexOpen; if (codexOpen) store.refreshCodex()">
        <ShieldCheck :size="13" :stroke-width="2" /> Codex / GPT 授权
        <span v-if="store.codex?.loggedIn" class="ae-ok">已授权</span>
      </button>
      <button class="auth-entry claude" @click="claudeOpen = !claudeOpen; if (claudeOpen) store.refreshClaudeAuth()">
        <KeyRound :size="13" :stroke-width="2" /> Claude 订阅授权
        <span v-if="store.claudeAuth?.loggedIn" class="ae-ok">已登录</span>
      </button>
    </div>

    <!-- Codex 授权卡 -->
    <div v-if="codexOpen" class="auth-card gpt">
      <template v-if="codexDevice">
        <p class="ac-note" v-if="codexDevice.mode === 'auto'">已打开 ChatGPT 授权页,登录并点「<b>Authorize</b>」即可,授权会自动送回 Polaris。</p>
        <p class="ac-note" v-else>已打开 ChatGPT 授权页,确认设备码 <b>{{ codexDevice.userCode }}</b> 后回到这里。</p>
        <p class="ac-poll"><span class="spinner" /> 等待浏览器中完成授权…</p>
        <div class="ac-act"><button class="btn ghost sm" @click="resetCodexAuth">取消</button></div>
      </template>
      <template v-else-if="store.codex?.loggedIn">
        <p class="ac-ok"><ShieldCheck :size="13" :stroke-width="2" /> 已授权 ChatGPT</p>
        <p class="ac-note">凭据已写入 <code>~/.codex/auth.json</code>,可让 Claude Code 经本地路由用上 ChatGPT 订阅（<code>gpt5.6-sol</code>）。</p>
        <div class="ac-act">
          <button class="btn ghost sm" :disabled="codexBusy" @click="startCodexAuth"><RefreshCw :size="12" :stroke-width="2" /> 重新授权</button>
          <button v-if="store.currentId !== 'codex'" class="btn sm gpt" @click="routeCodex"><Zap :size="12" :stroke-width="2" /> 用 GPT 对话</button>
        </div>
      </template>
      <template v-else>
        <p class="ac-note">用 ChatGPT 账号授权（无需安装 codex CLI）。点击后打开浏览器,登录并点「Authorize」即自动完成,凭据写入 <code>~/.codex/auth.json</code>。</p>
        <div class="ac-act"><button class="btn sm gpt big" :disabled="codexBusy" @click="startCodexAuth"><span v-if="codexBusy" class="spinner" /><LogIn v-else :size="13" :stroke-width="2" /> {{ codexBusy ? "正在发起…" : "ChatGPT 一键授权" }}</button></div>
      </template>
      <p v-if="codexErr" class="ac-err"><CircleAlert :size="12" :stroke-width="2" /> {{ codexErr }}</p>
    </div>

    <!-- Claude 授权卡 -->
    <div v-if="claudeOpen" class="auth-card claude">
      <template v-if="store.claudeAuth?.loggedIn && !claudeLogin">
        <p class="ac-ok claude"><ShieldCheck :size="13" :stroke-width="2" /> 已登录 Claude 订阅</p>
        <p class="ac-note">凭据已写入 <code>~/.claude/.credentials.json</code>,Polaris 与终端 <code>claude</code> 共用这份订阅。</p>
        <div class="ac-act"><button class="btn ghost sm" :disabled="claudeBusy" @click="startClaudeAuth()"><RefreshCw :size="12" :stroke-width="2" /> 重新授权</button></div>
      </template>
      <template v-else-if="claudeLogin">
        <template v-if="claudeLogin.mode === 'auto'">
          <p class="ac-note">已打开 Claude 登录页,登录并点「<b>Authorize</b>」即可,授权会自动送回 Polaris。</p>
          <p class="ac-poll"><span class="spinner" /> 等待浏览器中完成授权…</p>
          <div class="ac-act">
            <button class="btn ghost sm" @click="resetClaudeAuth">取消</button>
            <button class="btn ghost sm" @click="startClaudeAuth(true)">改用手工回贴</button>
            <button class="btn sm claude" @click="openClaudeAuthPage"><ExternalLink :size="12" :stroke-width="2" /> 重开登录页</button>
          </div>
        </template>
        <template v-else>
          <p class="ac-note">已打开 Claude 登录页。登录并点「Authorize」后,<b>整段复制</b>授权码（形如 <code>xxxx#yyyy</code>）粘到下面:</p>
          <textarea v-model="claudePasted" class="ac-input" rows="2" placeholder="在此粘贴授权码…" spellcheck="false" @keydown.enter.prevent="submitClaudeCode" />
          <div class="ac-act">
            <button class="btn ghost sm" @click="openClaudeAuthPage"><ExternalLink :size="12" :stroke-width="2" /> 重开登录页</button>
            <button class="btn sm claude" :disabled="claudeBusy || !claudePasted.trim()" @click="submitClaudeCode"><span v-if="claudeBusy" class="spinner" /><Check v-else :size="12" :stroke-width="2" /> {{ claudeBusy ? "验证中…" : "完成授权" }}</button>
          </div>
        </template>
      </template>
      <template v-else>
        <p class="ac-note">用 Claude 账号登录订阅（Pro / Max）。点击后打开浏览器,登录并点「Authorize」即自动完成,凭据写入 <code>~/.claude/.credentials.json</code>。</p>
        <div class="ac-act"><button class="btn sm claude big" :disabled="claudeBusy" @click="startClaudeAuth()"><span v-if="claudeBusy" class="spinner" /><LogIn v-else :size="13" :stroke-width="2" /> {{ claudeBusy ? "正在打开…" : "授权登录 Claude 订阅" }}</button></div>
      </template>
      <p v-if="claudeErr" class="ac-err"><CircleAlert :size="12" :stroke-width="2" /> {{ claudeErr }}</p>
    </div>

    <div v-if="store.error" class="psw-err">{{ store.error }}</div>
  </div>
</template>

<style scoped>
.psw { display: flex; flex-direction: column; gap: 12px; }
.psw-head { display: flex; align-items: center; justify-content: space-between; }
.psw-head h3 { margin: 0; display: flex; align-items: center; gap: 8px; }
.ccnote { font-size: var(--text-xs, 11px); color: var(--muted); font-weight: 400; }
.psw-head-act { display: flex; gap: 6px; }
.btn.sm { font-size: 12px; padding: 4px 10px; display: inline-flex; align-items: center; gap: 4px; }
.btn.ghost.sm { padding: 4px 8px; }

/* 当前供应商 */
.psw-now { display: flex; align-items: center; gap: 10px; padding: 10px 12px; border: 1px solid var(--border); border-radius: 10px; background: var(--bg-soft); }
.psw-now .dot { width: 10px; height: 10px; border-radius: 50%; flex-shrink: 0; }
.now-info { flex: 1; min-width: 0; }
.now-name { font-size: 13px; font-weight: 600; color: var(--text); display: flex; align-items: center; gap: 6px; }
.now-host { font-size: 10.5px; color: var(--muted); font-family: var(--mono); margin-top: 2px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.now-host .warn { color: #d97706; font-weight: 600; }
.now-using { display: inline-flex; align-items: center; gap: 3px; font-size: 10.5px; color: var(--primary-deep, var(--primary)); font-weight: 600; flex-shrink: 0; }
.tag { font-family: var(--mono); font-size: 8.5px; padding: 0 4px; border-radius: 3px; font-weight: 600; letter-spacing: 0.5px; }
.tag.gpt { color: #10a37f; border: 1px solid #10a37f66; }
.tag.oa { color: #7c5cff; border: 1px solid #7c5cff66; }

/* 开关 */
.psw-switches { display: flex; flex-direction: column; gap: 8px; }
.sw-row { display: flex; align-items: center; gap: 8px; padding: 8px 11px; border: 1px solid var(--border-soft); border-radius: 9px; background: var(--bg-soft); }
.sw-info { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
.sw-title { font-size: 12px; font-weight: 600; color: var(--text); }
.sw-desc { font-size: 10px; color: var(--dim, var(--muted)); font-family: var(--mono); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.toggle { position: relative; flex-shrink: 0; width: 34px; height: 19px; border: 1px solid var(--border); border-radius: 999px; background: var(--panel); padding: 0; cursor: pointer; transition: background .18s, border-color .18s; }
.toggle .knob { position: absolute; top: 2px; left: 2px; width: 13px; height: 13px; border-radius: 50%; background: var(--muted); transition: transform .18s, background .18s; }
.toggle.on { background: var(--primary); border-color: var(--primary); }
.toggle.on .knob { transform: translateX(15px); background: #fff; }

/* 搜索 */
.psw-search { display: flex; align-items: center; gap: 6px; padding: 5px 9px; border: 1px solid var(--border); border-radius: 8px; background: var(--bg-soft); }
.psw-search:focus-within { border-color: var(--primary); }
.psw-search .s-ic { color: var(--muted); flex-shrink: 0; }
.psw-search input { flex: 1; border: none; background: transparent; font-size: 12px; color: var(--text); }
.psw-search input:focus { outline: none; }

/* 列表 */
.psw-list { max-height: 360px; overflow-y: auto; display: flex; flex-direction: column; gap: 2px; border: 1px solid var(--border-soft); border-radius: 9px; padding: 4px; }
.prov-row { position: relative; display: flex; align-items: center; gap: 9px; padding: 7px 9px; border-radius: 7px; cursor: pointer; transition: background .12s; }
.prov-row:hover { background: var(--selection-bg, var(--bg-soft)); }
.prov-row.on { background: var(--primary-soft, var(--bg-soft)); }
.prov-row.pending { opacity: 0.6; }
.prov-dot { width: 9px; height: 9px; border-radius: 50%; flex-shrink: 0; }
.prov-info { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
.prov-name { font-size: 12.5px; color: var(--text); font-weight: 500; display: inline-flex; align-items: center; gap: 5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.prov-host { font-size: 10px; color: var(--muted); font-family: var(--mono); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.prov-tail { display: flex; align-items: center; gap: 4px; flex-shrink: 0; }
.badge-on { display: inline-flex; align-items: center; gap: 3px; font-size: 10px; color: var(--primary-deep, var(--primary)); font-weight: 600; }
.badge-need { font-size: 9.5px; color: var(--gold, #d97706); border: 1px solid var(--gold, #d97706); border-radius: 4px; padding: 1px 5px; opacity: 0.85; }
.badge-oauth { font-size: 9.5px; color: #10a37f; border: 1px solid #10a37f; border-radius: 4px; padding: 1px 5px; }
.row-actions { display: none; align-items: center; gap: 2px; }
.prov-row:hover .row-actions { display: inline-flex; }
.mini { border: none; background: transparent; color: var(--muted); width: 22px; height: 22px; border-radius: 5px; display: inline-flex; align-items: center; justify-content: center; cursor: pointer; }
.mini:hover { background: var(--border); color: var(--text); }
.mini.danger:hover { background: var(--vermilion-soft, #fde8e8); color: var(--vermilion, #dc2626); }
.list-empty { text-align: center; font-size: 11.5px; color: var(--muted); padding: 12px 0; }

.spinner { width: 12px; height: 12px; border: 2px solid var(--border); border-top-color: var(--primary); border-radius: 50%; animation: psw-spin .7s linear infinite; display: inline-block; }
@keyframes psw-spin { to { transform: rotate(360deg); } }

/* 授权入口 */
.psw-auth-entries { display: flex; gap: 8px; }
.auth-entry { flex: 1; display: inline-flex; align-items: center; justify-content: center; gap: 5px; padding: 7px 10px; border: 1px solid var(--border); background: var(--panel); color: var(--text-2, var(--text)); font-size: 11.5px; border-radius: 8px; cursor: pointer; }
.auth-entry.gpt:hover { border-color: #10a37f; color: #10a37f; background: #10a37f0c; }
.auth-entry.claude:hover { border-color: #cc785c; color: #cc785c; background: #cc785c0c; }
.ae-ok { font-size: 9.5px; padding: 0 5px; border-radius: 4px; background: var(--bg-soft); color: var(--muted); }

/* 授权卡 */
.auth-card { display: flex; flex-direction: column; gap: 7px; padding: 11px; border-radius: 10px; }
.auth-card.gpt { border: 1px solid #10a37f55; background: #10a37f0c; }
.auth-card.claude { border: 1px solid #cc785c55; background: #cc785c0c; }
.ac-note { margin: 0; font-size: 11px; color: var(--text-2, var(--text)); line-height: 1.6; }
.ac-note code, .ac-ok code { font-family: var(--mono); font-size: 10.5px; background: var(--code-bg, var(--bg-soft)); color: var(--code-text, var(--text)); padding: 1px 5px; border-radius: 4px; }
.ac-ok { margin: 0; display: inline-flex; align-items: center; gap: 5px; font-size: 12px; font-weight: 600; color: #10a37f; }
.ac-ok.claude { color: #b9664c; }
.ac-poll { margin: 0; display: inline-flex; align-items: center; gap: 6px; font-size: 11px; color: var(--text-2, var(--text)); }
.ac-act { display: flex; gap: 6px; justify-content: flex-end; }
.btn.sm.gpt { background: #10a37f; border-color: #10a37f; color: #fff; }
.btn.sm.gpt:hover { background: #0d8a6c; }
.btn.sm.claude { background: #cc785c; border-color: #cc785c; color: #fff; }
.btn.sm.claude:hover { background: #b9664c; }
.btn.sm.big { padding: 7px 14px; font-size: 12.5px; font-weight: 600; }
.ac-input { width: 100%; resize: vertical; min-height: 38px; font-family: var(--mono); font-size: 11.5px; line-height: 1.5; color: var(--text); background: var(--bg-soft); border: 1px dashed #cc785c66; border-radius: 7px; padding: 7px 9px; }
.ac-input:focus { outline: none; border-color: #cc785c; border-style: solid; }
.ac-err { margin: 1px 0 0; display: inline-flex; align-items: center; gap: 4px; font-size: 11px; color: var(--vermilion, #dc2626); background: var(--vermilion-soft, #fde8e8); border-radius: 6px; padding: 6px 9px; }
.psw-err { font-size: 11px; color: var(--vermilion, #dc2626); background: var(--vermilion-soft, #fde8e8); border-radius: 6px; padding: 6px 9px; }
</style>
