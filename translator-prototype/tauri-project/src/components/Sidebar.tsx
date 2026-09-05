import React from "react";
import "./Sidebar.css";

interface MenuItem {
  id: string;
  label: string;
  icon: string;
}

interface SidebarProps {
  menuItems: MenuItem[];
  activeMenu: string;
  onMenuChange: (menu: string) => void;
  onStartScreenshot: () => void;
}

const Sidebar: React.FC<SidebarProps> = ({
  menuItems,
  activeMenu,
  onMenuChange,
  onStartScreenshot,
}) => {
  return (
    <div className="sidebar">
      <h2 className="sidebar-title">设置菜单</h2>

      <div className="menu-list">
        {menuItems.map((item) => (
          <div
            key={item.id}
            className={`menu-item ${activeMenu === item.id ? "active" : ""}`}
            onClick={() => onMenuChange(item.id)}
          >
            <span className="menu-icon">{item.icon}</span>
            <span className="menu-label">{item.label}</span>
          </div>
        ))}
      </div>

      <div className="sidebar-footer">
        <button className="screenshot-button" onClick={onStartScreenshot}>
          📷 启动截图翻译
        </button>
      </div>
    </div>
  );
};

export default Sidebar;