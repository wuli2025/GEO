---
title: 上下文检索 (Contextual Retrieval)
aliases: [Contextual Retrieval, 上下文检索, Contextual Embeddings, Contextual BM25, Anthropic上下文检索]
tags: [高级架构, 切块, 情境注入, Anthropic]
type: architecture
status: stable
updated: 2026-06-01
---

# 上下文检索（Contextual Retrieval, Anthropic）

> 一把提升检索质量的利器：**给每个块前置一段情境说明再索引**。Anthropic 2024-09 提出，检索失败率最多降 67%。

## 解决什么

标准 [[分块策略|切块]] 会剥掉全局语境：一个块写「营收增长 3%」，丢了「哪家公司、哪个季度」，伤 embedding 也伤 [[BM25与全文检索|BM25]]。

## 怎么干

索引前，对每个块用 LLM（Anthropic 用 Claude 3 Haiku）生成 **50–100 token 的情境**（实体、时段、范围），**前置到块上**，再分别：
- **Contextual Embeddings**：嵌入「情境+块」；
- **Contextual BM25**：对「情境+块」建字面索引。

提示词大意：「请给一段简短情境，把此块置于整篇文档中以利检索。」

**省钱**：用 **prompt 缓存**（整篇文档缓存一次，复用于它的所有块），预处理约 **$1.02 / 百万文档 token**。

## 效果（top-20 失败率，基线 5.7%）

- 仅 Contextual Embeddings：**−35%**（→3.7%）
- + Contextual BM25：**−49%**（→2.9%）
- + [[重排rerank|重排]]：**−67%**（→1.9%）

Anthropic 建议：召回 **top-20**（胜过 top-5/10）；嵌入用 Gemini/Voyage 表现好；可定制领域情境提示。**若整库 <~200K token，干脆别 RAG，直接塞进 prompt 配缓存**（呼应 [[长上下文vs RAG之争]]）。

## vs [[Late-Chunking]]

| | Contextual Retrieval | Late Chunking |
|---|---|---|
| 做法 | LLM 给每块生成情境文本 | 先整篇编码，再块内 token 池化 |
| 改进 | dense + BM25 双索引 | 仅 embedding |
| 成本 | 较贵（每块一次 LLM） | 便宜（一次编码） |
| 可读 | 情境是人类可读文本 | 无 |
两者可互补。

## 与 llmwiki

llmwiki 的文章天然带标题/章节层级，本身就是「情境」。若做块级检索，可用此法把「所属文章标题+小节」自动前置，或干脆**召回整篇**（[[分块策略]]）从根上免去情境丢失。

## 关联
[[分块策略]] · [[Late-Chunking]] · [[BM25与全文检索]] · [[重排rerank]] · [[成本与延迟优化]]

## 来源
Anthropic《Introducing Contextual Retrieval》(2024-09)；Claude Cookbook contextual-embeddings。详见 [[来源汇总]]。
