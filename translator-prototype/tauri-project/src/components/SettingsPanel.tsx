import React, { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import TermsPanel from "./TermsPanel";
import HistoryPanel from "./HistoryPanel";
import "./SettingsPanel.css";

// ===== 常量 =====

// 截图菜单组件（chips 展示顺序默认值，长按拖动可排序）
const GROUP_COMPONENTS = [
  { id: "engine", label: "翻译引擎" },
  { id: "lang", label: "原/译语言" },
  { id: "copy", label: "复制" },
  { id: "close", label: "关闭" },
  { id: "settings", label: "设置" },
];

const ONLINE_PROVIDERS = [
  { id: "baidu", label: "百度翻译", engine: "百度翻译" },
  { id: "youdao", label: "有道智云", engine: "有道智云" },
  { id: "niutrans", label: "小牛翻译", engine: "小牛翻译" },
  { id: "deepl", label: "DeepL", engine: "DeepL" },
  { id: "tencent", label: "腾讯云翻译", engine: "腾讯云翻译" },
  { id: "ali", label: "阿里云翻译", engine: "阿里云翻译" },
  // engine=后端引擎名（current_engine 存储/截图菜单匹配用），label=界面显示
  { id: "custom", label: "自定义AI（OpenAI兼容）", engine: "自定义AI" },
];

const OCR_ENGINES = [
  { id: "windows", label: "Windows 内置 OCR", desc: "本地 · 免费 · 系统自带" },
  { id: "youdao", label: "有道 OCR", desc: "云端 · 体验金计费" },
  { id: "rapidocr", label: "RapidOCR 本地", desc: "本地 · 免费 · 离线（首次下载约15MB）" },
];

// 常用 AI 供应商预设（OpenAI 兼容端点，点击预填后仅需填 Key）
const AI_PRESETS = [
  { name: "DeepSeek", url: "https://api.deepseek.com/v1", model: "deepseek-chat" },
  {
    name: "通义千问",
    url: "https://dashscope.aliyuncs.com/compatible-mode/v1",
    model: "qwen-plus",
  },
  { name: "Kimi", url: "https://api.moonshot.cn/v1", model: "moonshot-v1-8k" },
  {
    name: "智谱GLM",
    url: "https://open.bigmodel.cn/api/paas/v4",
    model: "glm-4-flash",
  },
  { name: "OpenAI", url: "https://api.openai.com/v1", model: "gpt-4o-mini" },
  {
    name: "Gemini",
    url: "https://generativelanguage.googleapis.com/v1beta/openai",
    model: "gemini-1.5-flash",
  },
  {
    name: "Claude",
    url: "https://api.anthropic.com/v1",
    model: "claude-3-5-sonnet",
  },
  {
    name: "SiliconFlow",
    url: "https://api.siliconflow.cn/v1",
    model: "Qwen/Qwen2.5-7B-Instruct",
  },
];

// 菜单组件 SVG 小图标（文字后方展示）
const ComponentIcon: React.FC<{ id: string }> = ({ id }) => {
  const common = { width: 14, height: 14, viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", strokeWidth: 2, strokeLinecap: "round" as const, strokeLinejoin: "round" as const };
  switch (id) {
    case "engine":
      return (
        <svg {...common}>
          <circle cx="12" cy="12" r="3" />
          <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33h0a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51h0a1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82v0a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
        </svg>
      );
    case "lang":
      return (
        <svg {...common}>
          <path d="M5 8l6 6" />
          <path d="M4 14l6-6 2-3" />
          <path d="M2 5h12" />
          <path d="M7 2h1" />
          <path d="M22 22l-5-10-5 10" />
          <path d="M14 18h6" />
        </svg>
      );
    case "copy_src":
    case "copy_dst":
      return (
        <svg {...common}>
          <rect x="9" y="9" width="13" height="13" rx="2" />
          <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
        </svg>
      );
    case "close":
      return (
        <svg {...common}>
          <line x1="18" y1="6" x2="6" y2="18" />
          <line x1="6" y1="6" x2="18" y2="18" />
        </svg>
      );
    case "settings":
      return (
        <svg {...common}>
          <line x1="4" y1="21" x2="4" y2="14" />
          <line x1="4" y1="10" x2="4" y2="3" />
          <line x1="12" y1="21" x2="12" y2="12" />
          <line x1="12" y1="8" x2="12" y2="3" />
          <line x1="20" y1="21" x2="20" y2="16" />
          <line x1="20" y1="12" x2="20" y2="3" />
          <line x1="1" y1="14" x2="7" y2="14" />
          <line x1="9" y1="8" x2="15" y2="8" />
          <line x1="17" y1="16" x2="23" y2="16" />
        </svg>
      );
    default:
      return null;
  }
};

// ===== 快捷键录入框：点击后捕获组合键（录入期间全局热键被临时注销，不会被抢先触发）=====
const HotkeyInput: React.FC<{ value: string; onChange: (v: string) => void }> = ({
  value,
  onChange,
}) => {
  const [recording, setRecording] = useState(false);
  useEffect(() => {
    if (!recording) return;
    const onKey = async (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") {
        setRecording(false);
        invoke("set_hotkeys_suspended", { suspended: false }).catch(() => {});
        return;
      }
      if (e.key === "Control" || e.key === "Alt" || e.key === "Shift" || e.key === "Meta") {
        return; // 等待完整组合
      }
      const parts: string[] = [];
      if (e.ctrlKey) parts.push("Ctrl");
      if (e.altKey) parts.push("Alt");
      if (e.shiftKey) parts.push("Shift");
      parts.push(e.key.length === 1 ? e.key.toUpperCase() : e.key);
      onChange(parts.join("+"));
      setRecording(false);
      invoke("set_hotkeys_suspended", { suspended: false }).catch(() => {});
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [recording, onChange]);

  return (
    <button
      type="button"
      className={`hotkey-input ${recording ? "recording" : ""}`}
      onClick={async () => {
        setRecording(true);
        invoke("set_hotkeys_suspended", { suspended: true }).catch(() => {});
      }}
      title="点击后按下新的组合键；Esc 取消"
    >
      {recording ? "请按下组合键（Esc 取消）" : value || "点击设置"}
    </button>
  );
};

// ===== 菜单组件 chips：点击切换增删，长按 500ms 进入左右拖动排序 =====
const ComponentChips: React.FC<{
  order: string[];
  onOrderChange: (next: string[]) => void;
}> = ({ order, onOrderChange }) => {
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const chipRefs = useRef<Record<string, HTMLDivElement | null>>({});
  // 区分“点击切换”与“拖拽排序”：按下即拖拽模式，但未发生位移的松开视为点击
  const pressRef = useRef<{ id: string; x: number; y: number; moved: boolean } | null>(
    null
  );

  const allIds = GROUP_COMPONENTS.map((d) => d.id);
  // 显示顺序 = 配置顺序 + 未启用的组件（追加在后，同样可拖动）
  const visibleOrder: string[] = [
    ...order.filter((id) => allIds.includes(id)),
    ...allIds.filter((id) => !order.includes(id)),
  ];

  // 按住即进入拖拽模式（无阈值延迟）；松开时若未移动则视为点击切换
  const startHold = (id: string, x: number, y: number) => {
    pressRef.current = { id, x, y, moved: false };
    setDraggingId(id);
  };

  useEffect(() => {
    if (!draggingId) return;
    const onMove = (e: MouseEvent) => {
      if (pressRef.current) {
        const dx = e.clientX - pressRef.current.x;
        const dy = e.clientY - pressRef.current.y;
        if (Math.abs(dx) > 4 || Math.abs(dy) > 4) {
          pressRef.current.moved = true;
        }
      }
      if (!pressRef.current?.moved) return; // 未超过位移阈值不触发排序
      let targetId: string | null = null;
      for (const id of visibleOrder) {
        const el = chipRefs.current[id];
        if (!el) continue;
        const r = el.getBoundingClientRect();
        if (e.clientX >= r.left && e.clientX <= r.right) {
          targetId = id;
          break;
        }
      }
      if (!targetId || targetId === draggingId) return;
      const next = visibleOrder.filter((id) => id !== draggingId);
      const ti = next.indexOf(targetId);
      next.splice(ti, 0, draggingId);
      onOrderChange(next);
    };
    const onUp = () => {
      const press = pressRef.current;
      if (press && !press.moved) {
        // 未移动的松开 = 点击：切换该组件启用/禁用
        onOrderChange(
          order.includes(press.id)
            ? order.filter((x) => x !== press.id)
            : [...order, press.id]
        );
      }
      pressRef.current = null;
      setDraggingId(null);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    return () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
  }, [draggingId, visibleOrder, order, onOrderChange]);

  return (
    <div className="component-chips">
      {visibleOrder.map((id) => {
        const def = GROUP_COMPONENTS.find((d) => d.id === id);
        if (!def) return null;
        const enabled = order.includes(id);
        return (
          <div
            key={id}
            ref={(el) => {
              chipRefs.current[id] = el;
            }}
            className={`component-chip ${enabled ? "enabled" : ""} ${
              draggingId === id ? "dragging" : ""
            }`}
            onMouseDown={(e) => startHold(id, e.clientX, e.clientY)}
            title="点击启用/禁用 · 按住拖动排序"
          >
            <ComponentIcon id={id} />
            <span>{def.label}</span>
          </div>
        );
      })}
      <div className="component-tip">
        点击启用/禁用组件 · 按住组件左右拖动排序 · 从左到右显示在截图菜单组中
      </div>
    </div>
  );
};

// ===== 组件 =====

interface SettingsPanelProps {
  activeMenu: string;
  settings: any;
  // 受控模式：localSettings 由 App 持有（保存/恢复按钮在侧边栏底部统一渲染）
  localSettings: any;
  onLocalChange: (next: any) => void;
}

const SettingsPanel: React.FC<SettingsPanelProps> = ({
  activeMenu,
  localSettings,
  onLocalChange,
}) => {
  const [netOk, setNetOk] = useState<boolean | null>(null);

  useEffect(() => {
    if (activeMenu !== "basic") return;
    invoke<any>("get_network_status")
      .then((n) => setNetOk(!!n?.is_online))
      .catch(() => setNetOk(null));
  }, [activeMenu]);

  const handleSettingChange = (key: string, value: any) => {
    onLocalChange({ ...localSettings, [key]: value });
  };

  // 供应商凭据是否已填写（徽标：已接入 / 未接入）
  const providerReady = (id: string): boolean => {
    const s: any = localSettings;
    if (!s) return false;
    switch (id) {
      case "baidu":
        return !!(s.baidu_app_id && s.baidu_secret);
      case "youdao":
        return !!(s.youdao_app_key && s.youdao_app_secret);
      case "niutrans":
        return !!s.providers?.niutrans_api_key;
      case "deepl":
        return !!s.providers?.deepl_api_key;
      case "tencent":
        return !!(s.providers?.tencent_secret_id && s.providers?.tencent_secret_key);
      case "ali":
        return !!(s.providers?.ali_access_key_id && s.providers?.ali_access_key_secret);
      default:
        return false;
    }
  };

  const toggleOnlineApi = (id: string, on: boolean) => {
    const apis: string[] = localSettings?.online_apis || [];
    handleSettingChange(
      "online_apis",
      on ? [...new Set([...apis, id])] : apis.filter((a: string) => a !== id)
    );
  };

  const setProviderKey = (key: string, value: string) => {
    handleSettingChange("providers", {
      ...localSettings?.providers,
      [key]: value,
    });
  };

  // ===== 自定义多供应商 =====
  const customProviders: any[] = localSettings?.custom_providers || [];
  const updateProvider = (idx: number, field: string, value: string) => {
    onLocalChange({
      ...localSettings,
      custom_providers: customProviders.map((p, i) =>
        i === idx ? { ...p, [field]: value } : p
      ),
    });
  };
  const addProvider = (preset?: (typeof AI_PRESETS)[number]) => {
    const np = {
      id: `cp_${Date.now()}`,
      name: preset?.name || "",
      base_url: preset?.url || "",
      api_key: "",
      model: preset?.model || "",
    };
    onLocalChange({
      ...localSettings,
      custom_providers: [...customProviders, np],
    });
  };
  const removeProvider = (idx: number) => {
    onLocalChange({
      ...localSettings,
      custom_providers: customProviders.filter((_, i) => i !== idx),
    });
  };

  // ===== 菜单组件 chips =====
  const screenshotComponents: string[] = localSettings?.screenshot_components || [
    "engine",
    "lang",
    "copy",
    "close",
    "settings",
  ];

  // ===== 各页面 =====

  const renderBasicSettings = () => {
    return (
      <div className="settings-section">
        <h3 className="section-title">基础设置</h3>

        <div className="form-group">
          <div className="toggle-group">
            <label className="toggle-switch">
              <input
                type="checkbox"
                checked={localSettings?.autostart || false}
                onChange={(e) => handleSettingChange("autostart", e.target.checked)}
              />
              <span className="toggle-slider"></span>
            </label>
            <span className="toggle-label">开机自启动</span>
          </div>
        </div>

        <div className="form-group">
          <label className="form-label">启动时主界面</label>
          <div className="toggle-group">
            <label className="toggle-switch">
              <input
                type="checkbox"
                checked={localSettings?.startup_show_window || false}
                onChange={(e) =>
                  handleSettingChange("startup_show_window", e.target.checked)
                }
              />
              <span className="toggle-slider"></span>
            </label>
            <span className="toggle-label">
              {localSettings?.startup_show_window
                ? "显示在前台"
                : "隐藏在托盘（点击任务栏托盘图标唤出）"}
            </span>
          </div>
        </div>

        <div className="form-group">
          <label className="form-label">默认翻译方向</label>
          <select
            className="form-select"
            value={localSettings?.default_translation_direction || "auto->zh"}
            onChange={(e) =>
              handleSettingChange("default_translation_direction", e.target.value)
            }
          >
            <option value="auto->zh">自动检测 → 中文</option>
            <option value="zh->en">中文 → 英语</option>
            <option value="en->zh">英语 → 中文</option>
            <option value="zh->ja">中文 → 日语</option>
          </select>
        </div>

        <div className="form-group">
          <label className="form-label">主窗口大小</label>
          <div className="toggle-group">
            <label className="toggle-switch">
              <input
                type="checkbox"
                checked={localSettings?.window_size_mode === "fixed"}
                onChange={(e) =>
                  handleSettingChange(
                    "window_size_mode",
                    e.target.checked ? "fixed" : "last"
                  )
                }
              />
              <span className="toggle-slider"></span>
            </label>
            <span className="toggle-label">
              {localSettings?.window_size_mode === "fixed"
                ? "固定大小"
                : "记住上次大小"}
            </span>
          </div>
          {localSettings?.window_size_mode === "fixed" && (
            <div className="api-keys">
              <input
                type="number"
                className="form-input"
                placeholder="宽"
                value={localSettings?.window_fixed_width || 1200}
                onChange={(e) =>
                  handleSettingChange("window_fixed_width", Number(e.target.value))
                }
              />
              <input
                type="number"
                className="form-input"
                placeholder="高"
                value={localSettings?.window_fixed_height || 800}
                onChange={(e) =>
                  handleSettingChange("window_fixed_height", Number(e.target.value))
                }
              />
            </div>
          )}
        </div>

        <div className="form-group">
          <label className="form-label">网络状态</label>
          <div className="toggle-group">
            <span
              className={`status-indicator ${netOk === false ? "offline" : "online"}`}
            ></span>
            <span className="toggle-label">
              {netOk === null
                ? "检测中…"
                : netOk
                ? "在线 - 可使用在线翻译引擎"
                : "离线 - 将自动使用离线翻译"}
            </span>
          </div>
        </div>

        <div className="form-group">
          <label className="form-label">系统右键菜单</label>
          <p className="form-hint">
            已自动启用：在文件/文件夹上右键可见“智能翻译”菜单项，点击后捕获屏幕选中文本并翻译。
            选中文本的右键菜单由各应用私有（系统限制无法全局注入），请使用 Ctrl+Alt+X 划词。
          </p>
        </div>
      </div>
    );
  };

  const renderTranslationSettings = () => {
    const enabledRealProviders = ONLINE_PROVIDERS.filter((p) =>
      localSettings?.online_apis?.includes(p.id)
    );
    return (
      <div className="settings-section">
        <h3 className="section-title">翻译引擎</h3>

        <div className="form-group">
          <label className="form-label">在线翻译引擎</label>
          {ONLINE_PROVIDERS.map((p) => (
            <div className="toggle-group" key={p.id}>
              <label className="toggle-switch">
                <input
                  type="checkbox"
                  checked={localSettings?.online_apis?.includes(p.id) || false}
                  onChange={(e) => toggleOnlineApi(p.id, e.target.checked)}
                />
                <span className="toggle-slider"></span>
              </label>
              <span className="toggle-label">
                {p.label}
                {p.id !== "custom" && (
                  <span className="provider-badge">
                    {providerReady(p.id) ? "已接入" : "未接入"}
                  </span>
                )}
              </span>
            </div>
          ))}
          <p className="form-hint">
            启用的引擎与离线翻译会出现在截图翻译的引擎菜单中（启用的引擎在下方填写密钥）
          </p>
        </div>

        {/* 启用引擎的 API 密钥区（未启用隐藏，不挡视野） */}
        {localSettings?.online_apis?.includes("baidu") && (
          <div className="form-group">
            <label className="form-label">百度翻译 API</label>
            <div className="api-keys">
              <input
                type="text"
                className="form-input"
                placeholder="百度 APP ID"
                value={localSettings?.baidu_app_id || ""}
                onChange={(e) => handleSettingChange("baidu_app_id", e.target.value)}
              />
              <input
                type="password"
                className="form-input"
                placeholder="百度 密钥"
                value={localSettings?.baidu_secret || ""}
                onChange={(e) => handleSettingChange("baidu_secret", e.target.value)}
              />
            </div>
          </div>
        )}

        {localSettings?.online_apis?.includes("youdao") && (
          <div className="form-group">
            <label className="form-label">有道智云 API</label>
            <div className="api-keys">
              <input
                type="text"
                className="form-input"
                placeholder="有道 应用ID"
                value={localSettings?.youdao_app_key || ""}
                onChange={(e) => handleSettingChange("youdao_app_key", e.target.value)}
              />
              <input
                type="password"
                className="form-input"
                placeholder="有道 应用密钥"
                value={localSettings?.youdao_app_secret || ""}
                onChange={(e) => handleSettingChange("youdao_app_secret", e.target.value)}
              />
            </div>
          </div>
        )}

        {localSettings?.online_apis?.includes("niutrans") && (
          <div className="form-group">
            <label className="form-label">小牛翻译 API</label>
            <div className="api-keys">
              <input
                type="password"
                className="form-input"
                placeholder="API Key"
                value={localSettings?.providers?.niutrans_api_key || ""}
                onChange={(e) => setProviderKey("niutrans_api_key", e.target.value)}
              />
            </div>
          </div>
        )}

        {localSettings?.online_apis?.includes("deepl") && (
          <div className="form-group">
            <label className="form-label">DeepL API</label>
            <div className="api-keys">
              <input
                type="password"
                className="form-input"
                placeholder="DeepL Auth Key（Free版以 ..fx 结尾）"
                value={localSettings?.providers?.deepl_api_key || ""}
                onChange={(e) => setProviderKey("deepl_api_key", e.target.value)}
              />
            </div>
          </div>
        )}

        {localSettings?.online_apis?.includes("tencent") && (
          <div className="form-group">
            <label className="form-label">腾讯云翻译 API</label>
            <div className="api-keys">
              <input
                type="text"
                className="form-input"
                placeholder="SecretId"
                value={localSettings?.providers?.tencent_secret_id || ""}
                onChange={(e) => setProviderKey("tencent_secret_id", e.target.value)}
              />
              <input
                type="password"
                className="form-input"
                placeholder="SecretKey"
                value={localSettings?.providers?.tencent_secret_key || ""}
                onChange={(e) => setProviderKey("tencent_secret_key", e.target.value)}
              />
            </div>
          </div>
        )}

        {localSettings?.online_apis?.includes("ali") && (
          <div className="form-group">
            <label className="form-label">阿里云翻译 API</label>
            <div className="api-keys">
              <input
                type="text"
                className="form-input"
                placeholder="AccessKey ID"
                value={localSettings?.providers?.ali_access_key_id || ""}
                onChange={(e) => setProviderKey("ali_access_key_id", e.target.value)}
              />
              <input
                type="password"
                className="form-input"
                placeholder="AccessKey Secret"
                value={localSettings?.providers?.ali_access_key_secret || ""}
                onChange={(e) => setProviderKey("ali_access_key_secret", e.target.value)}
              />
            </div>
          </div>
        )}

        {localSettings?.online_apis?.includes("custom") && (
          <div className="form-group">
            <label className="form-label">自定义AI（OpenAI 兼容）· 支持多个供应商</label>

            {customProviders.map((cp, idx) => (
              <div className="custom-provider-row" key={cp.id || idx}>
                <div className="custom-provider-fields">
                  <input
                    type="text"
                    className="form-input"
                    placeholder="供应商名称（如 DeepSeek）"
                    value={cp.name || ""}
                    onChange={(e) => updateProvider(idx, "name", e.target.value)}
                  />
                  <input
                    type="text"
                    className="form-input"
                    placeholder="Base URL"
                    value={cp.base_url || ""}
                    onChange={(e) => updateProvider(idx, "base_url", e.target.value)}
                  />
                  <input
                    type="password"
                    className="form-input"
                    placeholder="API Key"
                    value={cp.api_key || ""}
                    onChange={(e) => updateProvider(idx, "api_key", e.target.value)}
                  />
                  <input
                    type="text"
                    className="form-input"
                    placeholder="模型名"
                    value={cp.model || ""}
                    onChange={(e) => updateProvider(idx, "model", e.target.value)}
                  />
                </div>
                <button
                  className="custom-provider-remove"
                  onClick={() => removeProvider(idx)}
                  title="删除此供应商"
                >
                  ✕
                </button>
              </div>
            ))}

            {/* 常用 AI 预设：右侧空白处 2行×n列 */}
            <div className="preset-area">
              <div className="preset-grid">
                {AI_PRESETS.map((ps) => (
                  <button
                    key={ps.name}
                    className="preset-btn"
                    onClick={() => addProvider(ps)}
                    title={`点击添加 ${ps.name} 供应商（预填地址与模型，仅需填 Key）`}
                  >
                    + {ps.name}
                  </button>
                ))}
              </div>
              <button className="custom-provider-add" onClick={() => addProvider()}>
                + 手动添加供应商
              </button>
            </div>
          </div>
        )}

        {localSettings?.online_apis?.length === 0 && (
          <div className="form-group">
            <p className="form-hint">
              未启用任何在线引擎——离线翻译仍可工作（见下方离线翻译引擎）
            </p>
          </div>
        )}

        {/* 当前翻译源 */}
        {(enabledRealProviders.length > 0 ||
          localSettings?.offline_engine !== "disabled") && (
          <div className="form-group">
            <label className="form-label">当前翻译源（默认引擎）</label>
            <select
              className="form-select"
              value={
                localSettings?.current_engine ||
                enabledRealProviders[0]?.engine ||
                "离线翻译"
              }
              onChange={(e) => handleSettingChange("current_engine", e.target.value)}
            >
              {enabledRealProviders.map((p) => (
                <option key={p.id} value={p.engine}>
                  {p.label}
                </option>
              ))}
              {localSettings?.offline_engine !== "disabled" && (
                <option value="离线翻译">离线翻译（本地模型）</option>
              )}
            </select>
            <div className="form-hint">
              与截图翻译菜单双向绑定；离线翻译作为默认源时完全无需联网
            </div>
          </div>
        )}
      </div>
    );
  };

  const renderOcrSettings = () => (
    <div className="settings-section">
      <h3 className="section-title">OCR 引擎</h3>
      {OCR_ENGINES.map((o) => (
        <div className="toggle-group" key={o.id}>
          <label className="toggle-switch">
            <input
              type="checkbox"
              checked={localSettings?.ocr_engine === o.id}
              onChange={() => handleSettingChange("ocr_engine", o.id)}
            />
            <span className="toggle-slider"></span>
          </label>
          <span className="toggle-label">
            {o.label}
            <span className="provider-badge">{o.desc}</span>
          </span>
        </div>
      ))}
      <p className="form-hint">
        单选：同时只有一个 OCR 引擎生效，用于截图翻译的文字识别（识别出的部首/符号错误可由术语管理的“常用符号纠错包”纠正）
      </p>
    </div>
  );

  const renderScreenshotSettings = () => {
    return (
      <div className="settings-section">
        <h3 className="section-title">
          截图翻译
          <button className="title-action-btn" onClick={async () => {
            try {
              await invoke("trigger_screenshot_cmd");
            } catch (e) {
              alert("打开截图窗口失败");
            }
          }}>
            📷 启动截图翻译
          </button>
        </h3>

        <div className="form-group">
          <label className="form-label">菜单组组件</label>
          <ComponentChips
            order={screenshotComponents}
            onOrderChange={(next) => handleSettingChange("screenshot_components", next)}
          />
        </div>

        <div className="form-group">
          <label className="form-label">覆盖样式</label>
          <div className="overlay-mode-row">
            {[
              ["dark", "黑底白字"],
              ["light", "白底黑字"],
              ["none", "无背景"],
            ].map(([v, label]) => (
              <label
                key={v}
                className={`overlay-mode-item ${
                  (localSettings?.overlay_mode || "dark") === v ? "active" : ""
                }`}
              >
                <input
                  type="radio"
                  name="overlay_mode"
                  checked={(localSettings?.overlay_mode || "dark") === v}
                  onChange={() => handleSettingChange("overlay_mode", v)}
                />
                {label}
              </label>
            ))}
          </div>
          <p className="form-hint">
            黑底/白底覆盖整个框选区域；无背景 = 仅显示翻译文字 + 白色光晕
          </p>
        </div>

        <div className="form-group">
          <div className="toggle-group">
            <label className="toggle-switch">
              <input
                type="checkbox"
                checked={localSettings?.overlay_expand || false}
                onChange={(e) =>
                  handleSettingChange("overlay_expand", e.target.checked)
                }
              />
              <span className="toggle-slider"></span>
            </label>
            <span className="toggle-label">
              严格对齐模式（译文空间 = 原文区域 +10%，字号自动填充）
            </span>
          </div>
        </div>

        <div className="form-group">
          <label className="form-label">截图翻译行为</label>
          <div className="toggle-group">
            <label className="toggle-switch">
              <input
                type="checkbox"
                checked={localSettings?.screenshot_behavior?.cover_original_text || false}
                onChange={(e) =>
                  handleSettingChange("screenshot_behavior", {
                    ...localSettings?.screenshot_behavior,
                    cover_original_text: e.target.checked,
                  })
                }
              />
              <span className="toggle-slider"></span>
            </label>
            <span className="toggle-label">默认覆盖原文显示翻译</span>
          </div>

          <div className="toggle-group">
            <label className="toggle-switch">
              <input
                type="checkbox"
                checked={localSettings?.screenshot_behavior?.left_click_toggle || false}
                onChange={(e) =>
                  handleSettingChange("screenshot_behavior", {
                    ...localSettings?.screenshot_behavior,
                    left_click_toggle: e.target.checked,
                  })
                }
              />
              <span className="toggle-slider"></span>
            </label>
            <span className="toggle-label">左键隐藏/显示翻译结果</span>
          </div>

          <div className="toggle-group">
            <label className="toggle-switch">
              <input
                type="checkbox"
                checked={localSettings?.screenshot_behavior?.right_click_exit || false}
                onChange={(e) =>
                  handleSettingChange("screenshot_behavior", {
                    ...localSettings?.screenshot_behavior,
                    right_click_exit: e.target.checked,
                  })
                }
              />
              <span className="toggle-slider"></span>
            </label>
            <span className="toggle-label">右键退出截图翻译</span>
          </div>
        </div>
      </div>
    );
  };

  const renderOfflineModel = () => (
    <div className="settings-section">
      <h3 className="section-title">离线翻译引擎</h3>

      <div className="form-group">
        <label className="form-label">离线模型</label>
        <select
          className="form-select"
          value={localSettings?.offline_model || "opus-mt"}
          onChange={(e) => handleSettingChange("offline_model", e.target.value)}
        >
          <option value="opus-mt">OPUS-MT（轻量快速 · 按语言对约30-80MB · 当前使用）</option>
          <option value="nllb-200" disabled>
            NLLB-200 蒸馏版（200种语言单模型 · 约600MB · 实验性完善中，暂建议 OPUS-MT）
          </option>
          <option value="disabled">禁用离线翻译</option>
        </select>
        <div className="form-hint">
          排在在线引擎之后自动兜底，也可在截图菜单手动选中。
          首次使用某语言对时自动下载模型并存于程序目录 models/mt/，此后完全离线。
          中↔英为专门模型直达；日/韩/俄/法/德/西/葡经英语中转。
        </div>
      </div>

      <div className="form-group">
        <label className="form-label">模型下载</label>
        <button
          className="button secondary"
          onClick={async () => {
            try {
              await invoke("download_offline_model");
              alert("离线模型已就绪");
            } catch (e) {
              alert(`模型下载失败：${e}`);
            }
          }}
        >
          手动下载当前方向模型
        </button>
        <div className="form-hint">
          按上方“默认翻译方向”预下载所需模型（进度提示会显示在截图翻译面板），下载后可离线使用
        </div>
      </div>

      <div className="form-group">
        <label className="form-label">术语优先级调整</label>
        <select
          className="form-select"
          value={localSettings?.term_base_priority || "auto+manual"}
          onChange={(e) => handleSettingChange("term_base_priority", e.target.value)}
        >
          <option value="auto+manual">自动调整 + 手动微调</option>
          <option value="auto">仅自动调整</option>
          <option value="manual">仅手动微调</option>
        </select>
        <div className="form-hint">
          <b>自动调整</b>：某译法被使用每累计 50 次，自动将其优先级 +1（越用越准）；
          <b>手动微调</b>：在术语管理中用 ▲▼ 手动设定优先级，手动设定优先于自动调整。
        </div>
      </div>
    </div>
  );

  const renderHotkeySettings = () => (
    <div className="settings-section">
      <h3 className="section-title">快捷键</h3>
      <p className="form-hint">
        点击输入框后直接按下新的组合键即可录入（录入期间全局快捷键暂停，避免被抢先触发）
      </p>

      <div className="form-group">
        <label className="form-label">截图翻译</label>
        <HotkeyInput
          value={localSettings?.hotkeys?.screenshot || "Ctrl+Alt+S"}
          onChange={(v) =>
            handleSettingChange("hotkeys", { ...localSettings?.hotkeys, screenshot: v })
          }
        />
      </div>

      <div className="form-group">
        <label className="form-label">打开翻译页面</label>
        <HotkeyInput
          value={localSettings?.hotkeys?.translate || "Ctrl+Alt+T"}
          onChange={(v) =>
            handleSettingChange("hotkeys", { ...localSettings?.hotkeys, translate: v })
          }
        />
      </div>

      <div className="form-group">
        <label className="form-label">划词翻译</label>
        <HotkeyInput
          value={localSettings?.hotkeys?.select || "Ctrl+Alt+X"}
          onChange={(v) =>
            handleSettingChange("hotkeys", { ...localSettings?.hotkeys, select: v })
          }
        />
      </div>

      <div className="form-group">
        <label className="form-label">反转原/译语言</label>
        <HotkeyInput
          value={localSettings?.hotkeys?.reverse || "Ctrl+Alt+B"}
          onChange={(v) =>
            handleSettingChange("hotkeys", { ...localSettings?.hotkeys, reverse: v })
          }
        />
        <div className="form-hint">仅在截图翻译界面内生效</div>
      </div>
    </div>
  );

  const renderAbout = () => (
    <div className="settings-section">
      <h3 className="section-title">关于</h3>
      <div className="about-info">
        <p>
          <strong>智能翻译软件</strong>
        </p>
        <p>版本: 0.2.0</p>
        <p>技术栈: Tauri 2 + React + Rust（ONNX Runtime 本地推理）</p>
        <p>
          开源地址:{" "}
          <a
            href="#"
            className="gh-link"
            onClick={(e) => {
              e.preventDefault();
              invoke("open_external", {
                url: "https://github.com/lugaoyu2005/smart-translator",
              });
            }}
          >
            github.com/lugaoyu2005/smart-translator
          </a>
        </p>
        <p>支持平台: Windows 11, Android (计划中)</p>
      </div>
    </div>
  );

  const renderContent = () => {
    switch (activeMenu) {
      case "basic":
        return renderBasicSettings();
      case "translation":
        return (
          <>
            {renderTranslationSettings()}
            {renderOcrSettings()}
            {renderOfflineModel()}
          </>
        );
      case "screenshot":
        return renderScreenshotSettings();
      case "hotkeys":
        return renderHotkeySettings();
      case "about":
        return renderAbout();
      case "terms":
        return <TermsPanel />;
      case "history":
        return <HistoryPanel />;
      default:
        return (
          <div className="settings-section">
            <h3 className="section-title">功能开发中</h3>
            <p>此功能正在开发中，敬请期待...</p>
          </div>
        );
    }
  };

  return (
    <div className="settings-panel">
      {renderContent()}

    </div>
  );
};

export default SettingsPanel;
