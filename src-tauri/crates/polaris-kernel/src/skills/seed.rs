use super::*;

/// 启动时确保「网站生成」技能在 ~/Polaris/skills 落盘。目录缺失 / 版本旧
/// （`.polaris_version` < `WEB_VERSION`）就（重）写；已是最新则跳过。best-effort。
pub fn seed_web_studio_skill() {
    let Some(root) = skills_dir() else {
        return;
    };
    let dest = root.join(WEB_ID);
    let ver_file = dest.join(".polaris_version");
    let stored = fs::read_to_string(&ver_file).unwrap_or_default();
    let present = dest.join("skill.md").exists();
    if present && stored.trim() == WEB_VERSION {
        return;
    }
    if write_web_studio_files(&dest).is_ok() {
        let _ = fs::write(&ver_file, WEB_VERSION);
    }
}

/// 把内嵌的「网站生成」全部文件写到目标目录。themes.css 复用 deck-studio 的同一份内容。
fn write_web_studio_files(dest: &Path) -> Result<(), String> {
    let assets = dest.join("assets");
    let templates = dest.join("templates");
    fs::create_dir_all(&assets).map_err(|e| e.to_string())?;
    fs::create_dir_all(&templates).map_err(|e| e.to_string())?;
    fs::write(dest.join("skill.md"), WEB_SKILL_MD).map_err(|e| e.to_string())?;
    fs::write(dest.join("LICENSE"), WEB_LICENSE).map_err(|e| e.to_string())?;
    fs::write(assets.join("site.css"), WEB_SITE_CSS).map_err(|e| e.to_string())?;
    fs::write(assets.join("themes.css"), DECK_THEMES_CSS).map_err(|e| e.to_string())?;
    fs::write(assets.join("runtime.js"), WEB_RUNTIME_JS).map_err(|e| e.to_string())?;
    fs::write(assets.join("motion.css"), WEB_MOTION_CSS).map_err(|e| e.to_string())?;
    fs::write(assets.join("motion.js"), WEB_MOTION_JS).map_err(|e| e.to_string())?;
    fs::write(templates.join("site.html"), WEB_TEMPLATE).map_err(|e| e.to_string())?;
    write_designers(dest)?; // 网站生成复用同一份设计师人格包
    Ok(())
}

/// 检查技能的执行入口(collab/checks.rs 按此协议跑脚本)。
#[derive(Debug, Clone, serde::Serialize)]
pub struct CheckSkillEntry {
    pub skill_id: String,
    /// 入口脚本绝对路径(按当前平台选 check_entry_windows / check_entry_unix)。
    pub entry: PathBuf,
    /// PowerShell 入口(true)还是 sh 入口(false)。
    pub windows: bool,
    pub timeout_secs: u64,
}

/// 解析某技能的检查协议(frontmatter 扁平键 check_entry_windows/check_entry_unix/check_timeout_secs)。
/// 只信 ~/Polaris/skills 下主机自装的技能——绝不从任务分支读,防协作者注入检查脚本。
pub fn resolve_check_skill(id: &str) -> Result<CheckSkillEntry, String> {
    // id 复用 delete_skill 同款安全闸:拒路径穿越。
    if id.is_empty()
        || id.contains("..")
        || id.contains('/')
        || id.contains('\\')
        || id.contains(':')
    {
        return Err(format!("检查技能 id 非法: {id}"));
    }
    let root = skills_dir().ok_or("无法获取用户目录")?;
    let dir = root.join(id);
    let skill_md = dir.join("skill.md");
    let content = fs::read_to_string(&skill_md)
        .map_err(|_| format!("检查技能 {id} 未安装(缺 skill.md),请在技能中心安装或重启应用"))?;
    let mut entry_win = String::new();
    let mut entry_unix = String::new();
    let mut timeout: u64 = 600;
    for line in content.lines().take(60) {
        if let Some((k, v)) = line.split_once(':') {
            let v = v.trim().trim_matches('"').trim_matches('\'');
            match k.trim() {
                "check_entry_windows" => entry_win = v.to_string(),
                "check_entry_unix" => entry_unix = v.to_string(),
                "check_timeout_secs" => timeout = v.parse().unwrap_or(600),
                _ => {}
            }
        }
    }
    let windows = cfg!(windows);
    let rel = if windows { &entry_win } else { &entry_unix };
    if rel.is_empty() {
        return Err(format!(
            "技能 {id} 未声明检查入口(check_entry_windows/check_entry_unix),不是检查技能"
        ));
    }
    if rel.contains("..") {
        return Err(format!("技能 {id} 检查入口路径非法: {rel}"));
    }
    let entry = dir.join(rel);
    if !entry.is_file() {
        return Err(format!("技能 {id} 检查入口脚本不存在: {rel}"));
    }
    Ok(CheckSkillEntry {
        skill_id: id.to_string(),
        entry,
        windows,
        timeout_secs: timeout.clamp(30, 3600),
    })
}

/// 列出本机已安装、声明了检查协议的技能(检查设置下拉用)。返回 (id, name)。
pub fn list_check_capable() -> Vec<(String, String)> {
    scan_user_skills()
        .into_iter()
        .filter(|s| resolve_check_skill(&s.id).is_ok())
        .map(|s| (s.id, s.name))
        .collect()
}

// 注：原 `migrate_consult_mao_for_seeded_kb`（为早期播种过毛主席资料库的老用户启动时
// 自动补装 consult-mao 技能）已移除。现「请教毛主席」默认隐藏，只在用户主动安装
// 「毛主席」名人资料包（kb_pack_install）时才装该技能，启动时不再自动补装。

/// 启动时确保「壹伴排版优化」技能在 ~/Polaris/skills 落盘（多文件，含 wechat_yiban.py 可执行脚本）。
///
/// 目录缺失 / 版本旧（`.polaris_version` < `WECHAT_TS_VERSION`）就（重）写；已是最新则跳过。
/// 脚本必须真落到磁盘，spawn 的 claude agent 才能 `python …/wechat_yiban.py` 跑它。
/// best-effort：失败只让「壹伴直送草稿」暂不可用，不阻断 App 启动。
pub fn seed_wechat_typesetter_skill() {
    let Some(root) = skills_dir() else {
        return;
    };
    let dest = root.join(WECHAT_TS_ID);
    let ver_file = dest.join(".polaris_version");
    let stored = fs::read_to_string(&ver_file).unwrap_or_default();
    let present = dest.join("skill.md").exists();
    if present && stored.trim() == WECHAT_TS_VERSION {
        return;
    }
    if write_wechat_typesetter_files(&dest).is_ok() {
        let _ = fs::write(&ver_file, WECHAT_TS_VERSION);
    }
}

/// 把内嵌的「壹伴排版优化」文件写到目标目录。技能正文写成小写 `skill.md`，与扫描约定一致。
fn write_wechat_typesetter_files(dest: &Path) -> Result<(), String> {
    let scripts = dest.join("scripts");
    fs::create_dir_all(&scripts).map_err(|e| e.to_string())?;
    fs::write(dest.join("skill.md"), WECHAT_TS_SKILL_MD).map_err(|e| e.to_string())?;
    fs::write(scripts.join("wechat_yiban.py"), WECHAT_TS_YIBAN_PY).map_err(|e| e.to_string())?;
    Ok(())
}
