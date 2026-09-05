import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import Sidebar from "./components/Sidebar";
import SettingsPanel from "./components/SettingsPanel";
import TranslatePanel from "./components/TranslatePanel";
import "./App.css";

function App() {
  const [activeMenu, setActiveMenu] = useState("basic");
  const [settings, setSettings] = useState<any>(null);

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
      alert("设置已保存");
    } catch (error) {
      console.error("Failed to save settings:", error);
      alert("保存设置失败");
    }
  };

  // 启动截图翻译：显示独立的截图窗口
  const handleStartScreenshot = async () => {
    try {
      const win = await WebviewWindow.getByLabel("screenshot");
      if (win) {
        await win.show();
        await win.setFocus();
      } else {
        alert("截图窗口未初始化");
      }
    } catch (e) {
      console.error("打开截图窗口失败:", e);
      alert("打开截图窗口失败");
    }
  };

  // 菜单：翻译测试作为单独入口
  const menuItems = [
    { id: "basic", label: "基本设置", icon: "⚙️" },
    { id: "translation", label: "翻译引擎", icon: "🌐" },
    { id: "translate", label: "翻译测试", icon: "🔤" },
    { id: "screenshot", label: "截图翻译", icon: "📷" },
    { id: "select", label: "划词翻译", icon: "🖱️" },
    { id: "terms", label: "术语管理", icon: "📚" },
    { id: "hotkeys", label: "快捷键", icon: "⌨️" },
    { id: "advanced", label: "高级设置", icon: "🔧" },
    { id: "about", label: "关于", icon: "ℹ️" },
  ];

  return (
    <div className="app-container">
      <Sidebar
        menuItems={menuItems}
        activeMenu={activeMenu}
        onMenuChange={setActiveMenu}
        onStartScreenshot={handleStartScreenshot}
      />

      <main className="main-content">
        {activeMenu === "translate" ? (
          <TranslatePanel />
        ) : (
          <SettingsPanel
            activeMenu={activeMenu}
            settings={settings}
            onSaveSettings={handleSaveSettings}
          />
        )}
      </main>
    </div>
  );
}

export default App;