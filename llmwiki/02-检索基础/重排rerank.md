---
title: 重排 (Rerank)
aliases: [rerank, reranking, 重排序, cross-encoder, ColBERT, 重排器]
tags: [检索基础, 重排, 高性价比]
type: concept
status: stable
updated: 2026-06-01
---

# 重排 (Rerank)

> **性价比最高的一步，先上。** 粗筛「宁可错抓不可放过」，召回偏多偏杂；重排把最相关的拨到前头。便宜，提升却大。

## 是什么

第一阶段检索（[[向量检索|向量]]/[[BM25与全文检索|BM25]]/[[混合检索与RRF|hybrid]]）快但糙。重排器对 top-K 候选（如 50–200）用更贵更准的模型重新打分，留 top-N 给模型。

## 三类方法（质量 vs 速度）

1. **Cross-encoder（交叉编码器）**—— 质量最高。把 `[query, 文档]` 一起送进一个 transformer 输出相关分。全交叉注意力 = 最准。代价：不能预算，查询时逐候选跑，慢，只能用于小候选集。
2. **ColBERT / 后期交互**—— 中间档。query 和文档分别编码成**逐 token**多向量，用 **MaxSim**（每个 query token 取与文档 token 的最大相似度再求和）打分。文档向量可预算，比 cross-encoder 快。代价：索引大。2026 被视为略小众，但 BGE-M3 原生支持 ColBERT 模式。
3. **重排 API/模型（2025–2026）**：
   - **Cohere Rerank**（rerank-3.5/v4）：托管 API，多语，最省事。
   - **BGE-reranker**（BAAI v2-m3 及大模型变体）：**自托管首选，中文/CJK 最强开源**。
   - **Qwen3-Reranker**：与 Qwen3 嵌入配对（已在 Milvus 验证）。
   - **Jina reranker v3**：多语 API/开源。

## 效果

Anthropic [[上下文检索-Contextual-Retrieval|Contextual Retrieval]] 实验：在 contextual hybrid 之上**加重排**，检索失败率从 5.7% 一路降到 **1.9%（总降 67%）**。普遍报告答案质量 +15–30%。

## 实践

- 只对 top **50–150** 候选重排控延迟。
- 不要在召回率没修好之前调重排——它救不回检索从没召回的文档（见 [[RAG评测方法]] 迭代顺序）。

## 与 llmwiki

中库/大库阶段，[[混合检索与RRF|hybrid]] 召回后接 BGE-reranker / Qwen3-Reranker 排座次，再把 top 整篇交模型漫游。见 [[RAG增强llmwiki方案]]。

## 关联
[[混合检索与RRF]] · [[向量检索]] · [[BM25与全文检索]] · [[嵌入模型选型]] · [[上下文检索-Contextual-Retrieval]]

## 来源
sentence-transformers CrossEncoder；FlagEmbedding (BGE)；Cohere/Jina rerank 文档；Anthropic Contextual Retrieval。详见 [[来源汇总]]。
