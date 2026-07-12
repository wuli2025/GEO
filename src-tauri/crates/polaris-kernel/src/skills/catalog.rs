use super::*;

// ═══════════════════════════════════════════════════════════════
// 统一目录 Catalog（编译期，只读）
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct CatalogSkill {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub source: &'static str, // official | third-party
    /// true = 预装（始终可用，无需安装），false = 市场技能（需点安装）
    pub preinstalled: bool,
    pub system_prompt: &'static str,
}

pub(crate) fn catalog() -> Vec<CatalogSkill> {
    vec![
        // ── 自媒体全链路运营（交互决策版，与「自动化」里的两条流程同源） ──
        CatalogSkill {
            id: "wechat-pipeline",
            name: "微信公众号 · 全链路运营",
            description: "选题→风格→成稿→排版出图一条龙；每个决策点先讲思考再给编号选项让你挑、也可直接输入覆盖；风格可调；支持全自动",
            source: "third-party",
            preinstalled: true,
            system_prompt: include_str!("../../../../src/templates/skills/wechat-pipeline.md"),
        },
        CatalogSkill {
            id: "xiaohongshu-pipeline",
            name: "小红书 · 全链路运营",
            description: "选题→风格→文案→图卡渲染一条龙；每个决策点先讲思考再给编号选项让你挑、也可直接输入覆盖；风格可调；支持全自动",
            source: "third-party",
            preinstalled: true,
            system_prompt: include_str!("../../../../src/templates/skills/xiaohongshu-pipeline.md"),
        },
        // ── 自媒体全链路·配套三件套（选题前置 / 数据复盘 / 社群应对，补全闭环） ──
        CatalogSkill {
            id: "hot-topic-radar",
            name: "选题雷达",
            description: "联网抓热点+对标爆文，归纳成 3-5 个选题方向、每个给 2-3 个具体选题并做爆款拆解（为什么火/适合哪个平台/时效难度），编号供勾选；读 KB 避免撞题。可独立用，也是全链路第一步",
            source: "third-party",
            preinstalled: true,
            system_prompt: include_str!("../../../../src/templates/skills/hot-topic-radar.md"),
        },
        CatalogSkill {
            id: "content-analytics-report",
            name: "数据复盘 · 运营周报",
            description: "把一批已发文章/笔记的数据做成运营周报：逐篇打优劣势、找「哪类选题/标题/发布时机」数据好的规律、给下轮主攻方向，并回写 KB 反哺选题",
            source: "third-party",
            preinstalled: true,
            system_prompt: include_str!("../../../../src/templates/skills/content-analytics-report.md"),
        },
        CatalogSkill {
            id: "community-engagement",
            name: "评论 · 社群应对",
            description: "把评论/私信分类（提问/夸赞/抬杠/求合作/负面），按账号人格逐条起草回复，标出需本人亲自处理的高敏感项，并把高频疑问提炼成选题线索回写 KB",
            source: "third-party",
            preinstalled: true,
            system_prompt: include_str!("../../../../src/templates/skills/community-engagement.md"),
        },
        CatalogSkill {
            id: "xhs-mao-pipeline",
            name: "小红书 · 毛选风格发布",
            description: "调毛主席知识库析毛选文风→就给定主题写小红书爆款文案→出图(HTML图卡转截图 或 AI配图)→调 post-to-xhs 浏览器自动发布;发前必人工确认、可先预览、需扫码登录",
            source: "third-party",
            preinstalled: true,
            system_prompt: include_str!("../../../../src/templates/skills/xhs-mao-pipeline.md"),
        },
        // ── 壹伴排版优化（公众号排版 + CloakBrowser 直送草稿，根治格式错） ──
        CatalogSkill {
            id: "wechat-md-typesetter",
            name: "壹伴排版优化",
            description: "壹伴式排版：只产出干净语义正文（零内联样式），随包壹伴脚本在 CloakBrowser 公众号编辑器 DOM 上按约定风格一键套样式（标题色块/引用卡/分割线/列表转段落全内联），填标题存草稿（绝不自动发布）——根治粘贴格式错乱",
            source: "third-party",
            preinstalled: true,
            system_prompt: WECHAT_TS_SKILL_MD,
        },
        // ── 源自 ClaudeSkills 合集的两个内容创作技能（全链路成稿/出图时调用） ──
        CatalogSkill {
            id: "gz-wechat-article-writer",
            name: "公众号文章创作（ClaudeSkills）",
            description: "微信公众号文章创作助手：风格灵活适配（企业官号/个人技术博客/活动回顾/产品评测），优化标题与结构。全链路成稿阶段的内容引擎",
            source: "third-party",
            preinstalled: true,
            system_prompt: include_str!("../../../../src/templates/skills/gz-wechat-article-writer.md"),
        },
        CatalogSkill {
            id: "gz-notion-infographic",
            name: "信息图 / 小红书图文（ClaudeSkills）",
            description: "按大纲自动研究并生成高质量可视化：Notion 手绘风信息图组图 / PPTX，适合小红书图文与社媒传播图。全链路渲染阶段的图卡引擎",
            source: "third-party",
            preinstalled: true,
            system_prompt: include_str!("../../../../src/templates/skills/gz-notion-infographic.md"),
        },
    ]
}

pub(crate) fn find_catalog(id: &str) -> Option<CatalogSkill> {
    catalog().into_iter().find(|c| c.id == id)
}

/// 目录技能的市场分组。集中一处映射，新增技能记得来这里归组，漏归落「通用」。
pub(crate) fn skill_category(id: &str) -> &'static str {
    match id {
        // 办公文档
        "pdf" | "xlsx" | "pptx" | "deep-research" | "web-search" => "办公文档",
        // 财务会计
        "financial-model" | "invoice-audit" | "bookkeeping-recon" => "财务会计",
        // 开发编程
        "project-skill"
        | "git-commit"
        | "gh-cli"
        | "frontend-ui"
        | "rest-api-design"
        | "docker-deploy"
        | "sql-optimization"
        | "security-audit"
        | "tech-writing"
        | "systematic-debugging"
        | "writing-plans"
        | "verification-before-completion"
        | "mcp-builder" => "开发编程",
        // 测试质检
        "webapp-testing" | "e2e-test-pipeline" | "bug-report-repro" => "测试质检",
        // 设计美工
        "canvas-design" | "brand-guidelines" | "algorithmic-art" | "image-gen" => "设计美工",
        // 自媒体运营
        "wechat-pipeline"
        | "xiaohongshu-pipeline"
        | "hot-topic-radar"
        | "content-analytics-report"
        | "community-engagement"
        | "xhs-mao-pipeline"
        | "wechat-md-typesetter"
        | "gz-wechat-article-writer"
        | "gz-notion-infographic" => "自媒体运营",
        // 音视频
        "edge-tts" | "hyperframes" | "web-video-presentation" | "web-video-presentation-guide" => {
            "音视频"
        }
        // 自动化与浏览器
        "cloak-browser" | "browser-use" | "turbo-download" | "wechat-tasks" => "自动化与浏览器",
        _ => "通用",
    }
}
