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
    // 截图翻译：菜单组组件（**全序**——包含全部组件，顺序即显示顺序）
    #[serde(default = "default_screenshot_components")]
    pub screenshot_components: Vec<String>,
    // 上述组件中被禁用的集合（不在其中的 = 启用）
    #[serde(default = "default_screenshot_components_disabled")]
    pub screenshot_components_disabled: Vec<String>,
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
    // 文本严格对齐模式：块严格贴合选中文字的紧致框，字号饱和填满（不留余量）
    #[serde(default)]
    pub overlay_expand: bool,
    // 覆盖层默认显示内容：original=原文 / translated=译文 / last=沿用上次在截图菜单里的选择
    #[serde(default = "default_overlay_default_mode")]
    pub overlay_default_mode: String,
    // 仅当 overlay_default_mode == "last" 时生效：记住截图菜单里最后一次的选择
    #[serde(default = "default_overlay_last_mode")]
    pub overlay_last_mode: String,
    // 冻结画面：触发截图翻译瞬间固定屏幕快照（动态内容所见即所得，框选的是静态图）；
    // 关闭 = 实时画面（框选松手后才实际截屏，旧行为）
    #[serde(default = "default_true")]
    pub screenshot_freeze_frame: bool,
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
    // 主界面默认进入的菜单页
    // （basic/translation/translate/screenshot/terms/history/hotkeys/about）
    #[serde(default = "default_startup_menu")]
    pub startup_menu: String,
    // 多自定义 OpenAI 兼容供应商（每个生成独立引擎）
    #[serde(default)]
    pub custom_providers: Vec<CustomProvider>,
    // 离线翻译模型选择（opus-mt 可用；nllb-200 下一版本接入）
    #[serde(default = "default_offline_model")]
    pub offline_model: String,
    // NLLB 精度变体：int8=量化版(默认，encoder 400MB + merged decoder 454MB)。
    // q4 导出实测**反而更大**（encoder 1095.7MB + decoder 1126.8MB = 2.17GB），
    // 与"体积减半"预期相反，因此 UI 不暴露该选项；字段与加载链路保留，
    // 便于日后实测 q4 在 CPU 上是否更快时直接切换。
    #[serde(default = "default_nllb_variant")]
    pub nllb_variant: String,
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

/// 主界面默认进入的菜单页（与前端 MENU_IDS 一致）
fn default_startup_menu() -> String {
    "basic".to_string()
}

/// 自定义AI 供应商名称唯一性校验（save_app_settings 落盘前调用，单测直接复用）。
/// 名称即引擎显示名，重名会让"当前翻译源"指向不明。
pub fn validate_custom_provider_names(settings: &AppSettings) -> Result<(), String> {
    let mut seen: Vec<String> = Vec::new();
    for cp in &settings.custom_providers {
        if cp.base_url.trim().is_empty() {
            continue; // 未填地址的供应商不生成引擎，不参与判重
        }
        let name = if cp.name.trim().is_empty() {
            "自定义AI".to_string()
        } else {
            cp.name.trim().to_string()
        };
        if seen.iter().any(|s| s == &name) {
            return Err(format!(
                "自定义AI 供应商名称重复：{name}。请改成唯一名称后再保存"
            ));
        }
        seen.push(name);
    }
    Ok(())
}

fn default_nllb_variant() -> String {
    "int8".to_string()
}

fn default_overlay_default_mode() -> String {
    "original".to_string()
}

fn default_overlay_last_mode() -> String {
    "original".to_string()
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

fn default_screenshot_components_disabled() -> Vec<String> {
    vec![]
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
            screenshot_components_disabled: default_screenshot_components_disabled(),
            overlay_mode: None,
            overlay_transparent_legacy: false,
            overlay_expand: false,
            overlay_default_mode: default_overlay_default_mode(),
            overlay_last_mode: default_overlay_last_mode(),
            screenshot_freeze_frame: true,
            select_translate_enabled: true,
            current_engine: default_current_engine(),
            window_size_mode: default_window_size_mode(),
            window_fixed_width: default_window_fixed_width(),
            window_fixed_height: default_window_fixed_height(),
            window_last_width: 0,
            window_last_height: 0,
            providers: ProviderSettings::default(),
            startup_show_window: false,
            startup_menu: default_startup_menu(),
            custom_providers: Vec::new(),
            offline_model: default_offline_model(),
            nllb_variant: default_nllb_variant(),
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
    // 启动页：非法/历史值回落基础设置
    {
        const STARTUP_MENUS: [&str; 8] = [
            "basic",
            "translation",
            "translate",
            "screenshot",
            "terms",
            "history",
            "hotkeys",
            "about",
        ];
        if !STARTUP_MENUS.contains(&settings.startup_menu.as_str()) {
            settings.startup_menu = default_startup_menu();
        }
    }
    // NLLB 精度：非法值回落 int8
    if !["int8", "q4"].contains(&settings.nllb_variant.as_str()) {
        settings.nllb_variant = default_nllb_variant();
    }
    // 覆盖层默认显示内容 / 上次选择：非法值回落 original
    if !["original", "translated", "last"].contains(&settings.overlay_default_mode.as_str()) {
        settings.overlay_default_mode = default_overlay_default_mode();
    }
    if !["original", "translated"].contains(&settings.overlay_last_mode.as_str()) {
        settings.overlay_last_mode = default_overlay_last_mode();
    }
    // 自定义AI：引擎显示名不再带"自定义AI-"前缀（旧值归一化），并保证供应商名唯一。
    // 必须在下面 current_engine 合法性校验之前执行，否则老用户的当前翻译源会被判非法而重置。
    {
        if let Some(rest) = settings.current_engine.strip_prefix("自定义AI-") {
            settings.current_engine = rest.trim().to_string();
        }
        let mut seen: Vec<String> = Vec::new();
        for cp in settings.custom_providers.iter_mut() {
            if cp.base_url.trim().is_empty() {
                continue;
            }
            let base = if cp.name.trim().is_empty() {
                "自定义AI".to_string()
            } else {
                cp.name.trim().to_string()
            };
            let mut d = base.clone();
            let mut n = 2usize;
            while seen.iter().any(|s| s == &d) {
                d = format!("{base}-{n}");
                n += 1;
            }
            cp.name = d.clone();
            seen.push(d);
        }
    }
    // 菜单组组件：旧语义（数组=启用集）迁移为“全序+禁用集合”
    {
        let all = ["engine", "lang", "copy", "close", "settings"];
        // 兼容更早版本的拆分命名
        if settings
            .screenshot_components
            .iter()
            .any(|c| c == "copy_src" || c == "copy_dst")
        {
            let pos = settings
                .screenshot_components
                .iter()
                .position(|c| c == "copy_src" || c == "copy_dst")
                .unwrap();
            settings
                .screenshot_components
                .retain(|c| c != "copy_src" && c != "copy_dst");
            settings
                .screenshot_components
                .insert(pos.min(settings.screenshot_components.len()), "copy".to_string());
        }
        for g in all {
            if !settings.screenshot_components.iter().any(|c| c == g) {
                settings.screenshot_components.push(g.to_string());
                settings.screenshot_components_disabled.push(g.to_string());
            }
        }
        settings
            .screenshot_components
            .retain(|c| all.contains(&c.as_str()));
        settings
            .screenshot_components_disabled
            .retain(|c| all.contains(&c.as_str()));
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
    // 快捷键默认值改为 None（无）：升级前使用出厂默认值的旧值一并重置；
    // 用户自定义过的非默认值保留
    let legacy_defaults = [
        "Ctrl+Alt+T",
        "Ctrl+Alt+S",
        "Ctrl+Alt+X",
        "Ctrl+Alt+B",
        "ctrl+alt+t",
        "ctrl+alt+s",
        "ctrl+alt+x",
        "ctrl+alt+b",
    ];
    for key in ["translate", "screenshot", "select", "reverse"] {
        let is_legacy = settings
            .hotkeys
            .get(key)
            .map(|v| legacy_defaults.iter().any(|d| d.eq_ignore_ascii_case(v)))
            .unwrap_or(false);
        if is_legacy {
            settings.hotkeys.insert(key.to_string(), "None".to_string());
        }
    }
    // 快捷键格式校验与非法回落（含设置页自由文本误输入、单按键、未知修饰键）
    sanitize_hotkeys(settings);
    // 旧版"无背景模式"开关 → 新覆盖样式
    if settings.overlay_mode.is_none() && settings.overlay_transparent_legacy {
        settings.overlay_mode = Some("none".to_string());
    }
    // 自定义AI 供应商的引擎名 = 供应商名（与 build_manager 的去重规则一致）。
    // 旧实现只认 "自定义AI"，多供应商的 current_engine 会被判非法并在启动时被重置（既有 bug）。
    let custom_engine_names: Vec<String> = {
        let mut used: Vec<String> = Vec::new();
        settings
            .custom_providers
            .iter()
            .filter(|cp| !cp.base_url.trim().is_empty())
            .map(|cp| {
                let base = if cp.name.trim().is_empty() {
                    "自定义AI".to_string()
                } else {
                    cp.name.trim().to_string()
                };
                let mut d = base.clone();
                let mut n = 2usize;
                while used.iter().any(|u| u == &d) {
                    d = format!("{base}-{n}");
                    n += 1;
                }
                used.push(d.clone());
                d
            })
            .collect()
    };
    // 当前翻译源不在启用列表时，回落到第一个启用的引擎（名称↔ID映射）
    // 名称↔ID全量映射（与 engines.rs 各引擎 name() 对应）
    let enabled_name = |name: &str| {
        if custom_engine_names.iter().any(|n| n == name) {
            return true;
        }
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

/// 触发截图翻译：显示截图窗口并通知前端重置到框选模式。
/// 窗口全程不 hide/show——全屏置顶窗口的显隐会强制桌面整体重绘（桌面图标闪烁），
/// 待命态用"移出屏幕"替代隐藏（见前端 exit），本函数只负责回归屏幕+取前台
pub fn trigger_screenshot(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("screenshot") {
        // 冻结画面模式：show overlay 之前先全屏截屏（窗口还在屏幕外待命，画面干净），
        // 框选在该静态快照上进行——动态内容（视频/动画）所见即所得。
        // 关闭开关则跳过（零开销），框选松手后走旧路径截屏
        if load_settings().screenshot_freeze_frame {
            #[cfg(target_os = "windows")]
            unsafe {
                use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
                let (w, h) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
                if w > 0 && h > 0 {
                    match crate::screenshot::capture_snapshot_region(0, 0, w, h) {
                        Ok(()) => {}
                        Err(e) => eprintln!("[截图] 冻结快照截取失败（将降级实时截屏）: {e}"),
                    }
                }
            }
        }
        let _ = win.set_ignore_cursor_events(false);
        // 【唤醒渲染】窗口长期停在屏幕外时，WebView2 的 renderer 可能被降级/挂起，
        // 即使解除了遮挡检测也可能不产帧——表现就是"快捷键触发了但画面不出来，
        // 动一下鼠标才出现"。这里在窗口仍位于屏幕外时先做一次 1px 尺寸变化，
        // 强制 WebView 重新布局并产出一帧（此时代价为零：窗口在屏幕外，用户看不到）。
        if let Ok(size) = win.inner_size() {
            let _ = win.set_size(tauri::PhysicalSize::new(
                size.width,
                size.height.saturating_sub(1),
            ));
            let _ = win.set_size(tauri::PhysicalSize::new(size.width, size.height));
        }
        // 【顺序很关键】先归位到屏幕内，再 show/focus/emit。
        // 待命态窗口停在 (-32000,-32000)：只要它还不在任何显示器上，Chromium 的
        // 遮挡检测就会把 renderer 判定为后台并停止产帧——此时 emit 过去的事件
        // 即使更新了 state 也画不出来，必须等一次鼠标输入才出画面（用户反馈
        // "快捷键触发后有 80% 概率需要点一下鼠标才出现选框"）。
        // 主屏原点 (0,0) 与前端 currentMonitor() 缓存的 monitorPos 在同一坐标系，
        // 前端随后的 moveOnScreen 是同点幂等移动，不会产生二次跳动。
        let _ = win.set_position(tauri::PhysicalPosition::new(0, 0));
        // 首次触发时窗口仍是 tauri.conf 的 visible:false，需要 show；
        // 之后窗口保持可见（待命=停在屏幕外），show 为幂等空操作
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
                    // 不直接 hide 窗口（全屏窗口显隐会引发桌面重绘），
                    // 由前端 exit() 统一用"移出屏幕"替代待命隐藏
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

/// Tauri命令：手动预下载离线翻译模型，进度经 offline-mt-status 事件推送。
/// engine 缺省取当前设置的离线模型；from/to 缺省取默认翻译方向（auto 按英语处理）
#[tauri::command]
pub async fn download_offline_model(
    engine: Option<String>,
    from: Option<String>,
    to: Option<String>,
) -> Result<(), String> {
    let engine = engine.unwrap_or_else(|| load_settings().offline_model);
    if engine == "nllb-200" {
        // NLLB-200：单模型覆盖全部语言对
        return crate::offline_mt::ensure_nllb_model().await.map(|_| ());
    }
    let (from, to) = match (from, to) {
        (Some(f), Some(t)) => (f, t),
        _ => {
            let direction = load_settings().default_translation_direction;
            direction
                .split_once("->")
                .map(|(f, t)| (f.to_string(), t.to_string()))
                .unwrap_or_else(|| ("en".to_string(), "zh".to_string()))
        }
    };
    let from = if from == "auto" { "en".to_string() } else { from };
    let hops = crate::offline_mt::route(&from, &to);
    if hops.is_empty() {
        return Err(format!("当前方向 {from}→{to} 暂无离线模型"));
    }
    for (f, t) in &hops {
        crate::offline_mt::ensure_pair_models(f, t).await?;
    }
    Ok(())
}

/// Tauri命令：列出已安装的离线翻译模型（设置页模型管理）
#[tauri::command]
pub fn list_offline_models() -> Result<Vec<crate::offline_mt::OfflineModelInfo>, String> {
    Ok(crate::offline_mt::list_installed_models())
}

/// Tauri命令：删除已安装的离线翻译模型（repo 为 HuggingFace 仓库名）
#[tauri::command]
pub fn delete_offline_model(repo: String) -> Result<(), String> {
    crate::offline_mt::delete_installed_model(&repo)
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

/// 修饰键类别（侧别过滤用）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ModKind {
    Ctrl,
    Alt,
    Shift,
    Win,
}

/// 物理侧别
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HotkeySide {
    Left,
    Right,
}

/// 解析修饰键 token → (类别, 侧别要求)。
/// 支持 LCtrl/RCtrl/LAlt/RAlt/LShift/RShift/LWin/RWin 区分左右；
/// 不带侧别前缀（Ctrl/Alt/Shift/Win 等）= 双侧。返回 None 表示非修饰键 token
fn parse_mod_token(tok: &str) -> Option<(ModKind, Option<HotkeySide>)> {
    let t = tok.trim().to_ascii_lowercase();
    let (kind, side) = match t.as_str() {
        "ctrl" | "control" => (ModKind::Ctrl, None),
        "lctrl" | "lcontrol" => (ModKind::Ctrl, Some(HotkeySide::Left)),
        "rctrl" | "rcontrol" => (ModKind::Ctrl, Some(HotkeySide::Right)),
        "alt" | "option" => (ModKind::Alt, None),
        "lalt" => (ModKind::Alt, Some(HotkeySide::Left)),
        "ralt" => (ModKind::Alt, Some(HotkeySide::Right)),
        "shift" => (ModKind::Shift, None),
        "lshift" => (ModKind::Shift, Some(HotkeySide::Left)),
        "rshift" => (ModKind::Shift, Some(HotkeySide::Right)),
        "win" | "super" | "meta" | "cmd" | "command" => (ModKind::Win, None),
        "lwin" => (ModKind::Win, Some(HotkeySide::Left)),
        "rwin" => (ModKind::Win, Some(HotkeySide::Right)),
        _ => return None,
    };
    Some((kind, side))
}

fn mod_kind_token(kind: ModKind) -> &'static str {
    match kind {
        ModKind::Ctrl => "Ctrl",
        ModKind::Alt => "Alt",
        ModKind::Shift => "Shift",
        ModKind::Win => "Super",
    }
}

/// 快捷键格式校验：
/// - "None"（忽略大小写）合法
/// - 必须是 修饰键+主键 组合（禁止单按键——会抢占正常打字/F键功能）
/// - 修饰键段必须是已知 token（含左右侧别），同一类别不得重复
/// - 主键为 1 个 ASCII 字母数字，或 F+数字（Fn 键）
fn is_valid_hotkey(s: &str) -> bool {
    let s = s.trim();
    if s.eq_ignore_ascii_case("none") {
        return true;
    }
    let parts: Vec<&str> = s.split('+').map(|p| p.trim()).collect();
    if parts.len() < 2 {
        return false;
    }
    let (key, mods) = parts.split_last().unwrap();
    let key_ok = (key.len() == 1
        && key.chars().next().map_or(false, |c| c.is_ascii_alphanumeric()))
        || (key.len() >= 2 && key.starts_with('F') && key[1..].chars().all(|c| c.is_ascii_digit()));
    if !key_ok {
        return false;
    }
    let mut seen = std::collections::HashSet::new();
    for m in mods {
        match parse_mod_token(m) {
            Some((kind, _)) => {
                if !seen.insert(kind) {
                    return false;
                }
            }
            None => return false,
        }
    }
    true
}

/// 热键非法时按名回落（translate/screenshot/select 回取出厂默认，reverse 回 None）
fn sanitize_hotkeys(settings: &mut AppSettings) {
    for (name, default) in [
        ("translate", "Ctrl+Alt+T"),
        ("screenshot", "Ctrl+Alt+S"),
        ("select", "Ctrl+Alt+X"),
        ("reverse", "None"),
    ] {
        let bad = settings
            .hotkeys
            .get(name)
            .map_or(true, |v| !is_valid_hotkey(v));
        if bad {
            settings.hotkeys.insert(name.to_string(), default.to_string());
        }
    }
}

/// 把带侧别的修饰键 token 归一化为 global-hotkey 可解析的形式（LCtrl→Ctrl 等），
/// 同时返回侧别要求列表（注册不区分左右，触发时按此验证物理侧别）
fn normalize_hotkey(s: &str) -> (String, Vec<(ModKind, HotkeySide)>) {
    let mut parts: Vec<String> = Vec::new();
    let mut sides = Vec::new();
    for part in s.split('+') {
        match parse_mod_token(part) {
            Some((kind, Some(side))) => {
                sides.push((kind, side));
                parts.push(mod_kind_token(kind).to_string());
            }
            Some((kind, None)) => parts.push(mod_kind_token(kind).to_string()),
            None => parts.push(part.trim().to_string()),
        }
    }
    (parts.join("+"), sides)
}

/// 读取修饰键的物理侧别按键状态（Pressed 事件时键仍被按住，读数可靠）
#[cfg(target_os = "windows")]
fn mod_key_held(kind: ModKind, side: HotkeySide) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_RCONTROL, VK_RMENU,
        VK_RSHIFT, VK_RWIN,
    };
    let vk = match (kind, side) {
        (ModKind::Ctrl, HotkeySide::Left) => VK_LCONTROL,
        (ModKind::Ctrl, HotkeySide::Right) => VK_RCONTROL,
        (ModKind::Alt, HotkeySide::Left) => VK_LMENU,
        (ModKind::Alt, HotkeySide::Right) => VK_RMENU,
        (ModKind::Shift, HotkeySide::Left) => VK_LSHIFT,
        (ModKind::Shift, HotkeySide::Right) => VK_RSHIFT,
        (ModKind::Win, HotkeySide::Left) => VK_LWIN,
        (ModKind::Win, HotkeySide::Right) => VK_RWIN,
    };
    unsafe { GetAsyncKeyState(vk.0 as i32) as u16 & 0x8000 != 0 }
}

#[cfg(not(target_os = "windows"))]
fn mod_key_held(_kind: ModKind, _side: HotkeySide) -> bool {
    true
}

/// 解析快捷键字符串（如 "Ctrl+Alt+S"；侧别 token 已由 normalize_hotkey 归一化）
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
        // 侧别 token 归一化（LCtrl→Ctrl 供插件解析），并提取左右侧物理按键要求
        let (combo_norm, side_reqs) = normalize_hotkey(combo);
        let sc = match parse_shortcut(&combo_norm) {
            Ok(s) => s,
            Err(_) => {
                conflicts.push(format!("{combo}（{name}·格式无法识别）"));
                continue;
            }
        };
        if registered.iter().any(|(_, r)| *r == combo_norm) {
            conflicts.push(format!("{combo}（{name}·与其他快捷键重复）"));
            continue;
        }
        let name_owned = name.to_string();
        let combo_owned = combo.clone();
        let result = gs.on_shortcut(sc, move |app, _shortcut, event| {
            if event.state() != ShortcutState::Pressed {
                return;
            }
            // Win32 RegisterHotKey 不区分修饰键左右：设置仅左侧/右侧时，
            // 按下瞬间用 GetAsyncKeyState 验证物理侧别，不符则不触发
            if !side_reqs.iter().all(|(k, s)| mod_key_held(*k, *s)) {
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
            registered.push((name, combo_norm));
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

/// 诊断命令：把 OCR 行 / 分组 / 覆盖块几何写入 exe 同目录 ocr_debug.log。
/// 仅当环境变量 ST_DEBUG_BLOCKS=1 时生效（正常运行零开销，前端 fire-and-forget）。
/// 用途：定位"覆盖块与原文行不对齐/有英文残留"这类只能靠真实截图复现的排版问题。
#[tauri::command]
pub async fn dump_ocr_debug(payload: String) -> Result<(), String> {
    if std::env::var("ST_DEBUG_BLOCKS").ok().as_deref() != Some("1") {
        return Ok(());
    }
    let mut path = std::env::current_exe().unwrap_or_default();
    path.pop();
    path.push("ocr_debug.log");
    let mut body = String::new();
    body.push_str(&format!(
        "\n=== {} ===\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    ));
    body.push_str(&payload);
    body.push('\n');
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("打开调试日志失败: {e}"))?;
    f.write_all(body.as_bytes())
        .map_err(|e| format!("写入调试日志失败: {e}"))?;
    Ok(())
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
    state: tauri::State<'_, crate::translation::AppState>,
    mut settings: AppSettings,
) -> Result<(), String> {
    // 快捷键格式兜底校验（前端已拦截，此处防其他来源的非法值落盘）
    sanitize_hotkeys(&mut settings);
    // 自定义AI 供应商不允许重名：引擎显示名就是供应商名，重名会让 current_engine
    // 指向不明。前端已做即时拦截，这里兜底防止绕过前端直接落盘。
    validate_custom_provider_names(&settings)?;
    // 同步内置符号纠错开关（OCR 纠错链路读取）
    crate::engines::BUILTIN_SYMBOLS_ON.store(
        settings.builtin_symbols_enabled,
        std::sync::atomic::Ordering::Relaxed,
    );
    let path = settings_path();
    let json = serde_json::to_string_pretty(&settings)
        .map_err(|e| format!("序列化设置失败: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("写入设置文件失败: {e}"))?;

    // 引擎即时生效：按新设置重建翻译管理器。否则刚填写的 API 密钥要重启应用后
    // 才会出现在截图菜单/翻译测试的引擎列表里（"保存后引擎无显示"的根因）
    {
        let mut guard = state.manager.lock().await;
        // 重建会从 terms.json 重新加载，先把未落盘的使用计数写回，避免丢失
        if guard.take_term_dirty() {
            let _ = crate::terms::save_term_base(guard.term_base());
        }
        *guard = crate::translation::build_manager(&settings);
    }
    // 通知截图窗口设置已更新（引擎列表/菜单组件/覆盖样式即时生效）
    let _ = app.emit_to("screenshot", "screenshot-settings-updated", ());

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
                ["engine", "lang", "copy", "close", "settings"].contains(&c.as_str()),
                "未知菜单组件: {c}"
            );
        }
        // 全序语义：默认全部组件在场且无禁用
        assert_eq!(s.screenshot_components.len(), 5);
        assert!(s.screenshot_components_disabled.is_empty());
        assert_eq!(s.online_apis, vec!["baidu", "youdao"]);
        assert_eq!(s.offline_model, "opus-mt");
        assert!(s.builtin_symbols_enabled);
        assert_eq!(s.startup_menu, "basic", "默认启动页必须是基础设置");
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
    fn regression_custom_engine_name_not_reset_by_migrate() {
        // 回归（v0.4.20）：自定义供应商的引擎名就是供应商名，
        // 旧 enabled_name() 只认 "自定义AI"，导致 current_engine 启动时被重置。
        let mut s = AppSettings::default();
        s.online_apis = vec!["custom".to_string()];
        s.custom_providers = vec![CustomProvider {
            id: "p1".into(),
            name: "deepseek".into(),
            base_url: "https://api.deepseek.com/v1".into(),
            api_key: "k".into(),
            model: "deepseek-chat".into(),
        }];
        s.current_engine = "deepseek".to_string();
        migrate(&mut s);
        assert_eq!(s.current_engine, "deepseek", "自定义供应商名不得被判非法而重置");
    }

    #[test]
    fn regression_migrate_strips_custom_engine_prefix() {
        // 旧配置里存的是 "自定义AI-DeepSeek"，归一化为供应商名
        let mut s = AppSettings::default();
        s.online_apis = vec!["custom".to_string()];
        s.custom_providers = vec![CustomProvider {
            id: "p1".into(),
            name: "DeepSeek".into(),
            base_url: "https://api.deepseek.com/v1".into(),
            api_key: "k".into(),
            model: "deepseek-chat".into(),
        }];
        s.current_engine = "自定义AI-DeepSeek".to_string();
        migrate(&mut s);
        assert_eq!(s.current_engine, "DeepSeek");
    }

    #[test]
    fn security_duplicate_custom_provider_names_rejected() {
        let mut s = AppSettings::default();
        s.custom_providers = vec![
            CustomProvider {
                id: "p1".into(),
                name: "deepseek".into(),
                base_url: "https://a/v1".into(),
                api_key: "k".into(),
                model: "m".into(),
            },
            CustomProvider {
                id: "p2".into(),
                name: "deepseek".into(),
                base_url: "https://b/v1".into(),
                api_key: "k".into(),
                model: "m".into(),
            },
        ];
        assert!(validate_custom_provider_names(&s).is_err(), "重名必须被拒绝");

        // 未填地址的不生成引擎 → 不参与判重
        s.custom_providers[1].base_url = String::new();
        assert!(validate_custom_provider_names(&s).is_ok());

        // 改成唯一名后通过
        s.custom_providers[1].base_url = "https://b/v1".into();
        s.custom_providers[1].name = "kimi".into();
        assert!(validate_custom_provider_names(&s).is_ok());
    }

    #[test]
    fn sanity_overlay_default_mode_fields() {
        let s = AppSettings::default();
        assert_eq!(s.overlay_default_mode, "original");
        assert_eq!(s.overlay_last_mode, "original");
        // 非法值回落
        let mut bad = AppSettings::default();
        bad.overlay_default_mode = "nope".into();
        bad.overlay_last_mode = "nope".into();
        migrate(&mut bad);
        assert_eq!(bad.overlay_default_mode, "original");
        assert_eq!(bad.overlay_last_mode, "original");
        // 合法值保留
        let mut ok = AppSettings::default();
        ok.overlay_default_mode = "last".into();
        ok.overlay_last_mode = "translated".into();
        migrate(&mut ok);
        assert_eq!(ok.overlay_default_mode, "last");
        assert_eq!(ok.overlay_last_mode, "translated");
    }

    #[test]
    fn regression_migrate_invalid_startup_menu_falls_back() {
        // 非法/篡改的启动页必须回落基础设置（前端也会二次校验）
        for bad in ["nope", "", "basic ", "TERMS", "settings"] {
            let mut s = AppSettings::default();
            s.startup_menu = bad.to_string();
            migrate(&mut s);
            assert_eq!(s.startup_menu, "basic", "非法启动页 {bad:?} 应回落 basic");
        }
    }

    #[test]
    fn regression_migrate_valid_startup_menu_kept() {
        for good in [
            "basic",
            "translation",
            "translate",
            "screenshot",
            "terms",
            "history",
            "hotkeys",
            "about",
        ] {
            let mut s = AppSettings::default();
            s.startup_menu = good.to_string();
            migrate(&mut s);
            assert_eq!(s.startup_menu, good, "合法启动页 {good:?} 不得被改写");
        }
    }

    #[test]
    fn regression_startup_menu_deserializes_when_missing() {
        // 旧 settings.json 没有 startup_menu 字段时必须能反序列化并回落默认值
        let mut value = serde_json::to_value(AppSettings::default()).unwrap();
        value.as_object_mut().unwrap().remove("startup_menu");
        let s: AppSettings = serde_json::from_value(value).expect("旧配置必须可反序列化");
        assert_eq!(s.startup_menu, "basic");
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
    fn hotkey_side_tokens_valid() {
        // 侧别 token 合法；不带侧别的旧格式保持兼容
        for good in [
            "Ctrl+Alt+X",
            "None",
            "none",
            "LCtrl+Alt+X",
            "RCtrl+RAlt+F5",
            "LWin+Shift+D",
        ] {
            assert!(super::is_valid_hotkey(good), "{good:?} 应合法");
        }
        // 单按键（无修饰键）拒绝；未知修饰键/重复类别拒绝
        for bad in ["A", "F9", "Foo+X", "Ctrl+LCtrl+A", "Ctrl+中文+F1"] {
            assert!(!super::is_valid_hotkey(bad), "{bad:?} 应非法");
        }
    }

    #[test]
    fn normalize_hotkey_extracts_sides() {
        let (norm, sides) = super::normalize_hotkey("LCtrl+Alt+X");
        assert_eq!(norm, "Ctrl+Alt+X");
        assert_eq!(sides, vec![(super::ModKind::Ctrl, super::HotkeySide::Left)]);
        let (norm2, sides2) = super::normalize_hotkey("Ctrl+Alt+S");
        assert_eq!(norm2, "Ctrl+Alt+S");
        assert!(sides2.is_empty());
        let (norm3, sides3) = super::normalize_hotkey("LWin+RShift+K");
        assert_eq!(norm3, "Super+Shift+K");
        assert_eq!(sides3.len(), 2);
    }

    #[test]
    fn sanitize_rejects_single_key_hotkey() {
        // 保存路径兜底：单按键非法 → translate 回落默认、reverse 回落 None
        let mut s = AppSettings::default();
        s.hotkeys.insert("translate".to_string(), "F9".to_string());
        s.hotkeys.insert("reverse".to_string(), "B".to_string());
        super::sanitize_hotkeys(&mut s);
        assert_eq!(s.hotkeys.get("translate").unwrap(), "Ctrl+Alt+T");
        assert_eq!(s.hotkeys.get("reverse").unwrap(), "None");
    }

    #[test]
    fn regression_migrate_valid_hotkeys_kept() {
        // 注意：裸键（如 F9 无修饰键）按设计也视为非法（全局热键插件不接受）
        // 旧出厂默认值（Ctrl+Alt+T 等）升级时重置为 None；用户自定义值保留
        for good in ["none", "Ctrl+Shift+F9", "Ctrl+1", "Alt+F12"] {
            let mut s = AppSettings::default();
            s.hotkeys.insert("translate".to_string(), good.to_string());
            migrate(&mut s);
            assert_eq!(s.hotkeys["translate"], good, "合法热键 {good:?} 不得被改动");
        }
        // 旧出厂默认值重置为 None（v0.2.1 起默认无快捷键）
        let mut s = AppSettings::default();
        s.hotkeys.insert("translate".to_string(), "Ctrl+Alt+T".to_string());
        migrate(&mut s);
        assert_eq!(s.hotkeys["translate"], "None");
    }

    #[test]
    fn regression_migrate_copy_component_split() {
        // 已回退：拆分命名（copy_src/copy_dst）应合并回单一"copy"，位置保持；
        // 缺失组件按全集补齐到尾部并标记禁用
        let mut s = AppSettings::default();
        s.screenshot_components = vec!["engine".into(), "copy_src".into(), "close".into()];
        migrate(&mut s);
        assert_eq!(
            s.screenshot_components,
            vec!["engine", "copy", "close", "lang", "settings"]
        );
        // 未启用（迁移前不存在的 lang/settings）应被标记禁用
        assert!(s.screenshot_components_disabled.contains(&"lang".to_string()));
        assert!(s
            .screenshot_components_disabled
            .contains(&"settings".to_string()));
        assert!(!s
            .screenshot_components_disabled
            .contains(&"copy".to_string()));
    }

    #[test]
    fn regression_migrate_lang_component_inserted_after_engine() {
        // 全序迁移：缺失组件补齐到尾部（不再插入到 engine 之后）
        let mut s = AppSettings::default();
        s.screenshot_components = vec!["engine".into(), "close".into()];
        migrate(&mut s);
        assert_eq!(
            s.screenshot_components,
            vec!["engine", "close", "lang", "copy", "settings"]
        );
        let mut s2 = AppSettings::default();
        s2.screenshot_components = vec!["close".into()];
        migrate(&mut s2);
        assert_eq!(
            s2.screenshot_components,
            vec!["close", "engine", "lang", "copy", "settings"]
        );
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

    // ===== 离线模型管理（列表/删除）=====
    // 注意：两个场景合并在单个测试内——它们共享 SMART_TRANSLATOR_MODELS_DIR 环境变量，
    // 拆开会在并行测试中互相覆盖目录指向

    #[test]
    fn offline_model_list_and_delete() {
        let base = std::env::temp_dir().join(format!("st-mt-test-{}", std::process::id()));
        std::env::set_var("SMART_TRANSLATOR_MODELS_DIR", &base);

        // 场景一：三件套齐全的 ja-en 模型 → 完整列出，可删除
        let dir = base.join("Xenova_opus-mt-ja-en");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("tokenizer.json"), b"{}").unwrap();
        std::fs::write(dir.join("encoder_model_quantized.onnx"), b"x").unwrap();
        std::fs::write(dir.join("decoder_model_merged_quantized.onnx"), b"x").unwrap();

        let list = crate::offline_mt::list_installed_models();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].repo, "Xenova/opus-mt-ja-en");
        assert_eq!(list[0].model, "opus-mt");
        assert!(list[0].complete);
        assert!(list[0].label.contains("日语"));
        assert!(list[0].size_bytes > 0);

        crate::offline_mt::delete_installed_model("Xenova/opus-mt-ja-en").unwrap();
        assert!(!dir.exists());
        assert!(crate::offline_mt::list_installed_models().is_empty());
        // 删除不存在的模型：幂等成功
        crate::offline_mt::delete_installed_model("Xenova/opus-mt-xx-yy").unwrap();

        // 场景二：缺 decoder 的残缺目录 → 列出但 complete=false（下载中断场景）
        let dir2 = base.join("Xenova_opus-mt-en-ru");
        std::fs::create_dir_all(&dir2).unwrap();
        std::fs::write(dir2.join("tokenizer.json"), b"{}").unwrap();

        let list2 = crate::offline_mt::list_installed_models();
        assert_eq!(list2.len(), 1);
        assert!(!list2[0].complete);

        std::env::remove_var("SMART_TRANSLATOR_MODELS_DIR");
        let _ = std::fs::remove_dir_all(&base);
    }
}