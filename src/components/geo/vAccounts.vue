<script setup lang="ts">
/** 账号矩阵：账号总表接真登录态（media_accounts_status / media_account_open）；分发与风控红线为设计稿静态。 */
import { ref, computed, onMounted } from "vue";
import { title } from "./render";
import { MOCK, P } from "./data";
import { mediaAccounts, type MediaAccountStatus, type MediaPlatform } from "../../tauri";
import { toast } from "../../composables/useToast";

const props = defineProps<{ sub: string; platform: string }>();

const head = computed(() =>
  title("账号矩阵", "资源 / 多账号管理与分布式发送 —— 每账号独立 profile + 独立 CDP 端口，cookie 互不串")
);

const statuses = ref<MediaAccountStatus[]>([]);
const busy = ref<string | null>(null);
const addingAccount = ref(false);
const addPlatform = ref("wechat");

async function loadStatus() {
  try {
    statuses.value = await mediaAccounts.status();
  } catch {
    statuses.value = [];
  }
}
onMounted(loadStatus);

const boundMap = computed(() => {
  const m = new Map<string, boolean>();
  for (const s of statuses.value) m.set(s.platform, s.bound);
  return m;
});

interface Row {
  pid: string; pname: string; name: string; role: string;
  cls: string; txt: string; profile: string; port: string; quota: number; recent7: number; real: boolean;
}
const rows = computed<Row[]>(() =>
  MOCK.accounts.map((a) => {
    const pid = a[0];
    let login = a[3];
    const rb = boundMap.value.get(pid);
    // 主号登录态用后端真值覆盖（后端按平台探测，对应主号 profile）
    const real = a[2] === "主号" && rb !== undefined;
    if (real) login = rb ? "ok" : "none";
    const lv = login === "ok" ? { cls: "ok", txt: "正常" } : login === "warn" ? { cls: "warn", txt: "待处理" } : { cls: "idle", txt: "未登录" };
    return { pid, pname: P(pid)?.name ?? pid, name: a[1], role: a[2], cls: lv.cls, txt: lv.txt, profile: a[4], port: String(a[5]), quota: a[6], recent7: a[7], real };
  })
);
const platformCount = computed(() => new Set(MOCK.accounts.map((a) => a[0])).size);

async function openLogin(pid: string) {
  busy.value = pid;
  try {
    const r = await mediaAccounts.open(pid as MediaPlatform, "login");
    toast.info(r?.message ?? "已打开登录窗口，扫码后关闭即可，登录态永久保留");
    setTimeout(loadStatus, 800);
  } catch (e: any) {
    toast.error(e?.message ?? String(e));
  } finally {
    busy.value = null;
  }
}
function checkAll() {
  loadStatus();
  toast.info("已按平台重新探测登录态（account-keeper 逐账号体检需 --account 支持，工程中）");
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
        <b>引擎现状（诚实声明）</b>：draft_uploader 目前按<b>平台</b>取 profile（每平台一个）。多账号需给它加
        <code>--account</code> 参数——profile 目录与 CDP 端口按账号分配，其余逻辑复用。改动很小，但<b>尚未实现</b>；
        本页登录态已接真（主号取后端探测值）。
      </div>
      <section>
        <div class="card">
          <h3>账号总表（{{ MOCK.accounts.length }} 个账号 / {{ platformCount }} 个平台）</h3>
          <div class="tbl-wrap">
            <table>
              <tr>
                <th>平台</th><th>账号</th><th>角色</th><th>登录态</th><th>profile（每账号独立）</th>
                <th>CDP 端口</th><th class="num-cell">日配额</th><th class="num-cell">近7天</th><th>操作</th>
              </tr>
              <tr v-for="(r, i) in rows" :key="i">
                <td>{{ r.pname }}</td>
                <td><b>{{ r.name }}</b></td>
                <td><span class="badge" :class="r.role === '主号' ? 'b-full' : 'b-ghost'">{{ r.role }}</span></td>
                <td><span class="sline"><span class="sdot" :class="r.cls"></span>{{ r.txt }}<span v-if="r.real" style="color: var(--muted); margin-left: 4px">· 实</span></span></td>
                <td><code>{{ r.profile }}</code></td>
                <td class="num-cell">{{ r.port }}</td>
                <td class="num-cell">{{ r.quota }}</td>
                <td class="num-cell">{{ r.recent7 }}</td>
                <td style="white-space: nowrap">
                  <button v-if="r.role === '主号'" class="btn sm" :disabled="busy === r.pid" @click="openLogin(r.pid)">
                    <span v-if="busy === r.pid" class="spin" style="margin-right: 4px">◔</span>打开登录
                  </button>
                  <span v-else style="color: var(--muted); font-size: var(--text-2xs)">矩阵（待 --account）</span>
                </td>
              </tr>
            </table>
          </div>
          <div style="margin-top: var(--space-xs); display: flex; gap: 8px; flex-wrap: wrap">
            <button class="btn sm" title="选平台后开登录窗扫码即挂号（每平台一个主号；矩阵号待 --account 支持）" @click="addingAccount = !addingAccount">＋ 添加账号</button>
            <template v-if="addingAccount">
              <select v-model="addPlatform" class="inp" style="width:auto;padding:4px 8px">
                <option v-for="r in rows" :key="r.pid" :value="r.pid">{{ r.pname }}</option>
              </select>
              <button class="btn sm" :disabled="!!busy" @click="openLogin(addPlatform); addingAccount = false">开登录窗扫码</button>
            </template>
            <button class="btn sm ghost" @click="checkAll">全矩阵体检</button>
          </div>
          <p class="foot">主号养权重、发主稿；矩阵号发差异化变体。待接入平台（CSDN/掘金/视频号）接入后在此挂账号。</p>
        </div>
      </section>
    </template>

    <div v-else-if="props.sub === 'dispatch'" v-html="dispatchHtml"></div>
    <div v-else v-html="risk2Html"></div>
  </div>
</template>
