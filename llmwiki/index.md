---
title: 内容地图 (MOC)
type: nav
updated: 2026-06-01
---

# 内容地图 · Map of Content

> 全库导航。每条一行摘要，方便人和模型快速定位。新增页后请在此登记一行（见 [[CLAUDE]] 规范）。

## 00 · 导航
- [[学习路线图]] —— 从零到精通的分阶段路线，配「该学 / 该懂 / 别死磕」三档。
- [[技术选型决策树]] —— 按「库多大、问什么、要多准」选技术的决策流程。
- [[术语表]] —— RAG / 检索 / Agent 高频中英术语对照与一句话定义。

## 01 · 总纲
- [[RAG与llmwiki核心论点]] —— 全库主线：RAG 当侦察兵不当主帅，功夫下在结构上。
- [[上下文工程]] —— context engineering：把对的知识、在对的时候、用对的格式送进窗口。
- [[长上下文vs RAG之争]] —— 大窗口杀死 RAG 了吗？lost-in-the-middle、context rot、混合派。

## 02 · 检索基础（「术」的根基）
- [[向量检索]] —— 语义检索、嵌入、ANN（HNSW/IVF/PQ）。
- [[BM25与全文检索]] —— 倒排索引、BM25、FTS5/Tantivy/ES，中文分词坑。
- [[混合检索与RRF]] —— 三路召回合兵一处，倒数排名融合。
- [[重排rerank]] —— cross-encoder、ColBERT、Cohere/BGE rerank，性价比最高的一步。
- [[查询改写]] —— query rewrite、多查询、step-back、子问题分解、[[HyDE]]。
- [[嵌入模型选型]] —— 2026 SOTA、MTEB、Matryoshka、中文首选 BGE-M3 / Qwen3。
- [[分块策略]] —— 切块各法、parent-document、为何「召回整篇而非碎片」。

## 03 · 高级 RAG 架构
- [[GraphRAG]] —— 微软图谱 RAG：实体图 + Leiden 社区摘要 + local/global 检索。
- [[Agentic-RAG]] —— 让模型用工具自己多轮检索；llmwiki 的进化方向。
- [[Self-RAG]] —— 自反思检索，反思 token 决定是否检索/是否采纳。
- [[CRAG]] —— 纠错式 RAG：检索质量评估 + 网络兜底。
- [[HyDE]] —— 假设性文档嵌入，先让模型写答案再去检索。
- [[RAPTOR]] —— 递归摘要树，多层抽象支撑全局问题。
- [[LightRAG与HippoRAG]] —— 轻量图 RAG 与海马体启发的多跳检索。
- [[Adaptive-RAG与路由]] —— 按问题难度自适应选择检索策略。
- [[上下文检索-Contextual-Retrieval]] —— Anthropic：给每块加情境，检索失败率降 49–67%。
- [[Late-Chunking]] —— Jina：先整篇编码再切块，块向量自带全文语境。

## 04 · 工程与企业实践
- [[向量数据库选型]] —— pgvector/Qdrant/Milvus/Vespa/Pinecone… 按规模选。
- [[RAG框架选型]] —— LlamaIndex / LangGraph / Haystack / DSPy 各擅长什么。
- [[托管RAG服务]] —— Bedrock KB / Azure AI Search / Vertex / OpenAI File Search。
- [[企业级RAG架构]] —— 摄入→索引→检索→生成→观测，多租户与权限。
- [[RAG失败模式]] —— RAG 七大失败点 + 修法。
- [[成本与延迟优化]] —— 语义缓存、prompt 缓存、量化、prompt 压缩。

## 05 · 评测（把「我觉得」变「实测」）
- [[RAG评测方法]] —— 检索指标 vs 生成指标，评测驱动迭代流程。
- [[RAGAS]] —— 主流评测框架，faithfulness / context precision 等怎么算。
- [[评测基准]] —— RGB（中英）、CRAG、FRAMES、RULER、LongBench、MTEB。
- [[黄金测试集构建]] —— 从真实查询 + 合成 + 专家校验造金标。

## 06 · 与 llmwiki 结合（主战场）
- [[RAG增强llmwiki方案]] —— 按库大小分三阶段的落地配方。
- [[知识补全与缺口检测]] —— 检测孤儿页、断链、缺失概念，自动补全。
- [[自主学习工作流]] —— 让库读源、写笔记、自己生长的端到端流程。
- [[STORM与Co-STORM]] —— 斯坦福：多视角提问自动起草带引用词条。
- [[LLM-Wiki维护方法论]] —— Karpathy 的三层架构、ingest/query/lint。

## 07 · 记忆与自主智能体
- [[Agent记忆系统]] —— MemGPT/Letta、Mem0、A-Mem（Zettelkasten 式自演化）。
- [[深度研究Agent]] —— OpenAI/Anthropic deep research，orchestrator-worker 模式。

---
- [[来源汇总]] —— 全部一手与二手来源链接。
- [[log]] —— 维护流水账。
