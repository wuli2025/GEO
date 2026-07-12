---
title: Agentic RAG
aliases: [Agentic RAG, 智能体检索, agentic retrieval, agentic search, 自主取知识]
tags: [高级架构, agent, 方向]
type: architecture
status: stable
updated: 2026-06-01
---

# Agentic RAG（智能体检索）

> **这是方向。** llmwiki/Polaris 让 Agent 拿 Read/Glob/Grep 自取，就是它的雏形。把检索从「后台一次盲摸」变成「模型手里会动脑的家伙」。见 [[RAG与llmwiki核心论点]]、[[上下文工程]]。

## 是什么

不再「一次性嵌入+检索」，而是**让模型用工具自己多轮检索**：reflection（反思）、planning（规划）、tool use（工具调用）、multi-agent（多智能体协作）。判据：能否**多轮执行**并按中间结果**自适应轮数**（而非一次性），是否走 **ReAct**（Reason+Act 交替）循环。综述见《Agentic RAG: A Survey》(arXiv 2501.09136)。

## 标志案例：Claude Code 放弃向量库

- Anthropic 放弃向量 RAG，改用 **agentic search**。负责人说它「**大幅胜过了一切**」。
- 机制：**Glob（文件名匹配）+ Grep（ripgrep 内容搜索）+ Read（按需读文件）**，边做边探索。
- 好处：**无需向量库、嵌入、切块、索引维护、rerank**；不存数据副本（隐私）、永远读当前文件（无陈旧）、自评自纠（更可靠）。
- 学术佐证：Amazon Science (2026-02) 称**纯关键词搜索 + agentic tool use 可达 RAG 90%+ 性能，且无需向量库**。

## 路线未定论（重要）

同期 **Cursor / Windsurf 仍走向量索引**（Cursor 对全仓做嵌入存 Turbopuffer，语义搜索约 +12.5% 准确率）。说明 agentic search vs 向量 RAG 在最前沿**仍有分叉**，取决于库规模、延迟、工程偏好——别武断站队。

## RL 训练的前沿

Search-R1、R1-Searcher、ReSearch 等用强化学习训练模型「自己学会何时检索、怎么检索」，把检索内化为推理能力（研究阶段）。

## 与 llmwiki（主场）

llmwiki + 长上下文 + 双链，是 agentic retrieval 的**理想地形**：
- 给模型配 **grep（[[BM25与全文检索]] 字面）+ 跳双链（[[GraphRAG|图]]）+ 再向量搜一把（[[向量检索]]）** 的工具组；
- 让它**一回不够探两回**，召回**整篇**自己漫游精读；
- 这就把 [[RAG与llmwiki核心论点|侦察兵进化成会动脑的侦察连]]。
落地配方见 [[RAG增强llmwiki方案]] 第五条与 [[自主学习工作流]]。

## 关联
[[上下文工程]] · [[查询改写]] · [[深度研究Agent]] · [[自主学习工作流]] · [[长上下文vs RAG之争]]

## 来源
arXiv 2501.09136；Anthropic Claude Code（agentic search）；Cursor 索引博客；Search-R1 (2503.09516)。详见 [[来源汇总]]。
