import { render, act } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import ScreenshotWindow from "./ScreenshotWindow";

/**
 * 回归锁：截图窗口风暴（BUG报告-截图翻译窗口风暴.md）
 *
 * 根因：getCurrentWebviewWindow() 每次调用返回新对象，若组件把它直接
 * 用在渲染体并放进 useEffect 依赖（[win]），effect 内 setState 会导致
 * 无限渲染循环 → 每轮循环 invoke preheat_screenshot / get_app_settings，
 * 驱动窗口反复 show/hide，抢占全系统前台焦点。
 *
 * 本测试模拟真实 API 行为（每次调用返回新实例），渲染后静置一段真实时间，
 * 断言后端调用次数有界。若循环回归，invoke 次数将随时间无限增长。
 */

const { invokeMock, calls } = vi.hoisted(() => {
  const calls = {
    setSize: vi.fn().mockResolvedValue(undefined),
    setPosition: vi.fn().mockResolvedValue(undefined),
    setIgnoreCursorEvents: vi.fn().mockResolvedValue(undefined),
    hide: vi.fn().mockResolvedValue(undefined),
    show: vi.fn().mockResolvedValue(undefined),
    setFocus: vi.fn().mockResolvedValue(undefined),
    setAlwaysOnTop: vi.fn().mockResolvedValue(undefined),
    outerPosition: vi.fn().mockResolvedValue({ x: 0, y: 0 }),
  };
  const unlisten = () => {};
  return {
    invokeMock: vi.fn(),
    calls: {
      ...calls,
      onFocusChanged: vi.fn().mockResolvedValue(unlisten),
      onResized: vi.fn().mockResolvedValue(unlisten),
      onMoved: vi.fn().mockResolvedValue(unlisten),
    },
  };
});

// 关键：像真实 API 一样，每次调用都返回“新对象”（共享同一组方法 spy）
vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: () => ({ ...calls }),
}));

vi.mock("@tauri-apps/api/window", () => ({
  currentMonitor: vi.fn().mockResolvedValue({
    name: "main",
    scaleFactor: 1,
    position: { x: 0, y: 0 },
    size: { width: 1920, height: 1080 },
  }),
}));

vi.mock("@tauri-apps/api/dpi", () => ({
  PhysicalSize: class {
    constructor(
      public width: number,
      public height: number
    ) {}
  },
  PhysicalPosition: class {
    constructor(
      public x: number,
      public y: number
    ) {}
  },
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
  emit: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

function defaultInvoke(cmd: string) {
  switch (cmd) {
    case "get_app_settings":
      return Promise.resolve({
        screenshot_components: ["engine", "lang", "close"],
        overlay_mode: "dark",
        overlay_expand: false,
        default_translation_direction: "auto->zh",
        hotkeys: { reverse: "Ctrl+Alt+B" },
        current_engine: "百度翻译",
      });
    case "list_engines":
      return Promise.resolve([
        { name: "百度翻译", engine_type: "Baidu", available: true, configured: true },
      ]);
    default:
      return Promise.resolve(null);
  }
}

beforeEach(() => {
  invokeMock.mockImplementation(defaultInvoke);
});

async function settle(ms: number) {
  await act(async () => {
    await new Promise((r) => setTimeout(r, ms));
  });
}

describe("ScreenshotWindow 窗口风暴回归锁", () => {
  it("Smoke: idle 态可渲染，初始化链路完整", async () => {
    render(<ScreenshotWindow />);
    await settle(120);
    expect(invokeMock).toHaveBeenCalledWith("get_app_settings");
    expect(invokeMock).toHaveBeenCalledWith("list_engines");
    expect(calls.setSize).toHaveBeenCalled();
  });

  it("回归: 连续重渲染后后端调用次数必须有界（win 依赖不得造成无限循环）", async () => {
    const { rerender } = render(<ScreenshotWindow />);
    await settle(80);
    for (let i = 0; i < 5; i++) {
      rerender(<ScreenshotWindow />);
      await settle(20);
    }
    await settle(120);

    const count = (cmd: string) =>
      invokeMock.mock.calls.filter((c) => c[0] === cmd).length;

    // 有界性：初始化 + StrictMode 无关的少量重试（历史bug下这两个数字会
    // 随时间无限增长：45秒内 show/hide 30次、每秒数十轮 invoke）
    expect(count("get_app_settings")).toBeLessThanOrEqual(4);
    expect(count("list_engines")).toBeLessThanOrEqual(4);
    expect(count("preheat_screenshot")).toBeLessThanOrEqual(4);
  });
});
