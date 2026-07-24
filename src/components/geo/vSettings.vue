<script setup lang="ts">
/**
 * 设置中心：原「API 中心」并入此处。
 * - chan：模型通道 —— 切换中心（cc-switch 复刻）当主角，一屏换模型；
 * - tier/img：模型分层 / 生图通道（火山方舟 ark 组的密钥与模型收在 img 页折叠区）；
 * - update：我们的更新（自建 CDN 主源 + 多源回退，转发 useUpdater 状态机）；
 * - env：环境医生（复用 EnvDoctor 面板模式，随时复检 / 重装）。
 */
import { ref, computed, onMounted } from "vue";
import { ark, type ArkConfig } from "../../tauri";
import { toast } from "../../composables/useToast";
import { title, vApiTierHtml } from "./render";
import ProviderSwitch from "./ProviderSwitch.vue";
// 生图模型配置面板(MiniMax / OpenAI / 豆包方舟 / 自定义):原挂在侧栏 ProviderDock,
// 通用外壳删除后迁到这里 —— 生图通道子页即其唯一入口。
import ImageProviderPanel from "../ImageProviderPanel.vue";
// 我们的更新 / 环境医生 —— 从启动流程/横幅迁进设置页
import EnvDoctor from "../EnvDoctor.vue";
import {
  currentVersion, updateVersion, updateNotes, updateError, updating,
  updateProgress, upToDate, checking, lastCheckedAt, manualCheck, applyUpdate,
} from "../../composables/useUpdater";

const props = defineProps<{ sub: string; platform: string }>();

// 头部随子页切换（api 三页共用旧文案，其余各自表述）
const HEADS: Record<string, [string, string]> = {
  chan: ["模型通道", "设置 / 点一下换一家模型 —— Kimi For Coding、MiniMax、豆包 Seed 或自填任意通道，切换即写环境"],
  img: ["生图通道", "设置 / 文生图独立链路 —— 火山方舟 Seedream 5.0 默认，另可配 MiniMax / OpenAI 兼容"],
  update: ["我们的更新", "设置 / 应用自动更新 —— 自建 CDN 托管、多源自动回退，下载安装后自动重启生效"],
  env: ["环境医生", "设置 / 运行环境监测与一键安装修复（Claude Code / Node / Shell / uv）"],
};
const head = computed(() => {
  const h = HEADS[props.sub];
  return h ? title(h[0], h[1]) : title("API 中心", "设置 / M6 模型与 Skill 调用 —— 内置默认通道，用户可换自己的 key");
});

const lastCheckedText = computed(() => {
  if (!lastCheckedAt.value) return "尚未检查";
  const d = new Date(lastCheckedAt.value);
  const p = (n: number) => String(n).padStart(2, "0");
  return `上次检查 ${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
});
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
    model: cfg.value.imageModel?.trim() || "doubao-seedream-5-0-260128",
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

    <!-- 模型通道：切换中心当主角，一屏之内换模型 -->
    <template v-if="props.sub === 'chan'">
      <section>
        <ProviderSwitch big />
      </section>

      <section style="margin-top: 16px">
        <div class="grid g2">
          <div class="card">
            <h3>换成自己的通道</h3>
            <ul>
              <li>列表里点 <b>Kimi For Coding</b> / <b>MiniMax</b> / <b>豆包 Seed</b> 等预设 → 弹窗粘 Key 即切；模型名留空走该家默认；</li>
              <li>没有的家点右上角 <b>添加供应商</b> 自填：名称 / Base URL / API Key / 模型名；OpenAI 协议的家勾「OpenAI 协议」，经 Polaris 本地路由翻译转发；</li>
              <li><b>联动系统 CLI</b> 打开时，切换会写进 <code>~/.claude/settings.json</code>，终端 <code>claude</code> 跟着一起变；关掉则只作用于 Polaris 自己。</li>
            </ul>
          </div>
          <div class="card">
            <h3>Claude 双通道（基座）</h3>
            <ul>
              <li><b>headless claude</b>（<code>headless.rs</code>）：流水线各步判分、事实校验、结构化决策；只读白名单（Read/Glob/Grep），stdin 喂 prompt，<b>输出 JSON 决策数据，Rust 执行改动</b>；</li>
              <li><b>chat_send 流式对话</b>：写作/评审等长产出 + 人在环节介入；skill 注入系统提示，平台宪法作为会话宪法。</li>
            </ul>
            <p class="foot">生图不走这里 —— 文生图是另一条 OpenAI 形状的链路，见「生图通道」子标签。</p>
          </div>
        </div>
      </section>
    </template>

    <!-- 模型分层（静态说明） -->
    <div v-else-if="props.sub === 'tier'" v-html="tierHtml"></div>

    <!-- 我们的更新（GitHub Releases 自动更新，转发后端状态机） -->
    <template v-else-if="props.sub === 'update'">
      <section>
        <div class="card">
          <h3>我们的更新
            <span style="font-size: var(--text-xs); color: var(--muted); font-weight: 400">自建 CDN 托管 · 多源自动回退 · 下载安装后自动重启</span>
          </h3>
          <div class="updrow">
            <div>
              <div class="updver">当前版本 <code>v{{ currentVersion || "—" }}</code></div>
              <div class="foot" style="margin: 4px 0 0">{{ lastCheckedText }}</div>
            </div>
            <div style="display: flex; gap: 8px; flex-wrap: wrap">
              <button class="btn ghost" :disabled="checking || updating" @click="manualCheck">
                <span v-if="checking" class="spin" style="margin-right: 6px">◔</span>检查更新
              </button>
              <button v-if="updateVersion" class="btn" :disabled="updating" @click="applyUpdate">
                <span v-if="updating" class="spin" style="margin-right: 6px">◔</span>{{ updating ? `更新中 ${updateProgress}%` : `立即更新到 v${updateVersion}` }}
              </button>
            </div>
          </div>

          <div v-if="updating" class="updbar"><div class="updbar-fill" :style="{ width: updateProgress + '%' }"></div></div>

          <div v-if="updateVersion" class="callout g" style="margin-top: 12px; font-size: var(--text-xs)">发现新版本 <b>v{{ updateVersion }}</b>，点「立即更新」即可后台下载安装并自动重启。</div>
          <div v-else-if="upToDate" class="callout g" style="margin-top: 12px; font-size: var(--text-xs)">已是最新版本 ✓</div>
          <div v-if="updateError" class="callout r" style="margin-top: 12px; font-size: var(--text-xs)">{{ updateError }}</div>

          <div v-if="updateNotes" class="updnotes">{{ updateNotes }}</div>

          <p class="foot">更新经后端唯一状态机（updater.rs）单飞执行，多次点击不重入；无网络 / 未发布 release / 非桌面运行时会被静默忽略。</p>
        </div>
      </section>
    </template>

    <!-- 环境医生（复用 EnvDoctor 面板模式，随时复检 / 重装） -->
    <template v-else-if="props.sub === 'env'">
      <section><EnvDoctor /></section>
    </template>

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
            <li>模型缺省 <code>doubao-seedream-5-0-260128</code>（Seedream 5.0）；接口报「模型不存在/未开通」时脚本自动 GET /models 捞 seedream 系列挨个重试；</li>
            <li>无密钥时回退 HTML 模拟图（保底不断流）；密钥入 providers 体系而非裸环境变量。</li>
          </ul>

          <!-- 方舟密钥：从「模型通道」页收进这里，默认折叠 —— 生图才用得上，平时不占眼 -->
          <details class="arkfold">
            <summary>方舟密钥与模型（仅生图用，一般不用动）</summary>
            <div class="fld" style="margin: 10px 0">
              <span>API Key</span>
              <div style="display: flex; gap: 6px">
                <input :type="showKey ? 'text' : 'password'" v-model="cfg.apiKey" class="inp" placeholder="ark api key" />
                <button class="btn ghost" @click="showKey = !showKey">{{ showKey ? "隐藏" : "明文" }}</button>
              </div>
            </div>
            <div class="fld" style="margin-bottom: 10px"><span>Base URL</span><input v-model="cfg.baseUrl" class="inp" placeholder="https://ark.cn-beijing.volces.com/api/v3" /></div>
            <div class="grid g2" style="margin-bottom: 10px">
              <div class="fld"><span>生图模型</span><input v-model="cfg.imageModel" class="inp" placeholder="doubao-seedream-5-0-260128" /></div>
              <div class="fld"><span>对话模型</span><input v-model="cfg.chatModel" class="inp" placeholder="doubao-…" /></div>
            </div>
            <div style="display: flex; gap: 8px; flex-wrap: wrap">
              <button class="btn" :disabled="saving" @click="save"><span v-if="saving" class="spin" style="margin-right: 6px">◔</span>保存</button>
              <button class="btn ghost" :disabled="!!testing" @click="testConn"><span v-if="testing === 'conn'" class="spin" style="margin-right: 6px">◔</span>测试连通性</button>
              <button class="btn ghost" :disabled="!!testing" @click="testChat"><span v-if="testing === 'chat'" class="spin" style="margin-right: 6px">◔</span>测试对话</button>
            </div>
            <div v-if="testRes" class="callout" :class="testRes.ok ? 'g' : 'r'" style="margin-top: 12px; font-size: var(--text-xs)">{{ testRes.text }}</div>
            <p class="foot">这份密钥只喂生图（<code>/images/generations</code>）；对话模型请在「模型通道」的切换中心选。账号需在 console.volcengine.com/ark 开通对应生图模型，报 <code>AccountOverdueError</code> 是欠费、报 <code>InvalidEndpointOrModel.NotFound</code> 是模型 id 写错。</p>
          </details>
        </div>
      </section>

      <!-- 生图模型清单（原在「模型通道」页，随生图一并搬来） -->
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
          <p class="foot">火山方舟 Seedream 5.0 为默认生图链路；MiniMax image-01 供故事视频插画；gpt-image-2 为技能占位。</p>
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

/* 方舟密钥折叠区（生图页） */
.arkfold { margin-top: 12px; border: 1px solid var(--border-soft); border-radius: 9px; padding: 9px 11px; background: var(--bg-soft); }
.arkfold > summary { cursor: pointer; font-size: 12px; color: var(--muted); user-select: none; }
.arkfold > summary:hover { color: var(--text); }
.arkfold[open] > summary { margin-bottom: 4px; color: var(--text); }

/* 我们的更新 */
.updrow { display: flex; align-items: center; justify-content: space-between; gap: 12px; flex-wrap: wrap; }
.updver { font-size: 13px; font-weight: 600; color: var(--text); }
.updbar { height: 6px; border-radius: 4px; background: var(--bg-soft); overflow: hidden; margin-top: 12px; }
.updbar-fill { height: 100%; background: var(--primary); transition: width 0.25s ease; }
.updnotes { margin-top: 12px; padding: 10px 12px; border: 1px solid var(--border-soft); border-radius: 9px; background: var(--bg-soft); font-size: 12px; color: var(--muted); white-space: pre-wrap; }
</style>
