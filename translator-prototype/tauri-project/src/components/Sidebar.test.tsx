import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import Sidebar from "./Sidebar";

// Sidebar 属纯展示组件：单元测试无需 mock 任何后端

const menuItems = [
  { id: "basic", label: "基础设置", icon: "⚙️" },
  { id: "translation", label: "翻译引擎", icon: "🌐" },
  { id: "translate", label: "翻译测试", icon: "🔤" },
];

describe("Sidebar", () => {
  it("Smoke: 渲染全部菜单项（图标+文字）", () => {
    render(
      <Sidebar menuItems={menuItems} activeMenu="basic" onMenuChange={() => {}} />
    );
    expect(screen.getByText("设置菜单")).toBeInTheDocument();
    for (const item of menuItems) {
      expect(screen.getByText(item.label)).toBeInTheDocument();
      expect(screen.getByText(item.icon)).toBeInTheDocument();
    }
  });

  it("可用性: 当前菜单高亮（active类），非当前不高亮", () => {
    render(
      <Sidebar menuItems={menuItems} activeMenu="translation" onMenuChange={() => {}} />
    );
    const active = screen.getByText("翻译引擎").closest(".menu-item");
    const inactive = screen.getByText("基础设置").closest(".menu-item");
    expect(active?.className).toContain("active");
    expect(inactive?.className).not.toContain("active");
  });

  it("功能: 点击菜单项回调对应ID", () => {
    const onMenuChange = vi.fn();
    render(
      <Sidebar menuItems={menuItems} activeMenu="basic" onMenuChange={onMenuChange} />
    );
    fireEvent.click(screen.getByText("翻译测试"));
    expect(onMenuChange).toHaveBeenCalledWith("translate");
  });

  it("单元: footer 插槽内容原样渲染", () => {
    render(
      <Sidebar
        menuItems={menuItems}
        activeMenu="basic"
        onMenuChange={() => {}}
        footer={<button>保存设置</button>}
      />
    );
    expect(screen.getByRole("button", { name: "保存设置" })).toBeInTheDocument();
  });
});
