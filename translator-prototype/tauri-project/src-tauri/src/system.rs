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
        "copy".to_string(),
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
        hotkeys.insert("translate".to_string(), "Ctrl+Alt+T".to_string());
        hotkeys.insert("screenshot".to_string(), "Ctrl+Alt+S".to_string());
        hotkeys.insert("select".to_string(), "Ctrl+Alt+X".to_string());

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
    // 划词翻译热键（历史配置缺失时补默认值）
    if !settings.hotkeys.contains_key("select") {
        settings
            .hotkeys
            .insert("select".to_string(), "Ctrl+Alt+X".to_string());
    }
    // 旧版"无背景模式"开关 → 新覆盖样式
    if settings.overlay_mode.is_none() && settings.overlay_transparent_legacy {
        settings.overlay_mode = Some("none".to_string());
    }
    // 当前翻译源不在启用列表时，回落到第一个启用的引擎（名称↔ID映射）
    let enabled_name = |name: &str| {
        let id = match name {
            "百度翻译" => "baidu",
            "有道智云" => "youdao",
            _ => "",
        };
        settings.online_apis.iter().any(|a| a == id)
    };
    if !enabled_name(&settings.current_engine) {
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
        let _ = win.show();
        let _ = win.set_focus();
        let _ = app.emit_to("screenshot", "trigger-screenshot", ());
    }
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
    let gs = app.global_shortcut();
    gs.unregister_all()
        .map_err(|e| format!("注销旧快捷键失败: {e}"))?;

    let hk_screenshot = settings
        .hotkeys
        .get("screenshot")
        .cloned()
        .unwrap_or_else(|| "Ctrl+Alt+S".to_string());
    let hk_translate = settings
        .hotkeys
        .get("translate")
        .cloned()
        .unwrap_or_else(|| "Ctrl+Alt+T".to_string());

    let sc_screenshot = parse_shortcut(&hk_screenshot)?;
    let sc_translate = parse_shortcut(&hk_translate)?;
    if sc_screenshot == sc_translate {
        return Err("截图与翻译快捷键不能相同".to_string());
    }

    gs.on_shortcut(sc_screenshot, |app, _shortcut, event| {
        if event.state() == ShortcutState::Pressed {
            trigger_screenshot(app);
        }
    })
    .map_err(|e| format!("注册截图快捷键「{hk_screenshot}」失败: {e}"))?;

    gs.on_shortcut(sc_translate, |app, _shortcut, event| {
        if event.state() == ShortcutState::Pressed {
            show_main_window(app);
            let _ = app.emit_to("main", "open-translate-page", ());
        }
    })
    .map_err(|e| format!("注册翻译快捷键「{hk_translate}」失败: {e}"))?;

    // 划词翻译：捕获前台应用选中文本 → 主窗口翻译页（未启用则跳过）
    if settings.select_translate_enabled {
        let hk_select = settings
            .hotkeys
            .get("select")
            .cloned()
            .unwrap_or_else(|| "Ctrl+Alt+X".to_string());
        if !hk_select.trim().is_empty() {
            let sc_select = parse_shortcut(&hk_select)?;
            if sc_select == sc_screenshot || sc_select == sc_translate {
                return Err("划词翻译快捷键与其他快捷键重复".to_string());
            }
            gs.on_shortcut(sc_select, move |app, _shortcut, event| {
                if event.state() == ShortcutState::Pressed {
                    let app = app.clone();
                    // 捕获含轮询等待，放后台线程执行
                    std::thread::spawn(move || {
                        if let Some(text) = crate::selection::capture_selected_text() {
                            show_main_window(&app);
                            let _ = app.emit_to("main", "translate-selection", text);
                        }
                    });
                }
            })
            .map_err(|e| format!("注册划词翻译快捷键「{hk_select}」失败: {e}"))?;
        }
    }

    Ok(())
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
}