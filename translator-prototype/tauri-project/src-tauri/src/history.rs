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

/// 插入到队首（新的在前）并按上限裁剪（纯函数，便于测试）
fn insert_and_truncate(mut list: Vec<HistoryEntry>, entry: HistoryEntry) -> Vec<HistoryEntry> {
    list.insert(0, entry);
    list.truncate(MAX_ENTRIES);
    list
}

/// 追加一条历史（新的在前，超上限裁剪）；记录失败静默（不影响翻译主流程）
pub fn record(from: &str, to: &str, source: &str, translation: &str, engine: &str) {
    let _guard = HISTORY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let list = load();
    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let list = insert_and_truncate(
        list,
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

    fn entry(id: u64) -> HistoryEntry {
        HistoryEntry {
            id,
            time: "2026-01-01 00:00:00".to_string(),
            from: "en".to_string(),
            to: "zh".to_string(),
            source: format!("src{id}"),
            translation: format!("译{id}"),
            engine: "测试引擎".to_string(),
        }
    }

    #[test]
    fn test_insert_newest_first() {
        let list = insert_and_truncate(vec![entry(1)], entry(2));
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, 2, "新记录必须排在最前");
        assert_eq!(list[1].id, 1);
    }

    #[test]
    fn test_history_capped_at_500() {
        // 回归：超过上限时裁剪最旧的，而不是无限增长
        let list: Vec<HistoryEntry> = (0..505).rev().map(entry).collect();
        let list = insert_and_truncate(list, entry(999));
        assert_eq!(list.len(), MAX_ENTRIES);
        assert_eq!(list[0].id, 999, "新记录在最前");
        assert_eq!(list[1].id, 504, "最旧的被裁剪掉");
    }

    #[test]
    fn test_history_entry_roundtrip() {
        let e = entry(7);
        let json = serde_json::to_string(&e).unwrap();
        let back: HistoryEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, 7);
        assert_eq!(back.source, "src7");
    }
}
