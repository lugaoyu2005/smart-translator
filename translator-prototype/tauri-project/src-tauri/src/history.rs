//! 翻译历史：自动记录每次翻译，本地 history.json 持久化（上限500条，新的在前）

use serde::{Deserialize, Serialize};
use std::sync::Mutex;

/// 单条历史：id 供删除定位（纳秒时间戳，唯一）
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HistoryEntry {
    pub id: u64,
    pub time: String,
    pub from: String,
    pub to: String,
    pub source: String,
    pub translation: String,
    pub engine: String,
}

const MAX_ENTRIES: usize = 500;
static HISTORY_LOCK: Mutex<()> = Mutex::new(());

fn history_path() -> std::path::PathBuf {
    let mut path = std::env::current_exe().unwrap_or_default();
    path.pop();
    path.push("history.json");
    path
}

fn load() -> Vec<HistoryEntry> {
    let path = history_path();
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(list) = serde_json::from_str::<Vec<HistoryEntry>>(&content) {
                return list;
            }
        }
    }
    Vec::new()
}

fn save(list: &[HistoryEntry]) -> Result<(), String> {
    let json =
        serde_json::to_string_pretty(list).map_err(|e| format!("序列化历史失败: {e}"))?;
    std::fs::write(history_path(), json).map_err(|e| format!("写入历史失败: {e}"))
}

/// 本地时间字符串（YYYY-MM-DD HH:MM:SS）
fn local_time_string() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// 追加一条历史（新的在前，超上限裁剪）；记录失败静默（不影响翻译主流程）
pub fn record(from: &str, to: &str, source: &str, translation: &str, engine: &str) {
    let _guard = HISTORY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut list = load();
    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    list.insert(
        0,
        HistoryEntry {
            id,
            time: local_time_string(),
            from: from.to_string(),
            to: to.to_string(),
            source: source.to_string(),
            translation: translation.to_string(),
            engine: engine.to_string(),
        },
    );
    list.truncate(MAX_ENTRIES);
    let _ = save(&list);
}

// Tauri命令：列出全部历史（新的在前）
#[tauri::command]
pub fn list_history() -> Result<Vec<HistoryEntry>, String> {
    let _guard = HISTORY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    Ok(load())
}

// Tauri命令：删除单条历史
#[tauri::command]
pub fn delete_history_entry(id: u64) -> Result<(), String> {
    let _guard = HISTORY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut list = load();
    list.retain(|e| e.id != id);
    save(&list)
}

// Tauri命令：清空历史
#[tauri::command]
pub fn clear_history() -> Result<(), String> {
    let _guard = HISTORY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    save(&[])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_time_format() {
        let t = local_time_string();
        // "YYYY-MM-DD HH:MM:SS" 长度19
        assert_eq!(t.len(), 19);
        assert_eq!(t.as_bytes()[4], b'-');
        assert_eq!(t.as_bytes()[10], b' ');
    }
}
