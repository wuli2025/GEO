<script setup lang="ts">
/**
 * 推广植入（GEO 品牌织入）——「模块选点与优化方案」的可视化落地板块。
 *
 * 本页只管**怎么用**（三个子页）：
 *   植入逻辑：在哪儿植（generate 拼 prompt 处）、为什么（写作时织入 ≠ 写完后贴）；
 *   强度矩阵：分平台 强/弱/零 植入（可视化编辑，落回 brand.json）；
 *   硬广守卫：Rust 确定性拦截的说明 + 现场试打。
 * **填什么**已独立成「品牌档案」页（vBrand.vue）——同一份 brand.json，两页共用。
 */
import { ref, computed, onMounted, watch } from "vue";
import { title } from "./render";
import { PLATFORMS } from "./data";
import { brand, type BrandProfile } from "../../tauri";
import { toast } from "../../composables/useToast";

const props = defineProps<{ sub: string; platform: string }>();

const head = computed(() =>
  title("推广植入", "资源 / GEO 品牌织入 —— 在写作时织入，而不是写完后再贴")
);

// ── 品牌档案（brand.json 真源） ──
const profile = ref<BrandProfile | null>(null);
const saving = ref(false);

async function load() {
  try {
    profile.value = await brand.get();
  } catch {
    profile.value = null; // 后端命令不可用（如纯前端 dev），页面降级为只读说明
  }
}
onMounted(load);

// 本页只改 strength 矩阵，其余字段原样回写（档案编辑在「品牌档案」页）
async function save() {
  if (!profile.value) return;
  saving.value = true;
  try {
    await brand.set(profile.value);
    toast.info("品牌档案已保存到 ~/PolarisGEO/data/brand.json——下一条 generate 即生效");
    refreshPreview();
  } catch (e: any) {
    toast.error(e?.message ?? String(e));
  } finally {
    saving.value = false;
  }
}

// ── 契约预览（所见即所喂）──
const previewPid = ref("zhihu");
const preview = ref<[string, string] | null>(null);
async function refreshPreview() {
  try {
    preview.value = await brand.contractPreview(previewPid.value);
  } catch {
    preview.value = null;
  }
}
onMounted(refreshPreview);
watch(previewPid, refreshPreview);

// ── 硬广守卫试打 ──
const guardPid = ref("xhs");
const guardText = ref("我一直用这个方法记笔记，详情见 https://example.com ，也可以加微信号 abc123 交流。");
const guardHits = ref<string[] | null>(null);
const guardBusy = ref(false);
async function runGuard() {
  guardBusy.value = true;
  try {
    guardHits.value = await brand.guardTest(guardPid.value, guardText.value);
  } catch (e: any) {
    toast.error(e?.message ?? String(e));
  } finally {
    guardBusy.value = false;
  }
}

const STRENGTH_LABEL: Record<string, string> = { strong: "强植入", weak: "弱植入", zero: "零植入" };
const STRENGTH_DESC: Record<string, [string, string]> = {
  strong: ["品牌名 + 域名 + 事实库数据（引用式，各 1~2 次）", "裸链堆砌（>3 条）、广告口吻、联系方式"],
  weak: ["仅品牌名，以「我一直用 X」式经验主体出现（≤2 次）", "域名、任何链接、二维码、微信号、手机号"],
  zero: ["什么都不出现——正文纯干货", "品牌名、链接、联系方式等一切引流信息"],
};
const strengthOf = (pid: string): string => profile.value?.strength?.[pid] || "weak";

// 逻辑页的静态图（复用 geo.css 的 flow/step 语言）
const logicHtml = `
<section><div class="callout y"><b>一句话结论：</b>植入发生在 <b>正文生成阶段（generate）拼提示词处</b>——读一份可配置的
<code>brand.json</code>，把「品牌植入契约」织进写作提示词。<b>在写作时织入，而不是写完后再贴</b>：
品牌与正文同源生成才自然，后贴品牌最容易被平台判硬广。</div></section>
<section><h3>植入点在流水线的哪一格</h3><div class="flow">
  <div class="step evo">① generate 生成正文<small><b>★ 植入就在这里</b>：专家画像 + 平台补丁 + <b>品牌契约</b> → 模型一次写成</small></div><span class="arr">→</span>
  <div class="step">② 硬广守卫<small>Rust 正则拦裸链/微信号/手机号——防封 backstop，命中即 fail</small></div><span class="arr">→</span>
  <div class="step">③ image 配图<small>封面 + 插图（不涉植入）</small></div><span class="arr">→</span>
  <div class="step">④ typeset 排版<small>仅公众号（不涉植入）</small></div><span class="arr">→</span>
  <div class="step ok">⑤ upload 存草稿<small>发布仍由人点</small></div></div>
<p class="foot">正文只在 generate 诞生一次——植入必须发生在这里，别处没有正文可动。写完后再改是「贴牌」，正好踩中最该避开的「一眼硬广」死法。</p></section>
<section><div class="card"><h3>为什么是「织入」不是「后贴」</h3><div class="tbl-wrap"><table>
  <tr><th style="width:130px">做法</th><th>效果</th><th style="width:110px">结论</th></tr>
  <tr><td><b>写作时织入</b><br><small>本系统方案</small></td><td>品牌作为案例/出处出现在论证链里，与上下文同源；零额外模型调用</td><td><span class="badge b-full">✔ 用它</span></td></tr>
  <tr><td>写完后重写植入</td><td>多一次模型调用（钱+延迟+失败点）；「后贴」的品牌段落与正文断层，最易判硬广</td><td>备选不用</td></tr>
  <tr><td>把品牌硬编进模板</td><td>换推广网站要改一堆文件，不可按活动配置，无留痕</td><td>✘ 否决</td></tr>
</table></div></div></section>
<section><div class="card"><h3>方案四件套</h3><div class="grid g2">
  <div><b>① 可配置品牌档案</b><p class="foot">brand.json 是全链路唯一真源：身份 / 内涵与业务 / 人群·痛点·场景 / 可信度弹药 / 关键词与手法。<b>填得越细，模型能挑到的自然落点越多</b>。换网站换活动只改这一处 →  顶栏<b>「品牌档案」</b>页（快捷键 D）</p></div>
  <div><b>② 分平台植入强度</b><p class="foot">强植入（百家/头条/知乎/公众号）可带域名走 GEO；弱植入（小红书/抖音等）只提品牌名；零植入转私域。→「强度矩阵」页</p></div>
  <div><b>③ 硬广守卫（防封核心）</b><p class="foot">Rust 确定性正则，不靠模型自觉：弱/零平台命中裸链/微信号/手机号/二维码即判失败，绝不污染草稿箱。→「硬广守卫」页</p></div>
  <div><b>④ 关键词 → 实体绑定</b><p class="foot">填的 GEO 锚词会被要求进小标题/首段，并与品牌名<b>同段共现一次</b>——AI 检索该主题时把你这个实体一起召回。配合「权威实体+数据出处」写法喂 ai_citations KPI（域名60/品牌40/排名30）：争 AI 回答里的引用权，不争正文点击。</p></div>
</div></div></section>
<section><div class="callout y" style="border-color:var(--ok)">✅ <b>核心认知</b>：防封的正解不是「把链接藏进去不被发现」（猫鼠游戏，必输），而是<b>让品牌以权威实体身份被平台判为干货、被 AI 判为可信信源</b>。这条路既更安全，又正好是系统 KPI 想要的。</div></section>`;
</script>

<template>
  <div>
    <div v-html="head"></div>

    <!-- ① 植入逻辑 -->
    <template v-if="props.sub === 'logic'">
      <div v-html="logicHtml"></div>
      <section>
        <div class="card">
          <h3>所见即所喂：当前档案在各平台织入的契约</h3>
          <div class="bd-bar" style="margin-bottom:8px">
            <select v-model="previewPid" class="inp auto" aria-label="预览平台">
              <option v-for="p in PLATFORMS" :key="p.id" :value="p.id">{{ p.name }}</option>
            </select>
            <span v-if="preview" class="badge b-full">{{ preview[0] }}</span>
            <span v-else class="foot">档案未启用/品牌名为空——generate 一字不加（去「品牌档案」页填好并启用）</span>
          </div>
          <pre v-if="preview" class="pre-box">{{ preview[1] }}</pre>
          <p class="foot">这段文本会原样追加到 generate 的写作提示词末尾，并随流程详情「提示词快照」全程留痕。</p>
        </div>
      </section>
    </template>

    <!-- ② 强度矩阵 -->
    <template v-else-if="props.sub === 'matrix'">
      <div class="callout y"><b>矩阵不是拍脑袋</b>：依据各平台画像固化——小红书「硬广容忍度最低，品牌名换成某厂商仍有干货才安全」；百家号「主体名与官网/事实库一致」。默认矩阵保守（未知平台一律弱植入），要放开在此显式改。</div>
      <section>
        <div class="card">
          <h3>三档强度的含义</h3>
          <div class="tbl-wrap"><table>
            <tr><th style="width:90px">强度</th><th>正文可出现</th><th>禁止</th></tr>
            <tr v-for="(d, k) in STRENGTH_DESC" :key="k">
              <td><b>{{ STRENGTH_LABEL[k] }}</b></td><td>{{ d[0] }}</td><td>{{ d[1] }}</td>
            </tr>
          </table></div>
        </div>
      </section>
      <section v-if="profile">
        <div class="card">
          <h3>各平台生效强度（改完点保存）</h3>
          <div class="tbl-wrap"><table>
            <tr><th>平台</th><th style="width:140px">强度</th><th>本平台植入规则</th></tr>
            <tr v-for="p in PLATFORMS" :key="p.id">
              <td><b>{{ p.name }}</b></td>
              <td>
                <select class="inp auto" v-model="profile!.strength[p.id]" :aria-label="`${p.name} 植入强度`">
                  <option value="strong">强植入</option>
                  <option value="weak">弱植入</option>
                  <option value="zero">零植入</option>
                </select>
              </td>
              <td class="foot">{{ STRENGTH_DESC[strengthOf(p.id)]?.[0] }}</td>
            </tr>
          </table></div>
          <div style="margin-top:10px"><button class="btn sm" :disabled="saving" @click="save">{{ saving ? "保存中…" : "保存矩阵" }}</button></div>
        </div>
      </section>
    </template>

    <!-- ③ 硬广守卫 -->
    <template v-else>
      <div class="callout y"><b>防封底线是代码，不是自觉</b>：正文生成后，Rust 按本平台强度做<b>确定性正则拦截</b>——弱/零平台命中
        <code>http(s) 链接</code>、<code>域名</code>、<code>微信号</code>、<code>手机号</code>、<code>二维码话术</code> 即整条 job 判失败，绝不流进草稿箱；强平台拦「裸链堆砌」（&gt;3 条）与联系方式。</div>
      <section>
        <div class="card">
          <h3>现场试打：贴段文字看会不会被拦</h3>
          <div class="bd-bar" style="margin-bottom:8px">
            <select v-model="guardPid" class="inp auto" aria-label="试打平台">
              <option v-for="p in PLATFORMS" :key="p.id" :value="p.id">{{ p.name }}（{{ STRENGTH_LABEL[strengthOf(p.id)] }}）</option>
            </select>
            <button class="btn sm" :disabled="guardBusy" @click="runGuard">{{ guardBusy ? "扫描中…" : "试打" }}</button>
          </div>
          <textarea class="inp" v-model="guardText" rows="4"></textarea>
          <div v-if="guardHits !== null" style="margin-top:10px">
            <div v-if="guardHits.length" class="callout r"><b>会被拦截（{{ guardHits.length }} 项）：</b>
              <ol style="margin:6px 0 0"><li v-for="(h, i) in guardHits" :key="i">{{ h }}</li></ol>
            </div>
            <div v-else class="callout y" style="border-color:var(--ok)">✅ 通过：未命中任何引流特征。</div>
          </div>
          <p class="foot">与流水线里跑的是同一个函数（brand::hard_ad_guard）——这里过不了的，正式跑也过不了。</p>
        </div>
      </section>
    </template>
  </div>
</template>

