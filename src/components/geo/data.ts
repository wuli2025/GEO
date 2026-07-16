/**
 * GEO 自媒体运营中心 · 静态数据与图标集（移植自设计稿 v2）
 *
 * 纯数据 + 无副作用的 HTML 小工具（ico / sdot / esc）。视图渲染函数见 render.ts，
 * 图表见 charts.ts。凡是能接真后端的地方由各视图组件在运行时覆盖这里的 mock。
 */

/* ── 自建图标集：单一笔触语言（24 栅格 · 1.6 描边 · round cap/join · currentColor）── */
export const ICONS: Record<string, string> = {
  board:
    '<path d="M4 19.5h16"/><path d="M7.5 19.5V11"/><path d="M12 19.5V4.5"/><path d="M16.5 19.5V8"/>',
  approve:
    '<path d="M6 4h8.5L19 8.5V20H6z"/><path d="M14.5 4v4.5H19"/><path d="M9 14l2 2 4-4.5"/>',
  plan: '<circle cx="12" cy="12" r="8"/><path d="M15.2 8.8l-2.4 5.6-5.6 2.4 2.4-5.6z"/>',
  brain:
    '<circle cx="6" cy="17" r="2.2"/><circle cx="12.5" cy="6.5" r="2.2"/><circle cx="18" cy="15.5" r="2.2"/><path d="M7.4 15.3l3.9-6.8"/><path d="M14.3 8.1l2.8 5.4"/><path d="M8.2 17.3l7.6-1.4"/>',
  experts:
    '<circle cx="9.5" cy="8" r="3.2"/><path d="M4 19.5a5.5 5.5 0 0 1 11 0"/><path d="M16.2 5.6a3.2 3.2 0 0 1 0 6"/><path d="M17.6 14a5.5 5.5 0 0 1 2.9 5.5"/>',
  kb: '<path d="M12 3.5L20.5 8 12 12.5 3.5 8z"/><path d="M3.5 12L12 16.5 20.5 12"/><path d="M3.5 16L12 20.5 20.5 16"/>',
  ask: '<path d="M4 6.5h10"/><path d="M4 11.5h10"/><path d="M4 16.5h5.5"/><path d="M13.6 15.2a2.3 2.3 0 1 1 2.7 2.3v1.1"/><circle cx="16.3" cy="21" r=".85" fill="currentColor" stroke="none"/>',
  send: '<path d="M4.5 15v3.5A1.5 1.5 0 0 0 6 20h12a1.5 1.5 0 0 0 1.5-1.5V15"/><path d="M12 4v11"/><path d="M8 8l4-4 4 4"/>',
  gate: '<path d="M4.5 5h15l-5.8 7v6.2l-3.4 2v-8.2z"/><path d="M10.3 12h3.4"/>',
  type: '<rect x="4.5" y="4" width="15" height="16" rx="1.5"/><path d="M8.5 8.5h7"/><path d="M12 8.5V16"/>',
  api: '<path d="M10.6 13.4a3.6 3.6 0 0 0 5.2 0l2.6-2.7a3.6 3.6 0 0 0-5.1-5.1l-1.3 1.3"/><path d="M13.4 10.6a3.6 3.6 0 0 0-5.2 0l-2.6 2.7a3.6 3.6 0 0 0 5.1 5.1l1.3-1.3"/>',
  matrix:
    '<rect x="4" y="4" width="7" height="7" rx="1.6"/><rect x="13" y="4" width="7" height="7" rx="1.6"/><rect x="4" y="13" width="7" height="7" rx="1.6"/><rect x="13" y="13" width="7" height="7" rx="1.6"/>',
};

/** 图标 → svg 字符串（aria-hidden，紧邻文字标签）。 */
export const ico = (n: string): string =>
  `<svg class="i" viewBox="0 0 24 24" aria-hidden="true">${ICONS[n] || ""}</svg>`;

/** 状态点：点承载颜色，紧邻文字承载含义（颜色永不单独表意）。 */
export const sdot = (lv: string, txt: string): string =>
  `<span class="sline"><span class="sdot ${lv}"></span>${txt}</span>`;

/** 极简 HTML 转义（用于把 mock 里的裸文本塞进 v-html）。 */
export function esc(s: unknown): string {
  return String(s)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

/* ── 平台（M1 项目模型 + M2 投递 + 卡点档案） ───────────────────────── */
export type LoginState = "ok" | "warn" | "bad" | "none";
export type AdapterKind = "full" | "partial" | "delegate" | "planned";

export interface Platform {
  id: string;
  name: string;
  adapter: AdapterKind;
  adapterText: string;
  engine: string;
  login: LoginState;
  loginNote: string;
  ai: string;
  weekPlan: number;
  pending: number;
  sendMode: "auto" | "manual";
  cover: string;
  style: string;
  redline: string;
  patch: string;
  blockers: [string, string][];
  cmd: string;
}

export const PLATFORMS: Platform[] = [
  {
    id: "wechat", name: "公众号", adapter: "delegate", adapterText: "专用链路",
    engine: "wechat_yiban.py（壹伴排版 + 封面直传 + CDP 保窗）",
    login: "ok", loginNote: "~/.polaris-mp-profile 常驻，扫码一次长期有效",
    ai: "腾讯元宝", weekPlan: 2, pending: 1, sendMode: "auto",
    cover: "900×383（次条 200×200）", style: "1500字+结构化长文 · 小标题分层 · 每节独立结论 · 必点「声明原创」 · 周更一篇好过日更三条",
    redline: "企业认证 300 元/年，主体名与官网/事实库一致；标题党拉低被引意愿",
    patch: "主笔补丁=公众号长文体；排版补丁=md2wechat 主题 + 21:9 封面对",
    blockers: [
      ["封面弹窗按钮在视口外", "「下一步」排在 y≈997，1000 高视口 mouse.click 打在视口外静默失败（不报错不生效，极难排查）——已用 1600×1300 视口 + 设备指标模拟根治"],
      ["playwright channel=chrome 段错误", "Chrome 152 + 既有 profile 启动即崩(0xC0000005)，有头无头都崩——已改 detached Chrome + CDP 接管（本次保窗方案的源头）"]],
    cmd: 'python ~/PolarisGEO/skills/wechat-md-typesetter/scripts/wechat_yiban.py --mode publish --body-file 正文.html --title "标题" --cover 封面.png',
  },
  {
    id: "zhihu", name: "知乎", adapter: "full", adapterText: "full*",
    engine: "draft_uploader.py（Draft.js 粘贴通道 + 自动存草稿）",
    login: "warn", loginNote: "Clash 规则模式下到 zhuanlan 连接被重置（网络层卡点，非 DOM）",
    ai: "DeepSeek", weekPlan: 3, pending: 1, sendMode: "auto",
    cover: "题图待做专用流程", style: "答题为主、发文为辅 · 首段 50 字内给结论 · 专栏与回答互引 · 专家个人号信任度常高于机构号",
    redline: "硬广容忍度最低；判断标准：品牌名换成「某厂商」仍有干货才安全",
    patch: "主笔补丁=知乎答题体（首段结论+论证链）",
    blockers: [
      ["网络层：Clash 挡路", "规则模式下 zhuanlan.zhihu.com 连接被重置（TCP 通、HTTP 挂）；恢复直连或加直连规则即可用"],
      ["题图无专用流程", "正文贴图可用，题图（封面）待做专用 cover 配置"]],
    cmd: 'python ~/PolarisGEO/skills/media-publisher/scripts/draft_uploader.py --platform zhihu --title "标题" --content-file 正文.md',
  },
  {
    id: "toutiao", name: "头条号", adapter: "full", adapterText: "full+封面",
    engine: "draft_uploader.py（ProseMirror + 页脚自动保存回执）",
    login: "ok", loginNote: "browser-profiles/toutiao 常驻",
    ai: "豆包", weekPlan: 3, pending: 2, sendMode: "auto",
    cover: "690×388（塞正文首图，平台自动采用）", style: "标题口语化半档 · 首段直给结论 · 与抖音企业号绑定：视频讲是什么、长文讲怎么做 · 被引与粉丝量无关",
    redline: "字节系内部关联信号有信源加成；同稿多发需与百家号差异化",
    patch: "主笔补丁=头条口语化半档；投递补丁=auto_save 页脚回执",
    blockers: [
      ["新版编辑器砍掉「存草稿」按钮", "改页脚 span.footer-tip-save 自动保存；老配置点按钮→落空→Ctrl+S 兜底弹出浏览器保存框且根本不存草稿。已改 auto_save：只等页脚回执，绝不再发 Ctrl+S"],
      ["回执长期停在「草稿保存中...」", "实测 20s+ 不 settle 成「已保存」——它的存在即证明编辑器已接管并持续写草稿，兜底接受为回执"],
      ["本地 Chrome 偶发一启动就退", "Target closed——引擎链自动回退（现为 CDP → channel=chrome → Cloak → chromium）"]],
    cmd: 'python ~/PolarisGEO/skills/media-publisher/scripts/draft_uploader.py --platform toutiao --title "标题" --content-file 正文.md --images 封面.png',
  },
  {
    id: "baijia", name: "百家号", adapter: "full", adapterText: "full+封面",
    engine: "draft_uploader.py（React 新编辑器 + 设置封面弹窗）",
    login: "ok", loginNote: "browser-profiles/baijia 常驻",
    ai: "文心", weekPlan: 2, pending: 0, sendMode: "auto",
    cover: "单图 3:2（弹窗内自动裁）", style: "问题式标题 · 首段结论 · 分小标题 · 领域垂直 · 周 1–2 篇稳定更新",
    redline: "蓝 V 600 元/年；与头条稿标题/开头必须差异化（防判搬运，流水线内置改写步）",
    patch: "主笔补丁=百家号问题式标题 + 差异化改写",
    blockers: [
      ["封面弹窗从未真正打开过", "旧配置点「选择封面」——该文字已不存在；真入口是 hover 缩略图冒出「更换」。已改 hover→点开→验证弹窗真开（点中标签 span 的假成功会被 _modal_open 筛掉），3 轮重试"],
      ["input[type=file] 是视频框", "喂图报「视频格式不正确」——封面只认 input[accept=image/*]，代码里写死绝不回退"],
      ["确认键异步启用", "上传/裁剪异步，盲等要么白等要么点到禁用键——改轮询 12s「确定」可点即点"],
      ["标题不是 input", "React 版标题是主帧唯一 div[contenteditable]，旧 input/textarea 选择器全落空"]],
    cmd: 'python ~/PolarisGEO/skills/media-publisher/scripts/draft_uploader.py --platform baijia --title "标题" --content-file 正文.md --images 封面.png',
  },
  {
    id: "xhs", name: "小红书", adapter: "delegate", adapterText: "专用链路",
    engine: "post-to-xhs 技能（图文/视频全流程、登录检查、只填不发）",
    login: "ok", loginNote: "独立登录检查（%LOCALAPPDATA% Chrome profile）",
    ai: "个人/小微客户", weekPlan: 2, pending: 0, sendMode: "auto",
    cover: "3:4 图卡", style: "标题带数字和场景 · 图卡为主 · 定位是改编分发，不是新生产线",
    redline: "主阵地连续 8 周稳定更新且出现引用后再开副阵地",
    patch: "主笔补丁=小红书笔记体；图卡补丁=guizang-social-card 3:4 版式",
    blockers: [["无近期卡点", "走 post-to-xhs 专用链路，图文/视频全流程可用"]],
    cmd: "（走 post-to-xhs 技能：图文/视频全流程）",
  },
  {
    id: "bilibili", name: "B站专栏", adapter: "partial", adapterText: "partial",
    engine: "draft_uploader.py（剪贴板辅助，标题可自动填）",
    login: "ok", loginNote: "browser-profiles/bilibili 常驻",
    ai: "AI 读文字层（标题+简介）", weekPlan: 1, pending: 0, sendMode: "manual",
    cover: "960×600", style: "标题+简介按证据文标准写（AI 读的是文字层）· 长内容沉淀",
    redline: "副阵地，改编分发为主",
    patch: "主笔补丁=B站专栏体；投递补丁=剪贴板辅助流程",
    blockers: [
      ["编辑器迁移 + 反自动化", "旧 #/new 已下线（SPA 空白）；新 #/web 弹「旧版编辑器已停止使用」模态，标题 textarea 与 div.ql-editor 虽在 DOM 但 visibility:false，点「前往」same-page 不跳转——本地 Chrome 与 CloakBrowser 均如此"],
      ["现行方案", "partial：自动填标题 + 正文进剪贴板，人工一次 Ctrl+V；待 B站迁移稳定或另寻 CDP/坐标方案"]],
    cmd: 'python ~/PolarisGEO/skills/media-publisher/scripts/draft_uploader.py --platform bilibili --title "标题" --content-file 正文.md',
  },
  {
    id: "douyin", name: "抖音图文", adapter: "full", adapterText: "full+图库",
    engine: "draft_uploader.py（semi-input + file_chooser 图库）",
    login: "ok", loginNote: "browser-profiles/douyin 常驻",
    ai: "豆包（字节系信源加成）", weekPlan: 2, pending: 0, sendMode: "auto",
    cover: "图库首图即封面", style: "与头条号绑定：视频讲是什么、长文讲怎么做 · 描述按证据文写",
    redline: "无草稿箱，只填不发；定时发布设远期 ≈ 草稿（本系统不用）",
    patch: "投递补丁=file_chooser 图库上传；正文只放作品描述",
    blockers: [
      ["上传区没有 input[type=file]", "点击直接触发系统对话框——必须 expect_file_chooser 捕获；图走图库上传，不塞正文"],
      ["没有草稿箱", "页面只有「发布」——脚本只填充绝不点发布，最后一步天然留给人工；填完保窗等核对是唯一正确收尾"],
      ["固定视口错位死区", "1600×1000 固定视口 vs 更大窗口，鼠标滚不到发布栏——已改真实窗口最大化（no_viewport / --start-maximized）"]],
    cmd: 'python ~/PolarisGEO/skills/media-publisher/scripts/draft_uploader.py --platform douyin --title "标题" --content-file 描述.md --images a.png,b.png',
  },
  {
    id: "csdn", name: "CSDN", adapter: "planned", adapterText: "待接入",
    engine: "计划走 Wechatsync driver 或 playwright 移植（草稿箱）",
    login: "none", loginNote: "适配未落地，接入后建 profile",
    ai: "DeepSeek（技术问答信源靠前）", weekPlan: 1, pending: 0, sendMode: "manual",
    cover: "待定", style: "技术实施笔记 / 踩坑记录改编分发 · 代码块与报错原文是最贵的独有信息",
    redline: "副阵地：主阵地稳定后再开；同稿需与掘金差异化",
    patch: "主笔补丁=技术笔记体（问题→环境→复现→修法→结论）",
    blockers: [
      ["【预判 · 未验证】编辑器为自研 Markdown", "预计可走粘贴通道；需真机 DOM 探测后再定选择器"],
      ["【预判 · 未验证】草稿接口", "CSDN 有草稿箱，预计可自动存草稿；未验证前先按 manual 走"]],
    cmd: "（尚未接入：适配完成后走 draft_uploader.py --platform csdn）",
  },
  {
    id: "juejin", name: "掘金", adapter: "planned", adapterText: "待接入",
    engine: "计划走 Wechatsync driver（草稿箱）",
    login: "none", loginNote: "适配未落地，接入后建 profile",
    ai: "DeepSeek", weekPlan: 1, pending: 0, sendMode: "manual",
    cover: "待定", style: "技术长文 · 与 CSDN 同源但标题/开头必须差异化",
    redline: "副阵地；与 CSDN 同稿多发需差异化（防判搬运）",
    patch: "主笔补丁=掘金技术长文体",
    blockers: [
      ["【预判 · 未验证】编辑器 Markdown 原生", "预计粘贴通道可用"],
      ["【预判 · 未验证】需与 CSDN 差异化改写", "流水线需内置改写步，同头条↔百家号那条一样"]],
    cmd: "（尚未接入：适配完成后走 draft_uploader.py --platform juejin）",
  },
  {
    id: "channels", name: "视频号", adapter: "planned", adapterText: "待接入",
    engine: "计划走 social-auto-upload（storage_state 登录）",
    login: "none", loginNote: "适配未落地，接入后建 profile",
    ai: "腾讯元宝", weekPlan: 1, pending: 0, sendMode: "manual",
    cover: "待定（竖版封面）", style: "与公众号绑定：视频讲是什么、长文讲怎么做 · 标题与简介按证据文写（AI 读文字层）",
    redline: "视频生产另立专项；定时发布设远期 ≈ 草稿——但本系统不用该技巧，只填不发",
    patch: "投递补丁=视频号发布页流程",
    blockers: [
      ["【预判 · 未验证】视频生产链路缺失", "本项目目前只做图文，视频素材从哪来是前置问题，须先定"],
      ["【预判 · 未验证】无真正草稿箱", "同抖音：预计只能填充留人工，保窗等核对"]],
    cmd: "（尚未接入：视频生产专项完成后再排）",
  },
];
export const P = (id: string): Platform | undefined => PLATFORMS.find((x) => x.id === id);

/* ── M7 统一专家团（14 人，平台差异只体现在提示词补丁） ─────────────── */
export const EXPERTS: string[][] = [
  ["策略", "content-strategist", "选题战略官", "读缺口清单+题库定选题", "writer", "¥0.42", "92%", "各平台热点偏好、红线、选题角度"],
  ["策略", "competitor-watcher", "竞品哨兵", "采竞品爆文进 KB 做差距分析", "writer", "¥0.31", "—", "各平台竞品名单"],
  ["创作", "news-researcher", "情报员", "联网调研攒素材包", "writer", "¥0.58", "88%", "各平台信源偏好"],
  ["创作", "writer", "主笔", "按平台宪法写 1200–2500 字", "writer", "¥0.94", "78%", "平台文风宪法（知乎答题体/小红书笔记体/公众号长文体…）"],
  ["创作", "de-aiflavor", "AI痕迹优化师", "去「近日体」通稿味（中文平台敏感）", "writer", "¥0.22", "95%", "各平台 AI 味阈值"],
  ["创作", "image-director", "图卡导演", "封面/插图 prompt + 规格化", "writer", "¥0.35", "90%", "平台封面规格（900×383 / 3:4 / 690×388…）"],
  ["创作", "typesetter", "排版师", "平台排版 skill 套版", "writer", "¥0.08", "99%", "排版版式参数（guizang 主题 / md2wechat 主题）"],
  ["质检", "reviewer", "七维评审官", "七维打分，56/70 放行", "reviewer", "¥0.29", "—", "各平台门禁阈值"],
  ["质检", "critic-strategist", "杠精（换供应商模型）", "防自夸，强制跑另一家模型", "reviewer", "¥0.33", "—", "各平台敏感词侧重"],
  ["质检", "fact-checker", "事实核查官（闸门B）", "抽数字/引语回查出处", "reviewer", "¥0.26", "—", "品牌口径统一，不随平台变"],
  ["分发", "publisher", "投递员", "进草稿箱 + 顺带采集阅读数", "writer", "¥0.04", "92%", "平台草稿箱流程与选择器"],
  ["分发", "account-keeper", "账号管家", "登录态监控与过期告警", "writer", "¥0.02", "100%", "各平台登录态探测方式"],
  ["分析", "analytics-engineer", "度量分析师", "归因/月报/反思", "reviewer", "¥0.37", "—", "各平台指标口径"],
  ["分析", "geo-restructurer", "旧文改造师", "旧文改证据结构", "writer", "¥0.41", "85%", "各平台可改造范围"],
];

/* ── 顶栏板块定义（全部功能键） ─────────────────────────────── */
export interface Zone {
  label: string;
  keys: [string, string, string, string][]; // [id, icon, text, hotkey]
}
export const ZONES: Zone[] = [
  { label: "总控", keys: [
    ["dashboard", "board", "数据看板", "G"], ["approvals", "approve", "审批队列", "Q"],
    ["autopilot", "plan", "自动规划", "A"], ["brain", "brain", "大脑·进化", "B"]] },
  { label: "资源", keys: [
    ["experts", "experts", "专家阵容", "X"], ["accounts", "matrix", "账号矩阵", "M"],
    ["kb", "kb", "知识库", "K"], ["questions", "ask", "题库", "T"]] },
  { label: "系统", keys: [
    ["engine", "send", "投递引擎", "E"], ["gate", "gate", "质检门禁", "F"],
    ["layout", "type", "排版中心", "L"], ["api", "api", "API 中心", "I"]] },
];

export const SUBTABS: Record<string, [string, string][]> = {
  dashboard: [["kpi", "KPI 卡带"], ["traffic", "流量与 AI 来源"], ["cite", "五引擎提及率"], ["matrix", "平台×指标"], ["radar", "AI 爬虫雷达"], ["health", "日健康度"], ["attr", "归因口径说明"]],
  autopilot: [["policy", "AutopilotPolicy"], ["multi", "多账号分发"], ["loop", "决策回路"], ["risk", "三级风险分级"], ["cron", "定时任务表"], ["cases", "触发式调配示例"]],
  accounts: [["roster", "账号总表"], ["dispatch", "分布式发送"], ["risk2", "风控红线"]],
  brain: [["timeline", "进化时间线"], ["cards", "insight 卡库"], ["tree", "prompt 版本树"], ["flywheel", "飞轮健康度"], ["dual", "双环共轴"]],
  experts: [["roster", "阵容总表"], ["format", "专家文件格式"], ["perf", "绩效与编成进化"]],
  kb: [["base", "三层知识底座"], ["gates", "双闸门机制"], ["graph", "星图（M5）"]],
  questions: [["bank", "题库与选题池"], ["lists", "三张清单"], ["probe", "周探测机制"]],
  engine: [["keep", "CDP 保窗机制"], ["chain", "投递全链路"], ["ledger", "七平台卡点总账"], ["proto", "输出协议"], ["matrix2", "平台×方案选型"]],
  gate: [["scorer", "GEO 九信号评分器"], ["err", "error 11 条"], ["warn", "warning 9 条"], ["anti", "防应付设计"]],
  layout: [["cover", "平台封面规格"], ["theme", "版式与主题参数"], ["how", "人机分工（人调模板参数）"]],
  api: [["chan", "模型通道"], ["tier", "模型分层"], ["img", "生图通道"]],
  portal: [["board", "工作流"], ["qbank", "选题·题库"], ["plan", "规划队列"], ["acct", "账号·发送方式"], ["team", "专家团补丁"], ["blockers", "卡点档案"], ["style", "文风宪法"]],
};

/** hotkey → viewId */
export const KEYMAP: Record<string, string> = {};
ZONES.forEach((z) => z.keys.forEach((k) => { if (k[3]) KEYMAP[k[3]] = k[0]; }));

/* ── Mock 数据 ──────────────────────────────────────────────────────── */
interface Series { name: string; color: string; raw: string; v: number[] }

export const MOCK = {
  accounts: [
    ["toutiao", "Polaris 科技", "主号", "ok", "browser-profiles/toutiao", "9332", 2, 3],
    ["toutiao", "极星 GEO 观察", "矩阵", "ok", "browser-profiles/toutiao--jixing", "9342", 1, 2],
    ["toutiao", "答案引擎笔记", "矩阵", "warn", "browser-profiles/toutiao--notes", "9343", 1, 0],
    ["baijia", "Polaris 科技", "主号", "ok", "browser-profiles/baijia", "9334", 2, 2],
    ["baijia", "极星观察", "矩阵", "ok", "browser-profiles/baijia--jixing", "9344", 1, 1],
    ["douyin", "Polaris 科技", "主号", "ok", "browser-profiles/douyin", "9335", 2, 2],
    ["douyin", "极星图文", "矩阵", "none", "（未登录，待建）", "—", 1, 0],
    ["wechat", "Polaris 公众号", "主号", "ok", "~/.polaris-mp-profile", "9222", 1, 2],
    ["zhihu", "Polaris 知乎", "主号", "warn", "browser-profiles/zhihu", "9331", 2, 0],
    ["xhs", "Polaris 小红书", "主号", "ok", "%LOCALAPPDATA% profile", "—", 1, 2],
    ["bilibili", "Polaris B站", "主号", "ok", "browser-profiles/bilibili", "9333", 1, 1],
  ] as [string, string, string, string, string, string, number, number][],
  dispatch: [
    ["DP-0716-01", "AI 搜索时代，中小企业内容突围的 5 条路", "头条号 ×3 账号", "标题+首段 改写变体 ×3", "10:00 / 10:40 / 11:20（错峰 ≥30min）", "待审批"],
    ["DP-0715-02", "为什么你的品牌在 AI 答案里查无此人", "百家号 ×2 账号", "标题+开头 差异化 ×2", "16:00 / 16:45", "已投草稿（2/2 保窗）"],
    ["DP-0714-03", "3 张图看懂 GEO", "抖音 ×1（矩阵号未登录，跳过）", "—", "11:55", "已投（1/2，1 跳过）"],
  ] as string[][],
  kpi: {
    pub7: 12, runs: 31, runOk: "84%", pub30: 47, avgWords: "2,180", reads: "38,420", readsMom: "+12%",
    aiShare: "23%", aiBreak: "豆包9% · DeepSeek8% · 元宝6%", cost: "¥3.1", token: "89.2M", llmCost: "¥96.4",
    pending: 4, loginBad: 1,
  },
  traffic: {
    days: ["7-03", "7-04", "7-05", "7-06", "7-07", "7-08", "7-09", "7-10", "7-11", "7-12", "7-13", "7-14", "7-15", "7-16"],
    series: [
      { name: "自然流量", color: "var(--s1)", raw: "var(--s1)", v: [1720, 1680, 1590, 1810, 1900, 1850, 1780, 1930, 2010, 1970, 2050, 2120, 2080, 2160] },
      { name: "AI 引荐", color: "var(--s2)", raw: "var(--s2)", v: [210, 240, 260, 290, 330, 360, 400, 430, 470, 520, 560, 610, 650, 720] },
      { name: "爬虫抓取", color: "var(--s3)", raw: "var(--s3)", v: [90, 110, 95, 130, 150, 140, 160, 180, 175, 200, 220, 210, 240, 260] },
    ] as Series[],
  },
  engines: {
    weeks: ["W21", "W22", "W23", "W24", "W25", "W26", "W27", "W28"],
    series: [
      { name: "豆包", color: "var(--s1)", raw: "var(--s1)", v: [2, 3, 4, 5, 6, 7, 8, 9] },
      { name: "DeepSeek", color: "var(--s2)", raw: "var(--s2)", v: [1, 2, 2, 4, 5, 6, 7, 8] },
      { name: "元宝", color: "var(--s3)", raw: "var(--s3)", v: [3, 3, 4, 4, 5, 5, 6, 6] },
      { name: "文心", color: "var(--s4)", raw: "var(--s4)", v: [1, 1, 2, 2, 3, 3, 4, 4] },
      { name: "Kimi", color: "var(--s5)", raw: "var(--s5)", v: [0, 1, 1, 2, 2, 2, 3, 3] },
    ] as Series[],
  },
  heat: {
    cols: ["发布(篇/30天)", "阅读(次)", "被引(次)", "成本(¥/篇) ↓越低越好"],
    rows: [
      ["公众号", [9, "12,400", 14, "3.4"]], ["知乎", [12, "9,860", 11, "2.8"]], ["头条号", [11, "8,120", 9, "2.6"]],
      ["百家号", [7, "4,530", 6, "3.0"]], ["小红书", [5, "2,410", 2, "3.9"]], ["B站专栏", [2, "760", 1, "4.6"]], ["抖音图文", [1, "340", 0, "4.1"]],
    ] as [string, (string | number)[]][],
  },
  radar: [
    ["Bytespider（豆包）", 412], ["GPTBot", 286], ["DeepSeekBot", 203], ["Baiduspider-render（文心）", 178],
    ["Google-Extended", 96], ["YuanbaoBot（元宝）", 74], ["MoonshotBot（Kimi）", 41],
  ] as [string, number][],
  approvals: [
    ["头条号", "AI 搜索时代，中小企业内容突围的 5 条路", "87 / A", "命中 6 卡 · 数字全对上", "writer v4 · reviewer v2", "¥3.2", "07-16 09:12"],
    ["头条号", "豆包为什么总推荐你的竞品", "82 / B+", "命中 4 卡 · 1 处数字降级表述", "writer v4 · critic v1", "¥2.9", "07-16 08:40"],
    ["知乎", "如何评价生成式引擎优化（GEO）？", "84 / B+", "答题体 · 首段 43 字结论", "writer v3 · reviewer v2", "¥2.7", "07-15 22:05"],
    ["公众号", "优码云方法论拆解（下）", "91 / A", "已配封面 900×383", "writer v5 · typesetter v2", "¥3.6", "07-15 18:30"],
  ] as string[][],
  // 最近投递：最后一列为状态点 [level, text]
  recent: [
    ["07-16 10:42", "头条号", "GEO 是什么？AI 时代的新 SEO", "draft_uploaded · paste · 1863字", "页脚回执（保存中态）", ["ok", "保窗预览"]],
    ["07-15 16:03", "百家号", "为什么你的品牌在 AI 答案里查无此人", "draft_uploaded · 封面已设置", "已保存", ["ok", "保窗预览"]],
    ["07-15 15:20", "公众号", "一文说清生成式引擎优化", "壹伴排版 + 封面直传", "草稿已入库", ["ok", "CDP 保窗"]],
    ["07-14 11:55", "抖音图文", "3 张图看懂 GEO", "填充完成 · 图库2张", "无草稿箱·人工核对", ["ok", "保窗等发布"]],
    ["07-14 09:31", "知乎", "—", "need_login → 网络被重置", "失败（Clash）", ["bad", "待网络恢复"]],
    ["07-13 20:10", "B站专栏", "GEO 实施手册精读", "manual_assist", "剪贴板已备", ["warn", "人工 Ctrl+V"]],
  ] as [string, string, string, string, string, [string, string]][],
  health: [
    ["good", "探测线正常", "五引擎周探测 W28 已完成，判分与手工抽查一致率 93%"],
    ["good", "门禁拦截 2 篇", "1 篇正文<800字、1 篇>1500字无 FAQ（均 error 级）"],
    ["warning", "知乎登录/网络异常", "Clash 规则模式连接被重置，account-keeper 已告警"],
    ["good", "昨日配额 5/5 未超发", "全局日配额硬锁生效（事故一防线）"],
  ] as string[][],
  lanes: {
    wechat: [["选题", ["元宝引用率月报解读"]], ["调研", []], ["写作", ["优码云方法论拆解（下）"]], ["评审", []], ["质检", []], ["配图排版", []], ["待审批", ["一文说清 GEO"]], ["草稿箱", ["AI 答案入口迁移史"]]],
    zhihu: [["选题", ["GEO 和 SEO 的本质区别？", "被 AI 引用需要几步"]], ["调研", ["DeepSeek 信源偏好"]], ["写作", ["如何评价 GEO？"]], ["评审", []], ["质检", []], ["配图排版", []], ["待审批", []], ["草稿箱", ["RAG 知识闸门实践"]]],
    toutiao: [["选题", ["中小企业 GEO 入门"]], ["调研", []], ["写作", ["豆包推荐机制观察"]], ["评审", []], ["质检", []], ["配图排版", []], ["待审批", ["AI 搜索突围 5 条路", "豆包为什么推荐竞品"]], ["草稿箱", ["GEO 是什么"]]],
    baijia: [["选题", ["文心信源偏好实测"]], ["调研", []], ["写作", []], ["评审", []], ["质检", []], ["配图排版", []], ["待审批", []], ["草稿箱", ["品牌在 AI 答案里查无此人"]]],
    xhs: [["选题", ["3 个数字看懂 GEO"]], ["调研", []], ["写作", ["GEO 入门图卡"]], ["评审", []], ["质检", []], ["配图排版", []], ["待审批", []], ["草稿箱", []]],
    bilibili: [["选题", ["GEO 手册精读（脚本）"]], ["调研", []], ["写作", []], ["评审", []], ["质检", []], ["配图排版", []], ["待审批", []], ["草稿箱", ["GEO 实施手册精读"]]],
    douyin: [["选题", ["图文：AI 答案入口"]], ["调研", []], ["写作", []], ["评审", []], ["质检", []], ["配图排版", []], ["待审批", []], ["草稿箱", ["3 张图看懂 GEO"]]],
  } as Record<string, [string, string[]][]>,
  questions: {
    wechat: [["MES 系统哪家好", "元宝", "被引", "被引清单"], ["GEO 怎么做", "元宝", "未提及", "缺口清单"], ["生成式引擎优化是什么", "元宝", "被引", "被引清单"]],
    zhihu: [["GEO 和 SEO 的区别", "DeepSeek", "被引", "被引清单"], ["如何让 AI 推荐我的品牌", "DeepSeek", "口径错误", "纠错清单"], ["RAG 知识库怎么建", "DeepSeek", "未提及", "缺口清单"]],
    toutiao: [["中小企业怎么做 AI 营销", "豆包", "被引", "被引清单"], ["豆包会推荐哪些品牌", "豆包", "未提及", "缺口清单"]],
    baijia: [["百度 AI 怎么选信源", "文心", "未提及", "缺口清单"], ["企业内容营销怎么做", "文心", "被引", "被引清单"]],
    xhs: [["AI 时代做内容的方法", "—", "未探测", "—"]],
    bilibili: [["GEO 实施步骤", "—", "未探测", "—"]],
    douyin: [["AI 答案入口迁移", "豆包", "未提及", "缺口清单"]],
  } as Record<string, string[][]>,
  policy: [
    ["L1", "cron 频率在安全区间内调整（不越 8–22 点窗口）", "直接生效 + policy_changes 留痕 + 时间线卡 + 一键回滚"],
    ["L1", "单平台周篇数 ±2（不超全局配额）", "同上"],
    ["L1", "专家 prompt 换已验证版本 / 评分线 ±3 分内校准 / skill 参数微调", "同上"],
    ["L2", "提高全局日配额上限 · 安装新 skill（限白名单货架）· 引入/删除专家 · 换模型供应商 · 改门禁 error 阈值", "进审批队列，附影响分析；批准后带 7 天观察期"],
    ["L3", "绕过 HITL 投递 · 自动对外发布 · 修改红线/纪律三条/风险分级本身 · 超 token 日预算继续跑", "代码级拦截并告警——手册教训：要紧规则必须是 error"],
  ] as string[][],
  // 定时任务表：最后一列为状态点 [level, text]
  cron: [
    ["每天 02:00", "选题规划", "各在役平台按排期与缺口清单出当日计划（每平台 0–2 篇，全局限额防灌水）", ["ok", "正常"]],
    ["每 30 分钟", "流水线推进", "推进中间态稿件（调研→写作→评审→评分→配图），到审批态停", ["ok", "正常"]],
    ["每 10 分钟", "卡死自愈", "超 25 分钟的任务重置重跑", ["ok", "正常"]],
    ["每天 09:00", "审批提醒", "队列非空则桌面通知（Tauri notification）", ["ok", "正常"]],
    ["每天 23:00", "多样性检查", "同话题当天 >2 篇告警，防近亲繁殖", ["ok", "正常"]],
    ["每周一 05:00", "AI 引用探测", "五引擎题库问答，规则打分（域名60/品牌40/排名30）+ 模型复判 → ai_citations", ["ok", "W28 完成"]],
    ["每周一 07:00", "三张清单", "自动生成被引/纠错/缺口清单，写入各平台选题池与知识库", ["ok", "正常"]],
    ["每周一 08:00", "账号体检", "全平台登录态探测，过期亮灯", ["warn", "知乎异常"]],
    ["每天整点", "反思回写", "发布满 48h 的内容对比表现写反思卡", ["ok", "正常"]],
    ["每月 1 号", "提示词进化 + 月报", "高分反思萃取进各平台宪法（版本化可回滚）；五指标月报", ["idle", "08-01"]],
    ["每天 05:30", "主 Agent 策略会", "读度量+记忆 → 生成策略变更提案 → 分级执行（M9 新增）", ["ok", "正常"]],
  ] as [string, string, string, [string, string]][],
  scorer: [
    ["开篇直答", 14, "首段 75 字内直接回答标题问题", "LLM 复判"], ["FAQ 区块", 14, ">1500 字必须有 FAQ", "规则（error）"],
    ["可引用短句", 12, "40–110 字、含数字、非疑问句", "规则（组合条件防应付）"], ["权威引用", 12, "外部权威来源链接", "规则"],
    ["结构", 12, "小标题分层、每节独立结论", "规则"], ["要点块", 10, "列表/表格等要点密度", "规则"],
    ["真实数据", 10, "含一手数据且有出处（闸门B 校验）", "规则+LLM"], ["结构化标记", 8, "JSON-LD", "规则"], ["深度", 8, "信息增量与独有信息", "LLM 复判"],
  ] as [string, number, string, string][],
  gateErr: ["标题 <14 字或 180 天内重复", "正文 <800 字", "无封面", "关键词密度 >3%", ">1500 字无 FAQ 区块", "模板套话命中", "编造数字（闸门B 无出处）", "闸门A 零命中裸写", "超当日全局配额", "同话题当天 >2 篇", "AI 内容未打标（合规第 35 章）"],
  gateWarn: ["首段超 75 字", "可引用短句 <3 句", "无结构化标记", "外链权威度不足", "小标题层级混乱", "段落过长", "标题未含数字/场景", "图片 alt 缺失", "内链 <3 条"],
};
