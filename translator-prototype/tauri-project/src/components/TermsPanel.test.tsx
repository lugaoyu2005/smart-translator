import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import TermsPanel from "./TermsPanel";

// TermsPanel 冒烟+功能测试（后端整体 mock）
const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const SCHEME = {
  id: "scheme_1",
  name: "方案一",
  enabled_packages: ["builtin_symbols"],
  enabled_entry_keys: [],
};

beforeEach(() => {
  invokeMock.mockImplementation((cmd: string) => {
    switch (cmd) {
      case "list_terms":
        return Promise.resolve([
          {
            source: "server",
            translations: ["服务器"],
            priority: [1],
            usage_count: 0,
          },
        ]);
      case "list_packages":
        return Promise.resolve([
          {
            id: "builtin_symbols",
            name: "常用符号纠错（内置）",
            mappings: [["乛", "→"]],
            builtin: true,
          },
        ]);
      case "list_schemes":
        return Promise.resolve([SCHEME]);
      case "get_active_scheme":
        return Promise.resolve("scheme_1");
      default:
        return Promise.resolve(null);
    }
  });
});

describe("TermsPanel（术语管理页）", () => {
  it("Smoke: 渲染方案/术语/术语包", async () => {
    render(<TermsPanel />);
    expect(screen.getByText("术语管理")).toBeInTheDocument();
    expect(await screen.findByText("方案一")).toBeInTheDocument();
    expect(await screen.findByText("server")).toBeInTheDocument();
    expect(await screen.findByText("常用符号纠错（内置）")).toBeInTheDocument();
  });

  it("可用性+功能: 空输入点添加 → 内联提示且不调用后端", async () => {
    render(<TermsPanel />);
    fireEvent.click(await screen.findByRole("button", { name: "添加" }));
    expect(await screen.findByText("源词和译法不能为空")).toBeInTheDocument();
    expect(invokeMock).not.toHaveBeenCalledWith(
      "add_term",
      expect.anything()
    );
  });

  it("功能: 填写源词译法后添加 → 调用后端并清空输入", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "add_term") return Promise.resolve(null);
      if (cmd === "list_terms" && invokeMock.mock.calls.filter((c) => c[0] === "add_term").length > 0) {
        return Promise.resolve([]);
      }
      return defaultReply(cmd);
    });
    render(<TermsPanel />);
    await screen.findByText("server");
    fireEvent.change(screen.getByPlaceholderText("源词（原文）"), {
      target: { value: "bug" },
    });
    fireEvent.change(screen.getByPlaceholderText("译法"), {
      target: { value: "缺陷" },
    });
    fireEvent.click(screen.getByRole("button", { name: "添加" }));
    // 添加成功后刷新列表（新列表为空）且输入框清空
    await waitFor(() => expect(screen.queryByText("server")).toBeNull());
    const src = screen.getByPlaceholderText("源词（原文）") as HTMLInputElement;
    const dst = screen.getByPlaceholderText("译法") as HTMLInputElement;
    expect(src.value).toBe("");
    expect(dst.value).toBe("");
    expect(invokeMock).toHaveBeenCalledWith(
      "add_term",
      expect.objectContaining({ source: "bug", translation: "缺陷" })
    );
  });

  it("功能: 搜索源词即时过滤", async () => {
    render(<TermsPanel />);
    await screen.findByText("server");
    fireEvent.change(screen.getByPlaceholderText("搜索源词..."), {
      target: { value: "不匹配" },
    });
    expect(screen.queryByText("server")).toBeNull();
  });
});

function defaultReply(cmd: string) {
  switch (cmd) {
    case "list_terms":
      return Promise.resolve([
        { source: "server", translations: ["服务器"], priority: [1], usage_count: 0 },
      ]);
    case "list_packages":
      return Promise.resolve([]);
    case "list_schemes":
      return Promise.resolve([SCHEME]);
    case "get_active_scheme":
      return Promise.resolve("scheme_1");
    default:
      return Promise.resolve(null);
  }
}
