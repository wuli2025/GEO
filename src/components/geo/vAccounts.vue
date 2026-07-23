<script setup lang="ts">
/** 账号矩阵：账号总表全接真登录态（media_accounts_status / media_account_open / media_account_forget）；分发与风控红线为设计稿静态。 */
import { ref, computed, onMounted, onUnmounted } from "vue";
import { title } from "./render";
import { MOCK } from "./data";
import { mediaAccounts, MEDIA_PLATFORMS, type MediaAccountStatus, type MediaPlatform } from "../../tauri";
import { toast } from "../../composables/useToast";

const props = defineProps<{ sub: string; platform: string }>();

const head = computed(() =>
  title("账号矩阵", "资源 / 多账号管理与分布式发送 —— 每账号独立 profile + 独立 CDP 端口，cookie 互不串")
);

const statuses = ref<MediaAccountStatus[]>([]);
const loadFailed = ref(false);
const busy = ref<string | null>(null);
const addingAccount = ref(false);
const addPlatform = ref<MediaPlatform>("wechat");

async function loadStatus() {
  try {
    statuses.value = await mediaAccounts.status();
    loadFailed.value = false;
  } catch {
    // 保留上一次的数据，别把表清空
    loadFailed.value = statuses.value.length === 0;
  }
}
onMounted(loadStatus);

// ── 登录后轮询：扫码是分钟级动作，开完窗口只刷一次必然看不到结果 ──
let pollTimer: ReturnType<typeof setInterval> | null = null;
let pollLeft = 0;
function stopPoll() {
  if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }
}
function pollUntilBound(pid: string) {
  stopPoll();
  pollLeft = 40; // 40 × 3s = 2 分钟窗口
  pollTimer = setInterval(async () => {
    await loadStatus();
    const s = statuses.value.find((x) => x.platform === pid);
    if (s?.bound) {
      toast.info(`${s.label} 登录态已保存到 profile，之后投递草稿不再重复扫码`);
      stopPoll();
    } else if (--pollLeft <= 0) {
      stopPoll();
    }
  }, 3000);
}
onUnmounted(stopPoll);

const boundCount = computed(() => statuses.value.filter((s) => s.bound).length);

function fmtActive(ts: number | null): string {
  if (!ts) return "—";
  const diff = Math.floor(Date.now() / 1000) - ts;
  if (diff < 60) return "刚刚";
  if (diff < 3600) return `${Math.floor(diff / 60)} 分钟前`;
  if (diff < 86400) return `${Math.floor(diff / 3600)} 小时前`;
  return `${Math.floor(diff / 86400)} 天前`;
}

async function openWindow(pid: string, target: "login" | "draft") {
  const key = `${pid}:${target}`;
  busy.value = key;
  const wasBound = statuses.value.find((x) => x.platform === pid)?.bound ?? false;
  try {
    const r = await mediaAccounts.open(pid as MediaPlatform, target);
    toast.info(r?.message ?? "已打开窗口，扫码登录后关闭即可，登录态永久保留");
    if (!wasBound) pollUntilBound(pid);
    else setTimeout(loadStatus, 2000);
  } catch (e: any) {
    toast.error(e?.message ?? String(e));
  } finally {
    busy.value = null;
  }
}

async function forget(s: MediaAccountStatus) {
  if (!confirm(`解绑「${s.label}」？将删除其登录态 profile，下次发文需重新扫码。`)) return;
  busy.value = `${s.platform}:forget`;
  try {
    const msg = await mediaAccounts.forget(s.platform);
    toast.info(msg);
    await loadStatus();
  } catch (e: any) {
    toast.error(e?.message ?? String(e));
  } finally {
    busy.value = null;
  }
}

function startAdd() {
  addingAccount.value = !addingAccount.value;
  if (addingAccount.value) {
    // 默认选中第一个还没绑定的平台
    const first = statuses.value.find((s) => !s.bound) ?? statuses.value[0];
    if (first) addPlatform.value = first.platform;
  }
}
const addOptions = computed(() =>
  (statuses.value.length
    ? statuses.value.map((s) => ({ id: s.platform, name: s.label, bound: s.bound }))
    : MEDIA_PLATFORMS.map((p) => ({ id: p.id, name: p.name, bound: false })))
);

function checkAll() {
  loadStatus();
  toast.info("已重新探测 9 平台登录态（account-keeper 逐账号体检需 --account 支持，工程中）");
}

const dispatchHtml = computed(() => {
  let h = `<section><h3>分布式发送：主稿一次过审，矩阵账号发「微调变体」</h3><div class="flow">
      <div class="step">① 主稿过审<small>正常走质检门禁 + HITL</small></div><span class="arr">→</span>
      <div class="step evo">② 变体生成<small>writer 按账号补丁改<b>标题+首段+结尾 CTA</b>，正文主体不动</small></div><span class="arr">→</span>
      <div class="step">③ 一次审批整个计划<small>主稿 + N 变体 <b>diff 一屏看完</b>，逐变体可勾掉</small></div><span class="arr">→</span>
      <div class="step">④ 错峰投递<small>同平台账号间隔 ≥30min，窗口 8–22 点</small></div><span class="arr">→</span>
      <div class="step ok">⑤ 逐账号存草稿+保窗<small>publish_log 逐账号留痕，发布仍由人点</small></div></div></section>
      <section><div class="card"><h3>分发计划队列</h3><div class="tbl-wrap"><table>
      <tr><th>计划</th><th>主稿</th><th>投放</th><th>变体策略</th><th>错峰排期</th><th>状态</th></tr>`;
  h += MOCK.dispatch.map((r) => `<tr>${r.map((c, i) => `<td>${i === 0 ? `<code>${c}</code>` : c}</td>`).join("")}</tr>`).join("");
  h += `</table></div><p class="foot">未登录的矩阵号自动跳过并在状态里注明（如 DP-0714-03），不阻塞整个计划——降级不崩溃的老规矩。</p></div></section>`;
  return h;
});

const risk2Html = `<section><div class="card"><h3>多账号风控红线（写进代码，不指望自觉）</h3><div class="tbl-wrap"><table>
      <tr><th style="width:210px">红线</th><th>为什么 / 怎么拦</th></tr>
      <tr><td><b>同稿不同账号必须差异化</b></td><td>标题+开头原样多发=判搬运/限流；变体生成是<b>强制步</b>，跳过则计划不能进审批（error 级）</td></tr>
      <tr><td><b>错峰间隔 ≥30min</b></td><td>同平台多账号同时发布是最容易被关联的行为特征；排期器硬约束</td></tr>
      <tr><td><b>单账号日配额独立计数</b></td><td>全局配额（5 篇/日）之下每账号再各有上限——配额检查在 dispatch 前原子扣减（事故一的教训）</td></tr>
      <tr><td><b>cookie 隔离</b></td><td>每账号独立 persistent profile + 独立 CDP 端口，浏览器层不串；⚠ <b>IP 同源是残余风险</b>，矩阵号数量建议克制（≤3/平台）</td></tr>
      <tr><td><b>绝不自动发布</b></td><td>矩阵号也一样：只进草稿箱/保窗，发布键逐账号由人点——L3 代码级，无例外</td></tr>
      </table></div></div></section>
      <section><div class="callout r"><b>别把矩阵做成灌水机</b>：多样性锁（同话题当天 >2 篇告警）在账号维度同样生效——矩阵的意义是<b>覆盖不同标题角度</b>吃更多入口，不是同一句话喊三遍。</div></section>`;
</script>

<template>
  <div>
    <div v-html="head"></div>

    <template v-if="props.sub === 'roster'">
      <div class="callout y">
        <b>引擎现状（诚实声明）</b>：每平台一个<b>主号</b> profile，登录态、最近活动、绑定状态均为后端实时探测的真值。
        矩阵多账号需给 draft_uploader 加 <code>--account</code> 参数（profile 目录与 CDP 端口按账号分配），改动很小但<b>尚未实现</b>。
      </div>
      <section>
        <div class="card">
          <h3>账号总表（{{ statuses.length || MEDIA_PLATFORMS.length }} 个平台 / {{ boundCount }} 个已绑定）</h3>
          <div class="tbl-wrap">
            <table>
              <tr>
                <th>平台</th><th>角色</th><th>登录态</th><th>最近活动</th>
                <th>profile（登录态目录）</th><th>操作</th>
              </tr>
              <tr v-if="!statuses.length">
                <td colspan="6" style="color: var(--muted)">
                  {{ loadFailed ? "登录态探测失败——后端命令不可用，点「全矩阵体检」重试" : "探测登录态中…" }}
                </td>
              </tr>
              <tr v-for="s in statuses" :key="s.platform">
                <td><b>{{ s.label }}</b></td>
                <td><span class="badge b-full">主号</span></td>
                <td>
                  <span class="sline" :title="s.detail">
                    <span class="sdot" :class="s.bound ? 'ok' : 'idle'"></span>{{ s.bound ? "已绑定" : "未登录" }}
                  </span>
                </td>
                <td>{{ fmtActive(s.lastActive) }}</td>
                <td><code :title="s.profileDir">{{ s.profileDir }}</code></td>
                <td style="white-space: nowrap">
                  <button class="btn sm" :disabled="busy === `${s.platform}:login`" @click="openWindow(s.platform, 'login')">
                    <span v-if="busy === `${s.platform}:login`" class="spin" style="margin-right: 4px">◔</span>打开登录
                  </button>
                  <button v-if="s.bound" class="btn sm ghost" style="margin-left: 6px" :disabled="busy === `${s.platform}:draft`" @click="openWindow(s.platform, 'draft')">发文窗口</button>
                  <button v-if="s.bound" class="btn sm ghost" style="margin-left: 6px" :disabled="busy === `${s.platform}:forget`" @click="forget(s)">解绑</button>
                </td>
              </tr>
            </table>
          </div>
          <div style="margin-top: var(--space-xs); display: flex; gap: 8px; flex-wrap: wrap; align-items: center">
            <button class="btn sm" title="选平台后开登录窗扫码即挂号（每平台一个主号；矩阵号待 --account 支持）" @click="startAdd">＋ 添加账号</button>
            <template v-if="addingAccount">
              <select v-model="addPlatform" class="inp" style="width: auto; padding: 4px 8px">
                <option v-for="o in addOptions" :key="o.id" :value="o.id">{{ o.name }}{{ o.bound ? "（已绑定）" : "" }}</option>
              </select>
              <button class="btn sm" :disabled="!!busy" @click="openWindow(addPlatform, 'login'); addingAccount = false">开登录窗扫码</button>
            </template>
            <button class="btn sm ghost" @click="checkAll">全矩阵体检</button>
          </div>
          <p class="foot">主号养权重、发主稿；矩阵号发差异化变体（待 --account 支持）。扫码后窗口自己关掉即可——登录态永久保留在 profile 目录，本页会自动探测到并更新。</p>
        </div>
      </section>
    </template>

    <div v-else-if="props.sub === 'dispatch'" v-html="dispatchHtml"></div>
    <div v-else v-html="risk2Html"></div>
  </div>
</template>
