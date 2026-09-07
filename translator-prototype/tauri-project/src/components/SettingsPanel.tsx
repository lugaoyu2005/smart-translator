import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import "./SettingsPanel.css";
import TermsPanel from "./TermsPanel";
import HistoryPanel from "./HistoryPanel";

// 菜单组组件定义（工具箱：设置里可增删、排序）
const GROUP_COMPONENTS = [
  { id: "engine", label: "翻译引擎" },
  { id: "lang", label: "原/译语言" },
  { id: "copy", label: "复制（原/译）" },
  { id: "close", label: "关闭" },
  { id: "settings", label: "设置" },
];

// 在线翻译引擎清单：status 非空 = 框架阶段（翻译实现后续接入）
// status 一律由代码动态计算（已接入/已接入·待填密钥），字段废弃
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

// OCR 引擎清单：ready=true 当前可用
const OCR_ENGINES = [
  { id: "windows", label: "Windows 内置 OCR", status: "已接入 · 本地 · 免费" },
  { id: "youdao", label: "有道 OCR", status: "已接入 · 云 · 体验金计费" },
  { id: "rapidocr", label: "RapidOCR 本地", status: "已接入 · 本地 · 免费 · 离线" },
  { id: "tesseract", label: "Tesseract", status: "未接入" },
];

interface SettingsPanelProps {
  activeMenu: string;
  settings: any;
  onSaveSettings: (settings: any) => void;
}

const SettingsPanel: React.FC<SettingsPanelProps> = ({
  activeMenu,
  settings,
  onSaveSettings,
}) => {
  const [localSettings, setLocalSettings] = useState<any>(settings);

  useEffect(() => {
    setLocalSettings(settings);
  }, [settings]);

  const handleSettingChange = (key: string, value: any) => {
    setLocalSettings((prev: any) => ({
      ...prev,
      [key]: value,
    }));
  };

  const handleSave = async () => {
    // 先等设置落盘，再重载引擎：两者并发时 reload_engines 可能读到旧配置，
    // 导致截图窗口引擎列表与设置不同步
    await onSaveSettings(localSettings);
    // 保存后重新加载引擎（应用新的API密钥）
    try {
      await invoke("reload_engines");
    } catch (e) {
      console.error("重新加载引擎失败:", e);
    }
    // 通知截图窗口：设置实时生效
    try {
      await emit("screenshot-settings-updated");
    } catch (e) {
      console.error("通知截图窗口失败:", e);
    }
  };

  // 菜单组组件：启用/禁用（保持定义顺序）
  const toggleComponent = (id: string, enabled: boolean) => {
    const comps: string[] = localSettings?.screenshot_components || [
      "engine",
      "copy",
      "close",
      "settings",
    ];
    const next = enabled
      ? [...comps, id]
      : comps.filter((c: string) => c !== id);
    const ordered = GROUP_COMPONENTS.map((d) => d.id).filter((gid) =>
      next.includes(gid)
    );
    handleSettingChange("screenshot_components", ordered);
  };

  // 菜单组组件：上移/下移
  const moveComponent = (id: string, dir: -1 | 1) => {
    const comps: string[] = [...(localSettings?.screenshot_components || [])];
    const idx = comps.indexOf(id);
    const target = idx + dir;
    if (idx < 0 || target < 0 || target >= comps.length) return;
    [comps[idx], comps[target]] = [comps[target], comps[idx]];
    handleSettingChange("screenshot_components", comps);
  };

  // 供应商凭据是否已填写（决定"已接入 / 已接入·待填密钥"徽标）
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
      case "custom":
        return !!(s.providers?.custom_openai_base_url && s.providers?.custom_openai_api_key);
      default:
        return false;
    }
  };

  // 在线翻译引擎：启用/停用（双向绑定截图翻译引擎菜单）
  const toggleOnlineApi = (id: string, on: boolean) => {
    const apis: string[] = localSettings?.online_apis || [];
    handleSettingChange(
      "online_apis",
      on ? [...new Set([...apis, id])] : apis.filter((a: string) => a !== id)
    );
  };

  // 供应商凭据字段更新
  const setProviderKey = (field: string, value: string) => {
    handleSettingChange("providers", {
      ...(localSettings?.providers || {}),
      [field]: value,
    });
  };

  const renderBasicSettings = () => (
    <div className="settings-section">
      <h3 className="section-title">基本设置</h3>
      
      <div className="form-group">
        <label className="form-label">开机自启动</label>
        <label className="toggle-switch">
          <input
            type="checkbox"
            checked={localSettings?.autostart || false}
            onChange={(e) => handleSettingChange("autostart", e.target.checked)}
          />
          <span className="toggle-slider"></span>
        </label>
        <span className="toggle-label">启用开机自启动</span>
      </div>
      
      <div className="form-group">
        <label className="form-label">默认翻译方向</label>
        <select
          className="form-select"
          value={localSettings?.default_translation_direction || "auto->zh"}
          onChange={(e) => handleSettingChange("default_translation_direction", e.target.value)}
        >
          <option value="auto->zh">自动检测 → 中文</option>
          <option value="zh->en">中文 → 英文</option>
          <option value="en->zh">英文 → 中文</option>
          <option value="zh->ja">中文 → 日文</option>
        </select>
      </div>
      
      <div className="form-group">
        <label className="form-label">主窗口大小</label>
        <div className="toggle-group">
          <label className="toggle-switch">
            <input
              type="checkbox"
              checked={(localSettings?.window_size_mode || "last") === "fixed"}
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
            固定大小（关闭 = 记住上次调整后的大小）
          </span>
        </div>
        {(localSettings?.window_size_mode || "last") === "fixed" && (
          <div className="api-keys">
            <input
              type="number"
              className="form-input"
              placeholder="宽（≥400）"
              min={400}
              value={localSettings?.window_fixed_width || 1200}
              onChange={(e) =>
                handleSettingChange(
                  "window_fixed_width",
                  Math.max(400, parseInt(e.target.value) || 1200)
                )
              }
            />
            <input
              type="number"
              className="form-input"
              placeholder="高（≥300）"
              min={300}
              value={localSettings?.window_fixed_height || 800}
              onChange={(e) =>
                handleSettingChange(
                  "window_fixed_height",
                  Math.max(300, parseInt(e.target.value) || 800)
                )
              }
            />
          </div>
        )}
      </div>

      <div className="form-group">
        <label className="form-label">网络状态</label>
        <div className="network-status">
          <div className={`status-indicator ${true ? "online" : "offline"}`}></div>
          <span>在线状态 - 使用在线翻译</span>
        </div>
      </div>
    </div>
  );

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
              <span className="provider-badge">
                {providerReady(p.id) ? "已接入" : "已接入 · 待填密钥"}
              </span>
            </span>
          </div>
        ))}
        <p className="form-hint">启用的引擎与离线翻译会出现在截图翻译的引擎菜单中</p>
      </div>

      {(enabledRealProviders.length > 0 || localSettings?.offline_engine !== "disabled") && (
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
          <div className="form-hint">与截图翻译菜单双向绑定；离线翻译作为默认源时完全无需联网</div>
        </div>
      )}

      {localSettings?.online_apis?.includes("baidu") && (
        <div className="form-group">
          <label className="form-label">
            百度翻译 API
            <button
              className="provider-link"
              onClick={() =>
                invoke("open_external", { url: "https://fanyi-api.baidu.com/" })
              }
              title="打开官网申请/查看密钥"
            >
              →
            </button>
          </label>
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
          <label className="form-label">
            有道智云 API
            <button
              className="provider-link"
              onClick={() => invoke("open_external", { url: "https://ai.youdao.com/" })}
              title="打开官网申请/查看密钥"
            >
              →
            </button>
          </label>
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
          <label className="form-label">小牛翻译 API（未接入）</label>
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
          <label className="form-label">DeepL API（未接入）</label>
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
          <label className="form-label">腾讯云翻译 API（未接入）</label>
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
          <label className="form-label">阿里云翻译 API（未接入）</label>
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
          <label className="form-label">自定义供应商（OpenAI 兼容 · 未接入）</label>
          <div className="api-keys">
            <input
              type="text"
              className="form-input"
              placeholder="Base URL（如 https://api.deepseek.com/v1）"
              value={localSettings?.providers?.custom_openai_base_url || ""}
              onChange={(e) => setProviderKey("custom_openai_base_url", e.target.value)}
            />
            <input
              type="password"
              className="form-input"
              placeholder="API Key"
              value={localSettings?.providers?.custom_openai_api_key || ""}
              onChange={(e) => setProviderKey("custom_openai_api_key", e.target.value)}
            />
            <input
              type="text"
              className="form-input"
              placeholder="模型名（如 deepseek-chat）"
              value={localSettings?.providers?.custom_openai_model || ""}
              onChange={(e) => setProviderKey("custom_openai_model", e.target.value)}
            />
          </div>
        </div>
      )}

      <div className="form-group">
        <label className="form-label">离线翻译引擎</label>
        <select
          className="form-select"
          value={localSettings?.offline_engine === "disabled" ? "disabled" : "marian"}
          onChange={(e) => handleSettingChange("offline_engine", e.target.value)}
        >
          <option value="marian">离线翻译 OPUS-MT（本地模型，免费无网络）</option>
          <option value="disabled">禁用离线翻译</option>
        </select>
        <div className="form-hint">
          排在在线引擎之后自动兜底：所有在线API失败时改用离线翻译，也可在截图菜单手动选中。
          首次使用某语言对时自动下载模型（每个约30-80MB，存于程序目录 models/mt/），此后完全离线。
          中↔英为专门模型直达；日/韩/俄/法/德/西/葡经英语中转。
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
          <option value="manual">仅手动调整</option>
        </select>
      </div>
    </div>
    );
  };

  const renderScreenshotSettings = () => {
    const comps: string[] = localSettings?.screenshot_components || [
      "engine",
      "copy",
      "close",
      "settings",
    ];
    return (
      <div className="settings-section">
        <h3 className="section-title">截图翻译</h3>

        <div className="form-group">
          <label className="form-label">OCR 引擎</label>
          {OCR_ENGINES.map((o) => {
            const enabled = o.status.indexOf("未接入") < 0;
            return (
              <div className="toggle-group" key={o.id}>
                <label className="toggle-switch">
                  <input
                    type="checkbox"
                    checked={localSettings?.ocr_engine === o.id}
                    disabled={!enabled}
                    onChange={() => handleSettingChange("ocr_engine", o.id)}
                  />
                  <span className="toggle-slider"></span>
                </label>
                <span className="toggle-label">
                  {o.label}
                  <span className="provider-badge">{o.status}</span>
                </span>
              </div>
            );
          })}
          <p className="form-hint">单选：同时只有一个 OCR 引擎生效，用于截图翻译的文字识别</p>
        </div>

        <div className="form-group">
          <label className="form-label">菜单组组件（增删 / 排序）</label>
          {GROUP_COMPONENTS.map((def) => {
            const enabled = comps.includes(def.id);
            return (
              <div key={def.id} className="component-row">
                <label className="toggle-switch">
                  <input
                    type="checkbox"
                    checked={enabled}
                    onChange={(e) => toggleComponent(def.id, e.target.checked)}
                  />
                  <span className="toggle-slider"></span>
                </label>
                <span className="toggle-label">{def.label}</span>
                <button
                  className="mini-btn"
                  disabled={!enabled}
                  onClick={() => moveComponent(def.id, -1)}
                  title="上移"
                >
                  ↑
                </button>
                <button
                  className="mini-btn"
                  disabled={!enabled}
                  onClick={() => moveComponent(def.id, 1)}
                  title="下移"
                >
                  ↓
                </button>
              </div>
            );
          })}
          <div className="component-tip">
            组件从左到右显示在截图选区下方的菜单组中，拖动菜单组可临时移动位置
          </div>
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

  const renderHotkeySettings = () => (
    <div className="settings-section">
      <h3 className="section-title">快捷键设置</h3>
      
      <div className="form-group">
        <label className="form-label">全局快捷键</label>
        <input
          type="text"
          className="form-input"
          value={localSettings?.hotkeys?.translate || "Ctrl+Alt+T"}
          onChange={(e) =>
            handleSettingChange("hotkeys", {
              ...localSettings?.hotkeys,
              translate: e.target.value,
            })
          }
        />
      </div>
      
      <div className="form-group">
        <label className="form-label">截图翻译快捷键</label>
        <input
          type="text"
          className="form-input"
          value={localSettings?.hotkeys?.screenshot || "Ctrl+Alt+S"}
          onChange={(e) =>
            handleSettingChange("hotkeys", {
              ...localSettings?.hotkeys,
              screenshot: e.target.value,
            })
          }
        />
      </div>

      <div className="form-group">
        <label className="form-label">划词翻译快捷键</label>
        <input
          type="text"
          className="form-input"
          value={localSettings?.hotkeys?.select || "Ctrl+Alt+X"}
          onChange={(e) =>
            handleSettingChange("hotkeys", {
              ...localSettings?.hotkeys,
              select: e.target.value,
            })
          }
        />
      </div>
    </div>
  );

  const renderSelectSettings = () => (
    <div className="settings-section">
      <h3 className="section-title">划词翻译</h3>

      <div className="form-group">
        <div className="toggle-group">
          <label className="toggle-switch">
            <input
              type="checkbox"
              checked={localSettings?.select_translate_enabled || false}
              onChange={(e) =>
                handleSettingChange(
                  "select_translate_enabled",
                  e.target.checked
                )
              }
            />
            <span className="toggle-slider"></span>
          </label>
          <span className="toggle-label">
            启用划词翻译（全局快捷键触发）
          </span>
        </div>
      </div>

      <div className="form-group">
        <label className="form-label">划词翻译快捷键</label>
        <input
          type="text"
          className="form-input"
          value={localSettings?.hotkeys?.select || "Ctrl+Alt+X"}
          onChange={(e) =>
            handleSettingChange("hotkeys", {
              ...localSettings?.hotkeys,
              select: e.target.value,
            })
          }
        />
      </div>

      <div className="form-group">
        <label className="form-label">使用说明</label>
        <p className="form-hint">
          在任意应用中选中文字，按上方快捷键（默认 Ctrl+Alt+X），翻译结果将显示在主窗口的翻译页。
          原理为模拟复制并读取剪贴板（随后自动还原），少数禁用复制功能的应用不支持。
          原文/译文语言可在截图翻译菜单或基本设置的"默认翻译方向"调整。
        </p>
      </div>
    </div>
  );

  const renderAbout = () => (
    <div className="settings-section">
      <h3 className="section-title">关于</h3>
      <div className="about-info">
        <p><strong>智能翻译软件</strong></p>
        <p>版本: 0.1.0</p>
        <p>技术栈: Tauri + React + Rust</p>
        <p>功能: 离线翻译、在线翻译、截图翻译、划词翻译</p>
        <p>支持平台: Windows 11, Android (计划中)</p>
      </div>
    </div>
  );

  const renderContent = () => {
    switch (activeMenu) {
      case "basic":
        return renderBasicSettings();
      case "translation":
        return renderTranslationSettings();
      case "screenshot":
        return renderScreenshotSettings();
      case "hotkeys":
        return renderHotkeySettings();
      case "select":
        // 划词翻译：增删改即时生效，无需外层"保存设置"按钮
        return renderSelectSettings();
      case "about":
        return renderAbout();
      case "terms":
        // 术语管理：增删改即时生效，无需外层"保存设置"按钮
        return <TermsPanel />;
      case "history":
        // 翻译历史：即时生效，无需外层"保存设置"按钮
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
      
      {activeMenu !== "about" && activeMenu !== "terms" && activeMenu !== "history" && (
        <div className="settings-actions">
          <button className="button primary" onClick={handleSave}>
            保存设置
          </button>
          <button className="button secondary" onClick={() => setLocalSettings(settings)}>
            恢复默认
          </button>
        </div>
      )}
    </div>
  );
};

export default SettingsPanel;