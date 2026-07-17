<script setup lang="ts">
/**
 * 审批队列（真实数据）：mediaops 队列里已有正文产物（articlePath）的稿件在此走 HITL——
 *  - 「通过·投草稿箱」= 用该稿正文启动 typeset→upload 流水线 job，随即打开生成流程盯着跑；
 *  - 「打回」= 填评审意见，稿件退回 queued 泳道（意见写进 note，重跑时可见）；
 *  - 每行可点开对应 job 的完整生成流程（步骤留痕 + 日志 + 产物）。
 */
import { computed, onMounted, ref, watch } from "vue";
import { vApprovalsHtml } from "./render";
import { P } from "./data";
import { mediaOps, mediaJob, type MediaQueueItem, type MediaJob, type MediaPlatform } from "../../tauri";
import { toast } from "../../composables/useToast";
import { openJobDetail, openJobId } from "./jobsBus";

defineProps<{ sub: string; platform: string }>();

const queue = ref<MediaQueueItem[]>([]);
const jobs = ref<MediaJob[]>([]);
const busy = ref<string | null>(null);
const rejecting = ref<string | null>(null);
const rejectNote = ref("");

const staticHtml = vApprovalsHtml();

async function load() {
  try {
    const s = await mediaOps.state();
    queue.value = s.queue ?? [];
  } catch { queue.value = []; }
  try { jobs.value = await mediaJob.list(); } catch { jobs.value = []; }
}
onMounted(load);
watch(openJobId, (v) => { if (!v) load(); });

const jobByQueue = computed(() => {
  const m: Record<string, MediaJob> = {};
  for (const j of [...jobs.value].sort((a, b) => a.createdAt - b.createdAt)) {
    if (j.queueId) m[j.queueId] = j;
  }
  return m;
});

/** 待审 = 有正文产物、还没投出去的稿件（生成完等人点头）。 */
const pending = computed(() =>
  queue.value.filter((q) => q.articlePath && (q.status === "queued" || q.status === "running")));
/** 已投/终态的最近记录，供追溯。 */
const recent = computed(() =>
  queue.value.filter((q) => ["draft_uploaded", "done", "failed"].includes(q.status)).slice(0, 12));

function fmtTime(sec: number): string {
  const d = new Date(sec * 1000);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}
const pName = (id: string) => P(id)?.name ?? id;

async function approve(q: MediaQueueItem) {
  busy.value = q.id;
  try {
    const j = await mediaJob.start({
      queueId: q.id, platform: q.platform as MediaPlatform, title: q.title,
      stages: ["typeset", "upload"], articlePath: q.articlePath ?? undefined,
    });
    toast.success(`已通过：投递流水线启动（job ${j.id.slice(0, 8)}），只进草稿箱、窗口保留供预览`);
    openJobDetail(j.id);
    await load();
  } catch (e: any) {
    toast.error(e?.message ?? String(e));
  } finally { busy.value = null; }
}

async function reject(q: MediaQueueItem) {
  busy.value = q.id;
  try {
    await mediaOps.queueUpdate(q.id, { status: "queued", note: `打回：${rejectNote.value.trim() || "需修改"}` });
    toast.info("已打回，评审意见已写入稿件备注");
    rejecting.value = null; rejectNote.value = "";
    await load();
  } catch (e: any) {
    toast.error(e?.message ?? String(e));
  } finally { busy.value = null; }
}
</script>

<template>
  <div>
    <section>
      <div class="card">
        <h3>待审稿件（{{ pending.length }}）<button class="btn sm ghost" style="margin-left:auto" @click="load">刷新</button></h3>
        <p v-if="!pending.length" class="foot">暂无待审稿件。门户「选题·题库」里点「生成→投递」，生成完的稿件会出现在这里等你点头。</p>
        <div v-else class="tbl-wrap">
          <table>
            <tr><th>平台</th><th>标题</th><th>状态</th><th>更新</th><th>备注</th><th style="min-width:230px">操作</th></tr>
            <tr
              v-for="q in pending" :key="q.id"
              :style="jobByQueue[q.id] ? 'cursor:pointer' : ''"
              :title="jobByQueue[q.id] ? '点击查看生成流程' : ''"
              @click="jobByQueue[q.id] && openJobDetail(jobByQueue[q.id].id)"
            >
              <td>{{ pName(q.platform) }}</td>
              <td><b>{{ q.title }}</b></td>
              <td>{{ q.status }}</td>
              <td style="white-space:nowrap">{{ fmtTime(q.updatedAt) }}</td>
              <td>{{ q.note || "—" }}</td>
              <td style="white-space:nowrap" @click.stop>
                <template v-if="rejecting === q.id">
                  <input v-model="rejectNote" class="inp" style="width:160px" placeholder="评审意见" @keydown.enter="reject(q)" />
                  <button class="btn sm danger" style="margin-left:6px" :disabled="busy === q.id" @click="reject(q)">确认打回</button>
                  <button class="btn sm ghost" style="margin-left:6px" @click="rejecting = null">取消</button>
                </template>
                <template v-else>
                  <button class="btn sm" :disabled="!!busy" @click="approve(q)">通过·投草稿箱</button>
                  <button class="btn sm danger" style="margin-left:6px" :disabled="!!busy" @click="rejecting = q.id">打回</button>
                  <button v-if="jobByQueue[q.id]" class="btn sm ghost" style="margin-left:6px" @click="openJobDetail(jobByQueue[q.id].id)">生成流程</button>
                </template>
              </td>
            </tr>
          </table>
        </div>
      </div>
    </section>

    <section v-if="recent.length">
      <div class="card">
        <h3>最近已处理（点任意一条回放生成流程）</h3>
        <div class="tbl-wrap">
          <table>
            <tr><th>平台</th><th>标题</th><th>状态</th><th>更新</th></tr>
            <tr
              v-for="q in recent" :key="q.id"
              :style="jobByQueue[q.id] ? 'cursor:pointer' : ''"
              @click="jobByQueue[q.id] && openJobDetail(jobByQueue[q.id].id)"
            >
              <td>{{ pName(q.platform) }}</td><td>{{ q.title }}</td><td>{{ q.status }}</td>
              <td style="white-space:nowrap">{{ fmtTime(q.updatedAt) }}</td>
            </tr>
          </table>
        </div>
      </div>
    </section>

    <div v-html="staticHtml"></div>
  </div>
</template>
