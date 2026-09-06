import React, { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./TranslatePanel.css";

interface HistoryEntry {
  id: u64;
  time: string;
  from: string;
  to: string;
  source: string;
  translation: string;
  engine: string;
}
type u64 = number;

const HistoryPanel: React.FC = () => {
  const [list, setList] = useState<HistoryEntry[]>([]);
  const [search, setSearch] = useState("");
  const [expanded, setExpanded] = useState<number | null>(null);

  const refresh = useCallback(async () => {
    try {
      setList(await invoke<HistoryEntry[]>("list_history"));
    } catch (e) {
      console.error("加载历史失败:", e);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const handleDelete = async (id: number) => {
    try {
      await invoke("delete_history_entry", { id });
      await refresh();
    } catch (e) {
      console.error("删除历史失败:", e);
    }
  };

  const handleClear = async () => {
    if (!window.confirm("确定清空全部翻译历史？此操作不可恢复。")) return;
    try {
      await invoke("clear_history");
      await refresh();
    } catch (e) {
      console.error("清空历史失败:", e);
    }
  };

  const handleCopy = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      const ta = document.createElement("textarea");
      ta.value = text;
      document.body.appendChild(ta);
      ta.select();
      document.execCommand("copy");
      document.body.removeChild(ta);
    }
  };

  const keyword = search.trim().toLowerCase();
  const filtered = list.filter(
    (h) =>
      !keyword ||
      h.source.toLowerCase().includes(keyword) ||
      h.translation.toLowerCase().includes(keyword)
  );

  return (
    <div className="settings-section">
      <h3 className="section-title">翻译历史</h3>

      <div className="history-toolbar">
        <input
          className="form-input"
          placeholder="搜索原文/译文…"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
        <button className="button secondary" onClick={handleClear}>
          清空全部
        </button>
      </div>

      {filtered.length === 0 ? (
        <p className="history-empty">
          {list.length === 0 ? "暂无翻译记录" : "无匹配记录"}
        </p>
      ) : (
        <div className="history-list">
          {filtered.map((h) => (
            <div className="history-card" key={h.id}>
              <div className="history-head">
                <span className="history-time">{h.time}</span>
                <span className="history-engine">{h.engine}</span>
                <span className="history-lang">
                  {h.from} → {h.to}
                </span>
                <button
                  className="mini-btn"
                  onClick={() => handleCopy(h.translation)}
                  title="复制译文"
                >
                  复制
                </button>
                <button
                  className="mini-btn"
                  onClick={() => handleDelete(h.id)}
                  title="删除此条"
                >
                  删除
                </button>
              </div>
              <div
                className="history-body"
                onClick={() => setExpanded(expanded === h.id ? null : h.id)}
                title="点击展开/收起"
              >
                <div className="history-source">
                  {expanded === h.id
                    ? h.source
                    : truncate(h.source, 80)}
                </div>
                <div className="history-translation">
                  {expanded === h.id
                    ? h.translation
                    : truncate(h.translation, 80)}
                </div>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
};

function truncate(s: string, max: number): string {
  const t = s.replace(/\n/g, " ");
  return t.length > max ? t.slice(0, max) + "…" : t;
}

export default HistoryPanel;
