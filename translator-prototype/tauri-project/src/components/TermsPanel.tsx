import React, { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./TermsPanel.css";

interface TermEntry {
  source: string;
  translations: string[];
  priority: number[];
  usage_count: number;
}

interface TermPackage {
  id: string;
  name: string;
  mappings: [string, string][];
  builtin: boolean;
}

interface TermScheme {
  id: string;
  name: string;
  enabled_packages: string[];
  enabled_entry_keys: string[];
}

const TermsPanel: React.FC = () => {
  const [terms, setTerms] = useState<TermEntry[]>([]);
  const [packages, setPackages] = useState<TermPackage[]>([]);
  const [schemes, setSchemes] = useState<TermScheme[]>([]);
  const [activeScheme, setActiveScheme] = useState<string>("");
  const [search, setSearch] = useState("");
  const [newSource, setNewSource] = useState("");
  const [newTranslation, setNewTranslation] = useState("");
  const [message, setMessage] = useState("");
  const [editingPkg, setEditingPkg] = useState<string | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const refresh = async () => {
    try {
      const [t, p, s, a] = await Promise.all([
        invoke<TermEntry[]>("list_terms"),
        invoke<TermPackage[]>("list_packages"),
        invoke<TermScheme[]>("list_schemes"),
        invoke<string>("get_active_scheme"),
      ]);
      setTerms(t);
      setPackages(p);
      setSchemes(s);
      setActiveScheme(a);
    } catch (e) {
      console.error("加载术语数据失败:", e);
    }
  };

  useEffect(() => {
    refresh();
  }, []);

  const active = schemes.find((s) => s.id === activeScheme);

  // 当前方案可见的术语（方案未指定独占术语 = 全部共享）
  const visibleTerms = terms.filter((t) => {
    if (!active) return true;
    if (active.enabled_entry_keys.length === 0) return true;
    return active.enabled_entry_keys.includes(t.source);
  });

  const filtered = visibleTerms.filter((t) =>
    t.source.toLowerCase().includes(search.toLowerCase())
  );

  const handleAdd = async () => {
    if (!newSource.trim() || !newTranslation.trim()) {
      setMessage("源词和译法不能为空");
      return;
    }
    try {
      await invoke("add_term", {
        source: newSource.trim(),
        translation: newTranslation.trim(),
        schemeId: activeScheme || null,
      });
      setNewSource("");
      setNewTranslation("");
      setMessage("");
      refresh();
    } catch (e) {
      setMessage(String(e));
    }
  };

  const handlePriority = async (
    source: string,
    translation: string,
    delta: number
  ) => {
    const entry = terms.find((t) => t.source === source);
    if (!entry) return;
    const idx = entry.translations.indexOf(translation);
    if (idx < 0) return;
    const current = entry.priority[idx] ?? 0;
    try {
      await invoke("set_term_priority", {
        source,
        translation,
        priority: Math.max(0, current + delta),
      });
      refresh();
    } catch (e) {
      setMessage(String(e));
    }
  };

  const handleDeleteTranslation = async (source: string, translation: string) => {
    await invoke("delete_term_translation", { source, translation }).catch(() => {});
    refresh();
  };

  const handleDeleteTerm = async (source: string) => {
    await invoke("delete_term", { source }).catch(() => {});
    refresh();
  };

  // ===== 方案操作 =====
  const activate = async (id: string) => {
    await invoke("activate_scheme", { id }).catch(() => {});
    refresh();
  };
  const addScheme = async () => {
    const name = prompt("新方案名称：", `方案${schemes.length + 1}`);
    if (!name) return;
    await invoke("add_scheme", { name }).catch(() => {});
    refresh();
  };
  const renameScheme = async (id: string, current: string) => {
    const name = prompt("重命名方案：", current);
    if (!name) return;
    await invoke("rename_scheme", { id, name }).catch(() => {});
    refresh();
  };
  const deleteScheme = async (id: string) => {
    if (!confirm("确定删除该方案？")) return;
    await invoke("delete_scheme", { id }).catch(() => {});
    refresh();
  };

  // ===== 术语包操作 =====
  const togglePackage = async (pkgId: string, on: boolean) => {
    if (!active) return;
    const next = on
      ? [...new Set([...active.enabled_packages, pkgId])]
      : active.enabled_packages.filter((p) => p !== pkgId);
    setSchemes((prev) =>
      prev.map((s) => (s.id === active.id ? { ...s, enabled_packages: next } : s))
    );
    await invoke("set_scheme_packages", { id: active.id, packageIds: next }).catch(
      () => {}
    );
    refresh();
  };
  const deletePackage = async (id: string) => {
    if (!confirm("确定删除该术语包？")) return;
    await invoke("delete_package", { id }).catch(() => {});
    refresh();
  };
  const newPackage = async () => {
    const name = prompt("新术语包名称：", "我的术语包");
    if (!name) return;
    await invoke("save_package", { id: null, name, mappings: [["示例", "示例译文"]] })
      .catch(() => {});
    refresh();
  };

  // ===== 导入术语包文件（CSV/TSV/TXT 两列）=====
  const handleImportFile = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    const content = await file.text();
    try {
      const res = await invoke<string>("import_package_text", {
        name: file.name.replace(/\.[^.]+$/, ""),
        content,
      });
      const count = res.split(":")[1] || "?";
      setMessage(`导入成功：${count} 条映射已存为新术语包`);
      refresh();
    } catch (err) {
      setMessage(String(err));
    }
    if (fileInputRef.current) fileInputRef.current.value = "";
  };

  return (
    <div className="settings-section terms-panel">
      <h3 className="section-title">术语管理</h3>

      {/* 使用方案 tabs */}
      <div className="scheme-tabs">
        {schemes.map((s) => (
          <div
            key={s.id}
            className={`scheme-tab ${s.id === activeScheme ? "active" : ""}`}
            onClick={() => activate(s.id)}
            title="点击切换使用方案"
          >
            <span className="scheme-name">{s.name}</span>
            <button
              className="scheme-mini-btn"
              title="重命名"
              onClick={(e) => {
                e.stopPropagation();
                renameScheme(s.id, s.name);
              }}
            >
              ✎
            </button>
            {schemes.length > 1 && (
              <button
                className="scheme-mini-btn"
                title="删除"
                onClick={(e) => {
                  e.stopPropagation();
                  deleteScheme(s.id);
                }}
              >
                ✕
              </button>
            )}
          </div>
        ))}
        <button className="scheme-add" onClick={addScheme} title="新增方案">
          +
        </button>
      </div>
      <p className="form-hint">
        不同方案可启用不同的术语包；翻译时仅使用当前激活方案的内容
      </p>

      {/* 术语包 */}
      <div className="form-group">
        <label className="form-label with-link">
          术语包（当前方案启用项）
          <button className="provider-link" onClick={newPackage} title="新建术语包">
            +
          </button>
          <button
            className="link-text-btn"
            onClick={() => fileInputRef.current?.click()}
            title="导入术语包文件（CSV/TSV/TXT/JSON，两列：原文[逗号/Tab/→]译文）"
          >
            导入
          </button>
          <input
            ref={fileInputRef}
            type="file"
            accept=".csv,.tsv,.txt,.json"
            style={{ display: "none" }}
            onChange={handleImportFile}
          />
        </label>
        {packages.map((p) => {
          const enabled = active?.enabled_packages.includes(p.id) ?? false;
          return (
            <div className="package-row" key={p.id}>
              <label className="toggle-switch">
                <input
                  type="checkbox"
                  checked={enabled}
                  onChange={(e) => togglePackage(p.id, e.target.checked)}
                />
                <span className="toggle-slider"></span>
              </label>
              <span className="package-name">
                {p.name}
                {p.builtin && <span className="provider-badge">内置 · 部首偏旁提前干预</span>}
                <span className="package-count">{p.mappings.length} 条</span>
              </span>
              {!p.builtin && (
                <>
                  <button
                    className="scheme-mini-btn"
                    onClick={() =>
                      setEditingPkg(editingPkg === p.id ? null : p.id)
                    }
                    title="查看/编辑映射"
                  >
                    {editingPkg === p.id ? "收起" : "编辑"}
                  </button>
                  <button
                    className="package-delete"
                    onClick={() => deletePackage(p.id)}
                    title="删除术语包"
                  >
                    ✕
                  </button>
                </>
              )}
            </div>
          );
        })}
        {/* 包映射编辑（展开） */}
        {packages
          .filter((p) => editingPkg === p.id && !p.builtin)
          .map((p) => (
            <PackageEditor
              key={`edit-${p.id}`}
              pkg={p}
              onSaved={(msg) => {
                setMessage(msg);
                refresh();
              }}
            />
          ))}
      </div>

      {/* 术语增删 */}
      <div className="term-add-form">
        <input
          type="text"
          placeholder="源词（原文）"
          value={newSource}
          onChange={(e) => setNewSource(e.target.value)}
        />
        <input
          type="text"
          placeholder="译法"
          value={newTranslation}
          onChange={(e) => setNewTranslation(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleAdd()}
        />
        <button className="button primary" onClick={handleAdd}>
          添加
        </button>
      </div>

      <input
        type="text"
        className="term-search"
        placeholder="搜索源词..."
        value={search}
        onChange={(e) => setSearch(e.target.value)}
      />

      {message && <div className="term-message">{message}</div>}

      <div className="term-list">
        {filtered.map((entry) => (
          <div className="term-card" key={entry.source}>
            <div className="term-header">
              <span className="term-source">{entry.source}</span>
              <span className="term-usage">已使用 {entry.usage_count} 次</span>
              <button
                className="term-delete"
                onClick={() => handleDeleteTerm(entry.source)}
              >
                删除
              </button>
            </div>
            {entry.translations.map((translation, idx) => (
              <div className="term-translation-row" key={translation}>
                <span className="term-translation">{translation}</span>
                <span className="term-priority">优先级 {entry.priority[idx] ?? 0}</span>
                <button
                  className="term-btn"
                  onClick={() => handlePriority(entry.source, translation, 1)}
                >
                  ▲
                </button>
                <button
                  className="term-btn"
                  onClick={() => handlePriority(entry.source, translation, -1)}
                >
                  ▼
                </button>
                <button
                  className="term-btn term-delete-btn"
                  onClick={() => handleDeleteTranslation(entry.source, translation)}
                >
                  ✕
                </button>
              </div>
            ))}
          </div>
        ))}
        {filtered.length === 0 && (
          <div className="term-empty">暂无术语（添加后将仅归属当前激活方案）</div>
        )}
      </div>
    </div>
  );
};

// 包映射编辑器：查看/删除/新增映射，保存写回
const PackageEditor: React.FC<{
  pkg: TermPackage;
  onSaved: (msg: string) => void;
}> = ({ pkg, onSaved }) => {
  const [rows, setRows] = useState<[string, string][]>(pkg.mappings);
  const [nf, setNf] = useState("");
  const [nt, setNt] = useState("");
  return (
    <div className="package-editor">
      {rows.map(([f, t], i) => (
        <div className="package-editor-row" key={i}>
          <span className="pkg-from">{f}</span>
          <span>→</span>
          <span className="pkg-to">{t}</span>
          <button
            className="scheme-mini-btn"
            onClick={() => setRows(rows.filter((_, k) => k !== i))}
            title="删除该映射"
          >
            ✕
          </button>
        </div>
      ))}
      <div className="package-editor-row">
        <input
          className="form-input"
          placeholder="原文"
          value={nf}
          onChange={(e) => setNf(e.target.value)}
        />
        <input
          className="form-input"
          placeholder="译文"
          value={nt}
          onChange={(e) => setNt(e.target.value)}
        />
        <button
          className="scheme-mini-btn"
          onClick={() => {
            if (nf.trim() && nt.trim()) {
              setRows([...rows, [nf.trim(), nt.trim()]]);
              setNf("");
              setNt("");
            }
          }}
        >
          + 添加
        </button>
      </div>
      <button
        className="button primary"
        onClick={async () => {
          try {
            await invoke("save_package", { id: pkg.id, name: pkg.name, mappings: rows });
            onSaved("术语包已保存");
          } catch (e) {
            onSaved(String(e));
          }
        }}
      >
        保存修改
      </button>
    </div>
  );
};

export default TermsPanel;
