# BUG 报告:截图翻译功能引发全系统焦点闪烁

> 时间:2026-09-09
> 报告来源:系统排障实录(ZCode 会话)
> 影响:`smart-translator.exe` 的**截图翻译**功能运行期间,整台电脑出现全局性焦点闪烁

## 现象

截图翻译功能启用期间(无需使用,只要功能在跑):

1. **任何前台应用的窗口外阴影忽隐忽现**(激活状态被反复夺走)
2. **任务栏图标在"选中/未选中"两个状态间来回跳**
3. **输入法指示器在中/英之间不停切换**(每次焦点切换都会重置 IME 状态)
4. 焦点停在桌面时上述现象不明显(桌面无阴影、无任务栏按钮)

用户主观描述为"电脑屏幕一闪一闪",极易被误判为显卡驱动/DWM/图标缓存问题。

## 实测证据

用 `EnumWindows` 以 300ms 间隔对顶层窗口做生死普查,45 秒内采样 142 次:

```text
05:46:44.528 WIN_BORN  hwnd=66586 pid=12784 proc=smart-translator title=截图翻译
05:46:44.856 WIN_DIED  hwnd=66586 pid=12784 proc=smart-translator title=截图翻译
05:46:45.184 WIN_BORN  hwnd=66586 pid=12784 proc=smart-translator title=截图翻译
05:46:45.499 WIN_DIED  hwnd=66586 pid=12784 proc=smart-translator title=截图翻译
...(45 秒内该窗口创建/销毁约 30 次,周期 300~800ms)
```

同时 `GetForegroundWindow` 高密度采样(250ms)显示焦点在 ZCode / 微信 / 桌面 之间反复被夺走又弹回,每次跳变 200~800ms——与上面窗口的生死周期一一对应。

## 根因

截图翻译的每一轮截图循环都在**新建一个可激活的顶层可见窗口来承载截图覆盖层,用完立即销毁**:

```
循环 { CreateWindow(截图翻译覆盖层) → 截屏 → DestroyWindow }
```

每轮 Create 都会:
- 激活该窗口 → 抢走系统前台焦点(WM_ACTIVATE 链)
- 任务栏为它建按钮再销毁 → 任务栏图标闪烁
- 焦点切换 → 目标窗口 IME 状态重置 → 中/英指示器乱跳
- 前台窗口失/复激活 → DWM 重绘窗口阴影 → 阴影闪烁

## 修复要求

1. **覆盖层窗口禁止激活**:创建时加扩展样式 `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW`,显示用 `ShowWindow(SW_SHOWNOACTIVATE)`,彻底不参与焦点竞争。
2. **窗口复用**:覆盖层窗口只创建一次并隐藏待命,每轮截图复用(Show → capture → Hide),不要 create → destroy 循环。
3. 顺带检查:截图触发若用了全局热键/定时器,确认没有把"无操作的空轮询"也当成有效截图执行(否则不翻译时也在闪)。
4. 附带建议(非本 bug 主因):`settings.json`/`history.json` 目前写在 exe 同目录且每 10~20 秒落盘一次,若程序放在桌面等特殊目录会持续触发 Explorer 刷新。建议持久化路径迁移到 `%APPDATA%\com.smart-translator.app\`(WebView2 数据已在此,保持一致)。

## 排障过程中的误判记录(避免重复踩坑)

- 图标/缩略图缓存损坏确实是**另一个独立问题**,已通过删除 `%LOCALAPPDATA%\Microsoft\Windows\Explorer\iconcache_*.db` + `thumbcache_*.db` 并重启 Explorer 修复。
- 禁用 MPO(`HKLM\SOFTWARE\Microsoft\Windows\Dwm\OverlayTestMode=5`)与本 bug 无关,但对该 AMD 平台无害。
- 曾经怀疑过迅雷/百度网盘/有道/荣耀驱动/MuMu 虚拟显示/微信抢焦点,均被 A/B 测试或窗口普查排除。
- 验证手法备忘:焦点问题用 `GetForegroundWindow` 高频采样 + `EnumWindows` 窗口生死普查,不要只看事件日志(本 bug 在事件日志里完全无痕)。

## 复核结论(2026-09-09,代码级根因确诊)

### 本报告"根因"一节的修正

代码里**不存在**"每轮 CreateWindow → DestroyWindow":`screenshot` 窗口是 `tauri.conf.json` 配置的**常驻窗口**(复用早已满足)。普查里同一个 hwnd=66586 反复"生/死",说明脚本测到的是**同一窗口的显示/隐藏**(EnumWindows 只枚举可见窗口),不是建/毁窗口。

### 真正根因:截图窗口前端的无限渲染循环

`src/screenshot/ScreenshotWindow.tsx`(修复前):

1. `const win = getCurrentWebviewWindow()` —— Tauri 该 API **每次调用返回新对象**(实现为 `new WebviewWindow(...)`);
2. 启动 `useEffect` 依赖数组写 `[win]` → 每次渲染后 `win` 身份必变 → effect 必重跑;
3. effect 内无条件 `setSettings(新对象)`/`setEngines(新数组)` → 必然再渲染 → **无限循环**,每轮 invoke `get_app_settings`/`list_engines`/`setSize`/`setPosition`;
4. 修复前每轮循环还会调 `preheat_screenshot`(当时无去重):`show()` 用 SW_SHOW **会激活窗口抢前台**,600ms 后 `hide()` —— 正好对应普查观测的 **300~800ms 周期**(600ms 延迟 ± 采样误差)与全部伴生症状(任务栏图标跳、IME 重置、窗口阴影闪)。隐藏态 rAF 挂起、显示瞬间集中放行,使该振荡自我维持。

### 修复内容(已落地)

| 项 | 状态 |
|---|---|
| 根因修复:`win` 改 `useMemo(() => getCurrentWebviewWindow(), [])` 稳定实例;启动 effect 依赖改 `[]` | ✅ 本次 |
| 症状级加固(前一轮已加):`preheat_screenshot` 加 `PREHEAT_DONE` 去重;启动时给截图窗口置 `WS_EX_NOACTIVATE\|WS_EX_TOOLWINDOW`(显示/点击永不抢前台,触发时由 `SetForegroundWindow` 程序化激活);删除启动时主线程预热 | ✅ 已在 |
| 回归锁:`src/screenshot/ScreenshotWindow.test.tsx` 模拟真实 API"每次调用返回新对象",断言连续重渲染后 `get_app_settings`/`preheat_screenshot` 调用次数有界(≤4)——循环复发时该测试必然失败 | ✅ 本次 |
| 报告要求 #3(空轮询截图):核实不存在,`poll_esc` 仅 150ms 读一次 ESC 物理键状态,无窗口操作 | ✅ 无需修 |
| 报告要求 #4(设置文件迁 `%APPDATA%`):合理,但属独立改进 | ⏳ 待另开任务 |

验证:前端 42/42 通过(含新回归锁)、`tsc --noEmit` 通过、`cargo check` 通过。**待用户按上一节标准 GUI 复测**(开启截图翻译期间:任务栏图标稳定、前台窗口阴影稳定、输入法指示不乱跳)后关闭本报告。

## 当前状态

- `smart-translator.exe` 已被终止,系统闪烁随之消失(可直接复验)
- 开机自启项仍指向 `D:\projects\翻译\translator-prototype\tauri-project\src-tauri\target\release\smart-translator.exe`(路径已是新位置,是否保留自启待用户决定)
- 修复后请在**开启截图翻译**状态下复测:任务栏图标稳定、前台窗口阴影稳定、输入法指示不乱跳,方视为通过


## 修复验证（2026-09-09 06:10，ZCode 会话）

对照报告修复要求逐项落实后,以报告同款手法(EnumWindows 300ms 采样 + GetForegroundWindow)复测 45 秒:

- 截图翻译窗口 144 次采样全程 vis=False(隐藏待命,零闪现)
- 前台被应用占据次数:36 次 → **0 次**
- 模拟按键完整链路: 触发显示+夺前台 ✓ / ESC 隐藏+释放前台 ✓ / 重复触发 ✓

### 落实的修复
1. WS_EX_NOACTIVATE 已加(与 TOOLWINDOW 并列)——窗口任何显隐/点击不再参与焦点竞争;触发截图时 SetForegroundWindow 程序化激活不受影响
2. 窗口复用确认:截图窗口为静态单例,无创建/销毁循环;此前报告捕获的 show/hide 循环经排查为启动预热在主屏全屏显隐所致,已改为移出屏幕外预热(前端首帧触发 preheat_screenshot)
3. 空轮询确认:poll_esc 仅读键状态不触发截图;另新增后端 ESC 监视线程(30ms GetAsyncKeyState)双保险退出
4. %APPDATA% 迁移建议:记录为待办,未在本轮实施
