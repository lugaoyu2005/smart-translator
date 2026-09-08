import { render, screen, fireEvent, within } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import HistoryPanel, { truncate, filterHistory } from "./HistoryPanel";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const LONG = "这是一段非常长的翻译内容".repeat(20); // 220字符

const ENTRIES = [
  {
    id: 1,
    time: "2026-09-09 10:00:00",
    from: "en",
    to: "zh",
    source: "hello world",
    translation: "你好世界",
    engine: "百度翻译",
  },
  {
    id: 2,
    time: "2026-09-09 11:00:00",
    from: "en",
    to: "zh",
    source: LONG,
    translation: "<img src=x onerror=alert(1)>",
    engine: "有道智云",
  },
];

beforeEach(() => {
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "list_history") return Promise.resolve(ENTRIES);
    return Promise.resolve(null);
  });
});

describe("HistoryPanel（翻译历史页）", () => {
  it("Smoke: 渲染历史列表（时间/引擎/方向）", async () => {
    render(<HistoryPanel />);
    expect(await screen.findByText("hello world")).toBeInTheDocument();
    expect(screen.getByText("百度翻译")).toBeInTheDocument();
    expect(screen.getByText("2026-09-09 11:00:00")).toBeInTheDocument();
  });

  it("可用性: 空历史显示友好空态", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_history") return Promise.resolve([]);
      return Promise.resolve(null);
    });
    render(<HistoryPanel />);
    expect(await screen.findByText("暂无翻译记录")).toBeInTheDocument();
  });

  it("单元（经UI）: 超长文本摘要截断加省略号", async () => {
    render(<HistoryPanel />);
    const body = await screen.findByText(/这是一段非常长的翻译内容/);
    expect(body.textContent!.endsWith("…")).toBe(true);
    expect(body.textContent!.length).toBeLessThanOrEqual(81);
  });

  it("安全: HTML注入内容按纯文本渲染（React转义）", async () => {
    const { container } = render(<HistoryPanel />);
    const payload = await screen.findByText(/<img src=x onerror=alert\(1\)>/);
    expect(payload).toBeInTheDocument();
    expect(container.querySelector("img")).toBeNull();
    expect(container.querySelector("script")).toBeNull();
  });

  it("功能: 关键词搜索过滤原文/译文", async () => {
    render(<HistoryPanel />);
    await screen.findByText("hello world");
    // "世界"只命中第一条的译文"你好世界"
    fireEvent.change(screen.getByPlaceholderText("搜索原文/译文…"), {
      target: { value: "世界" },
    });
    expect(screen.queryByText(/这是一段非常长的翻译内容/)).toBeNull();
    expect(screen.getByText("hello world")).toBeInTheDocument();
  });

  it("可用性: 搜索无结果显示'无匹配记录'", async () => {
    render(<HistoryPanel />);
    await screen.findByText("hello world");
    fireEvent.change(screen.getByPlaceholderText("搜索原文/译文…"), {
      target: { value: "不存在的关键词xyz" },
    });
    expect(screen.getByText("无匹配记录")).toBeInTheDocument();
  });

  it("功能: 删除单条调用后端并刷新", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_history") {
        return Promise.resolve(invokeMock.mock.calls.filter((c) => c[0] === "delete_history_entry").length > 0 ? [] : ENTRIES);
      }
      if (cmd === "delete_history_entry") return Promise.resolve(null);
      return Promise.resolve(null);
    });
    render(<HistoryPanel />);
    const card = (await screen.findByText("hello world")).closest<HTMLElement>(".history-card")!;
    fireEvent.click(within(card).getByText("删除"));
    await screen.findByText("暂无翻译记录");
    expect(invokeMock).toHaveBeenCalledWith("delete_history_entry", { id: 1 });
  });

  it("可用性+功能: 清空需确认；取消则不动后端", async () => {
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(false);
    render(<HistoryPanel />);
    await screen.findByText("hello world");
    fireEvent.click(screen.getByText("清空全部"));
    expect(confirmSpy).toHaveBeenCalledOnce();
    expect(invokeMock).not.toHaveBeenCalledWith("clear_history");
    confirmSpy.mockRestore();
  });

  it("功能: 确认后清空调用后端", async () => {
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_history") return Promise.resolve([]);
      if (cmd === "clear_history") return Promise.resolve(null);
      return Promise.resolve(null);
    });
    render(<HistoryPanel />);
    await screen.findByText("清空全部");
    fireEvent.click(screen.getByText("清空全部"));
    expect(await screen.findByText("暂无翻译记录")).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("clear_history");
    confirmSpy.mockRestore();
  });

  it("功能: 复制原文走剪贴板", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });
    render(<HistoryPanel />);
    const card = (await screen.findByText("hello world")).closest<HTMLElement>(".history-card")!;
    fireEvent.click(within(card).getByTitle("复制原文"));
    expect(writeText).toHaveBeenCalledWith("hello world");
  });
});

// ===== 纯函数单元测试（truncate / filterHistory） =====

const entry = (over: Partial<{ source: string; translation: string }>) => ({
  id: 1,
  time: "",
  from: "en",
  to: "zh",
  source: "",
  translation: "",
  engine: "",
  ...over,
});

describe("truncate（单元）", () => {
  it("换行替换为空格", () => {
    expect(truncate("a\nb\nc", 100)).toBe("a b c");
  });
  it("未超长原样返回", () => {
    expect(truncate("短文本", 80)).toBe("短文本");
    expect(truncate("x".repeat(80), 80)).toBe("x".repeat(80));
  });
  it("超长截断加省略号（81=80+…）", () => {
    const out = truncate("y".repeat(200), 80);
    expect(out.length).toBe(81);
    expect(out.endsWith("…")).toBe(true);
  });
});

describe("filterHistory（单元/性能/猴子）", () => {
  const data = [
    entry({ source: "Hello", translation: "你好" }),
    entry({ source: "world", translation: "世界" }),
  ];
  it("空关键词返回全部", () => {
    expect(filterHistory(data, "")).toHaveLength(2);
    expect(filterHistory(data, "   ")).toHaveLength(2);
  });
  it("大小写不敏感匹配原文或译文", () => {
    expect(filterHistory(data, "hello")).toHaveLength(1);
    expect(filterHistory(data, "世界")).toHaveLength(1);
    expect(filterHistory(data, "zzz")).toHaveLength(0);
  });

  it("性能: 2万条过滤+10万字符截断在1秒内", () => {
    const big = Array.from({ length: 20_000 }, (_, i) =>
      entry({ source: `src ${i}`, translation: `译 ${i}` })
    );
    const bigStr = "长".repeat(100_000);
    const start = performance.now();
    filterHistory(big, "19999");
    truncate(bigStr, 80);
    const elapsed = performance.now() - start;
    expect(elapsed).toBeLessThan(1000);
  });

  it("猴子: 随机怪异关键词与条目不崩溃且结果为子集", () => {
    const pool = ["\u{0}", "😀", "<script>", "  ", "中\n文", "%s%d", "'OR'1'='1"];
    let seed = 123;
    const rng = () => {
      seed ^= seed << 13;
      seed ^= seed >> 7;
      seed ^= seed << 17;
      return seed;
    };
    for (let i = 0; i < 500; i++) {
      const kw = pool[Math.abs(rng()) % pool.length];
      const list = Array.from({ length: 10 }, () =>
        entry({ source: pool[Math.abs(rng()) % pool.length], translation: "t" })
      );
      const out = filterHistory(list, kw);
      expect(out.length).toBeLessThanOrEqual(list.length);
      const t = truncate(kw.repeat(50), 80);
      expect(t.length).toBeLessThanOrEqual(81);
    }
  });
});
