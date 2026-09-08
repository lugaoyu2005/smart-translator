import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./TranslatePanel.css";

interface Language {
  code: string;
  name: string;
  native_name: string;
}

interface EngineInfo {
  name: string;
  engine_type: string;
  available: boolean;
  configured: boolean;
}

interface NetworkStatus {
  is_online: boolean;
  connection_type: string;
  latency: number | null;
}

interface TranslationResult {
  translated_text: string;
  engine_used: string;
  from: string;
  to: string;
}

interface TranslatePanelProps {
  // 划词捕获的待翻译文本（App监听事件后传入，自动翻译一次）
  pendingText?: { text: string; seq: number } | null;
  onConsumed?: () => void;
}

const TranslatePanel: React.FC<TranslatePanelProps> = ({ pendingText, onConsumed }) => {
  const [text, setText] = useState("");
  const [result, setResult] = useState("");
  const [engineUsed, setEngineUsed] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [languages, setLanguages] = useState<Language[]>([]);
  const [fromLang, setFromLang] = useState("auto");
  const [toLang, setToLang] = useState("zh");
  const [engines, setEngines] = useState<EngineInfo[]>([]);
  // 手动指定引擎（翻译测试用）：null=默认优先级（描边框在默认引擎上）
  const [selectedEngine, setSelectedEngine] = useState<string | null>(null);
  const [defaultEngine, setDefaultEngine] = useState("");
  // auto 状态下点击 ⇄ 的可见提示
  const [swapHint, setSwapHint] = useState(false);
  const [network, setNetwork] = useState<NetworkStatus | null>(null);

  useEffect(() => {
    loadLanguages();
    loadEngines();
    loadNetworkStatus();
  }, []);

  // 划词捕获：文本到达后填入并自动翻译一次
  useEffect(() => {
    if (pendingText && pendingText.text.trim()) {
      setText(pendingText.text);
      void handleTranslate(pendingText.text);
      onConsumed?.();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pendingText?.seq]);

  const loadLanguages = async () => {
    try {
      const langs = await invoke<Language[]>("get_supported_languages");
      setLanguages(langs);
    } catch (e) {
      console.error("加载语言失败:", e);
    }
  };

  const loadEngines = async () => {
    try {
      invoke<any>("get_app_settings")
        .then((s) => setDefaultEngine(String(s?.current_engine || "")))
        .catch(() => {});
      const engineList = await invoke<EngineInfo[]>("list_engines");
      setEngines(engineList);
    } catch (e) {
      console.error("加载引擎失败:", e);
    }
  };

  const loadNetworkStatus = async () => {
    try {
      const status = await invoke<NetworkStatus>("get_network_status");
      setNetwork(status);
    } catch (e) {
      console.error("网络检测失败:", e);
    }
  };

  const handleTranslate = async (override?: string) => {
    const source = override ?? text;
    if (!source.trim()) {
      setError("请输入要翻译的内容");
      return;
    }

    setLoading(true);
    setError("");
    try {
      const res = await invoke<TranslationResult>("translate_text", {
        text: source,
        from: fromLang,
        to: toLang,
        engine: selectedEngine,
      });
      setResult(res.translated_text);
      setEngineUsed(res.engine_used);
    } catch (e: any) {
      setError(typeof e === "string" ? e : "翻译失败，请检查网络或API配置");
    } finally {
      setLoading(false);
    }
  };

  const handleSwap = () => {
    if (fromLang === "auto") {
      // 可见提示：原文为自动检测时无法交换
      setSwapHint(true);
      window.setTimeout(() => setSwapHint(false), 2200);
      return;
    }
    const tmp = fromLang;
    setFromLang(toLang);
    setToLang(tmp);
  };

  // 交换语言后已有文本时自动重新翻译
  useEffect(() => {
    if (text.trim()) handleTranslate();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [fromLang, toLang]);

  const handleTextExample = () => {
    // 测试文本预处理：MiniMap 和 Mini_Map
    setText("MiniMap and Mini_Map test");
  };

  return (
    <div className="translate-panel">
      <div className="translate-header">
        <h3>翻译测试</h3>
        <div className="status-row">
          <span className={`net-dot ${network?.is_online ? "online" : "offline"}`}></span>
          <span className="net-text">
            {network
              ? network.is_online
                ? `在线（延迟 ${network.latency}ms）`
                : "离线"
              : "检测中..."}
          </span>
          <span className="engine-badges">
            {engines.map((e) => (
              <span
                key={e.name}
                className={`engine-badge ready ${
                  selectedEngine === e.name
                    ? "selected"
                    : selectedEngine === null && e.name === defaultEngine
                    ? "default-engine"
                    : ""
                }`}
                title={`点击${selectedEngine === e.name ? "取消指定" : "指定用此引擎"}测试`}
                onClick={() =>
                  setSelectedEngine((prev) => (prev === e.name ? null : e.name))
                }
                style={{ cursor: "pointer" }}
              >
                {e.name}
              </span>
            ))}
            {engines.length === 0 && (
              <span className="engine-badge unconfigured">无可用引擎</span>
            )}
            {engines.length > 0 && (
              <span className="engine-badge default-hint">
                {selectedEngine ? "指定引擎测试中" : "默认：按优先级自动选择"}
              </span>
            )}
          </span>
        </div>
      </div>

      <div className="lang-selector">
        <select value={fromLang} onChange={(e) => setFromLang(e.target.value)}>
          {languages.map((l) => (
            <option key={l.code} value={l.code}>
              {l.native_name}
            </option>
          ))}
        </select>
        <button
          className="swap-btn"
          onClick={handleSwap}
          disabled={fromLang === "auto"}
          title={
            fromLang === "auto"
              ? "原文为自动检测时无法交换，请先手动选择源语言"
              : "交换语言（自动重新翻译）"
          }
          style={fromLang === "auto" ? { opacity: 0.4, cursor: "not-allowed" } : undefined}
        >
          ⇄
        </button>
        {swapHint && (
          <div className="swap-hint">
            原文为自动检测时无法交换，请先手动选择源语言
          </div>
        )}
        <select value={toLang} onChange={(e) => setToLang(e.target.value)}>
          {languages
            .filter((l) => l.code !== "auto")
            .map((l) => (
              <option key={l.code} value={l.code}>
                {l.native_name}
              </option>
            ))}
        </select>
      </div>

      <textarea
        className="input-area"
        placeholder="输入要翻译的文本（支持 MiniMap、Mini_Map 自动分词）"
        value={text}
        onChange={(e) => setText(e.target.value)}
        rows={4}
      />

      <div className="action-row">
        <button className="translate-btn" onClick={() => handleTranslate()} disabled={loading}>
          {loading ? "翻译中..." : "翻译"}
        </button>
        <button className="example-btn" onClick={handleTextExample}>
          测试文本预处理
        </button>
      </div>

      {error && <div className="error-msg">{error}</div>}

      {result && (
        <div className="result-box">
          <div className="result-header">
            <span>翻译结果</span>
            {engineUsed && <span className="engine-used">引擎：{engineUsed}</span>}
          </div>
          <div className="result-text">{result}</div>
        </div>
      )}
    </div>
  );
};

export default TranslatePanel;