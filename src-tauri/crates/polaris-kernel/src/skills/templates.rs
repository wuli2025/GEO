use super::*;

// ───────── deck-studio 遗留资源（web-studio 复用，编译期内嵌）─────────
// polaris-deck-studio 技能本体已裁撤，但「网站生成」(web-studio) 复用其 17 套主题
// (themes.css) 与设计师人格包(designers/)，故这两项资源与 write_designers 予以保留。
pub(crate) const DECK_THEMES_CSS: &str =
    include_str!("../../../../src/templates/skills/polaris-deck-studio/assets/themes.css");

// ───────── 设计师人格包（designers/，编译期内嵌，随 web-studio 落盘）─────────
// 「选设计师」体系：11 位设计师人格 + 美学地基(_foundation) + 总索引(INDEX.md)。
// auto 模式按 INDEX.md 路由表按内容气质选人；用户指定则用指定的。网站生成复用同一份包。
pub(crate) const DECK_DESIGNERS: &[(&str, &str)] = &[
    (
        "INDEX.md",
        include_str!("../../../../src/templates/skills/polaris-deck-studio/designers/INDEX.md"),
    ),
    (
        "_foundation/aesthetics.md",
        include_str!("../../../../src/templates/skills/polaris-deck-studio/designers/_foundation/aesthetics.md"),
    ),
    (
        "_foundation/rubric.md",
        include_str!("../../../../src/templates/skills/polaris-deck-studio/designers/_foundation/rubric.md"),
    ),
    (
        "_foundation/taste.md",
        include_str!("../../../../src/templates/skills/polaris-deck-studio/designers/_foundation/taste.md"),
    ),
    (
        "bento-grid.md",
        include_str!("../../../../src/templates/skills/polaris-deck-studio/designers/bento-grid.md"),
    ),
    (
        "clay-soft.md",
        include_str!("../../../../src/templates/skills/polaris-deck-studio/designers/clay-soft.md"),
    ),
    (
        "doodle-hand.md",
        include_str!("../../../../src/templates/skills/polaris-deck-studio/designers/doodle-hand.md"),
    ),
    (
        "glass-crisp.md",
        include_str!("../../../../src/templates/skills/polaris-deck-studio/designers/glass-crisp.md"),
    ),
    (
        "keynote-tech.md",
        include_str!("../../../../src/templates/skills/polaris-deck-studio/designers/keynote-tech.md"),
    ),
    (
        "memphis-pop.md",
        include_str!("../../../../src/templates/skills/polaris-deck-studio/designers/memphis-pop.md"),
    ),
    (
        "mist-gradient.md",
        include_str!("../../../../src/templates/skills/polaris-deck-studio/designers/mist-gradient.md"),
    ),
    (
        "oriental-grandeur.md",
        include_str!("../../../../src/templates/skills/polaris-deck-studio/designers/oriental-grandeur.md"),
    ),
    (
        "pedagogy-clarity.md",
        include_str!("../../../../src/templates/skills/polaris-deck-studio/designers/pedagogy-clarity.md"),
    ),
    (
        "swiss-modernist.md",
        include_str!("../../../../src/templates/skills/polaris-deck-studio/designers/swiss-modernist.md"),
    ),
    (
        "xhs-life.md",
        include_str!("../../../../src/templates/skills/polaris-deck-studio/designers/xhs-life.md"),
    ),
];

/// 把设计师人格包写到 <dest>/designers/（含 _foundation 子目录）。web-studio 落盘时调用。
pub(crate) fn write_designers(dest: &Path) -> Result<(), String> {
    let designers = dest.join("designers");
    fs::create_dir_all(designers.join("_foundation")).map_err(|e| e.to_string())?;
    // 先剪除已裁撤的旧人格文件：write 只覆盖不删除，裁掉的设计师 .md 会残留在旧安装里，
    // 让 auto 路由仍能选到「不存在于花名册」的幽灵设计师。只扫顶层 *.md（_foundation 在子目录，
    // read_dir 非递归，不受影响），凡不在 DECK_DESIGNERS 白名单里的一律删。
    let keep: std::collections::HashSet<&str> =
        DECK_DESIGNERS.iter().map(|(rel, _)| *rel).collect();
    if let Ok(entries) = fs::read_dir(&designers) {
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().and_then(|s| s.to_str()) == Some("md") {
                if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
                    if !keep.contains(name) {
                        let _ = fs::remove_file(&p);
                    }
                }
            }
        }
    }
    for (rel, content) in DECK_DESIGNERS {
        fs::write(designers.join(rel), content).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ───────── 「网站生成」技能（落地页/单页站点，Polaris 自研，编译期内嵌，启动落盘）─────────
// 支撑「网站生成」UI 入口。复用 deck-studio 的 17 套主题（DECK_THEMES_CSS，不重复源文件），
// 配一套网站组件 site.css + 滚动揭示 runtime.js + 站点模板 + SKILL.md。
pub(crate) const WEB_ID: &str = "polaris-web-studio";
pub(crate) const WEB_VERSION: &str = "6";
pub(crate) const WEB_SKILL_MD: &str =
    include_str!("../../../../src/templates/skills/polaris-web-studio/SKILL.md");
pub(crate) const WEB_LICENSE: &str = include_str!("../../../../src/templates/skills/polaris-web-studio/LICENSE");
pub(crate) const WEB_SITE_CSS: &str =
    include_str!("../../../../src/templates/skills/polaris-web-studio/assets/site.css");
pub(crate) const WEB_RUNTIME_JS: &str =
    include_str!("../../../../src/templates/skills/polaris-web-studio/assets/runtime.js");
pub(crate) const WEB_MOTION_CSS: &str =
    include_str!("../../../../src/templates/skills/polaris-web-studio/assets/motion.css");
pub(crate) const WEB_MOTION_JS: &str =
    include_str!("../../../../src/templates/skills/polaris-web-studio/assets/motion.js");
pub(crate) const WEB_TEMPLATE: &str =
    include_str!("../../../../src/templates/skills/polaris-web-studio/templates/site.html");

// ───────── 「壹伴排版优化」多文件技能（公众号排版，编译期内嵌，启动落盘）─────────
// 升级成多文件：SKILL.md（只产语义正文）+ scripts/wechat_yiban.py（壹伴样式引擎 + CloakBrowser
// 驱动）。编译期内嵌、启动确保落到 ~/Polaris/skills（靠版本号比对覆盖），这样脚本能被 spawn
// 的 claude agent 在磁盘上直接 `python …/wechat_yiban.py` 执行。
pub(crate) const WECHAT_TS_ID: &str = "wechat-md-typesetter";
// 改动 SKILL.md 或 wechat_yiban.py 后必须 +1，让已安装用户下次启动拿到更新。
pub(crate) const WECHAT_TS_VERSION: &str = "11";
pub(crate) const WECHAT_TS_SKILL_MD: &str =
    include_str!("../../../../src/templates/skills/wechat-md-typesetter/SKILL.md");
pub(crate) const WECHAT_TS_YIBAN_PY: &str =
    include_str!("../../../../src/templates/skills/wechat-md-typesetter/scripts/wechat_yiban.py");

// ───────── 「多平台草稿投递官」多文件技能（自媒体投递，编译期内嵌，启动落盘）─────────
// SKILL.md（投递说明书）+ scripts/draft_uploader.py（7 平台草稿投递引擎，CloakBrowser 粘贴通道）
// + scripts/ark_image.py（火山方舟 Seedream 生图 CLI）。与 wechat-md-typesetter 同机制：
// 编译期内嵌、启动确保落到 ~/PolarisGEO/skills，spawn 的 claude agent 直接 `python …` 跑脚本。
pub(crate) const MEDIA_PUB_ID: &str = "media-publisher";
// 改动 SKILL.md 或任一 py 后必须 +1，让已安装用户下次启动拿到更新。
pub(crate) const MEDIA_PUB_VERSION: &str = "1";
pub(crate) const MEDIA_PUB_SKILL_MD: &str =
    include_str!("../../../../src/templates/skills/media-publisher/SKILL.md");
pub(crate) const MEDIA_PUB_UPLOADER_PY: &str =
    include_str!("../../../../src/templates/skills/media-publisher/scripts/draft_uploader.py");
pub(crate) const MEDIA_PUB_ARK_PY: &str =
    include_str!("../../../../src/templates/skills/media-publisher/scripts/ark_image.py");

// ───────── 「项目检测」默认检查技能 id（技能本体已裁撤，仅保留 id 供 polaris-collab 检查闸引用）─────────
// polaris-collab 的检查闸(collab/checks.rs / http.rs)以此为默认 check_skill id。技能模板与
// seed 已随本次裁剪移除，运行期若未安装同名检查技能，检查闸会回退到「技能缺失=fail」。
pub const PROJECT_CHECK_ID: &str = "project-check-default";
