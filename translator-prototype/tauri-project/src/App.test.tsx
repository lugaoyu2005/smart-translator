import { render, screen, fireEvent, waitFor, act, within } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import App from "./App";

// App 集成/回归测试：mock Tauri 后端（invoke/listen/窗口API）
const { invokeMock, listeners } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  listeners: {} as Record<string, (e: { payload?: unknown }) => void>,
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((event: string, handler: (e: { payload?: unknown }) => void) => {
    listeners[event] = handler;
    return Promise.resolve(() => {
      delete listeners[event];
    });
  }),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    show: vi.fn().mockResolvedValue(undefined),
    unminimize: vi.fn().mockResolvedValue(undefined),
    setFocus: vi.fn().mockResolvedValue(undefined),
  }),
}));

const SETTINGS = {
  autostart: true,
  default_translation_direction: "auto->zh",
  online_apis: ["baidu"],
  offline_engine: "marian",
  screenshot_components: ["engine", "lang", "close"],
  hotkeys: { translate: "Ctrl+Alt+T", screenshot: "Ctrl+Alt+S" },
};

function defaultInvoke(cmd: string) {
  switch (cmd) {
    case "get_app_settings":
      return Promise.resolve(SETTINGS);
    case "get_supported_languages":
      return Promise.resolve([
        { code: "auto", name: "Auto", native_name: "自动检测" },
        { code: "en", name: "English", native_name: "English" },
        { code: "zh", name: "Chinese", native_name: "中文" },
      ]);
    case "list_engines":
      return Promise.resolve([]);
    case "get_network_status":
      return Promise.resolve({ is_online: true, connection_type: "在线", latency: 10 });
    case "save_app_settings":
      return Promise.resolve(null);
    case "translate_text":
      return Promise.resolve({
        translated_text: "你好",
        engine_used: "测试引擎",
        from: "en",
        to: "zh",
      });
    default:
      return Promise.resolve(null);
  }
}

beforeEach(() => {
  for (const k of Object.keys(listeners)) delete listeners[k];
  invokeMock.mockImplementation(defaultInvoke);
});

afterEach(() => {
  vi.useRealTimers();
});

describe("App（主框架）", () => {
  it("回归: 设置加载完成后'加载中…'必须消失（历史bug：永远停在加载态）", async () => {
    render(<App />);
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("get_app_settings");
      expect(screen.queryByText("加载中…")).toBeNull();
    });
    // 侧边栏正常渲染
    expect(screen.getByText("设置菜单")).toBeInTheDocument();
  });

  it("回归: 设置加载失败时给出失败提示（而不是永远加载）", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_app_settings") return Promise.reject("后端不可用");
      return defaultInvoke(cmd);
    });
    render(<App />);
    expect(await screen.findByText(/设置加载失败：后端不可用/)).toBeInTheDocument();
  });

  it("功能: 保存设置 → 调用后端并弹出'设置已保存'，2秒后自动消失", async () => {
    // shouldAdvanceTime：真实时间继续走（findBy*可用），同时支持手动快进
    vi.useFakeTimers({ shouldAdvanceTime: true });
    render(<App />);
    const saveBtn = await screen.findByRole("button", { name: "保存设置" });
    fireEvent.click(saveBtn);
    expect(await screen.findByText("设置已保存")).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith(
      "save_app_settings",
      expect.objectContaining({ settings: expect.objectContaining({ autostart: true }) })
    );
    act(() => {
      vi.advanceTimersByTime(2100);
    });
    expect(screen.queryByText("设置已保存")).toBeNull();
  });

  it("集成: translate-selection事件 → 跳转翻译页并自动翻译划词文本", async () => {
    render(<App />);
    await screen.findByText("设置菜单");
    await act(async () => {
      listeners["translate-selection"]({ payload: "selected text" });
    });
    // 跳转到翻译测试页
    expect(await screen.findByRole("heading", { level: 3, name: "翻译测试" })).toBeInTheDocument();
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        "translate_text",
        expect.objectContaining({ text: "selected text" })
      );
    });
  });

  it("可用性: 侧边栏全部8个菜单项可见", async () => {
    render(<App />);
    await screen.findByText("设置菜单");
    const sidebar = screen.getByText("设置菜单").closest<HTMLElement>(".sidebar")!;
    for (const label of [
      "基础设置",
      "翻译引擎",
      "翻译测试",
      "截图翻译",
      "术语管理",
      "翻译历史",
      "快捷键",
      "关于",
    ]) {
      expect(within(sidebar).getByText(label)).toBeInTheDocument();
    }
  });

  it("集成: open-translate-page事件 → 跳转翻译页", async () => {
    render(<App />);
    await screen.findByText("设置菜单");
    await act(async () => {
      listeners["open-translate-page"]({});
    });
    expect(
      await screen.findByRole("heading", { level: 3, name: "翻译测试" })
    ).toBeInTheDocument();
  });
});
