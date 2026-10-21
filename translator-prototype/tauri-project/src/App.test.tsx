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

  it("可用性: 有未保存修改时给出可见提示（截图翻译只读已保存配置）", async () => {
    render(<App />);
    await screen.findByText("设置菜单");
    // 初始：草稿 == 已保存，按钮不高亮；也确认旧版文字提示已彻底移除
    expect(screen.getByRole("button", { name: "保存设置" }).className).not.toContain("dirty");
    expect(screen.queryByText("● 有未保存的修改")).toBeNull();
    // 切到"翻译引擎"页（OCR 引擎也在这个 tab），点一下"有道 OCR"整行
    // 注意：菜单项与"打开时进入的页面"下拉里的 option 同名，必须限定在侧边栏内取
    const sidebar = screen.getByText("设置菜单").closest<HTMLElement>(".sidebar")!;
    fireEvent.click(within(sidebar).getByText("翻译引擎"));
    fireEvent.click(await screen.findByText("有道 OCR"));
    // 有未保存改动 → 保存按钮高亮（不再用文字提示）
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "保存设置" }).className).toContain("dirty");
    });
    expect(screen.queryByText("● 有未保存的修改")).toBeNull();
    // 保存后不再高亮
    fireEvent.click(screen.getByRole("button", { name: "保存设置" }));
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "保存设置" }).className).not.toContain("dirty");
    });
  });

  it("功能: 截图菜单改引擎（app-settings-updated）→ 回填设置页「当前翻译源」", async () => {
    // 回归：save_app_settings 此前只 emit 给 screenshot 窗口，主窗口没有监听，
    // 导致截图菜单里改的引擎/语言永远回不到设置页（还会被设置页旧草稿覆盖）。
    // baseline：百度+有道都已启用，当前翻译源=百度（有道必须可选，否则 value 对不上）
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_app_settings")
        return Promise.resolve({
          ...SETTINGS,
          online_apis: ["baidu", "youdao"],
          current_engine: "百度翻译",
        });
      return defaultInvoke(cmd);
    });
    render(<App />);
    await screen.findByText("设置菜单");
    const sidebar = screen.getByText("设置菜单").closest<HTMLElement>(".sidebar")!;
    fireEvent.click(within(sidebar).getByText("翻译引擎"));
    // 初始：当前翻译源 = 百度翻译
    const sel0 = screen.getByLabelText("当前翻译源") as HTMLSelectElement;
    expect(sel0.value).toBe("百度翻译");

    // 前提：主窗口必须真的订阅了这个事件（回归点之一）
    expect(Object.keys(listeners)).toContain("app-settings-updated");
    // 模拟：截图窗口把 current_engine 改成「有道智云」并落盘，Rust 发来事件
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_app_settings")
        // 有道必须在该设置里是"已启用"，否则下拉里没有这个选项，value 对不上
        return Promise.resolve({
          ...SETTINGS,
          online_apis: ["baidu", "youdao"],
          current_engine: "有道智云",
        });
      return defaultInvoke(cmd);
    });
    await act(async () => {
      listeners["app-settings-updated"]({});
    });

    // 设置页草稿应被按字段回填成新引擎（此前这里永远是旧值）
    await waitFor(() => {
      expect(
        (screen.getByLabelText("当前翻译源") as HTMLSelectElement).value
      ).toBe("有道智云");
    });
  });

  it("功能: 点×隐藏到托盘（main-window-hidden）→ 丢弃未保存的草稿", async () => {
    render(<App />);
    await screen.findByText("设置菜单");
    const sidebar = screen.getByText("设置菜单").closest<HTMLElement>(".sidebar")!;
    fireEvent.click(within(sidebar).getByText("翻译引擎"));
    fireEvent.click(await screen.findByText("有道 OCR"));
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "保存设置" }).className).toContain("dirty");
    });
    // Rust 侧在 hide 之前发 main-window-hidden：草稿立刻回滚到上次保存状态
    await act(async () => {
      listeners["main-window-hidden"]({});
    });
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "保存设置" }).className).not.toContain("dirty");
    });
  });

  it("功能: startup_menu=hotkeys → 首屏停留在快捷键页", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_app_settings")
        return Promise.resolve({ ...SETTINGS, startup_menu: "hotkeys" });
      return defaultInvoke(cmd);
    });
    render(<App />);
    expect(
      await screen.findByRole("heading", { level: 3, name: "快捷键" })
    ).toBeInTheDocument();
  });

  it("可用性: 非法 startup_menu 回落基础设置页", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_app_settings")
        return Promise.resolve({ ...SETTINGS, startup_menu: "nope" });
      return defaultInvoke(cmd);
    });
    render(<App />);
    expect(
      await screen.findByRole("heading", { level: 3, name: "基础设置" })
    ).toBeInTheDocument();
  });

  it("功能: 保存/恢复默认在全部菜单 tab 可用（含关于页）", async () => {
    render(<App />);
    await screen.findByText("设置菜单");
    const sidebar = screen.getByText("设置菜单").closest<HTMLElement>(".sidebar")!;
    expect(within(sidebar).getByRole("button", { name: "保存设置" })).toBeInTheDocument();
    fireEvent.click(within(sidebar).getByText("关于"));
    expect(await screen.findByText("版本: 0.4.21")).toBeInTheDocument();
    // 此前关于/术语/历史/翻译测试四个 tab 不显示保存栏（用户反馈）
    expect(within(sidebar).getByRole("button", { name: "保存设置" })).toBeInTheDocument();
    expect(within(sidebar).getByRole("button", { name: "恢复默认" })).toBeInTheDocument();
  });
});
