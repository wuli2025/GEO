---
title: LightRAG 与 HippoRAG
aliases: [LightRAG, HippoRAG, PathRAG, KAG, 轻量图RAG]
tags: [高级架构, 图谱, 轻量, 多跳]
type: architecture
status: stable
updated: 2026-06-01
---

# LightRAG 与 HippoRAG（及其它图 RAG 变体）

> [[GraphRAG|微软 GraphRAG]] 又重又贵，这些是更省、更适合**持续生长的库**的变体。

## LightRAG

- **双层检索 (dual-level)**：low-level（具体实体）+ high-level（抽象主题）两路并取。
- **极致 token 效率**：号称复杂检索 <100 token，对比传统 GraphRAG ~610,000 token（约 6000×，单次 ~$0.15 vs $4–7）。⚠️ 厂商/作者自报，**待核**。
- **增量更新友好**：新文档可增量并入图，不必全量重建——这点对**不断长大的中文 KB 很关键**。

## HippoRAG

- 受神经生物学**海马体索引理论**启发；
- 用 **Personalized PageRank (PPR)** 在知识图上做链接式遍历检索；
- **多跳推理便宜 10–30×**；HippoRAG 1/2（arXiv 2405.14831 / 2502.14802）多跳证据召回 ~88–91% vs 朴素 RAG ~60–65%。

## 其它

- **PathRAG**（2502.14902）：路径剪枝，减冗余。
- **KAG**（2409.13731）：知识增强生成，**蚂蚁集团已生产部署**，生于中文语境，适合高风险中文专业域，但重。

## 选型共识

图 RAG 价值随**查询复杂度**上升；简单查询用图边际收益小却浪费算力（见 [[GraphRAG]] 的现实校准）。**持续增量、低成本**优先 LightRAG；**多跳为主**优先 HippoRAG；**高风险中文专业域**考虑 KAG。

## 与 llmwiki

你的双链 = 一张天然知识图。LightRAG 的**增量 + 双层 + 低 token** 思路，最契合「让 llmwiki 边长边可检索」；HippoRAG 的 PPR 遍历，印证「[[RAG与llmwiki核心论点|顺链跳比盲摸强]]」可量化。

## 关联
[[GraphRAG]] · [[RAPTOR]] · [[知识补全与缺口检测]] · [[自主学习工作流]]

## 来源
LightRAG (arXiv 2410.05779)；HippoRAG (2405.14831/2502.14802)；PathRAG (2502.14902)；KAG (2409.13731)。详见 [[来源汇总]]。
