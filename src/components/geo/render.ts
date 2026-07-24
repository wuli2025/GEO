/**
 * 静态视图的 HTML 构建器（供 v-html）。移植自设计稿的 vXxx() 渲染函数。
 *
 * 交互约定（壳层做事件委托，见 GeoOpsCenter.vue）：
 *   data-go="view"          切视图
 *   data-go="portal" data-portal="pid"   切到某平台门户
 *   data-gosub="key"        切当前（或 data-go 指定）视图的子标签

 * 图表悬停用 data-chart/data-i，由壳层统一处理。
 */
import { PLATFORMS, MOCK, P, ico, sdot, esc } from "./data";
import { stackedArea, lineChart, legend } from "./charts";
import type { EvolutionData } from "../../composables/useEvolution";
import type { MediaKpi, MediaJob, MediaCrawlSnapshot } from "../../tauri";

export function title(t: string, c: string): string {
  return `<h2 class="vtitle">${t}</h2><div class="vcrumb">${c}</div>`;
}

/** 可点文字（导航） */
const lnk = (go: string, txt: string, sub?: string) =>
  `<a class="glnk" data-go="${go}"${sub ? ` data-gosub="${sub}"` : ""}>${txt}</a>`;

/* ── KPI 数值渲染 ──
   空数据以前也走 32px 黑体，八张卡一起喊八个破折号——占位符不该比真数据还响。
   无值时压成 muted，同时把「▲ — 环比」这类只在有数时才成立的修饰整条略去。 */
const nil = (v: unknown) => v === "—" || v === "" || v == null;
// 叫 kpiNum 而不是 num：热力表那段里已有个局部 num()（parseFloat），同名会互相遮蔽。
const kpiNum = (v: unknown, unit = ""): string =>
  nil(v) ? `<div class="num nil">—</div>` : `<div class="num">${v}${unit}</div>`;

/* ── 数据看板（M8 运营大屏） ─────────────────────────────────────────── */

/** 大数字压成「12.8万 / 1.2亿」——KPI 卡宽度固定，八位数会把卡撑破。 */
const cn = (v: number): string => {
  if (!isFinite(v)) return "—";
  if (v >= 1e8) return (v / 1e8).toFixed(1).replace(/\.0$/, "") + "亿";
  if (v >= 1e4) return (v / 1e4).toFixed(1).replace(/\.0$/, "") + "万";
  return String(Math.round(v));
};

/** 抓取快照跨平台求和：只算抓到过的（ok），字段缺失按缺失处理而不是 0。 */
const crawlSum = (snaps: MediaCrawlSnapshot[], field: string): number | null => {
  const vals = snaps.filter((s) => s.ok).map((s) => (s.summary as any)?.[field])
    .filter((v) => typeof v === "number");
  return vals.length ? vals.reduce((a, b) => a + b, 0) : null;
};

export function vDashboardHtml(
  kpi: typeof MOCK.kpi = MOCK.kpi,
  jobs: MediaJob[] = [],
  crawls: MediaCrawlSnapshot[] = [],
  crawling = false,
): string {
  const k = { ...kpi };
  // 「阅读 / 点击」这张卡的真数据源是平台后台抓取，不是本地度量事件——本地只知道我们
  // 发了什么，不知道发出去之后有多少人看。抓到了就覆盖 mock 的破折号。
  const reads = crawlSum(crawls, "reads");
  const impressions = crawlSum(crawls, "impressions");
  if (reads !== null) k.reads = cn(reads);
  // 环比需要两次快照对比，只存最新一份时无从算起——空着，绝不编一个箭头出来。
  let h = title("数据看板", "总控 / 运营大屏");
  {
    h += `<section><div class="grid g5">
      <div class="card stat"><h3>近 7 天发布</h3>${kpiNum(k.pub7, "<small>篇</small>")}<div class="sub">${k.runs} 次 run · ${k.runOk} 成功</div></div>
      <div class="card stat"><h3>近 30 天发布</h3>${kpiNum(k.pub30, "<small>篇</small>")}<div class="sub">篇均 ${k.avgWords} 字</div></div>
      <div class="card stat"><h3>阅读 / 点击</h3>${kpiNum(k.reads)}<div class="sub">${
        impressions !== null ? `${cn(impressions)} 次展现`
          : nil(k.readsMom) ? "" : `<span class="up">▲ ${k.readsMom}</span> 环比`}</div></div>
      <div class="card stat"><h3>AI 来源占比</h3>${kpiNum(k.aiShare)}<div class="sub">${nil(k.aiBreak) ? "" : k.aiBreak}</div></div>
      <div class="card stat"><h3>单篇成本</h3>${kpiNum(k.cost)}<div class="sub"></div></div>
      <div class="card stat"><h3>30 天 token</h3>${kpiNum(k.token)}<div class="sub"></div></div>
      <div class="card stat"><h3>30 天 LLM 成本</h3>${kpiNum(k.llmCost)}<div class="sub"></div></div>
      <div class="card stat"><h3>待办</h3>${kpiNum(k.pending, "<small>待审</small>")}<div class="sub">${sdot(k.loginBad ? "warn" : "ok", k.loginBad + " 个账号异常")} · ${lnk("approvals", "进队列 →")}</div></div>
    </div></section>
    <section><h3>最近投递</h3><div class="card"><div class="tbl-wrap"><table>` +
      (jobs.length
        ? `<tr><th>时间</th><th>平台</th><th>标题</th><th>阶段</th><th>状态</th></tr>` +
          jobs.slice(0, 8).map((j) => {
            const dot = { pending: "idle", running: "warn", done: "ok", failed: "bad", canceled: "idle" }[j.status] || "idle";
            const d = new Date(j.updatedAt * 1000);
            const p2 = (n: number) => String(n).padStart(2, "0");
            const when = `${p2(d.getMonth() + 1)}-${p2(d.getDate())} ${p2(d.getHours())}:${p2(d.getMinutes())}`;
            return `<tr data-job="${esc(j.id)}" style="cursor:pointer" title="点击查看生成流程"><td style="white-space:nowrap">${when}</td><td>${P(j.platform)?.name ?? esc(j.platform)}</td><td>${esc(j.title)}</td><td>${esc(j.stage || j.stages.join("→"))}</td><td>${sdot(dot, j.status)}</td></tr>`;
          }).join("")
        : `<tr><th>时间</th><th>平台</th><th>标题</th><th>结果</th><th>保存回执</th><th>窗口</th></tr>` +
          (MOCK.recent.length
            ? MOCK.recent.map((r) => `<tr><td>${r[0]}</td><td>${r[1]}</td><td>${r[2]}</td><td>${r[3]}</td><td>${r[4]}</td><td>${sdot(r[5][0], r[5][1])}</td></tr>`).join("")
            : `<tr><td colspan="6"><p class="empty">暂无数据</p></td></tr>`)) +
      `</table></div></div></section>`;
  }
  {
    h += `<section><div class="card"><h3>每日流量三层构成</h3>
      <div class="chart">${stackedArea(MOCK.traffic, "traffic")}</div>${legend(MOCK.traffic.series)}</div></section>
      <section><div class="grid g2">
      <div class="card"><h3>AI 爬虫雷达 · 抓取次数</h3><div class="bars">` +
      (MOCK.radar.length
        ? MOCK.radar.map(([n, v]) => `<div class="row" title="${n}：${v} 次"><span class="lab">${n}</span><span class="track"><span class="fill" style="width:${((v / MOCK.radar[0][1]) * 100).toFixed(0)}%"></span></span><span class="val">${v}</span></div>`).join("")
        : `<p class="empty">暂无数据</p>`) +
      `</div></div>
      <div class="card"><h3>三张清单</h3>
        <div style="margin-top:8px"><span class="btn sm" data-go="questions">查看完整清单 →</span></div></div>
      </div></section>`;
  }
  {
    h += `<section><div class="card"><h3>五引擎品类词提及率</h3>
      <div class="chart">${lineChart(MOCK.engines, "engines")}</div>${legend(MOCK.engines.series)}</div></section>
      <section><div class="grid g3">
      <div class="card stat"><h3>品类词提及率</h3><div class="num nil">—</div><div class="sub"></div></div>
      <div class="card stat"><h3>声量份额 SoV</h3><div class="num nil">—</div><div class="sub"></div></div>
      <div class="card stat"><h3>事实准确率</h3><div class="num nil">—</div><div class="sub"></div></div>
      </div></section>`;
  }
  {
    // 抓到真数据就用真数据（列换成爬虫实际拿得到的字段），否则退回设计稿的列。
    // 不把「被引 / 成本」硬塞进来——那两列没有数据源，留着只会是一排 0，比空表更误导。
    const realCols = ["展现", "阅读", "点赞", "评论", "粉丝"];
    const realKeys = ["impressions", "reads", "likes", "comments", "fans"];
    const okSnaps = crawls.filter((s) => s.ok);
    const cols = okSnaps.length ? realCols : MOCK.heat.cols;
    const rows: [string, (string | number)[]][] = okSnaps.length
      ? okSnaps.map((s) => [
          s.name || P(s.platform)?.name || s.platform,
          realKeys.map((key) => (s.summary as any)?.[key] ?? 0),
        ])
      : MOCK.heat.rows;
    const num = (v: string | number) => parseFloat(String(v).replace(/,/g, ""));
    const ramp = ["var(--q1)", "var(--q2)", "var(--q3)", "var(--q4)", "var(--q5)"];
    let cells = "";
    rows.forEach(([name, vals]) => {
      cells += `<tr><td style="white-space:nowrap">${name}</td>`;
      vals.forEach((v, ci) => {
        const col = rows.map((r) => num(r[1][ci])), mn = Math.min(...col), mx = Math.max(...col);
        const t = mx === mn ? 0.5 : (num(v) - mn) / (mx - mn);
        const step = ramp[Math.min(4, Math.floor(t * 5))];
        cells += `<td class="cell" style="background:${step}" title="${name} · ${cols[ci]}：${v}">${v}</td>`;
      });
      cells += `</tr>`;
    });
    // 抓取状态条：这块数据什么时候抓的、哪些平台空手而归，必须写在表边上。
    // 一张没有时间戳的数据表会被当成实时的看——实际上可能是三天前的。
    const stale = crawls.length
      ? crawls.map((s) => `${s.name || s.platform} ${s.ok ? s.crawledAt : `<b>${s.crawledAt} 空手</b>`}`).join(" · ")
      : "从未抓取";
    h += `<section><div class="card"><h3>平台 × 指标热力表</h3><div class="tbl-wrap heat"><table>
      <tr><th>平台</th>${cols.map((c) => `<th style="text-align:center">${c}</th>`).join("")}</tr>${cells || `<tr><td colspan="${cols.length + 1}"><p class="empty">暂无数据</p></td></tr>`}</table></div>
      <p class="foot">数据来自各平台创作者后台抓取（<code>metrics_crawler.py</code>，复用已登录窗口）：${stale}
      　<span class="btn sm" data-crawl="1"${crawling ? " aria-disabled=\"true\"" : ""}>${crawling ? "抓取中…（每平台约 25 秒）" : "立即抓取"}</span></p>
      </div></section>`;
  }
  // AI 爬虫雷达原本还有一张独立子页，与上面「流量与 AI 来源」里的那张同源同数据，
  // 合并成一页后重复出现两次没有意义 —— 只留上面那张。
  {
    const lvmap: Record<string, string> = { good: "ok", warning: "warn", serious: "warn", critical: "bad" };
    h += `<section><div class="card"><h3>日健康度</h3><div class="tbl-wrap"><table>
      <tr><th>项</th><th>详情</th></tr>` +
      (MOCK.health.length
        ? MOCK.health.map(([lv, t, d]) => `<tr><td>${sdot(lvmap[lv], "<b>" + t + "</b>")}</td><td>${d}</td></tr>`).join("")
        : `<tr><td colspan="2"><p class="empty">暂无数据</p></td></tr>`) +
      `</table></div></div></section>`;
  }
  {
    // 每一层都标注**接没接**。原来这张表只写「数据源是什么」，读起来像四条都在跑，
    // 实际上只有第 ④ 层有采集器——把接入状态写进表里，空卡才有解释。
    h += `<section><div class="card"><h3>归因口径说明</h3><div class="tbl-wrap"><table>
      <tr><th>层</th><th>数据源</th><th>可信度</th><th>接入状态</th></tr>
      <tr><td><b>① AI 爬虫抓取</b></td><td>官网/博客服务器日志（GPTBot / ClaudeBot / Bytespider… UA）</td><td>${sdot("ok", "高")}</td><td>${sdot("bad", "未接入：需先有官网日志源")}</td></tr>
      <tr><td><b>② AI 引荐访问</b></td><td>官网埋点 → <code>traffic_events</code></td><td>${sdot("warn", "中")}</td><td>${sdot("bad", "未接入：需官网埋点")}</td></tr>
      <tr><td><b>③ AI 答案被引</b></td><td>题库问五引擎 → <code>ai_citations</code></td><td>${sdot("ok", "高")}</td><td>${sdot("bad", "未接入：周探测未落地")}</td></tr>
      <tr><td><b>④ 平台后台数据</b></td><td>各平台创作者后台 XHR 拦截（<code>metrics_crawler.py</code>）</td><td>${sdot("ok", "高")}</td><td>${sdot("ok", "已接入：热力表 / 阅读卡")}</td></tr>
      </table></div><p class="foot">前三层的空卡不是「业务为零」，是「还没有采集器」——它们依赖官网侧日志与埋点，不在本机浏览器自动化的能力范围内。</p></div></section>`;
  }
  return h;
}

/** metricsSummary → 覆盖 KPI（有真数才覆盖，取不到沿用 mock）。 */
export function mergeKpi(base: typeof MOCK.kpi, d7?: MediaKpi, d30?: MediaKpi): typeof MOCK.kpi {
  const out = { ...base };
  if (d7) { out.pub7 = d7.published; out.runs = d7.runs; out.runOk = Math.round(d7.successRate * 100) + "%"; }
  if (d30) { out.pub30 = d30.published; out.token = (d30.tokens / 1e6).toFixed(1) + "M"; out.llmCost = "¥" + d30.cost.toFixed(1); }
  return out;
}

/* ── 审批队列 ─────────────────────────────────────────────────────────── */
export function vApprovalsHtml(): string {
  let h = title("审批队列", "总控 / HITL 闸门");
  h += `<section><h3>审批即投递</h3><div class="flow">
      <div class="step">① 点「通过」</div><span class="arr">→</span>
      <div class="step">② 平台投递引擎</div><span class="arr">→</span>
      <div class="step">③ 存草稿 + 等回执</div><span class="arr">→</span>
      <div class="step ok">④ <b>保窗预览</b></div><span class="arr">→</span>
      <div class="step">⑤ 发布登记</div></div></section>`;
  return h;
}

/* ── 自动规划（M9） ───────────────────────────────────────────────────── */
/** 自动规划的设计稿区块（标题与真实排期表由 vAutopilot.vue 渲染在其上方）。 */
export function vAutopilotHtml(): string {
  let h = "";
  {
    h += `<section><div class="grid g3">
      <div class="card stat"><h3>本周提案</h3><div class="num nil">—</div><div class="sub"></div></div>
      <div class="card stat"><h3>观察期中</h3><div class="num nil">—</div><div class="sub"></div></div>
      <div class="card stat"><h3>本月自动回滚</h3><div class="num nil">—</div><div class="sub"></div></div>
      </div></section>
      <section><div class="card"><h3>可编辑记忆</h3><ul>
      <li>${lnk("brain", "大脑·进化 / insight 卡库", "cards")}</li>
      <li>${lnk("questions", "题库")}</li>
      <li>${lnk("kb", "知识库")}</li></ul></div></section>`;
  }
  {
    h += `<section><h3>规划回路</h3><div class="flow">
      <div class="step">① 选题规划</div><span class="arr">→</span>
      <div class="step">② 主稿产出</div><span class="arr">→</span>
      <div class="step evo">③ 账号分配</div><span class="arr">→</span>
      <div class="step evo">④ 变体生成</div><span class="arr">→</span>
      <div class="step">⑤ 分发计划进审批<small>${lnk("accounts", "账号矩阵 / 分布式发送")}</small></div></div></section>`;
  }
  {
    h += `<section><h3>决策回路</h3><div class="flow">
      <div class="step">读度量</div><span class="arr">→</span>
      <div class="step">读记忆</div><span class="arr">→</span>
      <div class="step evo">生成策略变更提案</div><span class="arr">→</span>
      <div class="step">分级执行</div><span class="arr">→</span>
      <div class="step">观察期</div></div></section>`;
  }
  {
    h += `<section><div class="card"><h3>三级风险分级</h3><div class="tbl-wrap"><table>
      <tr><th style="width:110px">风险级</th><th>范围</th><th>处置</th></tr>` +
      MOCK.policy.map(([lv, sc, ac]) => `<tr><td><span class="badge b-${lv.toLowerCase()}">${lv}${lv === "L1" ? " 自动生效" : lv === "L2" ? " 人工审批" : " 硬禁止"}</span></td><td>${sc}</td><td>${ac}</td></tr>`).join("") +
      `</table></div></div></section>`;
  }
  {
    h += `<section><div class="card"><h3>触发式调配示例</h3><div class="tbl-wrap"><table>
      <tr><th>触发</th><th>提案</th><th>级别</th></tr>
      <tr><td>探测发现「知乎被引率连涨 3 周」</td><td>知乎周篇数 3→4，并把知乎打法萃取成 playbook 卡供他平台参考</td><td><span class="badge b-l1">L1</span></td></tr>
      <tr><td>某专家连续 5 篇一次过审率 &lt;40%</td><td>回滚其 prompt 到上一版；再不行换模型档</td><td><span class="badge b-l1">L1</span> → <span class="badge b-l2">L2</span></td></tr>
      <tr><td>token 日消耗异常</td><td>立即降并行数并告警，沉淀 anti_pattern 卡</td><td><span class="badge b-l1">L1</span></td></tr>
      <tr><td>发现头条团缺「标题 AB 测试」能力</td><td>从白名单货架提案安装对应 skill</td><td><span class="badge b-l2">L2</span></td></tr>
      </table></div></div></section>`;
  }
  return h;
}

/** 定时任务表的设计稿部分：规划中的系统级任务（尚未接真，真实排期表在其上方）。 */
export function cronDesignHtml(): string {
  return `<section><div class="card"><h3>系统级定时任务（设计稿 · 尚未接真）</h3><div class="tbl-wrap"><table>
      <tr><th>时间</th><th>任务</th><th>干什么</th><th>状态</th></tr>` +
    // 状态一律标「未接真」：这一表整体就是设计稿，逐行点绿灯会让人以为有 11 个定时任务在跑，
    // 而实际在跑的只有排期巡检那一个。
    MOCK.cron.map((r) => `<tr><td style="white-space:nowrap">${r[0]}</td><td><b>${r[1]}</b></td><td>${r[2]}</td><td style="white-space:nowrap">${sdot("idle", "未接真")}</td></tr>`).join("") +
    `</table></div><p class="foot">此表为目标形态的设计稿；已接真的是上方「各平台发文排期」（后端 30 分钟一轮巡检）。</p></div></section>`;
}

/* ── 大脑·进化（M10，数据来自 useEvolution） ─────────────────────────── */
export function vBrainHtml(evo: EvolutionData): string {
  let h = "";
  {
    h += `<section><div class="card"><div class="legend" style="margin:0 0 12px">
        <span><i style="background:var(--s1)"></i>Prompt 进化</span><span><i style="background:var(--s2)"></i>Skill 进化</span>
        <span><i style="background:var(--s3)"></i>专家团进化</span><span><i style="background:var(--s4)"></i>调度进化</span></div>
      <div class="tl">` + evo.timeline.map(([cls, when, what, detail, st]) => {
      const lv = st === "已固化" ? "ok" : st === "已回滚" ? "bad" : "warn";
      return `<div class="tlitem ${cls}"><div class="when">${when} · ${sdot(lv, "<b>" + st + "</b>")}</div>
        <div class="what"><b>${what}</b><br>${detail}</div></div>`;
    }).join("") +
      `</div></div></section>`;
  }
  {
    const bc: Record<string, string> = { anti_pattern: "b-anti", rule: "b-rule", playbook: "b-play" };
    h += `<section><div class="card"><h3>insight 卡库</h3><div class="tbl-wrap"><table>
      <tr><th>类型</th><th>标题</th><th>内容</th><th>范围</th><th>功劳分</th><th>日期</th></tr>` +
      evo.cards.map((r) => `<tr><td><span class="badge ${bc[r[0]]}">${r[0]}</span></td><td><b>${r[1]}</b></td><td>${r[2]}</td><td>${r[3]}</td><td style="white-space:nowrap">${r[4]}</td><td>${r[5]}</td></tr>`).join("") +
      `</table></div></div></section>`;
  }
  {
    h += `<section><div class="card"><h3>prompt 版本树</h3><div class="tbl-wrap"><table>
      <tr><th>专家（补丁）</th><th>版本</th><th>状态与绩效</th><th>变更 / 结局</th></tr>` +
      evo.tree.map((r) => `<tr><td>${r[0]}</td><td><b>${r[1]}</b></td><td>${r[2]}</td><td>${r[3]}</td></tr>`).join("") +
      `</table></div></div></section>`;
  }
  {
    const f = evo.flywheel;
    h += `<section><div class="grid g3">
      <div class="card stat"><h3>飞轮健康度</h3><div class="num" style="color:var(--ok)">${f.health}</div><div class="sub"></div></div>
      <div class="card stat"><h3>本月 insight 卡</h3><div class="num">${f.cardsThisMonth}<small>张</small></div><div class="sub"></div></div>
      <div class="card stat"><h3>进化固化 / 回滚</h3><div class="num">${f.solidified}<small> / ${f.rolledBack}</small></div><div class="sub">${f.overdue > 0 ? `<b style="color:var(--bad)">${f.overdue} 条观察期超 7 天未裁决</b>` : ""}</div></div>
      </div></section>
      <section><div class="card"><h3>本月 ${f.evidence.length} 条证据</h3><ol>` +
      f.evidence.map((e) => `<li>${e}</li>`).join("") +
      `</ol></div></section>`;
  }
  {
    h += `<section><h3>主 Loop = 生产环 ⊕ 进化环</h3><div class="flow">
      <div class="step">生产环（日频）</div>
      <span class="arr">⇄</span>
      <div class="step evo">进化环（周频 + 事件）</div></div></section>
      <section><div class="card"><div class="tbl-wrap"><table>
      <tr><th style="width:150px">进化环步骤</th><th>做什么</th><th>产物</th></tr>
      <tr><td><b>① 度量汇集</b></td><td>周探测 SoV、三张清单、流量归因、专家绩效、门禁统计</td><td>周度量包</td></tr>
      <tr><td><b>② 反思（双人）</b></td><td>analytics-engineer 找因果假设；critic 负责证伪</td><td>反思纪要</td></tr>
      <tr><td><b>③ 沉淀 insight 卡</b></td><td>三类：anti_pattern / rule / playbook</td><td><code>insight_cards</code></td></tr>
      <tr><td><b>④ 四类进化执行</b></td><td>Prompt / Skill / 专家团 / 调度</td><td><code>evolution_log</code> + 提案</td></tr>
      <tr><td><b>⑤ 验证与固化</b></td><td>7 天 AB 观察，达预期→固化；未达→自动回滚</td><td>固化/回滚记录</td></tr>
      </table></div></div></section>`;
  }
  return h;
}

/* ── 题库（探测 + 选题池） ────────────────────────────────────────────── */
export function vQuestionsHtml(): string {
  let h = title("题库", "资源 / 探测题 + 选题池");
  {
    const rows = Object.entries(MOCK.questions).flatMap(([k, v]) => v.map((r) => [P(k)?.name ?? k, ...r]));
    h += `<section><div class="card"><h3>题库与选题池</h3><div class="tbl-wrap"><table>
      <tr><th>平台</th><th>问题</th><th>主打引擎</th><th>上次探测结果</th><th>归入清单</th></tr>` +
      (rows.length
        ? rows.map((r) => `<tr>${r.map((c) => `<td>${c}</td>`).join("")}</tr>`).join("")
        : `<tr><td colspan="5"><p class="empty">暂无数据</p></td></tr>`) +
      `</table></div></div></section>`;
  }
  {
    h += `<section><h3>三张清单</h3><div class="grid g3">
      <div class="card"><h3>被引清单</h3><p class="empty">暂无数据</p></div>
      <div class="card"><h3>纠错清单</h3><p class="empty">暂无数据</p></div>
      <div class="card"><h3>缺口清单</h3><p class="empty">暂无数据</p></div>
      </div></section>`;
  }
  {
    h += `<section><div class="card"><h3>周探测机制</h3><div class="flow">
      <div class="step">题库问五引擎</div><span class="arr">→</span>
      <div class="step">规则打分</div><span class="arr">→</span>
      <div class="step">模型复判</div><span class="arr">→</span>
      <div class="step">入 <code>ai_citations</code></div></div></section>`;
  }
  return h;
}

/* ── 投递引擎（M2） ───────────────────────────────────────────────────── */
export function vEngineHtml(): string {
  let h = title("投递引擎", "系统 / 投递矩阵");
  {
    h += `<section><h3>保窗机制</h3><div class="flow">
      <div class="step">detached Chrome 独立拉起</div><span class="arr">→</span>
      <div class="step">CDP 接管</div><span class="arr">→</span>
      <div class="step ok">断连不关窗，窗口常驻</div></div></section>`;
  }
  {
    h += `<section><h3>投递全链路</h3><div class="flow">
      <div class="step">① 开编辑页</div><span class="arr">→</span>
      <div class="step">② 登录检测</div><span class="arr">→</span>
      <div class="step">③ 填标题 + 粘贴正文</div><span class="arr">→</span>
      <div class="step">④ 配图</div><span class="arr">→</span>
      <div class="step">⑤ 存草稿 + 等回执</div><span class="arr">→</span>
      <div class="step ok">⑥ <b>断连保窗</b></div></div></section>`;
  }
  {
    h += `<section><div class="card"><h3>七平台「最后一步」卡点总账（真机记录，改版只改选择器）</h3><div class="tbl-wrap"><table>
      <tr><th>平台</th><th>最后一步长什么样</th><th>卡点（真机）</th><th>处置状态</th></tr>
      <tr><td><b>头条号</b></td><td>无存草稿按钮，页脚自动保存</td><td>老配置点按钮落空 → Ctrl+S 兜底弹浏览器保存框且不存草稿；回执长期停「保存中...」不 settle</td><td>${sdot("ok", "")}改 auto_save 只等页脚回执，禁 Ctrl+S；「保存中」兜底接受</td></tr>
      <tr><td><b>百家号</b></td><td>点「存草稿」+ 封面弹窗</td><td>封面弹窗要 hover 出「更换」才开（旧入口文字已消失 → 流程从未真开过）；input[type=file] 是视频框；确认键异步启用</td><td>${sdot("ok", "")}hover → 验证弹窗真开 → 只认 accept=image → 轮询确定键</td></tr>
      <tr><td><b>抖音图文</b></td><td><b>没有草稿箱</b>，只有发布键</td><td>最后一步天然缺失；上传区无 input[type=file] 必须 file_chooser</td><td>${sdot("ok", "")}只填充绝不点发布，保窗等人工核对 = 唯一正确收尾</td></tr>
      <tr><td><b>知乎</b></td><td>自动存草稿（顶部回执）</td><td>Clash 规则模式下连接被重置（网络层，非 DOM）</td><td>${sdot("warn", "")}待网络：直连或加直连规则即恢复；题图专用流程待做</td></tr>
      <tr><td><b>B站专栏</b></td><td>点「存草稿」</td><td>编辑器迁移 + 反自动化：#/new 空白、#/web 元素 visibility:false、「前往」不跳转</td><td>${sdot("warn", "")}partial：自动填标题 + 正文进剪贴板人工贴；等平台迁移稳定</td></tr>
      <tr><td><b>公众号</b></td><td>壹伴链路存草稿 + 封面</td><td>封面弹窗按钮排在视口外，点击静默失败；channel=chrome 段错误</td><td>${sdot("ok", "")}1600×1300 视口 + 设备模拟；CDP 接管（本方案源头）</td></tr>
      <tr><td><b>小红书</b></td><td>post-to-xhs 只填不发</td><td>无近期卡点</td><td>${sdot("ok", "")}专用链路稳定</td></tr>
      <tr><td colspan="2"><b>通用（所有平台）</b></td><td>① 保存回执只等 12s 常报「未见明确回执」→ 文案引导窗口目检；② 贴图曾把「选中态正文」整段替换掉（ProseMirror/Draft 有自己的选区模型，JS 层 collapse 无效）→ 已改真实光标 Ctrl+Home 收拢选区；③ <b>脚本一退窗口就关</b>，预览做不了</td><td>${sdot("ok", "")}③ 为本次核心修复：CDP 保窗，两阶段验证通过</td></tr>
      </table></div></div></section>`;
  }
  {
    h += `<section><div class="card"><h3>平台 × 方案选型</h3><div class="tbl-wrap"><table>
      <tr><th>平台</th><th>投递方式</th><th>适配</th><th>说明</th></tr>` +
      PLATFORMS.map((p) => {
        const b = { full: "b-full", partial: "b-partial", delegate: "b-delegate", planned: "b-planned" }[p.adapter];
        return `<tr><td><b>${p.name}</b></td><td>${p.engine}</td><td><span class="badge ${b}">${p.adapterText}</span></td><td>${p.redline}</td></tr>`;
      }).join("") +
      `</table></div></div></section>`;
  }
  return h;
}

/* ── 质检门禁（M4） ───────────────────────────────────────────────────── */
export function vGateHtml(): string {
  let h = title("质检门禁", "系统 / 评分器与两级门禁");
  {
    h += `<section><div class="card"><h3>GEO 九信号评分器</h3><div class="tbl-wrap"><table>
      <tr><th>信号</th><th class="num-cell">权重</th><th>判定</th><th>实现</th></tr>` +
      MOCK.scorer.map(([n, w, d, i]) => `<tr><td><b>${n}</b></td><td class="num-cell">${w}</td><td>${d}</td><td>${i}</td></tr>`).join("") +
      `<tr><td colspan="1"><b>合计</b></td><td class="num-cell"><b>100</b></td><td colspan="2"></td></tr>
      </table></div></div></section>`;
  }
  {
    h += `<section><div class="card"><h3>error 级（直接拦）</h3><div class="grid g2"><div><ol>` + MOCK.gateErr.slice(0, 6).map((x) => `<li>${x}</li>`).join("") + `</ol></div>
      <div><ol start="7">` + MOCK.gateErr.slice(6).map((x) => `<li>${x}</li>`).join("") + `</ol></div></div></div></section>`;
  }
  {
    h += `<section><div class="card"><h3>warning 级（放行但留痕）</h3><div class="grid g2"><div><ol>` + MOCK.gateWarn.slice(0, 5).map((x) => `<li>${x}</li>`).join("") + `</ol></div>
      <div><ol start="6">` + MOCK.gateWarn.slice(5).map((x) => `<li>${x}</li>`).join("") + `</ol></div></div></div></section>`;
  }
  return h;
}

/* ── 排版中心 ─────────────────────────────────────────────────────────── */
export function vLayoutHtml(): string {
  let h = title("排版中心", "系统 / 配图与排版");
  {
    h += `<section><div class="card"><h3>平台封面规格表</h3><div class="tbl-wrap"><table>
      <tr><th>平台</th><th>封面规格</th><th>上传通道</th></tr>` +
      PLATFORMS.map((p) => `<tr><td><b>${p.name}</b></td><td>${p.cover}</td><td>${p.adapter === "delegate" ? "专用链路" : p.id === "baijia" ? "设置封面弹窗（accept=image）" : p.id === "douyin" ? "file_chooser 图库（首图即封面）" : "正文首图（平台自动采用）"}</td></tr>`).join("") +
      `</table></div></div></section>`;
  }
  {
    h += `<section><div class="card"><h3>版式与主题参数</h3><ul>
      <li><b>guizang-social-card</b>：小红书图卡 + 公众号封面对</li>
      <li><b>md2wechat</b>：公众号正文主题</li></ul></div></section>`;
  }
  {
    h += `<section><div class="card"><h3>排版微调入口</h3><p>稿件卡 →「配图排版」步可打开排版预览器，切换版式/主题参数，保存为该平台默认。</p></div></section>`;
  }
  return h;
}

/* ── API 中心 · 静态子标签（tier；chan/img 由 vApi 组件接真） ─────────── */
export function vApiTierHtml(): string {
  return `<section><div class="card"><h3>模型分层</h3><div class="tbl-wrap"><table>
      <tr><th>档</th><th>用途</th><th>专家</th></tr>
      <tr><td><b>writer 档</b>（便宜模型）</td><td>生产：选题/调研/写作/排版/投递</td><td>content-strategist, news-researcher, writer, typesetter, publisher…</td></tr>
      <tr><td><b>reviewer 档</b>（贵模型）</td><td>评审判分：七维评审/杠精/事实核查/度量分析</td><td>reviewer, critic-strategist, fact-checker, analytics-engineer</td></tr>
      </table></div></div></section>`;
}

/* 门户抬头（标题 + 适配/登录态条）。门户正文只剩「工作流 + 专家团补丁」两块，
   都吃真数据、都可交互，故由 vPortal 组件直接渲染，这里不再有静态区块。 */

/** 门户标题块。单拆出来是因为 vPortal 要把它与右上角两颗主功能大按钮排成同一行。 */
export function portalTitleHtml(pid: string): string {
  const p = P(pid);
  if (!p) return "";
  return title(`${p.name} 门户`, `媒体门户 / 独立工作台 —— 主打 ${p.ai}`);
}

/** 标题以下的抬头：只剩异常告警。
   原先这里还有一条「适配 / 引擎 / 登录态」信息条 —— 三样都在下面说过第二遍
   （登录态在工作流那行开关里、适配与引擎在账号矩阵里），一条只读横幅白占一屏高度，撤掉。 */
export function portalHeaderHtml(pid: string): string {
  const p = P(pid);
  if (!p) return `<div class="callout r">请从顶栏「媒体门户」选一个平台</div>`;
  if (p.login === "none") return `<div class="callout"><b>本门户尚未接入：</b>${p.engine}</div>`;
  if (p.login !== "ok") return `<div class="callout r"><b>告警：</b>${p.loginNote}</div>`;
  return "";
}

