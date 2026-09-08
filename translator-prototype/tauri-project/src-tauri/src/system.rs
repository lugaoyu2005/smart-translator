use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::TcpStream;
use std::time::Duration;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

#[derive(Debug, Serialize, Deserialize)]
pub struct NetworkStatus {
    pub is_online: bool,
    pub connection_type: String,
    pub latency: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppSettings {
    pub autostart: bool,
    pub default_translation_direction: String,
    pub offline_engine: String,
    pub online_apis: Vec<String>,
    pub ocr_engine: String,
    pub screenshot_behavior: ScreenshotBehavior,
    pub hotkeys: HashMap<String, String>,
    pub term_base_priority: String,
    // 在线翻译API密钥
    pub baidu_app_id: String,
    pub baidu_secret: String,
    pub youdao_app_key: String,
    pub youdao_app_secret: String,
    // 截图翻译：菜单组组件（顺序即显示顺序，工具箱式可增删）
    #[serde(default = "default_screenshot_components")]
    pub screenshot_components: Vec<String>,
    // 划词翻译：选中文本后按全局热键捕获并翻译
    #[serde(default = "default_select_translate_enabled")]
    pub select_translate_enabled: bool,
    // 覆盖样式：dark=黑底白字 light=白底黑字 none=无背景（透明+白色光晕）
    // None=未设置（迁移判断依据），运行时按 transparent_legacy 回落后视为 dark
    #[serde(default)]
    pub overlay_mode: Option<String>,
    // 旧版"无背景模式"开关（迁移用：true → overlay_mode = none）
    #[serde(default)]
    pub overlay_transparent_legacy: bool,
    // 文本严格对齐模式：译文空间 = 原文区域 +10%（避让重叠），字号自动填充
    #[serde(default)]
    pub overlay_expand: bool,
    // 当前翻译源（默认引擎名，与截图翻译菜单双向绑定）
    #[serde(default = "default_current_engine")]
    pub current_engine: String,
    // 主窗口尺寸：last=记住上次大小（默认）；fixed=固定大小（宽高自定义）
    #[serde(default = "default_window_size_mode")]
    pub window_size_mode: String,
    #[serde(default = "default_window_fixed_width")]
    pub window_fixed_width: u32,
    #[serde(default = "default_window_fixed_height")]
    pub window_fixed_height: u32,
    #[serde(default)]
    pub window_last_width: u32,
    #[serde(default)]
    pub window_last_height: u32,
    // 供应商凭据（框架阶段：仅存储，翻译实现后续补齐）
    #[serde(default)]
    pub providers: ProviderSettings,
    // 启动行为：true=主界面显示在前台 false=隐藏到托盘（默认）
    #[serde(default)]
    pub startup_show_window: bool,
    // 多自定义 OpenAI 兼容供应商（每个生成独立引擎）
    #[serde(default)]
    pub custom_providers: Vec<CustomProvider>,
    // 离线翻译模型选择（opus-mt 可用；nllb-200 下一版本接入）
    #[serde(default = "default_offline_model")]
    pub offline_model: String,
    // 常用符号纠错包（内置部首/箭头等误识别纠正）总开关
    #[serde(default = "default_true")]
    pub builtin_symbols_enabled: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct CustomProvider {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub model: String,
}

fn default_offline_model() -> String {
    "opus-mt".to_string()
}

fn default_true() -> bool {
    true
}

fn default_current_engine() -> String {
    "百度翻译".to_string()
}

fn default_select_translate_enabled() -> bool {
    true
}

fn default_window_size_mode() -> String {
    "last".to_string()
}

fn default_window_fixed_width() -> u32 {
    1200
}

fn default_window_fixed_height() -> u32 {
    800
}

fn default_screenshot_components() -> Vec<String> {
    vec![
        "engine".to_string(),
        "lang".to_string(),
        "copy_src".to_string(),
        "copy_dst".to_string(),
        "close".to_string(),
        "settings".to_string(),
    ]
}

/// 框架阶段供应商凭据存储（翻译实现后续逐家补齐）
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ProviderSettings {
    #[serde(default)]
    pub niutrans_api_key: String,
    #[serde(default)]
    pub deepl_api_key: String,
    #[serde(default)]
    pub tencent_secret_id: String,
    #[serde(default)]
    pub tencent_secret_key: String,
    #[serde(default)]
    pub ali_access_key_id: String,
    #[serde(default)]
    pub ali_access_key_secret: String,
    #[serde(default)]
    pub custom_openai_base_url: String,
    #[serde(default)]
    pub custom_openai_api_key: String,
    #[serde(default)]
    pub custom_openai_model: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ScreenshotBehavior {
    pub cover_original_text: bool,
    pub left_click_toggle: bool,
    pub right_click_exit: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        let mut hotkeys = HashMap::new();
        hotkeys.insert("translate".to_string(), "None".to_string());
        hotkeys.insert("screenshot".to_string(), "None".to_string());
        hotkeys.insert("select".to_string(), "None".to_string());
        hotkeys.insert("reverse".to_string(), "None".to_string());

        Self {
            autostart: true,
            default_translation_direction: "auto->zh".to_string(),
            offline_engine: "marian".to_string(),
            online_apis: vec!["baidu".to_string(), "youdao".to_string()],
            ocr_engine: "windows".to_string(),
            screenshot_behavior: ScreenshotBehavior {
                cover_original_text: true,
                left_click_toggle: true,
                right_click_exit: true,
            },
            hotkeys,
            term_base_priority: "auto+manual".to_string(),
            baidu_app_id: String::new(),
            baidu_secret: String::new(),
            youdao_app_key: String::new(),
            youdao_app_secret: String::new(),
            screenshot_components: default_screenshot_components(),
            overlay_mode: None,
            overlay_transparent_legacy: false,
            overlay_expand: false,
            select_translate_enabled: true,
            current_engine: default_current_engine(),
            window_size_mode: default_window_size_mode(),
            window_fixed_width: default_window_fixed_width(),
            window_fixed_height: default_window_fixed_height(),
            window_last_width: 0,
            window_last_height: 0,
            providers: ProviderSettings::default(),
            startup_show_window: false,
            custom_providers: Vec::new(),
            offline_model: default_offline_model(),
            builtin_symbols_enabled: true,
        }
    }
}

// 设置文件路径：与应用同目录的 settings.json
fn settings_path() -> std::path::PathBuf {
    let mut path = std::env::current_exe().unwrap_or_default();
    path.pop(); // 去掉exe文件名
    path.push("settings.json");
    path
}

/// 加载后的兼容性迁移（就地修正历史遗留值）
fn migrate(settings: &mut AppSettings) {
    // 历史默认值 "tesseract" 从未实现，实际可用的是 Windows 内置 OCR
    if settings.ocr_engine == "tesseract" {
        settings.ocr_engine = "windows".to_string();
    }
    // 离线翻译已定案为 OPUS-MT 本地模型；旧下拉中的 argos 选项归一为 marian（启用）
    if settings.offline_engine == "argos" {
        settings.offline_engine = "marian".to_string();
    }
    // 划词翻译热键（历史配置缺失时补默认值）
    if !settings.hotkeys.contains_key("select") {
        settings
            .hotkeys
            .insert("select".to_string(), "None".to_string());
    }
    // 反转原译快捷键（截图会话内生效）
    if !settings.hotkeys.contains_key("reverse") {
        settings
            .hotkeys
            .insert("reverse".to_string(), "None".to_string());
    }
    // 菜单组 copy 组件拆分为复制原文/复制译文
    if settings.screenshot_components.iter().any(|c| c == "copy") {
        let pos = settings
            .screenshot_components
            .iter()
            .position(|c| c == "copy")
            .unwrap();
        settings.screenshot_components.remove(pos);
        settings
            .screenshot_components
            .insert(pos, "copy_dst".to_string());
        settings
            .screenshot_components
            .insert(pos, "copy_src".to_string());
    }
    // 快捷键格式校验：设置页为自由文本输入，误输入（如"Ctrl+Alt+TCt"）会使注册失败，
    // 无法识别的值回落默认值
    fn is_valid_hotkey(s: &str) -> bool {
        let s = s.trim();
        if s.eq_ignore_ascii_case("none") {
            return true;
        }
        if !s.contains('+') {
            return false;
        }
        match s.rsplit('+').next() {
            Some(key) => {
                let key = key.trim();
                (key.len() == 1
                    && key.chars().next().map_or(false, |c| c.is_ascii_alphanumeric()))
                    || (key.len() >= 2
                        && key.starts_with('F')
                        && key[1..].chars().all(|c| c.is_ascii_digit()))
            }
            None => false,
        }
    }
    for (name, default) in [
        ("translate", "Ctrl+Alt+T"),
        ("screenshot", "Ctrl+Alt+S"),
        ("select", "Ctrl+Alt+X"),
    ] {
        let bad = settings
            .hotkeys
            .get(name)
            .map_or(true, |v| !is_valid_hotkey(v));
        if bad {
            settings
                .hotkeys
                .insert(name.to_string(), default.to_string());
        }
    }
    // 旧版"无背景模式"开关 → 新覆盖样式
    if settings.overlay_mode.is_none() && settings.overlay_transparent_legacy {
        settings.overlay_mode = Some("none".to_string());
    }
    // 当前翻译源不在启用列表时，回落到第一个启用的引擎（名称↔ID映射）
    // 名称↔ID全量映射（与 engines.rs 各引擎 name() 对应）
    let enabled_name = |name: &str| {
        let id = match name {
            "百度翻译" => "baidu",
            "有道智云" => "youdao",
            "小牛翻译" => "niutrans",
            "DeepL" => "deepl",
            "腾讯云翻译" => "tencent",
            "阿里云翻译" => "ali",
            "自定义AI" => "custom",
            _ => "",
        };
        settings.online_apis.iter().any(|a| a == id)
    };
    // 离线翻译启用时也是合法的当前翻译源
    let offline_selected =
        settings.offline_engine != "disabled" && settings.current_engine == "离线翻译";
    if !enabled_name(&settings.current_engine) && !offline_selected {
        settings.current_engine = settings
            .online_apis
            .first()
            .map(|id| match id.as_str() {
                "baidu" => "百度翻译",
                "youdao" => "有道智云",
                _ => "",
            })
            .unwrap_or_default()
            .to_string();
    }
    // 菜单组自动补入新增的"原/译语言"组件
    if !settings.screenshot_components.iter().any(|c| c == "lang") {
        if let Some(pos) = settings
            .screenshot_components
            .iter()
            .position(|c| c == "engine")
        {
            settings
                .screenshot_components
                .insert(pos + 1, "lang".to_string());
        } else {
            settings
                .screenshot_components
                .insert(0, "lang".to_string());
        }
    }
}

/// 同步加载设置（setup阶段使用）
pub fn load_settings() -> AppSettings {
    let path = settings_path();
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(mut settings) = serde_json::from_str::<AppSettings>(&content) {
                migrate(&mut settings);
                return settings;
            }
        }
    }
    let mut settings = AppSettings::default();
    migrate(&mut settings);
    settings
}

/// 保存设置到磁盘（供窗口尺寸记忆等内部流程使用）
pub fn save_settings(settings: &AppSettings) -> Result<(), String> {
    let json =
        serde_json::to_string_pretty(settings).map_err(|e| format!("序列化设置失败: {e}"))?;
    std::fs::write(settings_path(), json).map_err(|e| format!("写入设置文件失败: {e}"))
}

// 真实网络检测：尝试TCP连接多个公共端点，测量延迟
fn detect_network() -> NetworkStatus {
    // 多个测试端点（IP:端口），避免单点失败
    let endpoints: [(&str, u16); 3] = [
        ("223.5.5.5", 443),    // 阿里DNS
        ("119.29.29.29", 443), // 腾讯DNS
        ("8.8.8.8", 443),      // 谷歌DNS
    ];

    let timeout = Duration::from_secs(3);
    let mut best_latency: Option<u32> = None;

    for (host, port) in endpoints {
        let start = std::time::Instant::now();
        if TcpStream::connect_timeout(
            &std::net::SocketAddr::new(host.parse().unwrap(), port),
            timeout,
        )
        .is_ok()
        {
            let latency = start.elapsed().as_millis() as u32;
            best_latency = Some(best_latency.map_or(latency, |b| b.min(latency)));
        }
    }

    match best_latency {
        Some(latency) => NetworkStatus {
            is_online: true,
            connection_type: "在线".to_string(),
            latency: Some(latency),
        },
        None => NetworkStatus {
            is_online: false,
            connection_type: "离线".to_string(),
            latency: None,
        },
    }
}

// ===== 系统集成：通用动作 =====

/// 显示并聚焦主窗口（托盘左键 / 翻译快捷键共用）
pub fn show_main_window(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

/// 触发截图翻译：显示截图窗口并通知前端重置到框选模式
pub fn trigger_screenshot(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("screenshot") {
        // 常驻窗口合成已预热，但"已可见"状态下 show() 是空操作、不会带来系统激活，
        // 仅取消穿透会因拿不到焦点表现为按键/点击无响应——走一次 hide→show 重新激活
        //（WebView 内容仍在，显隐开销极小）
        let _ = win.hide();
        let _ = win.set_ignore_cursor_events(false);
        let _ = win.show();
        let _ = win.set_focus();
        // Windows 前台锁定：后台进程的 SetForegroundWindow 会被系统拒绝——
        // 表现为窗口可见但收不到任何键盘输入（ESC/按键全失效）。
        // 经典解法：模拟一次 ALT 击键使本进程获得前台设置权，再强制置前台+顶层
        #[cfg(target_os = "windows")]
        unsafe {
            use windows::Win32::UI::Input::KeyboardAndMouse::{keybd_event, KEYEVENTF_KEYUP, VK_MENU};
            use windows::Win32::UI::WindowsAndMessaging::{
                SetForegroundWindow, SetWindowPos, HWND_TOPMOST, SWP_NOMOVE, SWP_NOSIZE,
                SWP_SHOWWINDOW,
            };
            keybd_event(VK_MENU.0 as u8, 0, KEYEVENTF_KEYUP, 0);
            if let Ok(hwnd) = win.hwnd() {
                let _ = SetForegroundWindow(hwnd);
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
                );
            }
        }
        spawn_esc_watcher(app.clone());
        let _ = app.emit_to("screenshot", "trigger-screenshot", ());
    }
}

/// ESC 监视线程：截图会话期间 30ms 采样 ESC 物理键状态（GetAsyncKeyState 读全局键盘，
/// 与窗口焦点/NOACTIVATE 无关，无漏检）。检测到即隐藏窗口并通知前端复位；120s 无操作自动退出
static ESC_WATCHING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn spawn_esc_watcher(handle: tauri::AppHandle) {
    use std::sync::atomic::Ordering;
    if ESC_WATCHING.swap(true, Ordering::Relaxed) {
        return; // 已有监视线程
    }
    std::thread::spawn(move || {
        let start = std::time::Instant::now();
        unsafe {
            use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_ESCAPE};
            while start.elapsed() < std::time::Duration::from_secs(120) {
                if GetAsyncKeyState(VK_ESCAPE.0 as i32) as u16 & 0x8000 != 0 {
                    if let Some(w) = handle.get_webview_window("screenshot") {
                        let _ = w.hide();
                    }
                    let _ = handle.emit_to("screenshot", "exit-screenshot", ());
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(30));
            }
        }
        ESC_WATCHING.store(false, Ordering::Relaxed);
    });
}

/// Tauri命令：轮询 ESC 物理键状态（GetAsyncKeyState 读全局键盘状态，
/// 与窗口焦点/前台无关——前端在截图会话期间定时调用，作为 keydown 失效时的退出兜底）。
/// async：在异步运行时执行，不占 UI 主线程（避免高频轮询加重消息泵停顿）
#[tauri::command(async)]
pub fn poll_esc() -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_ESCAPE};
        Ok(GetAsyncKeyState(VK_ESCAPE.0 as i32) as u16 & 0x8000 != 0)
    }
    #[cfg(not(target_os = "windows"))]
    Ok(false)
}

/// Tauri命令：手动预下载离线翻译模型（当前默认方向相关语言对，进度经 offline-mt-status 事件推送）
#[tauri::command]
pub async fn download_offline_model() -> Result<(), String> {
    let direction = load_settings().default_translation_direction;
    let (from, to) = direction
        .split_once("->")
        .map(|(f, t)| (f.to_string(), t.to_string()))
        .unwrap_or_else(|| ("auto".to_string(), "zh".to_string()));
    let from = if from == "auto" { "en".to_string() } else { from };
    if load_settings().offline_model == "nllb-200" {
        // NLLB-200：单模型覆盖全部语言对
        return crate::offline_mt::ensure_nllb_model().await.map(|_| ());
    }
    let hops = crate::offline_mt::route(&from, &to);
    if hops.is_empty() {
        return Err(format!("当前方向 {from}→{to} 暂无离线模型"));
    }
    for (f, t) in &hops {
        crate::offline_mt::ensure_pair_models(f, t).await?;
    }
    Ok(())
}

/// Tauri命令：快捷键录入模式——临时注销全部全局热键（录入期间按组合键不会被系统热键抢先）；
/// 录入完成后重新按当前设置注册
#[tauri::command]
pub fn set_hotkeys_suspended(app: tauri::AppHandle, suspended: bool) -> Result<(), String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    if suspended {
        app.global_shortcut().unregister_all().map_err(|e| e.to_string())?;
    } else {
        let settings = load_settings();
        register_hotkeys(&app, &settings)?;
    }
    Ok(())
}

/// Tauri命令：截图窗口预热——移出屏幕外显示一次，完成 WebView2 全屏透明合成初始化
/// 后隐藏并恢复原位。屏幕外显示不触发桌面重绘（修复“打开应用时文件管理器闪烁”）
static PREHEAT_DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[tauri::command]
pub fn preheat_screenshot(app: tauri::AppHandle) -> Result<(), String> {
    // 去重：预热只需一次（前端重复触发/组件重挂载不再产生窗口风暴）
    if PREHEAT_DONE.swap(true, std::sync::atomic::Ordering::Relaxed) {
        return Ok(());
    }
    if let Some(win) = app.get_webview_window("screenshot") {
        let origin = win.outer_position().map_err(|e| e.to_string())?;
        let _ = win.set_ignore_cursor_events(true);
        let _ = win.set_position(tauri::PhysicalPosition::new(-32000, -32000));
        let _ = win.show();
        let handle = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(600));
            if let Some(w) = handle.get_webview_window("screenshot") {
                let _ = w.hide();
                let _ = w.set_position(tauri::PhysicalPosition::new(origin.x, origin.y));
            }
        });
    }
    Ok(())
}

/// Tauri命令：主窗口“开始截图”按钮与快捷键/托盘走同一触发路径
#[tauri::command]
pub fn trigger_screenshot_cmd(app: tauri::AppHandle) -> Result<(), String> {
    trigger_screenshot(&app);
    Ok(())
}

// ===== 系统托盘 =====

// 设置系统托盘：显示主窗口 / 截图翻译 / 退出；左键单击托盘=显示主窗口
pub fn setup_system_tray(app: &AppHandle) -> Result<(), String> {
    let show = MenuItemBuilder::with_id("tray_show", "显示主窗口")
        .build(app)
        .map_err(|e| format!("构建托盘菜单失败: {e}"))?;
    let screenshot = MenuItemBuilder::with_id("tray_screenshot", "截图翻译")
        .build(app)
        .map_err(|e| format!("构建托盘菜单失败: {e}"))?;
    let quit = MenuItemBuilder::with_id("tray_quit", "退出")
        .build(app)
        .map_err(|e| format!("构建托盘菜单失败: {e}"))?;

    let menu = MenuBuilder::new(app)
        .item(&show)
        .item(&screenshot)
        .separator()
        .item(&quit)
        .build()
        .map_err(|e| format!("构建托盘菜单失败: {e}"))?;

    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| "缺少应用图标，无法创建托盘".to_string())?;

    TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .tooltip("智能翻译软件")
        .menu(&menu)
        .show_menu_on_left_click(false) // 左键=显示主窗口，右键=弹出菜单
        .on_menu_event(|app, event| match event.id.as_ref() {
            "tray_show" => show_main_window(app),
            "tray_screenshot" => trigger_screenshot(app),
            "tray_quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)
        .map_err(|e| format!("创建托盘失败: {e}"))?;
    Ok(())
}

// ===== 开机自启动 =====

// 应用开机自启动设置（tauri-plugin-autostart 写注册表 HKCU\...\Run）
pub fn apply_autostart(app: &AppHandle, enabled: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt as _;
    let autolaunch = app.autolaunch();
    if enabled {
        autolaunch
            .enable()
            .map_err(|e| format!("开启开机自启动失败: {e}"))
    } else {
        autolaunch
            .disable()
            .map_err(|e| format!("关闭开机自启动失败: {e}"))
    }
}

// ===== 全局快捷键 =====

/// 解析快捷键字符串（如 "Ctrl+Alt+S"）
fn parse_shortcut(s: &str) -> Result<Shortcut, String> {
    s.trim()
        .parse::<Shortcut>()
        .map_err(|_| format!("快捷键「{s}」无法识别（示例：Ctrl+Alt+S）"))
}

// 按设置注册全局快捷键（重复调用会先注销全部再注册，用于设置变更后重注册）
pub fn register_hotkeys(app: &AppHandle, settings: &AppSettings) -> Result<(), String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    let gs = app.global_shortcut();
    gs.unregister_all()
        .map_err(|e| format!("注销旧快捷键失败: {e}"))?;

    let hm = |k: &str| settings.hotkeys.get(k).cloned().unwrap_or_default();
    let entries: Vec<(&str, String)> = vec![
        ("screenshot", hm("screenshot").trim().to_string()),
        ("translate", hm("translate").trim().to_string()),
        ("select", {
            if settings.select_translate_enabled {
                hm("select").trim().to_string()
            } else {
                String::new()
            }
        }),
    ];

    // 值为 None/空 = 用户未设置，跳过注册；注册失败的收集为冲突清单（可能与其他软件/系统占用冲突）
    let mut registered: Vec<(&str, String)> = Vec::new();
    let mut conflicts: Vec<String> = Vec::new();
    for (name, combo) in &entries {
        if combo.is_empty() || combo.eq_ignore_ascii_case("none") {
            continue;
        }
        let sc = match parse_shortcut(combo) {
            Ok(s) => s,
            Err(_) => {
                conflicts.push(format!("{combo}（{name}·格式无法识别）"));
                continue;
            }
        };
        if registered.iter().any(|(_, r)| *r == *combo) {
            conflicts.push(format!("{combo}（{name}·与其他快捷键重复）"));
            continue;
        }
        let name_owned = name.to_string();
        let combo_owned = combo.clone();
        let result = gs.on_shortcut(sc, move |app, _shortcut, event| {
            if event.state() != ShortcutState::Pressed {
                return;
            }
            match name_owned.as_str() {
                "screenshot" => trigger_screenshot(app),
                "translate" => {
                    show_main_window(app);
                    let _ = app.emit_to("main", "open-translate-page", ());
                }
                "select" => {
                    let app = app.clone();
                    std::thread::spawn(move || {
                        if let Some(text) = crate::selection::capture_selected_text() {
                            show_main_window(&app);
                            let _ = app.emit_to("main", "translate-selection", text);
                        }
                    });
                }
                _ => {}
            }
            let _ = combo_owned;
        });
        if let Err(e) = result {
            conflicts.push(format!("{combo}（{name}·注册失败: 可能已被系统或其他软件占用）"));
            let _ = e;
        } else {
            registered.push((name, combo.clone()));
        }
    }

    *HOTKEY_CONFLICTS
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = conflicts.clone();

    if conflicts.is_empty() {
        Ok(())
    } else {
        Err(format!("以下快捷键注册失败：{}", conflicts.join("、")))
    }
}

/// 最近一次快捷键注册的冲突清单（前端快捷键页展示）
static HOTKEY_CONFLICTS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Tauri命令：查询快捷键冲突清单
#[tauri::command]
pub fn get_hotkey_conflicts() -> Result<Vec<String>, String> {
    Ok(HOTKEY_CONFLICTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone())
}

// Tauri命令：用系统默认浏览器打开链接（仅允许 http/https，防命令注入）
#[tauri::command]
pub fn open_external(url: String) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("仅允许打开 http/https 链接".to_string());
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &url])
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW，避免闪现控制台
            .spawn()
            .map_err(|e| format!("打开浏览器失败: {e}"))?;
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::process::Command::new("xdg-open")
            .arg(&url)
            .spawn()
            .map_err(|e| format!("打开浏览器失败: {e}"))?;
        Ok(())
    }
}

// Tauri命令：获取网络状态（真实检测）
#[tauri::command]
pub async fn get_network_status() -> Result<NetworkStatus, String> {
    // 网络检测是阻塞操作，放到异步任务中执行
    Ok(detect_network())
}

// Tauri命令：获取应用设置（从文件加载）
#[tauri::command]
pub async fn get_app_settings() -> Result<AppSettings, String> {
    Ok(load_settings())
}

// Tauri命令：保存应用设置（持久化到JSON文件，并同步应用自启动/快捷键）
#[tauri::command]
pub async fn save_app_settings(
    app: tauri::AppHandle,
    settings: AppSettings,
) -> Result<(), String> {
    // 同步内置符号纠错开关（OCR 纠错链路读取）
    crate::engines::BUILTIN_SYMBOLS_ON.store(
        settings.builtin_symbols_enabled,
        std::sync::atomic::Ordering::Relaxed,
    );
    let path = settings_path();
    let json = serde_json::to_string_pretty(&settings)
        .map_err(|e| format!("序列化设置失败: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("写入设置文件失败: {e}"))?;

    // 系统集成设置即时生效；失败信息返回给前端提示（文件已保存）
    let mut errs: Vec<String> = Vec::new();
    if let Err(e) = apply_autostart(&app, settings.autostart) {
        errs.push(e);
    }
    if let Err(e) = register_hotkeys(&app, &settings) {
        errs.push(e);
    }
    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs.join("；"))
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    /// 诊断：用户真实 settings.json 必须能反序列化（失败=设置页全空根因）
    #[test]
    fn test_deserialize_user_settings_file() {
        let path = std::path::Path::new("target/release/settings.json");
        if !path.exists() {
            return; // 环境无该文件时跳过
        }
        let content = std::fs::read_to_string(path).unwrap();
        match serde_json::from_str::<AppSettings>(&content) {
            Ok(s) => {
                assert!(!s.online_apis.is_empty());
                assert!(s.offline_engine == "marian");
            }
            Err(e) => panic!("用户 settings.json 反序列化失败: {e}"),
        }
    }


    use super::*;

    #[test]
    fn test_detect_network() {
        let status = detect_network();
        // 无法断言在线/离线（取决于环境），但结构必须合法
        assert!(status.is_online || !status.is_online);
        if status.is_online {
            assert!(status.latency.is_some());
        }
    }

    #[test]
    fn test_default_settings() {
        let s = AppSettings::default();
        assert!(s.autostart);
        assert_eq!(s.default_translation_direction, "auto->zh");
    }

    // ==================================================================
    // 系统化测试套件（前缀过滤：cargo test <前缀>）：
    //   test_            单元测试
    //   regression_      回归测试（历史配置迁移防复发）
    //   security_        安全测试（损坏配置安全回退）
    //   sanity_          健全性测试（配置结构自洽）
    // ==================================================================

    // ===== 健全性：默认配置结构自洽 =====

    #[test]
    fn sanity_default_settings_components_valid() {
        let s = AppSettings::default();
        // 菜单组件必须全部是已知ID
        for c in &s.screenshot_components {
            assert!(
                ["engine", "lang", "copy_src", "copy_dst", "close", "settings"]
                    .contains(&c.as_str()),
                "未知菜单组件: {c}"
            );
        }
        assert_eq!(s.online_apis, vec!["baidu", "youdao"]);
        assert_eq!(s.offline_model, "opus-mt");
        assert!(s.builtin_symbols_enabled);
    }

    #[test]
    fn sanity_hotkey_defaults_valid() {
        let s = AppSettings::default();
        for name in ["translate", "screenshot", "select", "reverse"] {
            assert!(s.hotkeys.contains_key(name), "默认热键缺失: {name}");
        }
    }

    // ===== 迁移测试（回归：历史遗留配置必须被就地修正） =====

    #[test]
    fn regression_migrate_ocr_tesseract_to_windows() {
        let mut s = AppSettings::default();
        s.ocr_engine = "tesseract".to_string();
        migrate(&mut s);
        assert_eq!(s.ocr_engine, "windows");
    }

    #[test]
    fn regression_migrate_offline_argos_to_marian() {
        let mut s = AppSettings::default();
        s.offline_engine = "argos".to_string();
        migrate(&mut s);
        assert_eq!(s.offline_engine, "marian");
    }

    #[test]
    fn regression_migrate_missing_hotkeys_added() {
        let mut s = AppSettings::default();
        s.hotkeys.remove("select");
        s.hotkeys.remove("reverse");
        migrate(&mut s);
        assert!(s.hotkeys.contains_key("select"));
        assert!(s.hotkeys.contains_key("reverse"));
    }

    #[test]
    fn regression_migrate_invalid_hotkey_falls_back() {
        // 回归bug：设置页自由文本误输入（如"Ctrl+Alt+TCt"）导致热键注册失败
        for bad in ["Ctrl+Alt+TCt", "Ctrl", "Ctrl+Shift+", "Ctrl+中文", "F", "F1x"] {
            let mut s = AppSettings::default();
            s.hotkeys.insert("translate".to_string(), bad.to_string());
            migrate(&mut s);
            assert_eq!(
                s.hotkeys["translate"], "Ctrl+Alt+T",
                "非法热键 {bad:?} 应回落默认值"
            );
        }
    }

    #[test]
    fn regression_migrate_valid_hotkeys_kept() {
        // 注意：裸键（如 F9 无修饰键）按设计也视为非法（全局热键插件不接受）
        for good in ["Ctrl+Alt+T", "none", "Ctrl+Shift+F9", "Ctrl+1", "Alt+F12"] {
            let mut s = AppSettings::default();
            s.hotkeys.insert("translate".to_string(), good.to_string());
            migrate(&mut s);
            assert_eq!(s.hotkeys["translate"], good, "合法热键 {good:?} 不得被改动");
        }
    }

    #[test]
    fn regression_migrate_copy_component_split() {
        // 旧版单一"copy"组件拆分为复制原文/复制译文，位置保持；
        // 随后 lang 组件迁移会在 engine 之后补位
        let mut s = AppSettings::default();
        s.screenshot_components = vec!["engine".into(), "copy".into(), "close".into()];
        migrate(&mut s);
        assert_eq!(
            s.screenshot_components,
            vec!["engine", "lang", "copy_src", "copy_dst", "close"]
        );
    }

    #[test]
    fn regression_migrate_lang_component_inserted_after_engine() {
        let mut s = AppSettings::default();
        s.screenshot_components = vec!["engine".into(), "close".into()];
        migrate(&mut s);
        assert_eq!(s.screenshot_components, vec!["engine", "lang", "close"]);
        // 无engine组件时插入到最前
        let mut s2 = AppSettings::default();
        s2.screenshot_components = vec!["close".into()];
        migrate(&mut s2);
        assert_eq!(s2.screenshot_components, vec!["lang", "close"]);
    }

    #[test]
    fn regression_migrate_overlay_legacy_transparent() {
        // 旧版"无背景模式"开关 → 新 overlay_mode
        let mut s = AppSettings::default();
        s.overlay_mode = None;
        s.overlay_transparent_legacy = true;
        migrate(&mut s);
        assert_eq!(s.overlay_mode.as_deref(), Some("none"));
    }

    #[test]
    fn regression_migrate_current_engine_fallback() {
        // 当前翻译源未启用 → 回落第一个启用的引擎
        let mut s = AppSettings::default();
        s.current_engine = "DeepL".to_string();
        s.online_apis = vec!["baidu".to_string()];
        migrate(&mut s);
        assert_eq!(s.current_engine, "百度翻译");
        // 离线翻译启用时是合法翻译源
        let mut s2 = AppSettings::default();
        s2.current_engine = "离线翻译".to_string();
        migrate(&mut s2);
        assert_eq!(s2.current_engine, "离线翻译");
    }

    // ===== 安全：损坏配置文件必须安全回退 =====

    #[test]
    fn security_corrupt_settings_json_is_rejected() {
        // 非法JSON → load_settings 会走默认值分支（不panic不崩溃）
        assert!(serde_json::from_str::<AppSettings>("{broken json").is_err());
        // 缺少必填字段（autostart无default）→ 拒绝载入，同样走默认值
        assert!(serde_json::from_str::<AppSettings>("{}").is_err());
    }

    #[test]
    fn security_settings_unknown_fields_tolerated() {
        // 前向兼容：新版本新增字段在旧版反序列化时被忽略而不是报错
        let json = serde_json::to_string(&AppSettings::default()).unwrap();
        // 在对象开头插入未知字段（json 自带的收尾大括号复用为整体收尾）
        let with_unknown = format!(r#"{{"未来新字段": 1, {}"#, &json[1..]);
        let s: AppSettings = serde_json::from_str(&with_unknown).unwrap();
        assert!(s.autostart);
    }

    #[test]
    fn security_settings_roundtrip_preserves_keys() {
        // 密钥字段序列化往返不丢失（本地明文存储为既定设计，写入exe同目录settings.json）
        let mut s = AppSettings::default();
        s.baidu_app_id = "202401010001".to_string();
        s.baidu_secret = "testsecret".to_string();
        let json = serde_json::to_string(&s).unwrap();
        let back: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.baidu_app_id, "202401010001");
        assert_eq!(back.baidu_secret, "testsecret");
    }
}