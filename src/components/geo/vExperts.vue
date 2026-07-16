<script setup lang="ts">
/**
 * 专家阵容：阵容总表接真 expert_media_list（能拿到就用真实启停/名称覆盖展示），
 * 每位专家挂「编辑提示词」入口 → ExpertPromptDrawer（doc + 平台 overlay 读写）。
 */
import { ref, computed, onMounted } from "vue";
import { title } from "./render";
import { EXPERTS } from "./data";
import { expertMedia } from "../../tauri";
import ExpertPromptDrawer from "./ExpertPromptDrawer.vue";

const props = defineProps<{ sub: string; platform: string }>();

const head = title("专家阵容", "资源 / M7 统一专家团 —— 一套专家团 + 平台提示词补丁（不再维护「公共池 + 平台团」两套体系）");

interface ExpertLite { id: string; name?: string; enabled?: boolean }
const liveList = ref<ExpertLite[]>([]);
onMounted(async () => {
  try {
    liveList.value = (await expertMedia.list()) as ExpertLite[];
  } catch {
    liveList.value = [];
  }
});
const enabledOf = (id: string): boolean => {
  const e = liveList.value.find((x) => x.id === id);
  return e ? e.enabled !== false : true;
};

const editing = ref<string | null>(null);

const rosterHtml = computed(() => {
  const groups = [...new Set(EXPERTS.map((e) => e[0]))];
  let h = `<div class="callout g"><b>v2.1 修订</b>：复用 Polaris 现有专家团系统，删除示例专家、换成自媒体阵容。<b>平台差异只体现在提示词</b>：每位专家 = 一份基础提示词 + 每平台一段「平台提示词补丁（platform overlay）」，运行时按当前平台拼接生效。${liveList.value.length ? `（后端已就绪：${liveList.value.length} 位在册）` : ""}</div>
      <section><div class="card"><h3>统一专家团（首期 ${EXPERTS.length} 人）</h3><div class="tbl-wrap"><table>
      <tr><th>职能</th><th>专家</th><th>职责</th><th>模型档</th><th>近30天均价</th><th>一次过审率</th><th>平台补丁改什么</th><th>提示词</th></tr>`;
  h += groups
    .map((g) =>
      EXPERTS.filter((e) => e[0] === g)
        .map((e, i) => {
          const first = i === 0 ? `<td rowspan="${EXPERTS.filter((x) => x[0] === g).length}"><b>${g}</b></td>` : "";
          const on = enabledOf(e[1]);
          return `<tr>${first}
        <td><code>${e[1]}</code><br><span style="color:var(--dim)">${e[2]}</span> ${on ? "" : '<span class="badge b-ghost">停用</span>'}</td><td>${e[3]}</td>
        <td>${e[4]}</td><td class="num-cell">${e[5]}</td><td class="num-cell">${e[6]}</td><td>${e[7]}</td>
        <td style="white-space:nowrap"><span class="btn sm" data-act="edit" data-id="${e[1]}">编辑提示词</span></td></tr>`;
        })
        .join("")
    )
    .join("");
  h += `</table></div><p class="foot">运行时拼接：<code>系统提示 = 基础画像 + 平台补丁(当前平台) + 闸门A注入(llmwiki / insight卡)</code>；无补丁的平台直接用基础画像。点「编辑提示词」看基础画像、读写各平台补丁。</p></div></section>
      <section><div class="card"><h3>职能切分（策略·创作·质检·分发·分析，缺一不可）</h3>
      <p>v1 流水线缺「策略」与「分析」两类常驻专家，v2 补齐；新增 <code>de-aiflavor</code>（中文平台对 AI 味道敏感）与 <code>competitor-watcher</code>（竞品爆文进 KB 做差距分析）。</p>
      <p style="margin-top:6px"><b>critic-strategist 强制跑另一家供应商模型</b>——防自夸、防同源模型互相认可（手册防应付设计）。</p></div></section>`;
  return h;
});

const formatHtml = `<section><div class="card"><h3>专家定义格式（角色文件模式）</h3><pre><code>templates/experts/writer.md
---
id: writer                 # 唯一ID，入 expert_profiles 表
model_tier: writer         # writer / reviewer 两档模型映射（M6）
tools: [Read, Grep, kb_search]            # 工具白名单
skills: [gz-wechat-article-writer]        # 挂载 skill
contract:                  # 交付契约（每次交接验 schema，不合格直接打回）
  input: brief_pack_v1     # 选题简报包 schema
  output: draft_pack_v1    # 稿件包 schema（md + frontmatter：标题/摘要/封面/标签）
evolvable: [style_notes, opening_formula] # 允许循环工程改写的段落锚点
platform_overlay:          # 平台提示词补丁
  zhihu:   "答题体。首段 50 字内给结论；品牌名换成「某厂商」仍有干货才允许提交。"
  wechat:  "长文体。1500 字以上，小标题分层，每节独立结论；必点声明原创。"
  toutiao: "标题口语化半档，首段直给结论。"
---
# 角色画像（进化知识库 playbook 卡按锚点注入到这里）
你是自媒体主笔。证据化写作：首段 75 字内直答、真实数据+来源、
可引用短句（40–110 字含数字非疑问句）、FAQ 区块…</code></pre>
      <ul><li>专家 = 一个 markdown 文件；Rust 侧 <code>expert_profiles</code> 表登记索引；运行时由 headless claude 以该文件为系统提示执行。</li>
      <li><b>evolvable 锚点</b>是进化的安全边界：循环工程只能改写锚点段落（版本化、可回滚），<b>角色骨架与红线不可自改</b>。</li>
      <li>交接一律走标准稿件包 schema，<b>每次交接验 schema</b>——不合格直接打回，不进下一位专家。</li></ul></div></section>`;

const perfHtml = `<section><div class="card"><h3>绩效（每次执行写 expert_runs）</h3>
      <p>记录：耗时、token、产物评分、过审/打回、下游探测归因的<b>功劳分摊</b>。专家卡片显示近 30 天：出稿量、一次过审率、平均 GEO 分、单产成本、贡献分。</p></div></section>
      <section><div class="card"><h3>编成进化（由 M9 主 Agent 提案）</h3><ul>
      <li>连续低分专家 → 换 prompt 版本 / 换模型档 / 停用（<span class="badge b-l1">L1</span>→<span class="badge b-l2">L2</span>）</li>
      <li>发现能力缺口 → 从白名单货架提案引入新专家（<span class="badge b-l2">L2</span> 人批）</li>
      <li><b>已登记风险 · 专家膨胀</b>（越加越多成本失控）→ 缓解：绩效末位停用机制 + 月度编成审计</li></ul></div></section>
      <section><div class="card"><h3>验收标准</h3><ul>
      <li>同一选题在知乎补丁与公众号补丁下产出文体<b>肉眼可辨</b>且各自过平台门禁；</li>
      <li>任一稿件可追溯「哪些专家、各自哪个 prompt 版本、消耗多少」；</li>
      <li>停用一名专家不影响其他平台。</li></ul></div></section>`;

// 本地事件委托：v-html 里的「编辑提示词」按钮 → 打开抽屉
function onClick(e: MouseEvent) {
  const el = (e.target as HTMLElement)?.closest?.("[data-act='edit']") as HTMLElement | null;
  if (el) editing.value = el.dataset.id ?? null;
}
</script>

<template>
  <div @click="onClick">
    <div v-html="head"></div>
    <div v-if="props.sub === 'roster'" v-html="rosterHtml"></div>
    <div v-else-if="props.sub === 'format'" v-html="formatHtml"></div>
    <div v-else v-html="perfHtml"></div>

    <ExpertPromptDrawer
      v-if="editing"
      :expert-id="editing"
      :platform="props.platform"
      @close="editing = null"
    />
  </div>
</template>
