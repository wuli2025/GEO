---
title: RAG 框架选型
aliases: [RAG框架, LlamaIndex, LangChain, LangGraph, Haystack, DSPy]
tags: [工程, 选型, 框架]
type: engineering
status: stable
updated: 2026-06-01
---

# RAG 框架选型

> 组件标准化后各框架准确率都接近 100%，差别在**开销、token 成本、人体工学、治理**，不在原始质量。

| 框架 | 是什么 | 最擅长 | 取舍 |
|---|---|---|---|
| **LlamaIndex** | 为 RAG/索引而生的数据框架 | 摄入与检索：160+ 连接器、多索引类型（向量/关键词/树/[[GraphRAG\|图谱]]）、多租户助手；token 用量低 | 不如 LangGraph 适合通用 agent 编排 |
| **LangChain** | 广集成/编排层 | 广度：70+ LLM、向量库、工具、链；接得快 | 框架开销与 token 偏高；抽象常变 |
| **LangGraph** | LangChain 的图式状态运行时 | 有环、多 agent、状态管理、人在回路、持久执行；**2026 [[Agentic-RAG]] 默认** | 心智模型陡；简单问答过度 |
| **Haystack** | 类型化 DAG 管线 | **可审计/可复现**（类型化 I/O、可视化），受**强监管行业**青睐；token 用量最低 | 生态小于 LangChain；快原型啰嗦 |
| **DSPy** | 程序化 prompt **编译/优化** | 用优化器替代手写 prompt、自改进、可测；开销最低 | 范式新有学习曲线；连接器/UI 不全 |

## 公认生产组合

**LlamaIndex（检索）+ LangGraph/LangChain（编排）+ [[RAGAS]]/LangSmith（[[RAG评测方法|评测]]）**。

## 与 llmwiki

llmwiki/Polaris 本身已是一套 [[Agentic-RAG|agentic 文件检索]] 框架（Read/Glob/Grep）。引入外部框架时：检索增强可借 LlamaIndex 的 hybrid/rerank 组件；agent 多轮编排可参考 LangGraph 的 self-RAG/CRAG 图；若重合规审计，Haystack 的类型化管线值得借鉴。**别为了用框架而用框架**——你已有的 agentic 自取往往更轻。

## 关联
[[Agentic-RAG]] · [[企业级RAG架构]] · [[RAG评测方法]] · [[托管RAG服务]]

## 来源
axiomlogica RAG 框架基准；morphllm/aimultiple 2026 框架对比。详见 [[来源汇总]]。
