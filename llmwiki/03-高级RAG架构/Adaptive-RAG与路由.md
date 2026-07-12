---
title: Adaptive RAG 与路由
aliases: [Adaptive RAG, 自适应RAG, 路由, routing, SELF-ROUTE, Speculative RAG, LongRAG]
tags: [高级架构, 路由, 自适应]
type: architecture
status: stable
updated: 2026-06-01
---

# Adaptive RAG 与路由

> 「不同的问，用不同的法。」按问题难度/类型，**自适应选择**检索策略——这是「具体问题具体分析」的工程化。

## Adaptive-RAG

论文 arXiv 2403.14403。先用一个分类器判断 query 复杂度，再路由：
- **简单事实** → 不检索，直接答（省钱省延迟）；
- **单跳** → 单次检索；
- **多跳/复杂** → 多步迭代检索（[[Agentic-RAG]]）。

## 相关路由思想

- **SELF-ROUTE**（arXiv 2407.16833）：让模型自判该走 **RAG** 还是 **长上下文**（见 [[长上下文vs RAG之争]]），混合优于二选一。
- **查询路由 (query routing)**：把问题路由到不同数据源/索引/工具（如「代码问→grep，概念问→向量，全局问→[[GraphRAG]]」），是 [[技术选型决策树]] 的运行时版本。

## 其它前沿变体

- **Speculative RAG**（2407.08223）：小模型并行起草多份基于不同片段子集的答案，大模型校验择优。需蒸馏 drafter，研究阶段。
- **LongRAG**（2406.15319）：用更长的检索单元（整段/整组文档）减少切块，配长上下文阅读器。与「[[分块策略|召回整篇]]」一脉相承。

## 优劣

- **强**：省成本、降延迟、按需提质——避免「杀鸡用牛刀」也避免「杀牛用鸡刀」；
- **弱**：路由判断本身可能出错；多一层分类逻辑。

## 与 llmwiki

[[技术选型决策树]] 的三问（库多大/问什么/要多准）就是一套 adaptive 路由。落地上：让 llmwiki 的 agent **先判断问题类型**，再决定「直接漫游 / 单次粗筛 / 多轮自取」，对简单问别启动重检索。

## 关联
[[Self-RAG]] · [[CRAG]] · [[Agentic-RAG]] · [[长上下文vs RAG之争]] · [[技术选型决策树]]

## 来源
arXiv 2403.14403 (Adaptive-RAG)；2407.16833 (SELF-ROUTE)；2407.08223 (Speculative RAG)；2406.15319 (LongRAG)。详见 [[来源汇总]]。
