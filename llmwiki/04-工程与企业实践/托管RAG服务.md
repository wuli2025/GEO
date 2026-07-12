---
title: 托管 RAG 服务
aliases: [RAG-as-a-service, 托管RAG, Bedrock Knowledge Bases, Azure AI Search, Vertex AI Search, OpenAI File Search]
tags: [工程, 托管, 选型]
type: engineering
status: stable
updated: 2026-06-01
---

# 托管 RAG 服务（RAG-as-a-Service）

> 想省运维、快上线时的选项。代价是**绑定生态、可移植性差、数据出本地**。

## 主要玩家

- **AWS Bedrock Knowledge Bases**：接 S3（及 web crawler/Confluence/SharePoint），选嵌入（Titan/Cohere）、选向量库（OpenSearch Serverless/Pinecone/Redis/MongoDB Atlas），自动切块/嵌入/检索。原生 hybrid。**单 KB 内靠元数据过滤做多租户**（有官方模式）。
- **Azure AI Search**（在 Azure AI Foundry）：hybrid = 向量 + BM25 + 微软 **semantic ranker**（托管重排）；可配切块；生成走 Azure OpenAI。**文档级访问控制/安全裁剪**强。
- **Google Vertex AI Search**：Cloud Next 2026 并入 **Gemini Enterprise Agent Platform**；Google 级语义搜索/排序、多模态、Gemini 生成，含 Agent 开发套件与可观测面板。
- **OpenAI File Search（Vector Stores）**：自动解析/切块/嵌入 + hybrid。**Assistants API 已弃用，2026-08-26 关停 → 改用 Responses API**。新建 vector store 可达 1 亿文件。计价：向量存储 $0.10/GB/天（首 GB 免），File Search 工具 $2.50/千次调用。
- **第三方**（Vectara、Ragie）：单 API 摄入→检索→生成，最快但最不可控。

## 取舍

托管服务消灭运维、与原生 IAM/合规/数据服务集成好，但**锁定生态**（迁移成本高）、数据需上云。

## 与 llmwiki

llmwiki 是**本地/自托管、结构优先、数据不外流**的路线，与托管服务哲学相反。建议**自建为主**（保住结构与隐私），仅在需要联网兜底（见 [[CRAG]]）或临时大规模处理时局部借用托管能力。若团队完全在某云内且重合规，可评估 Azure AI Search 的安全裁剪能力。

## 关联
[[企业级RAG架构]] · [[向量数据库选型]] · [[成本与延迟优化]] · [[CRAG]]

## 来源
bitslovers/internative 平台对比；OpenAI File Search 文档与定价。详见 [[来源汇总]]。
