<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { vEngineHtml } from "./render";
import { mediaJob, type MediaJob } from "../../tauri";
const props = defineProps<{ sub: string; platform: string }>();
const html = computed(() => vEngineHtml(props.sub));

// ── 全链路 job 试跑（media_engine.rs：generate→typeset→upload） ──
const PLATFORMS: [string, string][] = [
  ["wechat", "公众号"], ["zhihu", "知乎"], ["toutiao", "头条号"], ["baijia", "百家号"],
  ["xhs", "小红书"], ["bilibili", "B站专栏"], ["douyin", "抖音图文"],
];
const platformSel = ref("toutiao");
const title = ref("");
const topic = ref("");
const stGenerate = ref(true);
const stTypeset = ref(true);
const stUpload = ref(true);
const jobs = ref<MediaJob[]>([]);
const busy = ref(false);
const msg = ref("");
let timer: ReturnType<typeof setInterval> | null = null;

async function refreshJobs() {
  try {
    jobs.value = await mediaJob.list();
    // 有跑着的 job 就维持轮询，否则停表。
    const anyRunning = jobs.value.some((j) => j.status === "running" || j.status === "pending");
    if (anyRunning && !timer) timer = setInterval(refreshJobs, 3000);
    if (!anyRunning && timer) { clearInterval(timer); timer = null; }
  } catch { /* 后端不可用（纯浏览器预览）时静默 */ }
}

async function start() {
  if (!title.value.trim()) { msg.value = "标题必填"; return; }
  const stages = [
    ...(stGenerate.value ? ["generate"] : []),
    ...(stTypeset.value ? ["typeset"] : []),
    ...(stUpload.value ? ["upload"] : []),
  ];
  if (!stages.length) { msg.value = "至少选一个阶段"; return; }
  busy.value = true; msg.value = "";
  try {
    const j = await mediaJob.start({ platform: platformSel.value, title: title.value, topic: topic.value, stages });
    msg.value = `job ${j.id} 已启动（日志：${j.logPath}）`;
    await refreshJobs();
  } catch (e) { msg.value = String(e); } finally { busy.value = false; }
}

async function cancel(id: string) {
  try { await mediaJob.cancel(id); await refreshJobs(); } catch (e) { msg.value = String(e); }
}

const STATUS_DOT: Record<string, string> = { running: "warn", pending: "idle", done: "ok", failed: "bad", canceled: "idle" };

onMounted(refreshJobs);
onBeforeUnmount(() => { if (timer) clearInterval(timer); });
</script>
<template>
  <div>
    <div class="card" style="margin-bottom:12px">
      <h3>全链路试跑（生成 → 排版 → 上传草稿，只进草稿箱绝不发布）</h3>
      <div style="display:flex;gap:8px;flex-wrap:wrap;margin-top:6px;align-items:center">
        <select v-model="platformSel" class="geo-input">
          <option v-for="[id, name] in PLATFORMS" :key="id" :value="id">{{ name }}</option>
        </select>
        <input v-model="title" class="geo-input" placeholder="文章标题" style="flex:1;min-width:200px" />
        <input v-model="topic" class="geo-input" placeholder="选题/角度（可空）" style="flex:1;min-width:160px" />
      </div>
      <div style="display:flex;gap:14px;margin-top:8px;font-size:14px;color:var(--ink2);align-items:center;flex-wrap:wrap">
        <label><input type="checkbox" v-model="stGenerate" /> generate（Claude 生成）</label>
        <label><input type="checkbox" v-model="stTypeset" /> typeset（排版）</label>
        <label><input type="checkbox" v-model="stUpload" /> upload（进草稿箱）</label>
        <button class="btn sm" :disabled="busy" @click="start">启动 job</button>
        <button class="btn sm ghost" @click="refreshJobs">刷新</button>
      </div>
      <p v-if="msg" style="margin-top:6px;font-size:13px;color:var(--dim)">{{ msg }}</p>
      <div v-if="jobs.length" class="tbl-wrap" style="margin-top:10px">
        <table>
          <tr><th>job</th><th>平台</th><th>标题</th><th>阶段</th><th>状态</th><th>产物/日志</th><th></th></tr>
          <tr v-for="j in jobs" :key="j.id">
            <td style="font-variant-numeric:tabular-nums">{{ j.id.slice(0, 8) }}</td>
            <td>{{ j.platform }}</td>
            <td>{{ j.title }}</td>
            <td>{{ j.stage || j.stages.join("→") }}</td>
            <td><span class="sline"><span class="sdot" :class="STATUS_DOT[j.status] || 'idle'"></span>{{ j.status }}</span><span v-if="j.error" style="color:var(--bad)">：{{ j.error }}</span></td>
            <td style="font-size:12px;color:var(--muted);max-width:280px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap" :title="(j.articlePath || '') + ' ' + j.logPath">{{ j.articlePath || j.logPath }}</td>
            <td><button v-if="j.status === 'running' || j.status === 'pending'" class="btn sm danger" @click="cancel(j.id)">取消</button></td>
          </tr>
        </table>
      </div>
    </div>
    <div v-html="html"></div>
  </div>
</template>
<style scoped>
.geo-input {
  background: var(--code-bg, #0b0e1a);
  border: 1px solid var(--line, #2a3050);
  color: var(--ink, #e8eaf6);
  border-radius: 8px;
  padding: 6px 10px;
  font-size: 14px;
  font-family: inherit;
}
.geo-input:focus-visible { outline: 2px solid var(--focus, #8fa6ff); outline-offset: 1px; }
</style>
