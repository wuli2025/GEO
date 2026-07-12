# Polaris GEO

**GEO** 是 [Polaris](https://github.com/wuli2025/polaris_coworker) 的一个**隔离分支产品**：保留完整的桌面工作台（Tauri 2 + Vue 3 前端 / Rust 多 crate 后端），但把技能系统**精简为「自媒体运营 + 网页生成」**，并做了完整的**项目隔离**，作为独立产品发布、独立自动更新。

## 与上游的差异

| 维度 | 上游 Polaris | 本项目 GEO |
|---|---|---|
| 技能目录 | 全量（办公/财务/开发/测试/设计/自媒体/音视频/自动化…约 40+） | **仅自媒体运营 9 个 + web-studio + wechat-md-typesetter**，其余技能条目与模板文件已删 |
| 应用标识 | `com.polaris.app` | `com.polaris.geo` |
| 产品名 / 窗口标题 | Polaris / 北极星 · Polaris | **Polaris GEO / 北极星 · GEO** |
| 数据目录 | `~/Polaris` | **`~/PolarisGEO`**（项目/技能/数据/DB 全隔离，互不干扰） |
| 自动更新源 | llmwiki.cloud + polaris_coworker releases | **github.com/wuli2025/GEO releases**（签名密钥沿用，验签通过） |

## 保留的技能

- **自媒体运营（9）**：`wechat-pipeline`、`xiaohongshu-pipeline`、`hot-topic-radar`、`content-analytics-report`、`community-engagement`、`xhs-mao-pipeline`、`wechat-md-typesetter`、`gz-wechat-article-writer`、`gz-notion-infographic`
- **网页生成**：`polaris-web-studio`（forge 引擎：CDP 截图渲染 / 演示视频 / TTS）

`llmwiki/` 目录内置了一份 RAG 知识库分块（源自 llmwiki.rag）。

## 开发运行

```powershell
# 需要 Rust (cargo) + Node 20+ 在 PATH
npm install
npm run tauri:dev      # 桌面开发实例（窗口标题带 "(Dev x.y.z)"）
```

- 首次全量 debug 构建约 3 分钟（本机实测）。
- server/Docker 形态：`cargo build --no-default-features --features server`。
- 发桌面安装包：`npm run tauri:build`（Win NSIS / macOS dmg）。

## 首次运行须知

因数据目录隔离到 `~/PolarisGEO`，属**全新实例**：首次进入需在设置里配置一个 AI 供应商（接 Claude Code 的账号/模型），对话才能真正跑起来；技能中心此时只列出上面这几个自媒体 / 网页生成技能。

## 目录

```
GEO/
├── src/                # Vue 3 前端（三栏布局，Pinia）
├── src-tauri/          # Rust workspace（12 crate）+ 双壳(desktop/server)
│   ├── src/            # 组装壳：lib.rs 命令注册 / wiring.rs 引擎注入 / server.rs axum
│   └── crates/         # polaris-kernel(主底座) / polaris-forge(网页生成) / fable / wiki / collab …
├── llmwiki/            # 内置 RAG 知识库分块
├── docker/ · Dockerfile · docker-compose.yml
└── tauri.conf.json     # productName/identifier/updater 已改为 GEO
```

> 本仓库是从上游 `polaris_coworker` 剥离改造而来，非上游官方分发。
