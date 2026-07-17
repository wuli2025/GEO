//! 专家 CLAUDE.md 文档 — 每个专家一份可编辑的 .md 提示词（放仓库源码、编译期内嵌）。
//!
//! 取数顺序:
//!   1. `src/templates/experts/<group>/<id>.md` —— 该专家**亲自写好的**提示词(强烈推荐,
//!      调它就是调这个文件,改完重编即生效)。文件内可用下方变量占位,也可全篇手写散文。
//!   2. 找不到该文件 → 回落 `GENERIC.md` 通用骨架 + 变量替换(保证任何专家都有内容)。
//!
//! 变量: {{NAME}} · {{ID}} · {{GROUP}} · {{ROLE}} · {{DESCRIPTION}} · {{KEYWORDS}} ·
//!       {{CAPABILITIES}} · {{TRIGGER_SIGNALS}} · {{COMPLEMENTS}} ·
//!       {{EXCLUSIVE_WITH}} · {{COST_TIER}} · {{TIMESTAMP}}

use include_dir::{include_dir, Dir};
use std::time::{SystemTime, UNIX_EPOCH};

/// 整个专家提示词目录编进二进制 —— 每加一个 `<group>/<id>.md` 文件,重编后自动可用。
static EXPERTS_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/src/templates/experts");

/// 取某专家的完整提示词正文(供 expert_apply / expert_export / route_block 注入)。
///
/// 先找该专家专属 .md(`experts/<group>/<id>.md`),没有再用 GENERIC 骨架。两条路都会做
/// 变量替换,所以专属文件既能全篇手写、也能内嵌 {{NAME}} 等占位由元数据补全。
pub fn build_expert_doc(
    ref_path: &str,
    name: &str,
    role: &str,
    description: &str,
    keywords: &[String],
    capabilities: &[String],
    trigger_signals: &[String],
    complements: &str,
    exclusive_with: &[String],
    cost_tier: u8,
) -> Option<String> {
    // ref_path 形如 "experts/marketing/visual-designer.md";嵌入目录的根就是 experts/,
    // 所以去掉前缀后用 "marketing/visual-designer.md" 查。
    let rel = ref_path.trim_start_matches("experts/");
    let parts: Vec<&str> = rel.trim_end_matches(".md").split('/').collect();
    let id = parts.last().unwrap_or(&"unknown").to_string();
    let group = parts
        .get(parts.len().saturating_sub(2))
        .unwrap_or(&"unknown")
        .to_string();

    // ① 专家专属提示词文件优先;② 否则通用骨架。
    let template: String = EXPERTS_DIR
        .get_file(rel)
        .and_then(|f| f.contents_utf8())
        .map(|s| s.to_string())
        .unwrap_or_else(|| GENERIC_TEMPLATE.to_string());

    let timestamp = current_date();
    let mut result = template;
    result = result.replace("{{NAME}}", name);
    result = result.replace("{{ID}}", &id);
    result = result.replace("{{GROUP}}", &group);
    result = result.replace("{{ROLE}}", role);
    result = result.replace("{{DESCRIPTION}}", description);
    result = result.replace("{{KEYWORDS}}", &keywords.join("、"));
    result = result.replace(
        "{{CAPABILITIES}}",
        &capabilities
            .iter()
            .map(|s| format!("- **{}**", s))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    result = result.replace(
        "{{TRIGGER_SIGNALS}}",
        &trigger_signals
            .iter()
            .map(|s| format!("- **{}**", s))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    result = result.replace("{{COMPLEMENTS}}", complements);
    result = result.replace(
        "{{EXCLUSIVE_WITH}}",
        &exclusive_with
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("、"),
    );
    result = result.replace("{{COST_TIER}}", &cost_tier.to_string());
    result = result.replace("{{TIMESTAMP}}", &timestamp);
    Some(result)
}

fn current_date() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| chrono::DateTime::from_timestamp(d.as_secs() as i64, 0))
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| String::new())
}

/// GENERIC.md 通用骨架（编译期内嵌）—— 仅当某专家还没有专属 .md 时回落使用。
const GENERIC_TEMPLATE: &str = include_str!("../templates/experts/GENERIC.md");

// ───────────────────────── 自媒体：平台提示词补丁 ─────────────────────────
//
// 同一位自媒体专家的「基础画像」是平台无关的（templates/experts/media/{id}.md）；
// 各平台的文风宪法 / 标题公式 / 排版规范 / 红线以「补丁」形式叠加：
//   1. 运行时目录  ~/PolarisGEO/data/expert-overlays/{platform}/{expert_id}.md  （用户可编辑，最高优先）
//   2. 内嵌种子    templates/experts/media/overlays/{platform}--{expert_id}.md   （随包发布的默认补丁）
//   3. 都没有      只用基础画像

/// 只允许小写字母/数字/连字符，杜绝路径穿越（platform、expert_id 都过这道闸）。
fn sanitize_seg(s: &str) -> Option<String> {
    let s = s.trim();
    if s.is_empty() || s.len() > 64 {
        return None;
    }
    if s.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        Some(s.to_string())
    } else {
        None
    }
}

/// 运行时补丁根目录：~/PolarisGEO/data/expert-overlays。
fn overlay_runtime_root() -> Option<std::path::PathBuf> {
    directories::UserDirs::new().map(|u| {
        u.home_dir()
            .join("PolarisGEO")
            .join("data")
            .join("expert-overlays")
    })
}

/// 运行时补丁文件路径（不保证存在）。
fn overlay_runtime_path(platform: &str, expert_id: &str) -> Option<std::path::PathBuf> {
    let p = sanitize_seg(platform)?;
    let id = sanitize_seg(expert_id)?;
    Some(overlay_runtime_root()?.join(p).join(format!("{id}.md")))
}

/// 解析某平台某专家的补丁内容 + 来源。
/// 返回 (source, content)：source ∈ "runtime" | "seed" | "none"；none 时 content 为空串。
pub fn media_overlay_resolve(platform: &str, expert_id: &str) -> (String, String) {
    // 1) 运行时目录优先
    if let Some(path) = overlay_runtime_path(platform, expert_id) {
        if let Ok(txt) = std::fs::read_to_string(&path) {
            if !txt.trim().is_empty() {
                return ("runtime".to_string(), txt);
            }
        }
    }
    // 2) 内嵌种子 overlays/{platform}--{expert_id}.md
    if let (Some(p), Some(id)) = (sanitize_seg(platform), sanitize_seg(expert_id)) {
        let rel = format!("media/overlays/{p}--{id}.md");
        if let Some(txt) = EXPERTS_DIR
            .get_file(&rel)
            .and_then(|f| f.contents_utf8())
            .filter(|s| !s.trim().is_empty())
        {
            return ("seed".to_string(), txt.to_string());
        }
    }
    // 3) 无补丁
    ("none".to_string(), String::new())
}

/// 写入/删除运行时补丁。content 为空串 = 删除运行时补丁（回落种子/基础画像）。
/// 采用「临时文件 + rename」原子写，避免半截文件。
pub fn media_overlay_write(platform: &str, expert_id: &str, content: &str) -> Result<(), String> {
    let path = overlay_runtime_path(platform, expert_id)
        .ok_or_else(|| "非法的 platform / expert_id".to_string())?;

    // 空内容 = 删除运行时补丁（存在才删）
    if content.trim().is_empty() {
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("md.tmp");
    std::fs::write(&tmp, content).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(())
}
