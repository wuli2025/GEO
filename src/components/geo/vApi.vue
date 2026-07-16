<script setup lang="ts">
/** API 中心：模型通道（chan）与生图通道（img）接真火山方舟 ark 组；模型分层（tier）为静态说明。 */
import { ref, computed, onMounted } from "vue";
import { ark, type ArkConfig } from "../../tauri";
import { toast } from "../../composables/useToast";
import { title, vApiTierHtml } from "./render";

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
    </template>
  </div>
</template>
