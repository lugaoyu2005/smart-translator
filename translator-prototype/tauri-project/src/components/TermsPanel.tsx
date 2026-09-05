import React, { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./TermsPanel.css";

interface TermEntry {
  source: string;
  translations: string[];
  priority: number[];
  usage_count: number;
}

const TermsPanel: React.FC = () => {
  const [terms, setTerms] = useState<TermEntry[]>([]);
  const [search, setSearch] = useState("");
  const [newSource, setNewSource] = useState("");
  const [newTranslation, setNewTranslation] = useState("");
  const [message, setMessage] = useState("");

  const refresh = useCallback(async () => {
    try {
      const list = await invoke<TermEntry[]>("list_terms");
      setTerms(list);
    } catch (e) {
      setMessage(`加载术语失败: ${e}`);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const handleAdd = async () => {
    if (!newSource.trim() || !newTranslation.trim()) {
      setMessage("源词和译法不能为空");
      return;
    }
    try {
      await invoke("add_term", {
        source: newSource.trim(),
        translation: newTranslation.trim(),
      });
      setNewSource("");
      setNewTranslation("");
      setMessage("");
      await refresh();
    } catch (e) {
      setMessage(`添加失败: ${e}`);
    }
  };

  // 调整某译法优先级（数字越大越优先，0为下限）
  const handlePriority = async (term: TermEntry, translation: string, delta: number) => {
    const idx = term.translations.indexOf(translation);
    if (idx < 0) return;
    const current = term.priority[idx] ?? 0;
    const next = Math.max(0, current + delta);
    if (next === current) return;
    try {
      await invoke("set_term_priority", {
        source: term.source,
        translation,
        priority: next,
      });
      await refresh();
    } catch (e) {
      setMessage(`调整优先级失败: ${e}`);
    }
  };

  const handleDeleteTranslation = async (term: TermEntry, translation: string) => {
    try {
      await invoke("delete_term_translation", { source: term.source, translation });
      await refresh();
    } catch (e) {
      setMessage(`删除译法失败: ${e}`);
    }
  };

  const handleDeleteTerm = async (term: TermEntry) => {
    try {
      await invoke("delete_term", { source: term.source });
      await refresh();
    } catch (e) {
      setMessage(`删除术语失败: ${e}`);
    }
  };

  const keyword = search.trim().toLowerCase();
  const filtered = terms.filter(
    (t) =>
      t.source.toLowerCase().includes(keyword) ||
      t.translations.some((tr) => tr.toLowerCase().includes(keyword))
  );

  return (
    <div className="settings-section">
      <h3 className="section-title">术语管理</h3>
      <p className="terms-desc">
        翻译时自动将命中的源词替换为优先级最高的译法；译法每被使用50次自动提升优先级。
        源词已存在时，添加即为该词追加译法。
      </p>

      <div className="term-add-form">
        <input
          className="form-input"
          placeholder="源词（如 server）"
          value={newSource}
          onChange={(e) => setNewSource(e.target.value)}
        />
        <input
          className="form-input"
          placeholder="译法（如 服务器）"
          value={newTranslation}
          onChange={(e) => setNewTranslation(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleAdd()}
        />
        <button className="button primary" onClick={handleAdd}>
          添加
        </button>
      </div>

      <input
        className="form-input term-search"
        placeholder="搜索术语…"
        value={search}
        onChange={(e) => setSearch(e.target.value)}
      />

      {message && <p className="term-message">{message}</p>}

      <div className="term-list">
        {filtered.length === 0 ? (
          <p className="term-empty">
            {terms.length === 0 ? "暂无术语，添加后翻译时自动生效" : "无匹配术语"}
          </p>
        ) : (
          filtered.map((t) => (
            <div className="term-card" key={t.source}>
              <div className="term-head">
                <span className="term-source">{t.source}</span>
                <span className="term-usage">使用 {t.usage_count} 次</span>
                <button className="button danger small" onClick={() => handleDeleteTerm(t)}>
                  删除
                </button>
              </div>
              <div className="term-translations">
                {t.translations.map((tr, i) => (
                  <div className="term-row" key={tr}>
                    <span className="term-translation">{tr}</span>
                    <span className="term-priority">优先级 {t.priority[i] ?? 0}</span>
                    <button
                      className="icon-btn"
                      title="提高优先级"
                      onClick={() => handlePriority(t, tr, 1)}
                    >
                      ▲
                    </button>
                    <button
                      className="icon-btn"
                      title="降低优先级"
                      onClick={() => handlePriority(t, tr, -1)}
                    >
                      ▼
                    </button>
                    <button
                      className="icon-btn danger"
                      title="删除译法"
                      onClick={() => handleDeleteTranslation(t, tr)}
                    >
                      ✕
                    </button>
                  </div>
                ))}
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
};

export default TermsPanel;
