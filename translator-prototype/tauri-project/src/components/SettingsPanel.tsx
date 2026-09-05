import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import "./SettingsPanel.css";
import TermsPanel from "./TermsPanel";

// 菜单组组件定义（工具箱：设置里可增删、排序）
const GROUP_COMPONENTS = [
  { id: "engine", label: "翻译引擎" },
  { id: "copy", label: "复制（原/译）" },
  { id: "close", label: "关闭" },
  { id: "settings", label: "设置" },
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
        <label className="form-label">网络状态</label>
        <div className="network-status">
          <div className={`status-indicator ${true ? "online" : "offline"}`}></div>
          <span>在线状态 - 使用在线翻译</span>
        </div>
      </div>
    </div>
  );

  const renderTranslationSettings = () => (
    <div className="settings-section">
      <h3 className="section-title">翻译引擎</h3>
      
      <div className="form-group">
        <label className="form-label">离线翻译引擎</label>
        <select
          className="form-select"
          value={localSettings?.offline_engine || "marian"}
          onChange={(e) => handleSettingChange("offline_engine", e.target.value)}
        >
          <option value="marian">Marian/NMT (推荐)</option>
          <option value="argos">Argos Translate</option>
          <option value="disabled">禁用离线翻译</option>
        </select>
      </div>
      
      <div className="form-group">
        <label className="form-label">在线翻译API</label>
        <div className="toggle-group">
          <label className="toggle-switch">
            <input
              type="checkbox"
              checked={localSettings?.online_apis?.includes("baidu") || false}
              onChange={(e) => {
                const apis = localSettings?.online_apis || [];
                if (e.target.checked) {
                  handleSettingChange("online_apis", [...apis, "baidu"]);
                } else {
                  handleSettingChange("online_apis", apis.filter((api: string) => api !== "baidu"));
                }
              }}
            />
            <span className="toggle-slider"></span>
          </label>
          <span className="toggle-label">百度翻译 (免费额度)</span>
        </div>

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
        
        <div className="toggle-group">
          <label className="toggle-switch">
            <input
              type="checkbox"
              checked={localSettings?.online_apis?.includes("youdao") || false}
              onChange={(e) => {
                const apis = localSettings?.online_apis || [];
                if (e.target.checked) {
                  handleSettingChange("online_apis", [...apis, "youdao"]);
                } else {
                  handleSettingChange("online_apis", apis.filter((api: string) => api !== "youdao"));
                }
              }}
            />
            <span className="toggle-slider"></span>
          </label>
          <span className="toggle-label">有道智云 (免费额度)</span>
        </div>

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
          <label className="form-label">OCR引擎</label>
          <select
            className="form-select"
            value={localSettings?.ocr_engine || "windows"}
            onChange={(e) => handleSettingChange("ocr_engine", e.target.value)}
          >
            <option value="windows">Windows内置OCR (推荐，无需安装)</option>
            <option value="tesseract">Tesseract OCR</option>
            <option value="baidu">百度OCR (有免费额度)</option>
          </select>
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
          <label className="form-label">覆盖色块背景色</label>
          <input
            type="color"
            className="color-input"
            value={localSettings?.overlay_bg_color || "#ffffff"}
            onChange={(e) =>
              handleSettingChange("overlay_bg_color", e.target.value)
            }
          />
        </div>

        <div className="form-group">
          <label className="form-label">
            覆盖色块不透明度：
            {Math.round((localSettings?.overlay_opacity ?? 0.95) * 100)}%
          </label>
          <input
            type="range"
            className="opacity-range"
            min="0.5"
            max="1"
            step="0.05"
            value={localSettings?.overlay_opacity ?? 0.95}
            onChange={(e) =>
              handleSettingChange("overlay_opacity", parseFloat(e.target.value))
            }
          />
        </div>

        <div className="form-group">
          <div className="toggle-group">
            <label className="toggle-switch">
              <input
                type="checkbox"
                checked={localSettings?.overlay_transparent || false}
                onChange={(e) =>
                  handleSettingChange("overlay_transparent", e.target.checked)
                }
              />
              <span className="toggle-slider"></span>
            </label>
            <span className="toggle-label">
              无背景模式（覆盖块透明，仅显示翻译文字 + 白色光晕）
            </span>
          </div>
        </div>

        <div className="form-group">
          <div className="toggle-group">
            <label className="toggle-switch">
              <input
                type="checkbox"
                checked={localSettings?.overlay_bg_fit_text || false}
                onChange={(e) =>
                  handleSettingChange("overlay_bg_fit_text", e.target.checked)
                }
              />
              <span className="toggle-slider"></span>
            </label>
            <span className="toggle-label">
              背景随文字自适应（有背景时色块贴合翻译文字，消除留白）
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
      case "about":
        return renderAbout();
      case "terms":
        // 术语管理：增删改即时生效，无需外层"保存设置"按钮
        return <TermsPanel />;
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
      
      {activeMenu !== "about" && activeMenu !== "terms" && (
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