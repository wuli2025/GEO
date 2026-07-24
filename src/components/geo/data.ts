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
  back: '<path d="M19.5 12h-15"/><path d="M10.5 5.5L4 12l6.5 6.5"/>',
  target:
    '<circle cx="12" cy="12" r="8"/><circle cx="12" cy="12" r="4.6"/><circle cx="12" cy="12" r="1.3" fill="currentColor" stroke="none"/>',
  // 运营助手：对话气泡（原先是 💬 emoji —— 唯一一个彩色表情混在单色描线图标里）
  chat: '<path d="M20 12.5c0 3.6-3.6 6.5-8 6.5-.9 0-1.8-.12-2.6-.35L4.5 20.5l1.2-3.4C4.35 15.9 3.5 14.3 3.5 12.5c0-3.6 3.6-6.5 8-6.5s8.5 2.9 8.5 6.5z"/>',
  // 品牌档案：盾牌 + 勾（权威主体 / 可信信源）
  brand:
    '<path d="M12 3.5l7.5 2.7v5.3c0 4.3-3.1 7.6-7.5 8.9-4.4-1.3-7.5-4.6-7.5-8.9V6.2z"/><path d="M9 12l2.2 2.2L15.3 10"/>',
  // 立即发一篇：纸飞机（把稿子送出去）——原先是 ⚡ emoji
  launch:
    '<path d="M20.4 3.6L3.3 10.3a.42.42 0 0 0 .02.79l6.5 2.1 2.1 6.5a.42.42 0 0 0 .79.02z"/><path d="M20.4 3.6L9.82 13.19"/>',
  // 深度搜索选题：放大镜压在题面上——原先是 🔍 emoji
  dig:
    '<circle cx="11" cy="11" r="6.3"/><path d="M15.6 15.6l4 4"/><path d="M8.5 9.7h5"/><path d="M8.5 12.5h3.2"/>',
};

/** 图标 → svg 字符串（aria-hidden，紧邻文字标签）。 */
export const ico = (n: string): string =>
  `<svg class="i" viewBox="0 0 24 24" aria-hidden="true">${ICONS[n] || ""}</svg>`;

/** 平台品牌徽标：品牌色圆角方块 + 简化标志性图形/首字（自绘示意，非官方素材）。 */
const badge = (bg: string, inner: string): string =>
  `<svg class="pi" viewBox="0 0 16 16" aria-hidden="true"><rect width="16" height="16" rx="4" fill="${bg}"/>${inner}</svg>`;
const glyph = (ch: string, size = 9): string =>
  `<text x="8" y="${size >= 10 ? 11.8 : 11.4}" text-anchor="middle" font-size="${size}" font-weight="600" font-family="system-ui,sans-serif" fill="#fff">${ch}</text>`;

const PLATFORM_ICONS: Record<string, string> = {
  wechat: badge("#07C160",
    '<path d="M8 3.7c-2.55 0-4.6 1.66-4.6 3.7 0 1.18.68 2.23 1.74 2.9l-.44 1.46 1.66-.84c.52.14 1.07.21 1.64.21 2.55 0 4.6-1.66 4.6-3.73S10.55 3.7 8 3.7z" fill="#fff"/><circle cx="6.4" cy="7.1" r=".72" fill="#07C160"/><circle cx="9.6" cy="7.1" r=".72" fill="#07C160"/>'),
  zhihu: badge("#0084FF", glyph("知")),
  toutiao: badge("#ED4040", glyph("头")),
  baijia: badge("#4E6EF2", glyph("百")),
  xhs: badge("#FF2442", glyph("红")),
  bilibili: badge("#00AEEC",
    '<path d="M5.4 3.2l1.7 1.9M10.6 3.2L8.9 5.1" stroke="#fff" stroke-width="1.2" stroke-linecap="round"/><rect x="2.9" y="5.1" width="10.2" height="7" rx="1.6" fill="#fff"/><circle cx="6" cy="8.6" r=".8" fill="#00AEEC"/><circle cx="10" cy="8.6" r=".8" fill="#00AEEC"/>'),
  douyin: badge("#161823",
    '<path d="M9.1 3.2h1.7c.25 1.3 1.05 2.1 2.3 2.3v1.75c-.85-.02-1.63-.28-2.3-.75v3.6a3.25 3.25 0 1 1-3.25-3.25c.17 0 .34.01.5.04v1.85a1.5 1.5 0 1 0 1.05 1.43V3.2z" fill="#fff"/>'),
  csdn: badge("#FC5531", glyph("C", 10)),
  juejin: badge("#1E80FF", glyph("掘")),
  channels: badge("#FA9D3B", '<path d="M6.3 4.9l5.2 3.1-5.2 3.1z" fill="#fff"/>'),
};

/** 平台 id → 品牌徽标 svg（无则空串）。 */
export const pico = (id: string): string => PLATFORM_ICONS[id] || "";

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
    ai: "腾讯元宝", weekPlan: 0, pending: 0, sendMode: "auto",
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
    ai: "DeepSeek", weekPlan: 0, pending: 0, sendMode: "auto",
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
    ai: "豆包", weekPlan: 0, pending: 0, sendMode: "auto",
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
    ai: "文心", weekPlan: 0, pending: 0, sendMode: "auto",
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
    ai: "个人/小微客户", weekPlan: 0, pending: 0, sendMode: "auto",
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
    ai: "AI 读文字层（标题+简介）", weekPlan: 0, pending: 0, sendMode: "manual",
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
    ai: "豆包（字节系信源加成）", weekPlan: 0, pending: 0, sendMode: "auto",
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
    id: "csdn", name: "CSDN", adapter: "full", adapterText: "full",
    engine: "draft_uploader.py（StackEdit 系 markdown 源码通道 + 存草稿按钮）",
    login: "ok", loginNote: "browser-profiles/csdn 常驻",
    ai: "DeepSeek（技术问答信源靠前）", weekPlan: 0, pending: 0, sendMode: "auto",
    cover: "无需（封面在发布弹窗内，只存草稿不触及）",
    style: "技术实施笔记 / 踩坑记录改编分发 · 代码块与报错原文是最贵的独有信息",
    redline: "副阵地：主阵地稳定后再开；同稿需与掘金差异化",
    patch: "主笔补丁=技术笔记体（问题→环境→复现→修法→结论）；投递补丁=markdown 源码直灌",
    blockers: [
      ["编辑器是 StackEdit 系、不是 CodeMirror", "正文 = pre.editor__inner[contenteditable]，里面直接放 md 源码；无 .CodeMirror、window.CodeMirror 也不存在——按 CodeMirror 猜的选择器全落空"],
      ["只能喂 text/plain，不能喂 HTML", "md 编辑器要的是源码；塞 text/html 会被转换/吞语法（#、**、``` 全丢）——已加 body_mode=markdown 专用通道，只贴 text/plain"],
      ["首开必弹「模版库」全屏 modal", "div.modal(z-index:100) 吃掉后续所有点击（Playwright 明报 intercepts pointer events）——已加 dismiss_selectors 先关它"],
      ["标题「点开才出 input」", "常态是 div.article-bar__title-display，input 本体 display:none，直接 fill 被可操作性检查挡下——已加 title_pre_click 先点显示层"],
      ["保存回执抓不到", "成功 toast 一闪即逝（3.5s 内已消失）；改用 URL 出现 ?articleId=xxx 作硬证据——服务端真发了草稿 id"],
      ["入口带 ?not_checkout=1 会被重定向", "实测跳去内容管理页、落不到编辑器——只用裸 https://editor.csdn.net/md/"]],
    cmd: 'python ~/PolarisGEO/skills/media-publisher/scripts/draft_uploader.py --platform csdn --title "标题" --content-file 正文.md',
  },
  {
    id: "juejin", name: "掘金", adapter: "planned", adapterText: "待接入",
    engine: "计划走 Wechatsync driver（草稿箱）",
    login: "none", loginNote: "适配未落地，接入后建 profile",
    ai: "DeepSeek", weekPlan: 0, pending: 0, sendMode: "manual",
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
    ai: "腾讯元宝", weekPlan: 0, pending: 0, sendMode: "manual",
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
  // 品牌形象与大脑·进化对调了位置：品牌是每篇稿子都要落到的「我是谁」，天天要看，
  // 提到总控常驻；大脑·进化是周月尺度的复盘，收进专家模式即可。
  { label: "总控", keys: [
    ["dashboard", "board", "数据看板", "G"], ["approvals", "approve", "审批队列", "Q"],
    ["autopilot", "plan", "自动规划", "A"], ["brand", "brand", "品牌形象", "D"]] },
  { label: "资源", keys: [
    ["experts", "experts", "专家阵容", "X"],
    ["brain", "brain", "大脑·进化", "B"], ["promo", "target", "推广植入", "P"],
    ["accounts", "matrix", "账号矩阵", "M"],
    ["kb", "kb", "知识库", "K"], ["questions", "ask", "题库", "T"]] },
  { label: "系统", keys: [
    ["engine", "send", "投递引擎", "E"], ["gate", "gate", "质检门禁", "F"],
    ["layout", "type", "排版中心", "L"], ["settings", "api", "设置", "I"]] },
];

/**
 * 第三排子标签（bar3）。
 *
 * 除「设置」与「知识库」外，其余视图一律**不设子标签**——原来的分页内容
 * 已在各自视图里从上到下合并成一页（门户只留「工作流 + 专家团补丁」）。
 * 这里留空即等于 bar3 自动隐藏（GeoOpsCenter 里 `v-if="subtabs.length"`）。
 */
export const SUBTABS: Record<string, [string, string][]> = {
  kb: [["files", "资料文件"], ["graph", "星图"]],
  settings: [["chan", "模型通道"], ["tier", "模型分层"], ["img", "生图通道"], ["update", "我们的更新"], ["env", "环境医生"]],
};

/** hotkey → viewId */
export const KEYMAP: Record<string, string> = {};
ZONES.forEach((z) => z.keys.forEach((k) => { if (k[3]) KEYMAP[k[3]] = k[0]; }));

/* ── Mock 数据 ──────────────────────────────────────────────────────── */
interface Series { name: string; color: string; raw: string; v: number[] }

export const MOCK = {
  dispatch: [] as string[][],
  kpi: {
    pub7: 0, runs: 0, runOk: "—", pub30: 0, avgWords: "—", reads: "—", readsMom: "—",
    aiShare: "—", aiBreak: "—", cost: "—", token: "—", llmCost: "—",
    pending: 0, loginBad: 0,
  },
  traffic: {
    days: [] as string[],
    series: [
      { name: "自然流量", color: "var(--s1)", raw: "var(--s1)", v: [] },
      { name: "AI 引荐", color: "var(--s2)", raw: "var(--s2)", v: [] },
      { name: "爬虫抓取", color: "var(--s3)", raw: "var(--s3)", v: [] },
    ] as Series[],
  },
  engines: {
    weeks: [] as string[],
    series: [
      { name: "豆包", color: "var(--s1)", raw: "var(--s1)", v: [] },
      { name: "DeepSeek", color: "var(--s2)", raw: "var(--s2)", v: [] },
      { name: "元宝", color: "var(--s3)", raw: "var(--s3)", v: [] },
      { name: "文心", color: "var(--s4)", raw: "var(--s4)", v: [] },
      { name: "Kimi", color: "var(--s5)", raw: "var(--s5)", v: [] },
    ] as Series[],
  },
  heat: {
    cols: ["发布(篇/30天)", "阅读(次)", "被引(次)", "成本(¥/篇) ↓越低越好"],
    rows: [] as [string, (string | number)[]][],
  },
  radar: [] as [string, number][],
  approvals: [] as string[][],
  // 最近投递：最后一列为状态点 [level, text]
  recent: [] as [string, string, string, string, string, [string, string]][],
  health: [] as string[][],
  lanes: {
    wechat: [["选题", []], ["调研", []], ["写作", []], ["评审", []], ["质检", []], ["配图排版", []], ["待审批", []], ["草稿箱", []]],
    zhihu: [["选题", []], ["调研", []], ["写作", []], ["评审", []], ["质检", []], ["配图排版", []], ["待审批", []], ["草稿箱", []]],
    toutiao: [["选题", []], ["调研", []], ["写作", []], ["评审", []], ["质检", []], ["配图排版", []], ["待审批", []], ["草稿箱", []]],
    baijia: [["选题", []], ["调研", []], ["写作", []], ["评审", []], ["质检", []], ["配图排版", []], ["待审批", []], ["草稿箱", []]],
    xhs: [["选题", []], ["调研", []], ["写作", []], ["评审", []], ["质检", []], ["配图排版", []], ["待审批", []], ["草稿箱", []]],
    bilibili: [["选题", []], ["调研", []], ["写作", []], ["评审", []], ["质检", []], ["配图排版", []], ["待审批", []], ["草稿箱", []]],
    douyin: [["选题", []], ["调研", []], ["写作", []], ["评审", []], ["质检", []], ["配图排版", []], ["待审批", []], ["草稿箱", []]],
  } as Record<string, [string, string[]][]>,
  questions: {
    wechat: [],
    zhihu: [],
    toutiao: [],
    baijia: [],
    xhs: [],
    bilibili: [],
    douyin: [],
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
