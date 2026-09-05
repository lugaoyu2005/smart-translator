# 双击固定为"原生样式选中整段"，删除开关与蓝色提示栏

## 确认的目标
- 双击块 = **原生选区选中整段文字**（实心蓝、贴字高亮，与拖动/原生双击同一条渲染路径）
- Ctrl+双击 = 整段文字加入多选片段（与 Ctrl+拖动同机制）
- 删除"双击只选一个词"开关（开/关两种状态都不再保留，只留整段行为）
- 删除顶部蓝色"已选N段"提示栏

## 改动清单
1. **删除配置 `dblclick_select_word` 全链路**
   - `system.rs`：AppSettings 字段 + Default 默认值
   - `ScreenshotWindow.tsx`：ShotSettings 接口、DEFAULT_SETTINGS、两处 get_app_settings 映射
   - `SettingsPanel.tsx`：开关 UI
2. **双击逻辑固定为整段选中**（恢复为单一分支）：
   - mousedown：`e.detail >= 2 → preventDefault()`（抑制原生选词快闪）
   - dblclick：Ctrl/Shift → 整段加入 pickedFragments；普通双击 → `selectNodeContents` 整段选中
3. **删除顶部蓝色提示栏**：`picked-badge` JSX + 对应 CSS（pickedFragments 累积逻辑保留，复制仍按序合并）
4. **顺带修正**：删除提交说明与实现不一致的隐患（选词模式分支整体移除）

## 应急预案
若整段选中在你机器上高亮样式仍与拖动不一致（程序化选区渲染差异），下一步改用 CSS `user-select: all` 纯原生方案（代价：块内拖选部分文字失效），届时再确认取舍。

## 收尾
- `cargo check` + `npm run build` 验证
- git 提交 + 更新 `当前开发进度.txt`