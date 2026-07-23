<script setup lang="ts">
/**
 * 平台门户：header/board/team/blockers/style 移植设计稿；
 * 选题·题库（qbank）与规划队列（plan）接真 mediaOps；账号·发送方式（acct）接真
 * mediaAccounts + mediaOps 设置；专家团补丁「编辑补丁」→ ExpertPromptDrawer（锁定本平台）。
 */
import { ref, computed, watch, onMounted } from "vue";
import {
  portalHeaderHtml, portalBoardHtml, portalBlockersHtml, portalStyleHtml, portalTeamHtml,
} from "./render";
import { P, MOCK, sdot } from "./data";
import {
  mediaOps, mediaAccounts, mediaJob, MEDIA_PLATFORMS,
  type MediaTopic, type MediaQueueItem, type MediaPlatformSettings, type MediaAccountStatus, type MediaPlatform, type MediaJob,
} from "../../tauri";
import { toast } from "../../composables/useToast";
import ExpertPromptDrawer from "./ExpertPromptDrawer.vue";
import { openJobDetail, openJobId } from "./jobsBus";
import { requestPlan } from "./planBus";

const props = defineProps<{ sub: string; platform: string }>();

const REAL = MEDIA_PLATFORMS.map((p) => p.id) as string[];
const isReal = computed(() => REAL.includes(props.platform));
const plat = computed(() => props.platform as MediaPlatform);
const pname = computed(() => P(props.platform)?.name ?? props.platform);

const head = computed(() => portalHeaderHtml(props.platform));
const boardHtml = computed(() => portalBoardHtml(props.platform));
const blockersHtml = computed(() => portalBlockersHtml(props.platform));
const styleHtml = computed(() => portalStyleHtml(props.platform));
const teamHtml = computed(() => portalTeamHtml(props.platform));

// ── 真数据 ──
const topics = ref<MediaTopic[]>([]);
const queue = ref<MediaQueueItem[]>([]);
const settings = ref<MediaPlatformSettings[]>([]);
const accts = ref<MediaAccountStatus[]>([]);
const newTopic = ref({ title: "", angle: "", keywords: "" });

async function loadState() {
  if (!isReal.value) { topics.value = []; queue.value = []; settings.value = []; return; }
  try {
    const s = await mediaOps.state();
    topics.value = (s.topics ?? []).filter((t) => t.platform === plat.value);
    queue.value = (s.queue ?? []).filter((q) => q.platform === plat.value);
    settings.value = s.settings ?? [];
  } catch {
    topics.value = []; queue.value = []; settings.value = [];
  }
}
async function loadAccts() {
  try { accts.value = await mediaAccounts.status(); } catch { accts.value = []; }
}
async function loadJobs() {
  try { jobs.value = await mediaJob.list(); } catch { jobs.value = []; }
}
onMounted(() => { loadState(); loadAccts(); loadJobs(); });
watch(() => props.platform, () => { loadState(); loadJobs(); newTopic.value = { title: "", angle: "", keywords: "" }; });
// 详情抽屉里可能取消/重跑——关抽屉时刷新队列与 job 映射
watch(openJobId, (v) => { if (!v) { loadState(); loadJobs(); } });

// ── 流程打通：选题 → 队列 → 流水线 job → 点进生成流程 ──
const jobs = ref<MediaJob[]>([]);
/** 每个队列项对应的最新 job（详情入口） */
const jobByQueue = computed(() => {
  const m: Record<string, MediaJob> = {};
  for (const j of [...jobs.value].sort((a, b) => a.createdAt - b.createdAt)) {
    if (j.queueId) m[j.queueId] = j;
  }
  return m;
});
const producing = ref<string | null>(null); // 正在排产/启动的 topic 或 queue id

/**
 * 选题点「生成→投递」：不再立刻排产，先把规划请求投给对话框——
 * 对话坞里流式出一份撰写规划，人看完点「开始」才真跑，点「否决」就作罢。
 */
function produceTopic(t: MediaTopic) {
  if (producing.value) return;
  requestPlan({
    laneKey: `portal:${plat.value}`,
    platform: plat.value,
    platformName: pname.value,
    title: t.title,
    angle: t.angle || undefined,
    keywords: t.keywords && t.keywords.length ? t.keywords : undefined,
    onApprove: () => doProduce(t),
  });
}

/** 真正排产：入队 → 标记 picked → 启动全链路 job。记录落在对话框，不再自动弹抽屉。 */
async function doProduce(t: MediaTopic): Promise<{ jobId: string }> {
  producing.value = t.id;
  try {
    const q = await mediaOps.queueAdd(plat.value, t.title, t.id);
    await mediaOps.topicUpdate(t.id, { status: "picked" }).catch(() => {});
    const j = await mediaJob.start({ queueId: q.id, topic: t.angle || undefined });
    toast.success(`已排产并启动流水线（job ${j.id.slice(0, 8)}）`);
    await loadState(); await loadJobs();
    return { jobId: j.id };
  } finally { producing.value = null; }
}

/** 队列项手动跑一条全链路 job */
async function runQueueItem(q: MediaQueueItem) {
  if (producing.value) return;
  producing.value = q.id;
  try {
    const j = await mediaJob.start({ queueId: q.id });
    toast.success(`流水线已启动（job ${j.id.slice(0, 8)}）`);
    openJobDetail(j.id);
    await loadState(); await loadJobs();
  } catch (e: any) {
    toast.error(e?.message ?? String(e));
  } finally { producing.value = null; }
}

const platSettings = computed(() =>
  settings.value.find((s) => s.platform === plat.value)
);
const sendMode = computed<"ai" | "manual">(() => {
  const s = platSettings.value?.sendMode;
  if (s) return s;
  return P(props.platform)?.sendMode === "manual" ? "manual" : "ai";
});

async function addTopic() {
  const t = newTopic.value.title.trim();
  if (!t || !isReal.value) return;
  const kws = newTopic.value.keywords.split(/[,，、\s]+/).map((s) => s.trim()).filter(Boolean);
  try {
    const created = await mediaOps.topicAdd(plat.value, t, newTopic.value.angle.trim(), kws, "manual");
    topics.value = [created, ...topics.value];
    newTopic.value = { title: "", angle: "", keywords: "" };
    toast.success("已加入选题池");
  } catch (e: any) {
    toast.error(e?.message ?? String(e));
  }
}
async function delTopic(id: string) {
  try { await mediaOps.topicDelete(id); topics.value = topics.value.filter((t) => t.id !== id); } catch (e: any) { toast.error(e?.message ?? String(e)); }
}
async function toggleSend() {
  if (!isReal.value) return;
  const next: "ai" | "manual" = sendMode.value === "ai" ? "manual" : "ai";
  try {
    const updated = await mediaOps.settingsSet(plat.value, { sendMode: next });
    const exists = settings.value.some((x) => x.platform === updated.platform);
    settings.value = exists ? settings.value.map((x) => (x.platform === updated.platform ? updated : x)) : [...settings.value, updated];
    toast.info(next === "ai" ? "已切到 AI 直传草稿箱" : "已切到手动辅助");
  } catch (e: any) {
    toast.error(e?.message ?? String(e));
  }
}
const acctBusy = ref(false);
async function openLogin() {
  if (!isReal.value) return;
  acctBusy.value = true;
  try {
    const r = await mediaAccounts.open(plat.value, "login");
    toast.info(r?.message ?? "已打开登录窗口，扫码后关闭即可");
    setTimeout(loadAccts, 800);
  } catch (e: any) { toast.error(e?.message ?? String(e)); } finally { acctBusy.value = false; }
}
const platAcct = computed(() => accts.value.find((a) => a.platform === plat.value));

// 探测题库（该平台）— 设计稿 mock
const probeRows = computed(() => MOCK.questions[props.platform] ?? []);

// 规划队列静态排期区块
const planStaticHtml = computed(() => {
  const p = P(props.platform)!;
  return `<section><div class="card"><h3>排期设置</h3><div class="tbl-wrap"><table>
    <tr><th style="width:110px">项</th><th>当前</th><th>说明</th></tr>
    <tr><td><b>周篇数</b></td><td class="num-cell">${p.weekPlan} 篇/周</td><td>发布窗口 8–22 点；全局日配额 5 篇硬锁（事故一防线）</td></tr>
    <tr><td><b>审批档位</b></td><td><code>manual</code></td><td>连续 4 周零事故可放权到 <code>auto_draft</code>；「自动对外发布」永不开放（L3）</td></tr>
    </table></div></div></section>
    <section><div class="card"><h3>该平台的 cron 节奏与策略变更提案</h3><div class="tbl-wrap"><table>
    <tr><th>时间</th><th>任务</th><th>状态</th></tr>
    <tr><td>每天 02:00</td><td>选题规划（0–2 篇）</td><td>${sdot("ok", "正常")}</td></tr>
    <tr><td>每 30 分钟</td><td>流水线推进（到审批态停）</td><td>${sdot("ok", "正常")}</td></tr>
    <tr><td>每周一 05:00</td><td>AI 引用探测（${p.ai}）</td><td>${p.login === "ok" ? sdot("ok", "W28 完成") : p.login === "none" ? sdot("idle", "未接入") : sdot("warn", "网络受阻")}</td></tr>
    </table></div>
    <div class="callout" style="margin-top:10px">${props.platform === "zhihu" ? "<b>本平台有活跃提案</b>：探测发现「知乎被引率连涨 3 周」→ 提案周篇数 3→4（<span class='badge b-l1'>L1</span> 已自动生效，观察期至 07-23）。" : "该平台本周无策略变更提案。"} <a class="glnk" data-go="autopilot">进自动规划 →</a></div></div></section>`;
});

// ── 看板真实泳道：选题池 + 队列 + job 按状态分列，每张卡点开真实生成流程 ──
type LaneCard = { key: string; title: string; jobId?: string; note?: string; topic?: MediaTopic; hot?: boolean };
const lanes = computed<{ name: string; cards: LaneCard[] }[]>(() => {
  const jb = jobByQueue.value;
  const qCard = (q: MediaQueueItem, hot = false): LaneCard => ({
    key: q.id, title: q.title, jobId: jb[q.id]?.id, note: q.note || undefined, hot,
  });
  const byStatus = (st: MediaQueueItem["status"]) => queue.value.filter((q) => q.status === st);
  const running = queue.value.filter((q) => q.status === "running"
    || (jb[q.id] && (jb[q.id].status === "running" || jb[q.id].status === "pending") && q.status === "queued"));
  const runningIds = new Set(running.map((q) => q.id));
  return [
    { name: "选题池", cards: topics.value.filter((t) => t.status !== "picked").map((t) => ({ key: t.id, title: t.title, topic: t })) },
    { name: "排队中", cards: byStatus("queued").filter((q) => !runningIds.has(q.id)).map((q) => qCard(q)) },
    { name: "流水线在跑", cards: running.map((q) => qCard(q, true)) },
    { name: "草稿已投·待人预览", cards: byStatus("draft_uploaded").map((q) => qCard(q, true)) },
    { name: "完成", cards: byStatus("done").map((q) => qCard(q)) },
    { name: "失败", cards: byStatus("failed").map((q) => qCard(q)) },
  ];
});
function laneCardClick(c: LaneCard) {
  if (c.jobId) { openJobDetail(c.jobId); return; }
  if (c.topic) { produceTopic(c.topic); return; }
  const q = queue.value.find((x) => x.id === c.key);
  if (q) runQueueItem(q);
}

// 门户「编辑补丁」→ 抽屉（锁定本平台）
const editingExpert = ref<string | null>(null);
function onClick(e: MouseEvent) {
  const el = (e.target as HTMLElement)?.closest?.("[data-act='edit-overlay']") as HTMLElement | null;
  if (el) editingExpert.value = el.dataset.expert ?? null;
}
</script>

<template>
  <div @click="onClick">
    <div v-html="head"></div>

    <template v-if="props.sub === 'board'">
      <section>
        <div class="lanes">
          <div v-for="l in lanes" :key="l.name" class="lane">
            <h5>{{ l.name }}<span class="cnt">{{ l.cards.length }}</span></h5>
            <div
              v-for="c in l.cards"
              :key="c.key"
              class="draft"
              :title="c.jobId ? '点击查看这条流程的生成过程' : c.topic ? '点击：先在对话框出规划，再定夺开始/否决' : '点击：启动全链路流水线'"
              @click="laneCardClick(c)"
            >
              {{ c.title }}
              <div class="tags">
                <span class="tag">{{ pname }}</span>
                <span v-if="c.hot" class="tag hot">{{ c.jobId ? "点开看进度" : "等点头" }}</span>
                <span v-else-if="c.topic" class="tag">▶ 生成→投递</span>
                <span v-if="c.note" class="tag">{{ c.note }}</span>
              </div>
            </div>
            <div v-if="!l.cards.length" style="color: var(--muted); font-size: var(--text-2xs); padding: 5px 2px">（空）</div>
          </div>
        </div>
      </section>
      <div v-html="boardHtml"></div>
    </template>
    <div v-else-if="props.sub === 'blockers'" v-html="blockersHtml"></div>
    <div v-else-if="props.sub === 'style'" v-html="styleHtml"></div>
    <div v-else-if="props.sub === 'team'" v-html="teamHtml"></div>

    <!-- 选题·题库（接真 mediaOps 选题池 + 探测 mock） -->
    <template v-else-if="props.sub === 'qbank'">
      <section>
        <div class="card">
          <h3>{{ pname }}·选题池（{{ topics.length }}）{{ isReal ? "" : "— 该平台待接入，选题池只读" }}</h3>
          <div v-if="isReal" style="display: flex; gap: 8px; flex-wrap: wrap; margin-bottom: 10px">
            <input v-model="newTopic.title" class="inp" style="flex: 2; min-width: 180px" placeholder="选题标题" @keydown.enter="addTopic" />
            <input v-model="newTopic.angle" class="inp" style="flex: 1; min-width: 120px" placeholder="切入角度（可选）" />
            <input v-model="newTopic.keywords" class="inp" style="flex: 1; min-width: 120px" placeholder="关键词，逗号分隔" />
            <button class="btn" @click="addTopic">＋ 加入</button>
          </div>
          <div v-if="!topics.length" class="foot">选题池为空{{ isReal ? "，加一个，或让选题雷达抓几个" : "" }}。</div>
          <div v-else class="tbl-wrap">
            <table>
              <tr><th>标题</th><th>角度</th><th>关键词</th><th>状态</th><th></th></tr>
              <tr v-for="t in topics" :key="t.id">
                <td><b>{{ t.title }}</b></td>
                <td>{{ t.angle || "—" }}</td>
                <td>{{ (t.keywords || []).join("、") || "—" }}</td>
                <td>{{ t.status }}</td>
                <td style="white-space: nowrap">
                  <button class="btn sm" :disabled="!!producing" title="先在对话框里出一份撰写规划，你点「开始」才入队跑 生成→排版→投递 全链路" @click="produceTopic(t)">
                    <span v-if="producing === t.id" class="spin" style="margin-right: 4px">◔</span>生成→投递
                  </button>
                  <button class="btn sm danger" style="margin-left: 6px" @click="delTopic(t.id)">删除</button>
                </td>
              </tr>
            </table>
          </div>
        </div>
      </section>
      <section>
        <div class="card">
          <h3>探测题（上次五引擎探测结果）</h3>
          <div class="tbl-wrap">
            <table>
              <tr><th>问题</th><th>主打引擎</th><th>上次探测</th><th>归入清单</th></tr>
              <tr v-if="!probeRows.length"><td colspan="4" style="color: var(--muted)">（该平台题库为空）</td></tr>
              <tr v-for="(r, i) in probeRows" :key="i"><td v-for="(c, j) in r" :key="j">{{ c }}</td></tr>
            </table>
          </div>
          <div style="margin-top: 8px"><span class="btn sm ghost" data-go="questions" data-gosub="lists">看三张清单 →</span></div>
        </div>
      </section>
    </template>

    <!-- 规划队列（接真 mediaOps 队列 + 静态排期） -->
    <template v-else-if="props.sub === 'plan'">
      <div v-html="planStaticHtml"></div>
      <section>
        <div class="card">
          <h3>{{ pname }}·规划队列（{{ queue.length }}）</h3>
          <div v-if="!queue.length" class="foot">队列为空。在选题池点「生成→投递」即可排产登记到这里。</div>
          <div v-else class="tbl-wrap">
            <table>
              <tr><th>标题</th><th>状态</th><th>排期</th><th>备注</th><th>流程</th></tr>
              <tr v-for="q in queue" :key="q.id" :style="jobByQueue[q.id] ? 'cursor:pointer' : ''" :title="jobByQueue[q.id] ? '点击查看生成流程' : ''" @click="jobByQueue[q.id] && openJobDetail(jobByQueue[q.id].id)">
                <td><b>{{ q.title }}</b></td><td>{{ q.status }}</td><td>{{ q.scheduledAt || "—" }}</td><td>{{ q.note || "—" }}</td>
                <td style="white-space: nowrap">
                  <button v-if="jobByQueue[q.id]" class="btn sm ghost" @click.stop="openJobDetail(jobByQueue[q.id].id)">生成流程</button>
                  <button
                    v-if="!jobByQueue[q.id] || ['done', 'failed', 'canceled'].includes(jobByQueue[q.id].status)"
                    class="btn sm" style="margin-left: 6px" :disabled="!!producing"
                    :title="'启动 生成→排版→投递 全链路'" @click.stop="runQueueItem(q)"
                  ><span v-if="producing === q.id" class="spin" style="margin-right: 4px">◔</span>▶ 跑流水线</button>
                </td>
              </tr>
            </table>
          </div>
        </div>
      </section>
    </template>

    <!-- 账号·发送方式（接真登录态 + 发送模式） -->
    <template v-else-if="props.sub === 'acct'">
      <section>
        <div class="grid g2">
          <div class="card">
            <h3>本平台账号（浏览器登录态，扫码一次长期有效）</h3>
            <div v-if="platAcct" class="tbl-wrap">
              <table>
                <tr><th>账号</th><th>登录态</th><th>profile</th></tr>
                <tr>
                  <td><b>{{ platAcct.label || pname }}</b></td>
                  <td><span class="sline"><span class="sdot" :class="platAcct.bound ? 'ok' : 'idle'"></span>{{ platAcct.bound ? "已登录" : "未登录" }}</span></td>
                  <td><code>{{ platAcct.profileDir }}</code></td>
                </tr>
              </table>
            </div>
            <p v-else class="foot">读取登录态中…（{{ isReal ? "后端未就绪时为空" : "平台待接入" }}）</p>
            <div style="margin-top: var(--space-xs); display: flex; gap: 8px; flex-wrap: wrap">
              <button class="btn sm" :disabled="acctBusy || !isReal" @click="openLogin"><span v-if="acctBusy" class="spin" style="margin-right: 4px">◔</span>扫码登录 / 续期</button>
              <button class="btn sm ghost" data-go="accounts">账号矩阵 →</button>
              <button class="btn sm ghost" data-go="accounts" data-gosub="dispatch">分布式发送 →</button>
            </div>
            <p class="foot">监控：account-keeper 每周一 08:00 全平台逐账号体检。</p>
          </div>
          <div class="card">
            <h3>发送方式（平台级配置）</h3>
            <div class="switch">
              <button :class="{ on: sendMode === 'ai' }" @click="sendMode !== 'ai' && toggleSend()">AI 直传草稿箱</button>
              <button :class="{ on: sendMode === 'manual' }" @click="sendMode !== 'manual' && toggleSend()">手动辅助</button>
            </div>
            <ul style="margin-top: 9px">
              <li><b>AI 直传</b>：自动填标题 → 粘贴正文 → 配图 → 存草稿 → <b>保窗供你预览</b>；</li>
              <li><b>手动辅助</b>：只打开平台后台并把标题正文备进系统剪贴板，你 Ctrl+V 一贴完事；</li>
              <li>任何一步失败<b>自动降级手动辅助</b>，绝不崩溃甩锅。</li>
            </ul>
          </div>
        </div>
      </section>
    </template>

    <ExpertPromptDrawer
      v-if="editingExpert"
      :expert-id="editingExpert"
      :platform="props.platform"
      lock-platform
      @close="editingExpert = null"
    />
  </div>
</template>
