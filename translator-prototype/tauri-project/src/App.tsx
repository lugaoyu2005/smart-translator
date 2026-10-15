import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import Sidebar from "./components/Sidebar";
import SettingsPanel from "./components/SettingsPanel";
import TranslatePanel from "./components/TranslatePanel";
import "./App.css";

/** 菜单页 id（与 menuItems 一致）：用于「打开时进入的页面」设置校验 */
const MENU_IDS = [
  "basic",
  "translation",
  "translate",
  "screenshot",
  "terms",
  "history",
  "hotkeys",
  "about",
];

function App() {
  const [activeMenu, setActiveMenu] = useState("basic");
  const [settings, setSettings] = useState<any>(null);
  // 保存设置成功提示（GUI弹窗：仅标题，点击遮罩关闭，0.5秒后自动消失）
  const [savedTip, setSavedTip] = useState<{ ok: boolean; msg: string } | null>(null);
  // 设置页草稿（受控）：保存/恢复按钮位于侧边栏底部
  const [localSettings, setLocalSettings] = useState<any>(null);

  useEffect(() => {
    if (!savedTip) return;
    const t = setTimeout(() => setSavedTip(null), 500);
    return () => clearTimeout(t);
  }, [savedTip]);
  // 划词捕获的待翻译文本（seq 递增保证重复文本也能触发）
  const [pendingSelection, setPendingSelection] = useState<{
    text: string;
    seq: number;
  } | null>(null);
  const selectionSeq = useRef(0);

  // 划词翻译：后台捕获选中文本后，显示主窗口并跳转翻译页自动翻译
  useEffect(() => {
    const un = listen<string>("translate-selection", (e) => {
      selectionSeq.current += 1;
      setPendingSelection({ text: e.payload, seq: selectionSeq.current });
      setActiveMenu("translate");
    });
    return () => {
      un.then((f) => f());
    };
  }, []);

  useEffect(() => {
    // 加载应用设置
    loadSettings();
  }, []);

  // 截图窗口⚙按钮：显示主窗口并跳转截图设置页（截图会话保留）
  useEffect(() => {
    const un = listen("open-screenshot-settings", async () => {
      setActiveMenu("screenshot");
      try {
        const cur = getCurrentWindow();
        await cur.show();
        await cur.unminimize();
        await cur.setFocus();
      } catch (e) {
        console.error("显示主窗口失败:", e);
      }
    });
    return () => {
      un.then((f) => f());
    };
  }, []);

  // 全局快捷键 Ctrl+Alt+T / 托盘：Rust侧已显示主窗口，这里跳转翻译测试页
  useEffect(() => {
    const un = listen("open-translate-page", () => {
      setActiveMenu("translate");
    });
    return () => {
      un.then((f) => f());
    };
  }, []);

  // 已保存设置的镜像：给下面的事件回调用，避免把订阅绑在 settings 上反复重建
  const savedSettingsRef = useRef<any>(null);
  useEffect(() => {
    savedSettingsRef.current = settings;
  }, [settings]);

  // 点 × 隐藏到托盘 = 放弃未保存的改动（Rust 侧 hide 之前发的事件）：
  // 草稿直接回滚到"上次保存"，不静默留着，语义与"未保存的改动在退出程序后丢弃"一致
  useEffect(() => {
    const un = listen("main-window-hidden", () => {
      setLocalSettings(savedSettingsRef.current);
    });
    return () => {
      un.then((f) => f());
    };
  }, []);

  const loadSettings = async () => {
    try {
      const settings = await invoke<any>("get_app_settings");
      setSettings(settings);
      setLocalSettings(settings); // 同步设置页草稿（缺失会导致页面永远“加载中…”）
      // 启动页：按设置进入指定菜单（默认基础设置）
      const startup = String(settings?.startup_menu || "basic");
      if (MENU_IDS.includes(startup)) setActiveMenu(startup);
    } catch (error) {
      console.error("Failed to load settings:", error);
      setSavedTip({ ok: false, msg: `设置加载失败：${error}` });
    }
  };

  // 保存当前草稿（侧边栏底部按钮）
  const saveDraft = async () => {
    if (!localSettings) return;
    try {
      await invoke("save_app_settings", { settings: localSettings });
      setSettings(localSettings);
      setSavedTip({ ok: true, msg: "设置已保存" });
    } catch (error) {
      console.error("Failed to save settings:", error);
      setSavedTip({ ok: false, msg: `保存失败：${error}` });
    }
  };
  const resetDraft = () => setLocalSettings(settings);

  // 草稿与"已保存配置"是否不一致。
  // 重要：截图翻译流水线在识别/翻译那一刻是**从 settings.json 重新读**的
  // （screenshot.rs: load_settings()），所以未保存的改动对截图完全不可见。
  // 用户反馈"我选了有道 OCR，结果还在用 RapidOCR"就是踩了这个坑：
  // 加上可见提示，避免把"改了但没保存"误判成"设置不生效"。
  const dirty =
    !!localSettings &&
    !!settings &&
    JSON.stringify(localSettings) !== JSON.stringify(settings);

  // 菜单：翻译测试作为单独入口
  const menuItems = [
    { id: "basic", label: "基础设置", icon: "⚙️" },
    { id: "translation", label: "翻译引擎", icon: "🌐" },
    { id: "translate", label: "翻译测试", icon: "🔤" },
    { id: "screenshot", label: "截图翻译", icon: "📷" },
    { id: "terms", label: "术语管理", icon: "📚" },
    { id: "history", label: "翻译历史", icon: "🕘" },
    { id: "hotkeys", label: "快捷键", icon: "⌨️" },
    { id: "about", label: "关于", icon: "ℹ️" },
  ];

  return (
    <div className="app-container">
      <Sidebar
          menuItems={menuItems}
          activeMenu={activeMenu}
          onMenuChange={setActiveMenu}
          footer={
            // 全菜单 tab 均显示保存/恢复：草稿是全局单份，一次保存即应用全部设置
            localSettings ? (
              <div className="sidebar-save">
                <button
                  className={`button primary ${dirty ? "dirty" : ""}`}
                  onClick={saveDraft}
                  title="保存全部设置；未保存的改动在退出程序后丢弃（点 × 隐藏到托盘也会丢弃未保存的改动）"
                >
                  保存设置
                </button>
                <button className="button secondary" onClick={resetDraft}>
                  恢复默认
                </button>
              </div>
            ) : null
          }
        />

      <main className="main-content">
        {activeMenu === "translate" ? (
          <TranslatePanel
            pendingText={pendingSelection}
            onConsumed={() => setPendingSelection(null)}
          />
        ) : localSettings ? (
          <SettingsPanel
            activeMenu={activeMenu}
            settings={settings}
            localSettings={localSettings}
            onLocalChange={setLocalSettings}
          />
        ) : (
          <div className="settings-section">
            <h3 className="section-title">加载中…</h3>
          </div>
        )}
      </main>

      {/* 保存成功提示：仅标题居中，点击遮罩任意处关闭 */}
      {savedTip && (
        <div className="saved-mask" onClick={() => setSavedTip(null)}>
          <div className="saved-modal" onClick={(e) => e.stopPropagation()}>
            <div className={`saved-title ${savedTip.ok ? "" : "failed"}`}>
              {savedTip.msg}
            </div>
          </div>
        </div>
      )}
    </div>
  );
};

export default App;