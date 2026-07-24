<script setup lang="ts">
/**
 * 专家提示词编辑抽屉（真数据）：
 *  - expert_media_doc         看基础画像 + 当前平台拼接后的提示词
 *  - expert_media_overlay_get 读该平台补丁（runtime / seed / none）
 *  - expert_media_overlay_set 写该平台补丁（textarea 编辑 + 保存）
 * 用户点名的「各专家以及各影响结果的提示词可修改」的落点。
 *
 * 保存即版本化：overlaySet 之外还会 prompt_version_add 记一版 + evolution_add 留一张进化卡
 * （PRD v2 不变式⑥：任何 prompt 变更都要在进化时间线上留卡）。此前 save 只写 overlay，
 * 版本树从无写入点，「可回滚」是句空话；回滚也需人工把内容抄回专家文件——现在回滚直接写回。
 */
import { ref, watch, onMounted, computed } from "vue";
import {
  expertMedia,
  evolutionApi,
  MEDIA_PLATFORMS,
  type MediaPlatform,
  type PromptVersion,
} from "../../tauri";
import { toast } from "../../composables/useToast";
import { EXPERTS } from "./data";

/** 版本树锚点：整段平台补丁即一个可进化单元（与后端 ANCHOR_PLATFORM_OVERLAY 对齐）。 */
const ANCHOR = "platform_overlay";

const props = defineProps<{
  expertId: string;
  platform: string;
  /** 从门户打开时锁定平台（不显示平台切换器） */
  lockPlatform?: boolean;
}>();
const emit = defineEmits<{ (e: "close"): void }>();

const REAL = MEDIA_PLATFORMS.map((p) => p.id) as string[];
function isReal(p: string): p is MediaPlatform {
  return REAL.includes(p);
}

const expertName = computed(() => {
  const e = EXPERTS.find((x) => x[1] === props.expertId);
  return e ? `${e[1]} · ${e[2]}` : props.expertId;
});

/**
 * 流水线里真正把画像 + 补丁喂给模型的只有这两位：写作（generate）与配图（image）。
 * 排版是 Rust 确定性转换、投递是 Python 脚本，都不下发提示词——那两格的补丁写得再好
 * 也进不了任何一次模型调用。这里如实说明，别让人对着一个不生效的输入框调半天。
 */
const DRIVES_MODEL = ["media-writer", "media-imagedirector"];
const drivesModel = computed(() => DRIVES_MODEL.includes(props.expertId));

const plat = ref<string>(isReal(props.platform) ? props.platform : "wechat");
const doc = ref("");
const docErr = ref<string | null>(null);
const docLoading = ref(false);
const overlay = ref("");
const overlaySource = ref("none");
const saving = ref(false);

async function load() {
  doc.value = "";
  overlay.value = "";
  overlaySource.value = "none";
  docErr.value = null;
  if (!isReal(plat.value)) {
    docErr.value = "该平台尚未接入后端专家补丁通道（仅 7 个已接入平台支持读写）。";
    return;
  }
  docLoading.value = true;
  try {
    doc.value = await expertMedia.doc(props.expertId, plat.value);
  } catch (e: any) {
    docErr.value = "基础画像读取失败（后端开发中）：" + (e?.message ?? String(e));
  }
  try {
    const o = await expertMedia.overlayGet(plat.value, props.expertId);
    overlay.value = o?.content ?? "";
    overlaySource.value = o?.source ?? "none";
  } catch {
    overlay.value = "";
    overlaySource.value = "none";
  } finally {
    docLoading.value = false;
  }
}

/** 本专家 + 本平台 + 本锚点的版本历史，新版在前。 */
const versions = ref<PromptVersion[]>([]);
const versionsErr = ref("");
async function loadVersions() {
  versionsErr.value = "";
  try {
    const st = await evolutionApi.state();
    versions.value = st.promptVersions
      .filter(
        (v) =>
          v.expertId === props.expertId &&
          v.platform === plat.value &&
          v.anchor === ANCHOR,
      )
      .sort((a, b) => b.version - a.version);
  } catch (e: any) {
    versionsErr.value = e?.message ?? String(e);
  }
}

/**
 * 极简行级 diff：只标出「消失的行」与「新增的行」，用于进化卡上的可读留档。
 * 不做最小编辑距离——行内改动会显示成一删一增，对留档足够，别为它引依赖。
 */
function lineDiff(before: string, after: string): string {
  const A = before.split("\n");
  const B = after.split("\n");
  const setA = new Set(A);
  const setB = new Set(B);
  const out = [
    ...A.filter((l) => l.trim() && !setB.has(l)).map((l) => `- ${l}`),
    ...B.filter((l) => l.trim() && !setA.has(l)).map((l) => `+ ${l}`),
  ];
  return out.join("\n").slice(0, 4000) || "（无行级变化）";
}

async function save() {
  if (!isReal(plat.value)) return;
  saving.value = true;
  const before = versions.value.find((v) => v.status === "active")?.content ?? "";
  try {
    await expertMedia.overlaySet(plat.value, props.expertId, overlay.value);
    overlaySource.value = "runtime";
  } catch (e: any) {
    toast.error(e?.message ?? String(e));
    saving.value = false;
    return;
  }
  // 补丁已经生效；版本化登记失败不该让用户以为白存了，故分开 catch、分开报。
  try {
    const v = await evolutionApi.promptVersionAdd(
      props.expertId,
      ANCHOR,
      overlay.value,
      plat.value,
    );
    await evolutionApi.add(
      "prompt",
      `${expertName.value} @ ${platName(plat.value)} 平台补丁 → v${v.version}`,
      {
        detail: `人工在专家提示词抽屉里改写平台补丁（${before.length} → ${overlay.value.length} 字符）。`,
        diff: lineDiff(before, overlay.value),
        proposer: "human",
        // 证据 = 这次落地的版本 id：飞轮健康度数的就是「能追溯到具体度量/版本的变更」，
        // 不给证据的话这条永远不计入，健康度恒为 0，红灯就成了常量。
        evidence: [`prompt-version:${v.id}`],
        // 保存即生效，不是待验证的提案 —— 直接记「已固化」，不进观察期。
        status: "已固化",
      },
    );
    await loadVersions();
    toast.success(`已保存并记为 v${v.version}，可回滚`);
  } catch (e: any) {
    toast.info(`补丁已保存并生效，但版本化登记失败：${e?.message ?? String(e)}`);
  } finally {
    saving.value = false;
  }
}

/** 回滚：版本树切 active + 把该版内容真正写回 overlay 文件（否则回滚只改账本不改行为）。 */
async function rollback(v: PromptVersion) {
  if (!isReal(plat.value)) return;
  saving.value = true;
  try {
    const target = await evolutionApi.promptVersionRollback(v.id);
    await expertMedia.overlaySet(plat.value, props.expertId, target.content);
    overlay.value = target.content;
    overlaySource.value = "runtime";
    await evolutionApi.add(
      "prompt",
      `${expertName.value} @ ${platName(plat.value)} 回滚到 v${target.version}`,
      {
        detail: "人工在专家提示词抽屉里一键回滚，内容已写回 overlay 文件。",
        proposer: "human",
        evidence: [`prompt-version:${target.id}`],
        status: "已固化",
      },
    );
    await Promise.all([loadVersions(), load()]);
    toast.success(`已回滚到 v${target.version} 并写回生效`);
  } catch (e: any) {
    toast.error(e?.message ?? String(e));
  } finally {
    saving.value = false;
  }
}

function platName(id: string): string {
  return MEDIA_PLATFORMS.find((p) => p.id === id)?.name ?? id;
}

function fmtDate(sec: number): string {
  const d = new Date(sec * 1000);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

const VER_TEXT: Record<string, string> = {
  active: "生效中", superseded: "已被替代", rolled_back: "已回滚",
};

watch(plat, () => { load(); loadVersions(); });
onMounted(() => { load(); loadVersions(); });
</script>

<template>
  <div class="gd-mask" @click.self="emit('close')">
    <div class="gd">
      <div class="gd-h">
        <span>专家提示词 · {{ expertName }}</span>
        <button class="xbtn" title="关闭" @click="emit('close')">✕</button>
      </div>
      <div class="gd-body">
        <div v-if="!drivesModel" class="card" style="padding:10px 14px;border-color:var(--warn)">
          <span style="font-size:var(--text-xs);color:var(--dim)">
            这一格<b style="color:var(--ink2)">不下发提示词</b>——排版是本地确定性转换、投递是浏览器脚本，
            都不调模型。这里的补丁只作为该环节的规约留档，不会进入任何一次模型调用。
            真正吃提示词的是<b style="color:var(--ink2)">写作</b>与<b style="color:var(--ink2)">配图</b>两格。
          </span>
        </div>
        <div v-if="!lockPlatform" class="fld">
          <span>平台（补丁按平台生效）</span>
          <select v-model="plat" class="inp">
            <option v-for="p in MEDIA_PLATFORMS" :key="p.id" :value="p.id">{{ p.name }}</option>
          </select>
        </div>
        <div v-else class="card" style="padding: 10px 14px">
          <span style="font-size: var(--text-xs); color: var(--dim)">平台：<b style="color: var(--ink2)">{{ platName(plat) }}</b>（门户锁定）</span>
        </div>

        <div class="card">
          <h3>该专家在「{{ platName(plat) }}」的拼接提示词（基础画像 + 平台补丁）</h3>
          <div v-if="docLoading" class="foot"><span class="spin">◔</span> 读取中…</div>
          <div v-else-if="docErr" class="foot">{{ docErr }}</div>
          <pre v-else class="gd-doc">{{ doc || "（空）" }}</pre>
        </div>

        <div class="card">
          <h3>本平台补丁（platform overlay）· 来源：{{ overlaySource }}</h3>
          <textarea
            v-model="overlay"
            class="ta"
            rows="8"
            :disabled="!!docErr && overlaySource === 'none'"
            placeholder="在这里为该专家写一段只在本平台生效的补充提示词（会与基础画像拼接后驱动生产）…"
          ></textarea>
          <div style="margin-top: 10px">
            <button class="btn" :disabled="saving || !!docErr" @click="save">
              <span v-if="saving" class="spin" style="margin-right: 6px">◔</span>保存补丁（记一版）
            </button>
          </div>
          <p class="foot">保存即在版本树记一版并在进化时间线留卡；运行时按当前平台拼接：<code>系统提示 = 基础画像 + 平台补丁 + 闸门A注入</code>。</p>
        </div>

        <div class="card">
          <h3>版本历史（{{ platName(plat) }} · 整段补丁为锚）</h3>
          <p v-if="versionsErr" class="foot" style="color: var(--bad)">读取版本树失败：{{ versionsErr }}</p>
          <p v-else-if="!versions.length" class="foot">
            还没有版本记录——这个专家在本平台的补丁自版本树上线后还没被改过。保存一次即产生 v1。
          </p>
          <div v-else class="vlist">
            <div v-for="v in versions" :key="v.id" class="vrow">
              <div class="vmeta">
                <b>v{{ v.version }}</b>
                <span class="vtag" :class="v.status">{{ VER_TEXT[v.status] ?? v.status }}</span>
                <span style="color: var(--muted)">{{ fmtDate(v.createdAt) }} · {{ v.content.length }} 字符</span>
              </div>
              <div class="vact">
                <button
                  v-if="v.status !== 'active'"
                  class="btn sm ghost"
                  :disabled="saving"
                  @click="rollback(v)"
                >回滚到此版</button>
                <button class="btn sm ghost" :disabled="saving" @click="overlay = v.content">载入编辑框</button>
              </div>
            </div>
          </div>
          <p class="foot">回滚会把该版内容写回 overlay 文件并立即生效，同时在进化时间线留卡。</p>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.vlist { display: flex; flex-direction: column; gap: 6px; margin-top: 8px; }
.vrow {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
  padding: 7px 10px;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--bg-soft);
}
.vmeta { display: flex; align-items: center; gap: 8px; font-size: var(--text-xs); }
.vact { display: flex; gap: 6px; }
.vtag {
  font-size: var(--text-2xs);
  padding: 1px 7px;
  border-radius: 9px;
  border: 1px solid var(--border);
  color: var(--dim);
}
.vtag.active { border-color: var(--ok); color: var(--ok); }
.vtag.rolled_back { border-color: var(--warn); color: var(--warn); }
</style>
