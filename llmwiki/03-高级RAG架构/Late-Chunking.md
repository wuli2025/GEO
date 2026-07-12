---
title: Late Chunking (后期分块)
aliases: [Late Chunking, 后期分块, Jina late chunking]
tags: [高级架构, 切块, 情境注入, Jina]
type: architecture
status: stable
updated: 2026-06-01
---

# Late Chunking（后期分块，Jina AI）

> 把常规顺序**倒过来**：不先切块再嵌入，而是**先整篇编码，再切块池化**。论文 arXiv 2409.04701。

## 怎么干

1. 用**长上下文嵌入模型**（如 jina-embeddings-v2/v3，8192 token）把**整篇文档先编码**，得到带全文注意力的**逐 token 向量**；
2. **然后**定义块边界，对每个块内的 token 向量做**均值池化**，得到块向量。

因此每个块向量「知道整篇」——指代、代词、跨边界信息都被解析进去了。

## 为什么有效

朴素切块里，含「它/该公司/这个版本」的块嵌入时不知所指；late chunking 的 token 向量是在**全文语境**下算的，块向量自带已解析的上下文。

## 要求与变体

- 需**长上下文嵌入模型**让整篇进一次前向；
- 超长文档用「long late chunking」分重叠大窗处理。

## 效果与对比

检索基准上稳定优于朴素切块，跨边界信息处增益最大。vs [[上下文检索-Contextual-Retrieval]]：late chunking **更便宜**（无逐块 LLM 调用，只一次编码+池化）但**只改 embedding**；Anthropic 法更贵但**同时改 embedding 和 BM25** 且情境人类可读。两者可互补。

## 与 llmwiki

若 llmwiki 做块级向量检索，late chunking 是**低成本**保住块语境的好选择（尤其文章内多指代时）。但若坚持 [[分块策略|召回整篇]]，则更多是「锦上添花」。工具：Milvus/Qdrant/Elasticsearch + Jina v2/v3 已支持。

## 关联
[[上下文检索-Contextual-Retrieval]] · [[分块策略]] · [[嵌入模型选型]] · [[向量检索]]

## 来源
arXiv 2409.04701；github.com/jina-ai/late-chunking；Milvus/Elastic late-chunking 集成。详见 [[来源汇总]]。
