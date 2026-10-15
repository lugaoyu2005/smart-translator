import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { readFileSync } from "node:fs";
import path from "node:path";
import SettingsPanel from "./SettingsPanel";

// 样式回归用：直接读源码 CSS（vitest 会把 css 导入 stub 掉，拿不到原文）；
// vitest 的 cwd 固定为项目根，按相对路径读即可
const translatePanelCss = readFileSync(
  path.resolve(process.cwd(), "src/components/TranslatePanel.css"),
  "utf8"
);

// 黑盒测试：mock 掉 Tauri 后端（invoke/listen），只观察界面输出
const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

const BASE = {
  autostart: true,
  startup_show_window: false,
  startup_menu: "basic",
  default_translation_direction: "auto->zh",
  online_apis: ["baidu"],
  hotkeys: {
    screenshot: "LCtrl+A",
    translate: "Ctrl+Alt+T",
    select: "None",
    reverse: "None",
  },
  screenshot_components: ["engine", "lang", "copy", "close", "settings"],
  screenshot_components_disabled: [],
};

function renderPanel(activeMenu: string, settings: Record<string, unknown> = BASE) {
  invokeMock.mockReset();
  invokeMock.mockResolvedValue(null);
  const onLocalChange = vi.fn();
  const utils = render(
    <SettingsPanel
      activeMenu={activeMenu}
      settings={settings}
      localSettings={settings}
      onLocalChange={onLocalChange}
    />
  );
  return { ...utils, onLocalChange };
}

const CUSTOM = {
  ...BASE,
  online_apis: ["custom"],
  custom_providers: [
    { id: "p1", name: "deepseek", base_url: "https://a/v1", api_key: "k", model: "m" },
    { id: "p2", name: "deepseek", base_url: "https://b/v1", api_key: "k", model: "m" },
  ],
};

describe("SettingsPanel（设置页）", () => {
  it("改进1: 默认引擎徽标不再用蓝色系（与手动指定统一为绿色描边）", () => {
    const at = translatePanelCss.indexOf(".engine-badge.default-engine");
    expect(at).toBeGreaterThan(-1);
    const rule = translatePanelCss.slice(at, translatePanelCss.indexOf("}", at));
    expect(rule).toContain("#4caf50"); // 绿描边
    expect(rule).not.toContain("#64b5f6"); // 旧的蓝描边
    expect(rule).not.toContain("#e3f2fd"); // 旧的蓝底
  });

  it("功能: 自定义供应商重名 → 行内报错且输入框标红", () => {
    const { container } = renderPanel("translation", CUSTOM);
    expect(screen.getByText(/供应商名称重复/)).toBeInTheDocument();
    expect(container.querySelectorAll(".form-input.input-error").length).toBe(2);
  });

  it("功能: 覆盖层默认显示提供 原文/译文/上次 三个选项", () => {
    renderPanel("screenshot");
    const sel = screen.getByLabelText("覆盖层默认显示") as HTMLSelectElement;
    expect(sel.value).toBe("original");
    expect(
      Array.from(sel.querySelectorAll("option")).map((o) => o.textContent)
    ).toEqual(["原文", "译文", "上次（沿用截图菜单里最后一次的选择）"]);
  });

  it("可用性: 基础设置已移除「系统右键菜单」说明块", () => {
    renderPanel("basic");
    expect(screen.queryByText("系统右键菜单")).toBeNull();
    expect(screen.queryByText(/已自动启用/)).toBeNull();
  });

  it("功能: 基础设置可切换「打开时进入的页面」", () => {
    const { onLocalChange } = renderPanel("basic");
    const select = screen.getByLabelText("打开时进入的页面") as HTMLSelectElement;
    // 受控组件：初始值来自 localSettings
    expect(select.value).toBe("basic");
    // 8 个菜单页都可选
    expect(select.querySelectorAll("option")).toHaveLength(8);
    fireEvent.change(select, { target: { value: "screenshot" } });
    expect(onLocalChange).toHaveBeenCalledWith(
      expect.objectContaining({ startup_menu: "screenshot" })
    );
  });

  it("回归: 点击 OCR 引擎整行文字即可切换（不必精准点到小开关）", () => {
    const { onLocalChange } = renderPanel("translation");
    fireEvent.click(screen.getByText("有道 OCR"));
    expect(onLocalChange).toHaveBeenCalledWith(
      expect.objectContaining({ ocr_engine: "youdao" })
    );
  });

  it("可用性: 快捷键下方显示左右侧别反馈", () => {
    renderPanel("hotkeys");
    // LCtrl+A → 明确说明仅左侧触发
    expect(
      screen.getByText(/左 Ctrl 专用（仅该侧按键触发）/)
    ).toBeInTheDocument();
    // Ctrl+Alt+T → 说明双侧通用
    expect(
      screen.getByText(/Ctrl\/Alt 两侧均可触发（未区分左右）/)
    ).toBeInTheDocument();
    // None → 明确说明未设置
    expect(screen.getAllByText("未设置（不注册全局热键）")).toHaveLength(2);
  });
});
