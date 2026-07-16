<script setup lang="ts">
/**
 * 专家提示词编辑抽屉（真数据）：
 *  - expert_media_doc         看基础画像 + 当前平台拼接后的提示词
 *  - expert_media_overlay_get 读该平台补丁（runtime / seed / none）
 *  - expert_media_overlay_set 写该平台补丁（textarea 编辑 + 保存）
 * 用户点名的「各专家以及各影响结果的提示词可修改」的落点。
 */
import { ref, watch, onMounted, computed } from "vue";
import {
  expertMedia,
  MEDIA_PLATFORMS,
  type MediaPlatform,
} from "../../tauri";
import { toast } from "../../composables/useToast";
import { EXPERTS } from "./data";

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

async function save() {
  if (!isReal(plat.value)) return;
  saving.value = true;
  try {
    await expertMedia.overlaySet(plat.value, props.expertId, overlay.value);
    overlaySource.value = "runtime";
    toast.success(`已保存 ${expertName.value} 在「${platName(plat.value)}」的补丁`);
  } catch (e: any) {
    toast.error(e?.message ?? String(e));
  } finally {
    saving.value = false;
  }
}

function platName(id: string): string {
  return MEDIA_PLATFORMS.find((p) => p.id === id)?.name ?? id;
}

watch(plat, load);
onMounted(load);
</script>

<template>
  <div class="gd-mask" @click.self="emit('close')">
    <div class="gd">
      <div class="gd-h">
        <span>专家提示词 · {{ expertName }}</span>
        <button class="xbtn" title="关闭" @click="emit('close')">✕</button>
      </div>
      <div class="gd-body">
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
              <span v-if="saving" class="spin" style="margin-right: 6px">◔</span>保存补丁
            </button>
          </div>
          <p class="foot">补丁版本化、可回滚；运行时按当前平台拼接：<code>系统提示 = 基础画像 + 平台补丁 + 闸门A注入</code>。</p>
        </div>
      </div>
    </div>
  </div>
</template>
