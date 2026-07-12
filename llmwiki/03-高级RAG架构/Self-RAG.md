---
title: Self-RAG
aliases: [Self-RAG, 自反思RAG, reflection token]
tags: [高级架构, 自反思]
type: architecture
status: stable
updated: 2026-06-01
---

# Self-RAG（自反思检索）

> 让模型「自己判断要不要检索、检索回来的能不能用、答得忠不忠实」。论文 arXiv 2310.11511 (2023)。

## 核心思想

训练模型在生成中输出特殊的 **反思 token (reflection tokens)**：
- **Retrieve?**：这步该不该检索（不是每问都检索）。
- **IsRel**：检索片段是否相关。
- **IsSup**：生成内容是否被片段**支撑**（防幻觉，对应 [[RAG评测方法|faithfulness]]）。
- **IsUse**：答案是否有用。

模型据此自适应检索、自我批判、按 critique 分数挑最优续写。

## 怎么干

1. 预测是否需要检索；
2. 若需要，并行处理多个检索片段，各自生成并自评相关性/支撑度；
3. 按反思 token 打分选最佳输出。

## 优劣

- **强**：减少无谓检索、降幻觉、可控；
- **弱**：原版需训练带反思 token 的模型。实践中常**近似实现**为「检索→LLM 给相关性/支撑性打分→决定是否重检索/采纳」的 grader 循环（无需训练），与 [[CRAG]]、[[Agentic-RAG]] 思路相通。LangGraph 有教程。

## 与 llmwiki

「自己判断要不要再翻一篇、翻回来的有没有用」正是 [[Agentic-RAG|agentic 漫游]] 的内在逻辑。小库漫游时模型本就在做隐式 self-reflection；大库可显式加 grader 提质。

## 关联
[[CRAG]] · [[Adaptive-RAG与路由]] · [[Agentic-RAG]] · [[RAG评测方法]]

## 来源
arXiv 2310.11511；LangGraph Self-RAG 教程。详见 [[来源汇总]]。
