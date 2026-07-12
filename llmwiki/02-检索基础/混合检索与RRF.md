---
title: 混合检索与 RRF
aliases: [hybrid search, 混合检索, RRF, 倒数排名融合, Reciprocal Rank Fusion]
tags: [检索基础, 混合, 必修]
type: concept
status: stable
updated: 2026-06-01
---

# 混合检索 (Hybrid Search) 与 RRF

> 「一条腿瘸，三条腿稳。」别只靠向量。这是 [[RAG增强llmwiki方案|侦察兵五条打法]] 的第二条，也是 2026 几乎所有生产 RAG 的默认组合。

## 是什么

并行跑多路检索再合并排名：
- **① [[向量检索|向量]]** 管「意思相近」；
- **② [[BM25与全文检索|BM25]]** 管「字面精准」；
- **③ 双链/[[GraphRAG|图谱]]** 管「顺藤摸瓜」。
三路各探各的，再融合。

## 分数尺度问题

余弦分（~0–1）和 BM25 分（无上界）尺度不同，直接相加会失败。两种解法：
1. **分数归一化**（min-max / z-score）后加权求和——可更优但要调参；
2. **基于排名的融合**——RRF，免调参。

## RRF（倒数排名融合）

只用排名位置合并，忽略原始分数：

```
RRF(d) = Σ over retrievers  1 / (k + rank_i(d))
```

- `k` 常数，**通常取 60**，抑制头部主导、防单一检索器独大。
- 被多路同时排前的文档胜出（奖励「跨检索器共识」）。
- **一行代码、无需校准分数尺度**——这就是它无处不在的原因。

## 优劣

- **稳、免调参**，是安全默认；
- 代价：丢弃了分数大小信息（自信的 #1 和勉强的 #1 同等对待）。精心校准的加权融合有时更优，但 RRF 是首选。

## 工具

Elasticsearch（`rrf` retriever）、OpenSearch、Weaviate（原生 hybrid）、Qdrant、Azure AI Search、Chroma 均原生支持。

## 与 llmwiki

llmwiki 已有向量和图谱，**补上 [[BM25与全文检索|BM25]]** 即可三路齐全，用 RRF 一合，立竿见影。融合后接 [[重排rerank|重排]]，召回整篇交模型漫游。报告数据：BM25+向量 RRF 融合可把 recall@10 从 ~65–78% 提到 **~91%**，而融合开销仅约 6ms（对比 LLM 调用 500ms–2s）。

## 关联
[[向量检索]] · [[BM25与全文检索]] · [[重排rerank]] · [[RAG增强llmwiki方案]]

## 来源
OpenSearch RRF 博客；Elasticsearch RRF 参考；DigitalApplied《Hybrid search 2026》。详见 [[来源汇总]]。
