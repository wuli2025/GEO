---
title: GraphRAG
aliases: [GraphRAG, 图谱RAG, 知识图谱RAG, Microsoft GraphRAG, Leiden, 社区检测]
tags: [高级架构, 图谱, 多跳, 全局问]
type: architecture
status: stable
updated: 2026-06-01
---

# GraphRAG（图谱增强 RAG）

> 你 llmwiki 已有知识图谱 + 社区检测——这页帮你看清它在 RAG 里的位置与微软的标准做法。**强项是多跳推理和「整个语料主题是什么」这类全局问题。**

## 解决什么

传统 [[向量检索|向量 RAG]] 擅长找「局部」片段，但面对**全局性 (global sensemaking)** 问题（「这批文档整体在讲什么」）失效——那本质是面向查询的**摘要**，不是检索。GraphRAG 先把文档抽成实体关系图，再社区划分+摘要，同时支撑局部与全局，并天然支持多跳。

## 微软 GraphRAG 流水线

论文《From Local to Global》(Edge et al., arXiv 2404.16130, 2024)。

**索引四步**：
1. **TextUnits 切分**——切成可分析、可溯源的文本单元。
2. **实体/关系/声明抽取**——LLM 逐单元抽取，建加权实体图。
3. **层次社区检测 (Leiden)**——Leiden 算法（改进 Louvain，避免「不连通社区」）把图划分为 C0–C3 层次社区。
4. **社区摘要**——自底向上为每层每个社区生成自然语言摘要。

**查询四模式**：
- **Global Search**：用社区摘要整体推理，**map-reduce**（各社区出部分答案→评分排序填满窗口→合成）。根级摘要比直接处理原文省约 97% token。
- **Local Search**：聚焦实体，向邻居扇出。
- **DRIFT Search**：local + 额外引入社区上下文。
- **Basic Search**：退化为向量相似度。

## ⚠️ 现实校准（hype vs 实证）

- 《When to use Graphs in RAG》(arXiv 2506.05690) 关键发现：**图 RAG 在简单事实查询上常不如朴素 RAG，却多花 10–40× token**（MS GraphRAG global ~40K token vs 朴素 ~900）。
- **只在多跳推理、跨文档归纳、摘要上才决定性取胜**（HippoRAG 多跳证据召回 ~88–91% vs RAG ~60–65%）。
- 选型按**查询分布**走，不是越图越好。
- 「准确率 86% vs 32%」等第三方数字**待核**。

## 轻量变体

见 [[LightRAG与HippoRAG]]：LightRAG（双层检索、极致省 token、增量友好）、HippoRAG（海马体启发、PPR 遍历、多跳便宜 10–30×）、PathRAG、KAG（蚂蚁集团已部署，重）。

## ⚠️ 中文坑

所有图/抽取方法的通病是**中文实体抽取/OpenIE 质量**，英文基准测不出。KAG 生于中文语境，高风险中文专业域适配最好。

## 与 llmwiki

你的双链本身就是一张图，**结构即图谱**。GraphRAG 的思想印证「[[RAG与llmwiki核心论点|顺链跳比盲摸强]]」。落地上：用图谱/双链做第三路召回（见 [[混合检索与RRF]]），全局问走 community-summary 思路；增量生长用 LightRAG 式思路最省。

## 关联
[[LightRAG与HippoRAG]] · [[RAPTOR]] · [[混合检索与RRF]] · [[知识补全与缺口检测]] · [[RAG与llmwiki核心论点]]

## 来源
arXiv 2404.16130；microsoft.github.io/graphrag；Neo4j GraphRAG 教程；arXiv 2506.05690 / 2502.11371。详见 [[来源汇总]]。
