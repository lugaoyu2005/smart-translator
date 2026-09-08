import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import TranslatePanel from "./TranslatePanel";

// 黑盒/集成测试：把 @tauri-apps 后端整体 mock 掉，组件当黑盒测
const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const LANGS = [
  { code: "auto", name: "Auto Detect", native_name: "自动检测" },
  { code: "en", name: "English", native_name: "English" },
  { code: "zh", name: "Chinese", native_name: "中文" },
];

const ENGINES = [
  { name: "百度翻译", engine_type: "Baidu", available: true, configured: true },
  { name: "离线翻译", engine_type: "Offline", available: true, configured: true },
];

function setupInvoke() {
  invokeMock.mockImplementation((cmd: string) => {
    switch (cmd) {
      case "get_supported_languages":
        return Promise.resolve(LANGS);
      case "list_engines":
        return Promise.resolve(ENGINES);
      case "get_network_status":
        return Promise.resolve({ is_online: true, connection_type: "在线", latency: 25 });
      default:
        return Promise.resolve(null);
    }
  });
}

beforeEach(() => {
  setupInvoke();
});

describe("TranslatePanel（翻译测试页）", () => {
  it("Smoke: 渲染输入区/语言选择器/引擎徽章/网络状态", async () => {
    render(<TranslatePanel />);
    expect(screen.getByPlaceholderText(/输入要翻译的文本/)).toBeInTheDocument();
    expect(await screen.findByText("离线翻译")).toBeInTheDocument();
    // 可用性：网络状态可见
    expect(await screen.findByText(/在线（延迟 25ms）/)).toBeInTheDocument();
  });

  it("功能+可用性: 空输入点翻译 → 提示且不调用后端", async () => {
    render(<TranslatePanel />);
    fireEvent.click(await screen.findByRole("button", { name: "翻译" }));
    expect(await screen.findByText("请输入要翻译的内容")).toBeInTheDocument();
    const translateCalls = invokeMock.mock.calls.filter((c) => c[0] === "translate_text");
    expect(translateCalls).toHaveLength(0);
  });

  it("功能: 输入文本翻译 → 展示结果与所用引擎", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "translate_text") {
        return Promise.resolve({
          translated_text: "你好世界",
          engine_used: "百度翻译",
          from: "auto",
          to: "zh",
        });
      }
      return setupDefault(cmd);
    });
    render(<TranslatePanel />);
    const input = screen.getByPlaceholderText(/输入要翻译的文本/);
    fireEvent.change(input, { target: { value: "hello world" } });
    fireEvent.click(screen.getByRole("button", { name: "翻译" }));
    expect(await screen.findByText("你好世界")).toBeInTheDocument();
    expect(screen.getByText("引擎：百度翻译")).toBeInTheDocument();
    // 集成：后端收到的参数完整
    expect(invokeMock).toHaveBeenCalledWith(
      "translate_text",
      expect.objectContaining({ text: "hello world", from: "auto", to: "zh", engine: null })
    );
  });

  it("可用性: 翻译进行中按钮禁用并显示'翻译中...'", async () => {
    let resolveTranslate!: (v: unknown) => void;
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "translate_text") {
        return new Promise((res) => {
          resolveTranslate = res;
        });
      }
      return setupDefault(cmd);
    });
    render(<TranslatePanel />);
    fireEvent.change(screen.getByPlaceholderText(/输入要翻译的文本/), {
      target: { value: "abc" },
    });
    fireEvent.click(screen.getByRole("button", { name: "翻译" }));
    const btn = await screen.findByRole("button", { name: "翻译中..." });
    expect(btn).toBeDisabled();
    resolveTranslate({ translated_text: "译", engine_used: "e", from: "auto", to: "zh" });
    await screen.findByText("译");
  });

  it("集成失败路径: 后端错误字符串直接展示给用户", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "translate_text") return Promise.reject("有道翻译失败: 签名校验失败（错误码202）");
      return setupDefault(cmd);
    });
    render(<TranslatePanel />);
    fireEvent.change(screen.getByPlaceholderText(/输入要翻译的文本/), {
      target: { value: "abc" },
    });
    fireEvent.click(screen.getByRole("button", { name: "翻译" }));
    expect(
      await screen.findByText("有道翻译失败: 签名校验失败（错误码202）")
    ).toBeInTheDocument();
  });

  it("集成失败路径: 非字符串错误回落通用提示", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "translate_text") return Promise.reject({ code: 500 });
      return setupDefault(cmd);
    });
    render(<TranslatePanel />);
    fireEvent.change(screen.getByPlaceholderText(/输入要翻译的文本/), {
      target: { value: "abc" },
    });
    fireEvent.click(screen.getByRole("button", { name: "翻译" }));
    expect(
      await screen.findByText("翻译失败，请检查网络或API配置")
    ).toBeInTheDocument();
  });

  it("可用性: 源语言为自动检测时交换按钮禁用且有说明", async () => {
    render(<TranslatePanel />);
    const swap = await screen.findByRole("button", { name: "⇄" });
    expect(swap).toBeDisabled();
    expect(swap.getAttribute("title")).toContain("自动检测时无法交换");
  });

  it("灰盒: 点击引擎徽章 → 指定该引擎调用后端", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "translate_text") {
        return Promise.resolve({
          translated_text: "译文",
          engine_used: "离线翻译",
          from: "auto",
          to: "zh",
        });
      }
      return setupDefault(cmd);
    });
    render(<TranslatePanel />);
    fireEvent.click(await screen.findByText("离线翻译"));
    fireEvent.change(screen.getByPlaceholderText(/输入要翻译的文本/), {
      target: { value: "hi" },
    });
    fireEvent.click(screen.getByRole("button", { name: "翻译" }));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        "translate_text",
        expect.objectContaining({ engine: "离线翻译" })
      );
    });
  });

  it("可用性: 无可用引擎时显示占位徽章", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_engines") return Promise.resolve([]);
      return setupDefault(cmd);
    });
    render(<TranslatePanel />);
    expect(await screen.findByText("无可用引擎")).toBeInTheDocument();
  });
});

// 未被用例覆写的命令走默认实现
function setupDefault(cmd: string) {
  switch (cmd) {
    case "get_supported_languages":
      return Promise.resolve(LANGS);
    case "list_engines":
      return Promise.resolve(ENGINES);
    case "get_network_status":
      return Promise.resolve({ is_online: false, connection_type: "离线", latency: null });
    default:
      return Promise.resolve(null);
  }
}
