import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import Sidebar from "./components/Sidebar";
import SettingsPanel from "./components/SettingsPanel";
import TranslatePanel from "./components/TranslatePanel";
import "./App.css";

function App() {
  const [activeMenu, setActiveMenu] = useState("basic");
  const [settings, setSettings] = useState<any>(null);
  // 保存设置成功提示（GUI弹窗：仅标题，点击遮罩关闭，2秒后自动消失）
  const [savedTip, setSavedTip] = useState<{ ok: boolean; msg: string } | null>(null);

  useEffect(() => {
    if (!savedTip) return;
    const t = setTimeout(() => setSavedTip(null), 2000);
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

  const loadSettings = async () => {
    try {
      const settings = await invoke("get_app_settings");
      setSettings(settings);
    } catch (error) {
      console.error("Failed to load settings:", error);
    }
  };

  const handleSaveSettings = async (newSettings: any) => {
    try {
      await invoke("save_app_settings", { settings: newSettings });
      setSettings(newSettings);
      setSavedTip({ ok: true, msg: "设置已保存" });
    } catch (error) {
      console.error("Failed to save settings:", error);
      setSavedTip({ ok: false, msg: `保存失败：${error}` }); // 与成功同款 GUI 弹窗
    }
  };

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
      <Sidebar menuItems={menuItems} activeMenu={activeMenu} onMenuChange={setActiveMenu} />

      <main className="main-content">
        {activeMenu === "translate" ? (
          <TranslatePanel
            pendingText={pendingSelection}
            onConsumed={() => setPendingSelection(null)}
          />
        ) : (
          <SettingsPanel
            activeMenu={activeMenu}
            settings={settings}
            onSaveSettings={handleSaveSettings}
          />
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