//! 术语库：terms.json 持久化 + 增删改命令
//! 存储位置：与 settings.json 同目录的 terms.json

use crate::engines::{TermBase, TermEntry};
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
) -> Result<(), String> {
    let source = source.trim().to_string();
    let translation = translation.trim().to_string();
    if source.is_empty() || translation.is_empty() {
        return Err("源词和译法不能为空".to_string());
    }
    let mut manager = state.manager.lock().await;
    manager
        .term_base_mut()
        .add_translation(&source, &translation);
    save_term_base(manager.term_base())
}

// Tauri命令：删除整条术语
#[tauri::command]
pub async fn delete_term(
    state: tauri::State<'_, AppState>,
    source: String,
) -> Result<(), String> {
    let mut manager = state.manager.lock().await;
    manager.term_base_mut().remove_entry(&source);
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
