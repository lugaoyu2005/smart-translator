//! 术语库：terms.json 持久化 + 增删改命令
//! 存储位置：与 settings.json 同目录的 terms.json

use crate::engines::{TermBase, TermEntry, TermPackage, TermScheme};
use crate::translation::AppState;

fn terms_path() -> std::path::PathBuf {
    let mut path = std::env::current_exe().unwrap_or_default();
    path.pop(); // 去掉exe文件名
    path.push("terms.json");
    path
}

/// 同步加载术语库（setup阶段使用）；文件不存在或损坏时返回空库
pub fn load_term_base() -> TermBase {
    let path = terms_path();
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(tb) = serde_json::from_str::<TermBase>(&content) {
                return tb;
            }
        }
    }
    TermBase::default()
}

/// 持久化术语库到 terms.json
pub fn save_term_base(tb: &TermBase) -> Result<(), String> {
    let json = serde_json::to_string_pretty(tb).map_err(|e| format!("序列化术语库失败: {e}"))?;
    std::fs::write(terms_path(), json).map_err(|e| format!("写入术语库失败: {e}"))
}

// Tauri命令：列出全部术语（按源词排序）
#[tauri::command]
pub async fn list_terms(state: tauri::State<'_, AppState>) -> Result<Vec<TermEntry>, String> {
    let manager = state.manager.lock().await;
    let mut terms = manager.list_terms();
    terms.sort_by(|a, b| a.source.to_lowercase().cmp(&b.source.to_lowercase()));
    Ok(terms)
}

// Tauri命令：新增术语/译法（已存在时静默跳过）
#[tauri::command]
pub async fn add_term(
    state: tauri::State<'_, AppState>,
    source: String,
    translation: String,
    scheme_id: Option<String>,
) -> Result<(), String> {
    let source = source.trim().to_string();
    let translation = translation.trim().to_string();
    if source.is_empty() || translation.is_empty() {
        return Err("源词和译法不能为空".to_string());
    }
    let mut manager = state.manager.lock().await;
    let tb = manager.term_base_mut();
    tb.add_translation(&source, &translation);
    // 术语归属当前激活方案（有独占方案的术语仅在该方案生效）
    if let Some(sid) = scheme_id.filter(|s| !s.is_empty()) {
        if let Some(scheme) = tb.schemes.iter_mut().find(|s| s.id == sid) {
            if !scheme.enabled_entry_keys.is_empty()
                && !scheme.enabled_entry_keys.iter().any(|k| k == &source)
            {
                scheme.enabled_entry_keys.push(source);
            }
        }
    }
    save_term_base(manager.term_base())
}

// Tauri命令：删除整条术语
#[tauri::command]
pub async fn delete_term(
    state: tauri::State<'_, AppState>,
    source: String,
) -> Result<(), String> {
    let mut manager = state.manager.lock().await;
    let tb = manager.term_base_mut();
    tb.remove_entry(&source);
    for s in &mut tb.schemes {
        s.enabled_entry_keys.retain(|k| k != &source);
    }
    save_term_base(manager.term_base())
}

// Tauri命令：删除术语的某个译法（删光后整条移除）
#[tauri::command]
pub async fn delete_term_translation(
    state: tauri::State<'_, AppState>,
    source: String,
    translation: String,
) -> Result<(), String> {
    let mut manager = state.manager.lock().await;
    manager
        .term_base_mut()
        .remove_translation(&source, &translation);
    save_term_base(manager.term_base())
}

// ===== 术语包 =====

// Tauri命令：列出全部术语包
#[tauri::command]
pub async fn list_packages(state: tauri::State<'_, AppState>) -> Result<Vec<TermPackage>, String> {
    let manager = state.manager.lock().await;
    let mut list: Vec<TermPackage> = manager.term_base().packages.values().cloned().collect();
    list.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(list)
}

// Tauri命令：新建/更新术语包（id 为空=新建；内置包仅允许改名）
#[tauri::command]
pub async fn save_package(
    state: tauri::State<'_, AppState>,
    id: Option<String>,
    name: String,
    mappings: Vec<(String, String)>,
) -> Result<String, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("包名不能为空".to_string());
    }
    let mappings: Vec<(String, String)> = mappings
        .into_iter()
        .map(|(f, t)| (f.trim().to_string(), t.trim().to_string()))
        .filter(|(f, t)| !f.is_empty() && !t.is_empty())
        .collect();
    if mappings.is_empty() {
        return Err("至少需要一条有效映射".to_string());
    }
    let mut manager = state.manager.lock().await;
    let tb = manager.term_base_mut();
    tb.ensure_defaults();
    let pid = match id {
        Some(existing) if tb.packages.contains_key(&existing) => {
            let pkg = tb.packages.get_mut(&existing).unwrap();
            pkg.name = name;
            if !pkg.builtin {
                pkg.mappings = mappings;
            }
            existing
        }
        _ => {
            let new_id = format!(
                "pkg_{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis().to_string())
                    .unwrap_or_else(|_| "0".to_string())
            );
            tb.packages.insert(
                new_id.clone(),
                TermPackage {
                    id: new_id.clone(),
                    name,
                    mappings,
                    builtin: false,
                },
            );
            new_id
        }
    };
    save_term_base(manager.term_base())?;
    Ok(pid)
}

// Tauri命令：删除术语包（内置包不可删除）
#[tauri::command]
pub async fn delete_package(state: tauri::State<'_, AppState>, id: String) -> Result<(), String> {
    let mut manager = state.manager.lock().await;
    let tb = manager.term_base_mut();
    if tb.packages.get(&id).map(|p| p.builtin).unwrap_or(false) {
        return Err("内置包不可删除".to_string());
    }
    if tb.packages.remove(&id).is_none() {
        return Err("包不存在".to_string());
    }
    for s in &mut tb.schemes {
        s.enabled_packages.retain(|p| p != &id);
    }
    save_term_base(manager.term_base())
}

// Tauri命令：导入术语包（前端读文件文本传入；CSV/TSV/TXT 两列：原文[逗号/Tab/→]译文）
#[tauri::command]
pub async fn import_package_text(
    state: tauri::State<'_, AppState>,
    name: String,
    content: String,
) -> Result<String, String> {
    let mut mappings: Vec<(String, String)> = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let sep = ["\u{2192}", "\t", ",", "\u{FF0C}"]
            .iter()
            .find_map(|s| line.find(s).map(|i| (i, s.len())));
        if let Some((i, l)) = sep {
            let from = line[..i].trim().to_string();
            let to = line[i + l..].trim().to_string();
            if !from.is_empty() && !to.is_empty() {
                mappings.push((from, to));
            }
        }
    }
    if mappings.is_empty() {
        return Err("未解析到有效映射（需两列：原文[逗号/Tab/→]译文）".to_string());
    }
    let count = mappings.len();
    let mut manager = state.manager.lock().await;
    let tb = manager.term_base_mut();
    tb.ensure_defaults();
    let new_id = format!(
        "pkg_import_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis().to_string())
            .unwrap_or_else(|_| "0".to_string())
    );
    tb.packages.insert(
        new_id.clone(),
        TermPackage {
            id: new_id.clone(),
            name: format!("导入\u{00b7}{}", name.trim()),
            mappings,
            builtin: false,
        },
    );
    save_term_base(manager.term_base())?;
    Ok(format!("{new_id}:{count}"))
}

// ===== 使用方案 =====

// Tauri命令：列出全部方案
#[tauri::command]
pub async fn list_schemes(state: tauri::State<'_, AppState>) -> Result<Vec<TermScheme>, String> {
    let mut manager = state.manager.lock().await;
    manager.term_base_mut().ensure_defaults();
    Ok(manager.term_base().schemes.clone())
}

// Tauri命令：当前激活方案ID
#[tauri::command]
pub async fn get_active_scheme(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let manager = state.manager.lock().await;
    Ok(manager.term_base().active_scheme.clone())
}

// Tauri命令：新增方案
#[tauri::command]
pub async fn add_scheme(state: tauri::State<'_, AppState>, name: String) -> Result<String, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("方案名不能为空".to_string());
    }
    let mut manager = state.manager.lock().await;
    let tb = manager.term_base_mut();
    tb.ensure_defaults();
    let new_id = format!(
        "scheme_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis().to_string())
            .unwrap_or_else(|_| "0".to_string())
    );
    tb.schemes.push(TermScheme {
        id: new_id.clone(),
        name,
        enabled_packages: vec!["builtin_symbols".to_string()],
        enabled_entry_keys: vec![],
    });
    save_term_base(manager.term_base())?;
    Ok(new_id)
}

// Tauri命令：重命名方案
#[tauri::command]
pub async fn rename_scheme(
    state: tauri::State<'_, AppState>,
    id: String,
    name: String,
) -> Result<(), String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("方案名不能为空".to_string());
    }
    let mut manager = state.manager.lock().await;
    let tb = manager.term_base_mut();
    match tb.schemes.iter_mut().find(|s| s.id == id) {
        Some(s) => s.name = name,
        None => return Err("方案不存在".to_string()),
    }
    save_term_base(manager.term_base())
}

// Tauri命令：删除方案（至少保留一个）
#[tauri::command]
pub async fn delete_scheme(state: tauri::State<'_, AppState>, id: String) -> Result<(), String> {
    let mut manager = state.manager.lock().await;
    let tb = manager.term_base_mut();
    if tb.schemes.len() <= 1 {
        return Err("至少保留一个方案".to_string());
    }
    tb.schemes.retain(|s| s.id != id);
    if tb.active_scheme == id {
        tb.active_scheme = tb.schemes[0].id.clone();
    }
    save_term_base(manager.term_base())
}

// Tauri命令：激活方案
#[tauri::command]
pub async fn activate_scheme(state: tauri::State<'_, AppState>, id: String) -> Result<(), String> {
    let mut manager = state.manager.lock().await;
    let tb = manager.term_base_mut();
    tb.ensure_defaults();
    if !tb.schemes.iter().any(|s| s.id == id) {
        return Err("方案不存在".to_string());
    }
    tb.active_scheme = id;
    save_term_base(manager.term_base())
}

// Tauri命令：设置方案启用的术语包
#[tauri::command]
pub async fn set_scheme_packages(
    state: tauri::State<'_, AppState>,
    id: String,
    package_ids: Vec<String>,
) -> Result<(), String> {
    let mut manager = state.manager.lock().await;
    let tb = manager.term_base_mut();
    match tb.schemes.iter_mut().find(|s| s.id == id) {
        Some(s) => s.enabled_packages = package_ids,
        None => return Err("方案不存在".to_string()),
    }
    save_term_base(manager.term_base())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_term_base_roundtrip() {
        let mut tb = TermBase::default();
        tb.set_priority("server", "服务器", 1);
        tb.set_priority("server", "服务", 2);
        tb.add_translation("bug", "缺陷");
        tb.set_priority("bug", "漏洞", 5);

        let json = serde_json::to_string_pretty(&tb).unwrap();
        let loaded: TermBase = serde_json::from_str(&json).unwrap();

        assert_eq!(loaded.entries["server"].translations, vec!["服务器", "服务"]);
        assert_eq!(loaded.entries["server"].priority, vec![1, 2]);
        assert_eq!(loaded.entries["bug"].best_translation(), Some("漏洞"));
        assert_eq!(loaded.entries["bug"].translations, vec!["缺陷", "漏洞"]);
    }

    #[test]
    fn test_remove_translation_cleans_entry() {
        let mut tb = TermBase::default();
        tb.add_translation("mini", "小");
        assert!(tb.remove_translation("mini", "小"));
        assert!(!tb.entries.contains_key("mini"));
    }
}
