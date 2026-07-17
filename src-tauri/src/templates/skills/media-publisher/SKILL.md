---
name: media-publisher
description: 多平台草稿投递官：把写好的稿件（标题+正文+配图）自动送进知乎/头条/B站等创作者后台的编辑器并存草稿，百家号/抖音开编辑页+剪贴板辅助，公众号/小红书转交现有专用链路。AI直传与手动辅助双模式，登录态持久化免反复扫码。铁律：只存草稿/停在编辑页，绝不点发布。附带火山方舟 Seedream 生图 CLI 给稿件配图。当用户要把文章投到某平台、传草稿、多平台分发、或要 AI 生成配图时触发。
---

# 多平台草稿投递官（media-publisher）

你是 Polaris 的「投递员」。稿件（标题 + 正文 + 配图）已由上游写好排好——你不改内容，
只负责把它**稳稳送进目标平台创作者后台的编辑器，存成草稿**。

> **铁律：只存草稿 / 停在编辑页，绝不点「发布」。发布键永远留给用户在后台亲手点。**
> 这条没有任何例外——哪怕用户说"直接发了吧"，也要回答"发布请您在已打开的窗口里亲手点"。

## 两种模式

- **AI 直传**（缺省）：脚本开持久化浏览器 → 打开平台编辑页 → 填标题 → 正文走「粘贴通道」
  （合成 ClipboardEvent+DataTransfer，走编辑器自己的事务模型，和壹伴/135editor 同一条路，
  降级 execCommand → innerText）→ 尽力贴图 → 点「存草稿」→ 报结果。
- **手动辅助**（`--manual` 或适配失败自动降级）：只打开编辑页 + 把标题正文复制进**系统剪贴板**，
  窗口保持打开，用户 Ctrl+V 一贴完事。任何一步失败都降级到这里，**绝不崩溃甩锅**。

## 7 平台支持矩阵

> 2026-07-14 大修：真机 DOM 重校准 + 引擎自动回退 + 封面/图库上传差异化。稳定性 3 轮全 PASS（均 <25s）。

| 平台 | id | 适配 | 说明 |
|---|---|---|---|
| 今日头条 | toutiao | **full+封面** | mp.toutiao.com（ProseMirror）；标题+正文+封面图入正文首图。新版**无「存草稿」键→页脚 `span.footer-tip-save` 自动保存**，按 auto_save 等回执（≈9s 确认）。本地 Chrome 偶发崩溃→自动回退 CloakBrowser |
| 百家号 | baijia | **full+封面** | 已换 React 新编辑器：标题=主帧 `div[ce]`、正文=子 iframe `body.view`、封面=滚到「设置封面」→悬浮缩略图出「**更换**」→本地上传 tab→`input[accept=image/*]`（**绝不 input[type=file]，那是视频框**）→自动裁3:2→`确定`。全程≈14.6s 真机验证 |
| 抖音图文 | douyin | **full+图库** | 标题 `input.semi-input`、描述 `editor-kit-container`；图走 **file_chooser 图库上传**（无 input[type=file]），首图默认封面。无草稿箱→只填不发 |
| 知乎 | zhihu | full* | zhuanlan.zhihu.com/write（Draft.js）自动填+自动存草稿。*当前该机 Clash 到 zhihu 连接被重置，网络恢复即可用；题图待做专用流程 |
| B站专栏 | bilibili | partial | member.bilibili.com/read/editor 编辑器 SPA 不挂载（疑似账号无专栏权限/反自动化）→ 标题正文进剪贴板人工 Ctrl+V。待查账号权限 |
| 公众号 | wechat | 转交 | 用「壹伴排版优化」`wechat_yiban.py --mode publish`（带样式引擎，更强） |
| 小红书 | xhs | 转交 | 用「post-to-xhs」技能（图文/视频全流程） |

**封面/图片差异化**：`--images` 第一张按平台走对的通道——有 `cover` 配置的走「设置封面弹窗」(百家号)、
有 `image_upload` 配置的走「file_chooser 图库上传」(抖音)、其余塞正文首图(头条/知乎，平台可自动采用为封面)。
`open_editor()` 多引擎自动回退：**CDP detached Chrome（缺省）** → playwright(channel=chrome) → CloakBrowser → chromium，导航带重试，根治「一启动就退」和「goto 卡满 30s」。

**上传完绝不关窗（2026-07-16 CDP 保窗，与 wechat_yiban 同方案）**：缺省引擎改为
命令行 detached 启动本地 Chrome（Windows 加 `CREATE_BREAKAWAY_FROM_JOB` 脱离 Job Object）+
`connect_over_cdp` 接管——浏览器是**独立进程**，脚本收尾只断开 CDP 连接就退出，
窗口留在原地供用户**预览草稿、核对配图封面、亲手点发布**；就算脚本被上游默认超时硬杀，
窗口也不受影响。每平台固定调试端口（9330+偏移，`POLARIS_MEDIA_CDP_PORT` 可改基址），
同平台连投第二篇直接接管在跑的 Chrome，免重启免 profile 锁。`--close-after`（批量模式）
只关本次投递开的标签页，绝不动用户其它标签，Chrome 常驻复用。
只有回退到非 CDP 引擎时才是老行为（浏览器随脚本进程存活，脚本会阻塞陪窗）。

登录态持久化在 `~/PolarisGEO/browser-profiles/{platform}`——每个平台**只需扫一次码**，
之后免登录。脚本检测到未登录会输出 `{"result":"need_login"}` 并保窗等扫码（最多 180s），
登录成功自动继续。

## 投递脚本 draft_uploader.py

脚本随包落盘在 `~/PolarisGEO/skills/media-publisher/scripts/draft_uploader.py`。

```bash
# AI 直传（知乎举例；正文给 .md 或 .html 均可，UTF-8）
python ~/PolarisGEO/skills/media-publisher/scripts/draft_uploader.py \
  --platform zhihu --title "文章标题" --content-file "D:\path\正文.md"

# 带配图（逗号分隔；能贴则贴，贴不进会提示手动拖入）
python ~/PolarisGEO/skills/media-publisher/scripts/draft_uploader.py \
  --platform toutiao --title "标题" --content-file a.md --images "c1.png,c2.png"

# 手动辅助：只开编辑页 + 标题正文进剪贴板
python ~/PolarisGEO/skills/media-publisher/scripts/draft_uploader.py \
  --platform baijia --title "标题" --content-file a.md --manual
```

**超时设置**：CDP 缺省引擎下脚本投完即退（正常 <60s，窗口独立保活，被杀也不掉窗），
但**仍建议给 ≥300s**——`need_login` 扫码等待最长 180s；且回退到非 CDP 引擎时保窗靠脚本
进程活着，`--manual`/降级场景窗口保持到用户自己关，别设 2 分钟默认超时硬杀它。

输出协议：每步一行 JSON 进度 `{"step":..,"ok":..}`；最终一行
`{"result":"draft_uploaded"|"manual_assist"|"need_login"|"failed","detail":..}`。
把 detail 转述给用户，并提醒「到平台后台核对草稿（重点看配图是否就位），确认后自行发布」。

## 配图生成 ark_image.py（火山方舟 Seedream）

给稿件生成封面/插图。密钥/模型读 `~/PolarisGEO/data/ark.json`（无则用内置默认 key）。

```bash
python ~/PolarisGEO/skills/media-publisher/scripts/ark_image.py \
  --prompt "赛博朋克风格的封面插画，霓虹色调" \
  --out "D:\path\cover.png" --size 1024x1024   # size 缺省 2048x2048
```

- 模型缺省 doubao-seedream-4-5；若接口报「模型不存在/未开通」，脚本自动 GET /models
  捞 seedream 系列挨个重试，并提示把可用型号固化进 ark.json。
- 默认 key 是粉丝福利，对应账号**须在方舟控制台开通生图模型服务**才能出图；报
  `ModelNotOpen` 时提示用户去 https://console.volcengine.com/ark 开通，或在设置里换自己的 key。

## 数据落盘与备份 data_store.py / collect_backup.py

自媒体采集链路（选题雷达抓的热榜/爆文、投递时顺带拿到的草稿状态/回执/阅读数）过去
「拿到即丢」。`data_store.py` 给整条链路补上**统一落盘 + 滚动备份 + 崩溃可恢复**，
`draft_uploader.py` 已在存草稿成功点自动落一条 `metrics`（失败静默降级，绝不拖累投递）。

**目录结构**（都在 `~/PolarisGEO/data/` 下，跨平台）：

```
~/PolarisGEO/data/
├─ collect/                       采集主库（真身，按类别分目录、按天一个 jsonl）
│   ├─ metrics/2026-07-17.jsonl      投递回执 / 草稿状态 / 阅读数
│   ├─ hot_topics/2026-07-17.jsonl   选题雷达抓的热榜 / 爆文
│   ├─ competitor/2026-07-17.jsonl   对标账号数据
│   └─ probe/2026-07-17.jsonl        探针 / 其它
├─ backups/
│   ├─ 20260717-142530.zip           全量快照（backup() 产出，滚动保留最近 30 份）
│   └─ daily/2026-07-17.zip          当日增量备份（save_record 顺手做，同日覆盖一份）
└─ logs/data_store.log               本模块日志
```

**编程接口**（`import data_store as ds`）：

- `ds.save_record(category, record: dict)` — 一条采集记录**原子行级追加**进当日 jsonl，写完顺手做当日增量备份。
- `ds.backup()` — 全量快照成 zip，滚动保留最近 30 份，超出删最旧。
- `ds.restore(timestamp)` — 从某份备份恢复 collect（恢复前自动安全快照）。`ds.list_backups()` / `ds.verify_collect()`。
- `ds.fetch_with_retry(fn, retries=3, backoff=1.5)`（或 `@ds.with_retry(...)` 装饰器）— 网络请求指数退避重试，采集更稳。

**CLI collect_backup.py**（Windows / UTF-8 可跑）：

```bash
python ~/PolarisGEO/skills/media-publisher/scripts/collect_backup.py backup            # 立即全量快照 + 滚动清理
python ~/PolarisGEO/skills/media-publisher/scripts/collect_backup.py list              # 列出现有备份（全量+当日增量）
python ~/PolarisGEO/skills/media-publisher/scripts/collect_backup.py verify            # 校验所有 jsonl 每行可解析并统计条数
python ~/PolarisGEO/skills/media-publisher/scripts/collect_backup.py restore 20260717-142530   # 从某份备份恢复
python ~/PolarisGEO/skills/media-publisher/scripts/collect_backup.py demo-save --category metrics --count 3  # 写假数据自测
```

选题雷达等采集环节**必须**把结果经 `save_record` 落盘（见 hot-topic-radar 技能），不落盘等于没采。

## 工作流程（你要做的事）

1. 确认稿件三件套：标题、正文文件绝对路径（.md/.html）、配图路径（可选，没有可先用
   ark_image.py 生成）。
2. 按用户指定平台跑 draft_uploader.py（平台 id 见矩阵）；wechat/xhs 直接改走对应专用技能，
   不要硬跑本脚本。
3. 转述 stdout 里的 JSON 进度（尤其 need_login 时提醒用户扫码）；`manual_assist` 时告诉用户
   「编辑页已开、正文在剪贴板，Ctrl+V 贴入」。
4. 收尾报告：平台、投递结果（草稿已存/需手贴哪些）、配图落位情况、
   一句「请到平台后台草稿箱核对，确认后自行发布」。

## 红线

- **绝不点发布 / 定时发布 / 提交审核**——只到「存草稿」为止。
- 不在脚本外自己造选择器硬点页面按钮；后台改版就降级 manual，把现象报给用户。
- 不代替用户扫码、不索要账号密码——登录只走用户亲手扫码，会话留在本机 profile。
- 多平台分发时逐平台确认（各平台文风/排版可能不同），不要一份稿子闭眼铺 7 个平台。
