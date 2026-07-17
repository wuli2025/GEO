<script setup lang="ts">
/** API 中心：模型通道（chan）与生图通道（img）接真火山方舟 ark 组；模型分层（tier）为静态说明。 */
import { ref, computed, onMounted } from "vue";
import { ark, type ArkConfig } from "../../tauri";
import { toast } from "../../composables/useToast";
import { title, vApiTierHtml } from "./render";
import ProviderSwitch from "./ProviderSwitch.vue";
// 生图模型配置面板(MiniMax / OpenAI / 豆包方舟 / 自定义):原挂在侧栏 ProviderDock,
// 通用外壳删除后迁到这里 —— 生图通道子页即其唯一入口。
import ImageProviderPanel from "../ImageProviderPanel.vue";

const props = defineProps<{ sub: string; platform: string }>();

const head = title("API 中心", "系统 / M6 模型与 Skill 调用 —— 内置默认通道，用户可换自己的 key");
const tierHtml = vApiTierHtml();

const cfg = ref<ArkConfig>({ apiKey: "", baseUrl: "", imageModel: "", chatModel: "" });
const showKey = ref(false);
const saving = ref(false);
const testing = ref<string | null>(null);
const testRes = ref<{ ok: boolean; text: string } | null>(null);

// 生图
const imgPrompt = ref("赛博朋克风格的封面插画，霓虹色调");
const imgSize = ref("1024x1024");
const imgBusy = ref(false);
const imgRes = ref<{ ok: boolean; text: string } | null>(null);

onMounted(async () => {
  try {
    cfg.value = await ark.configGet();
  } catch {
    /* 后端未就绪 */
  }
});

async function save() {
  saving.value = true;
  try {
    cfg.value = await ark.configSet({ ...cfg.value });
    toast.success("已保存 Ark 配置");
  } catch (e: any) {
    toast.error(e?.message ?? String(e));
  } finally {
    saving.value = false;
  }
}
async function testConn() {
  testing.value = "conn";
  testRes.value = null;
  try {
    const r = await ark.test();
    testRes.value = { ok: r.ok, text: r.ok ? `连通 · 延迟 ${r.latencyMs}ms` : r.message };
  } catch (e: any) {
    testRes.value = { ok: false, text: e?.message ?? String(e) };
  } finally {
    testing.value = null;
  }
}
async function testChat() {
  testing.value = "chat";
  testRes.value = null;
  try {
    const r = await ark.chatTest("你好，一句话介绍生成式引擎优化");
    testRes.value = { ok: r.ok, text: `${r.content.slice(0, 80)}（${r.latencyMs}ms）` };
  } catch (e: any) {
    testRes.value = { ok: false, text: e?.message ?? String(e) };
  } finally {
    testing.value = null;
  }
}
async function genImage() {
  imgBusy.value = true;
  imgRes.value = null;
  try {
    const r = await ark.imageGenerate(imgPrompt.value, imgSize.value);
    imgRes.value = { ok: true, text: `生图成功 · ${r.model} · 落盘：${r.path}` };
    toast.success("生图成功");
  } catch (e: any) {
    imgRes.value = { ok: false, text: e?.message ?? String(e) };
  } finally {
    imgBusy.value = false;
  }
}

// ── 生图模型清单（据真实链路核实标注；状态：已接通 / 占位） ──
type ImgStatus = "live" | "placeholder";
interface ImgModel {
  model: string;
  channel: string;
  status: ImgStatus;
  note: string;
  /** true = ark 当前配置的默认生图模型（随 cfg.imageModel 高亮） */
  arkDefault?: boolean;
}
const imageModels = computed<ImgModel[]>(() => [
  {
    model: cfg.value.imageModel?.trim() || "doubao-seedream-4-5-251128",
    channel: "火山方舟 Ark",
    status: "live",
    note: "真接线 ark_image_generate（ark.rs）· 生图通道当前默认",
    arkDefault: true,
  },
  {
    model: "MiniMax image-01",
    channel: "MiniMax（api.minimaxi.com）",
    status: "live",
    note: "故事视频技能 minimax-image.mjs · 复用「粉丝福利」MiniMax key",
  },
  {
    model: "gpt-image-2",
    channel: "OpenAI 生图技能",
    status: "placeholder",
    note: "技能「AI 生图 gpt-image-2」· 未安装占位，装后按描述生图",
  },
]);
const statusLabel: Record<ImgStatus, string> = { live: "已接通", placeholder: "占位" };

const imgCode = computed(
  () =>
    'python ~/PolarisGEO/skills/media-publisher/scripts/ark_image.py \\\n' +
    `  --prompt "${imgPrompt.value}" \\\n` +
    `  --out "cover.png" --size ${imgSize.value}   # size 缺省 2048x2048`
);
</script>

<template>
  <div>
    <div v-html="head"></div>

    <!-- 模型通道（接真 ark 配置） -->
    <template v-if="props.sub === 'chan'">
      <section>
        <div class="grid g2">
          <div class="card">
            <h3>火山引擎 Ark（默认通道，可换自己的 key）</h3>
            <div class="fld" style="margin-bottom: 10px">
              <span>API Key</span>
              <div style="display: flex; gap: 6px">
                <input :type="showKey ? 'text' : 'password'" v-model="cfg.apiKey" class="inp" placeholder="ark api key" />
                <button class="btn ghost" @click="showKey = !showKey">{{ showKey ? "隐藏" : "明文" }}</button>
              </div>
            </div>
            <div class="fld" style="margin-bottom: 10px"><span>Base URL</span><input v-model="cfg.baseUrl" class="inp" placeholder="https://ark.cn-beijing.volces.com/api/v3" /></div>
            <div class="grid g2" style="margin-bottom: 10px">
              <div class="fld"><span>生图模型</span><input v-model="cfg.imageModel" class="inp" placeholder="doubao-seedream-4-5" /></div>
              <div class="fld"><span>对话模型</span><input v-model="cfg.chatModel" class="inp" placeholder="doubao-…" /></div>
            </div>
            <div style="display: flex; gap: 8px; flex-wrap: wrap">
              <button class="btn" :disabled="saving" @click="save"><span v-if="saving" class="spin" style="margin-right: 6px">◔</span>保存</button>
              <button class="btn ghost" :disabled="!!testing" @click="testConn"><span v-if="testing === 'conn'" class="spin" style="margin-right: 6px">◔</span>测试连通性</button>
              <button class="btn ghost" :disabled="!!testing" @click="testChat"><span v-if="testing === 'chat'" class="spin" style="margin-right: 6px">◔</span>测试对话</button>
            </div>
            <div v-if="testRes" class="callout" :class="testRes.ok ? 'g' : 'r'" style="margin-top: 12px; font-size: var(--text-xs)">{{ testRes.text }}</div>
            <p class="foot">默认 key 对应账号须在方舟控制台开通生图模型服务；报 <code>ModelNotOpen</code> 时去 console.volcengine.com/ark 开通，或在此换自己的 key。</p>
          </div>
          <div class="card">
            <h3>MiniMax M3 通道</h3>
            <p>备用国产模型通道；密钥入 providers 体系而非裸环境变量。</p>
            <h3 style="margin-top: 12px">Claude 双通道（基座）</h3>
            <ul>
              <li><b>headless claude</b>（<code>headless.rs</code>）：流水线各步判分、事实校验、结构化决策；只读白名单（Read/Glob/Grep），stdin 喂 prompt，<b>输出 JSON 决策数据，Rust 执行改动</b>；</li>
              <li><b>chat_send 流式对话</b>：写作/评审等长产出 + 人在环节介入；skill 注入系统提示，平台宪法作为会话宪法。</li>
            </ul>
          </div>
        </div>
      </section>

      <!-- 切换中心（cc-switch 复刻的供应商坞·内联版；与侧栏坞共用 store） -->
      <section style="margin-top: 16px">
        <ProviderSwitch />
      </section>

      <!-- 生图模型清单 -->
      <section style="margin-top: 16px">
        <div class="card">
          <h3>生图模型
            <span style="font-size: var(--text-xs); color: var(--muted); font-weight: 400">已接通 / 占位 · 据真实链路核实</span>
          </h3>
          <div class="imglist">
            <div v-for="m in imageModels" :key="m.model" class="imgrow" :class="{ live: m.status === 'live', def: m.arkDefault }">
              <span class="imgdot" :class="m.status" />
              <div class="imginfo">
                <div class="imgname">
                  {{ m.model }}
                  <span v-if="m.arkDefault" class="imgtag def">当前默认</span>
                </div>
                <div class="imgnote">{{ m.channel }} · {{ m.note }}</div>
              </div>
              <span class="imgstat" :class="m.status">{{ statusLabel[m.status] }}</span>
            </div>
          </div>
          <div style="display: flex; gap: 8px; margin-top: 12px; align-items: center">
            <button class="btn ghost" data-gosub="img">前往生图通道（img 子标签）→</button>
            <span class="foot" style="margin: 0">火山方舟 Seedream 为默认生图链路；MiniMax image-01 供故事视频插画；gpt-image-2 为技能占位</span>
          </div>
        </div>
      </section>
    </template>

    <!-- 模型分层（静态说明） -->
    <div v-else-if="props.sub === 'tier'" v-html="tierHtml"></div>

    <!-- 生图通道（接真 ark 生图） -->
    <template v-else>
      <section>
        <div class="card">
          <h3>生图通道（ark_image.py · 火山方舟 Seedream）</h3>
          <div class="grid g2" style="margin-bottom: 10px">
            <div class="fld" style="grid-column: 1 / -1"><span>画面描述 prompt</span><input v-model="imgPrompt" class="inp" placeholder="画面描述…" /></div>
            <div class="fld"><span>尺寸</span>
              <select v-model="imgSize" class="inp">
                <option value="1024x1024">1024×1024</option>
                <option value="2048x2048">2048×2048</option>
                <option value="1280x720">1280×720</option>
                <option value="900x383">900×383（公众号）</option>
              </select>
            </div>
            <div class="fld" style="justify-content: flex-end">
              <button class="btn" :disabled="imgBusy" @click="genImage"><span v-if="imgBusy" class="spin" style="margin-right: 6px">◔</span>生成封面</button>
            </div>
          </div>
          <div v-if="imgRes" class="callout" :class="imgRes.ok ? 'g' : 'r'" style="font-size: var(--text-xs)">{{ imgRes.text }}</div>
          <pre><code>{{ imgCode }}</code></pre>
          <ul>
            <li>模型缺省 <code>doubao-seedream-4-5</code>；接口报「模型不存在/未开通」时脚本自动 GET /models 捞 seedream 系列挨个重试；</li>
            <li>无密钥时回退 HTML 模拟图（保底不断流）；密钥入 providers 体系而非裸环境变量。</li>
          </ul>
        </div>
      </section>

      <!-- 生图模型配置（MiniMax / OpenAI / 豆包方舟 / 自定义 OpenAI 兼容通道） -->
      <section style="margin-top: 16px">
        <div class="card">
          <h3>生图模型配置<span style="font-size: var(--text-xs); color: var(--muted); font-weight: 400">MiniMax / OpenAI / 豆包方舟 / 自定义</span></h3>
          <ImageProviderPanel />
        </div>
      </section>
    </template>
  </div>
</template>

<style scoped>
.imglist { display: flex; flex-direction: column; gap: 6px; margin-top: 10px; }
.imgrow { display: flex; align-items: center; gap: 10px; padding: 9px 11px; border: 1px solid var(--border-soft); border-radius: 9px; background: var(--bg-soft); }
.imgrow.def { border-color: var(--primary); background: var(--primary-soft, var(--bg-soft)); }
.imgdot { width: 9px; height: 9px; border-radius: 50%; flex-shrink: 0; }
.imgdot.live { background: #16a34a; box-shadow: 0 0 0 3px #16a34a22; }
.imgdot.placeholder { background: var(--muted); }
.imginfo { flex: 1; min-width: 0; }
.imgname { font-size: 12.5px; font-weight: 600; color: var(--text); font-family: var(--mono); display: flex; align-items: center; gap: 6px; }
.imgtag { font-size: 8.5px; padding: 0 5px; border-radius: 3px; font-weight: 600; letter-spacing: 0.5px; }
.imgtag.def { color: var(--primary-deep, var(--primary)); border: 1px solid var(--primary); }
.imgnote { font-size: 10.5px; color: var(--muted); margin-top: 2px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.imgstat { font-size: 10px; font-weight: 600; padding: 2px 8px; border-radius: 5px; flex-shrink: 0; }
.imgstat.live { color: #16a34a; background: #16a34a14; border: 1px solid #16a34a55; }
.imgstat.placeholder { color: var(--muted); background: var(--bg-soft); border: 1px solid var(--border); }
</style>
