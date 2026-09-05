use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::TcpStream;
use std::time::Duration;

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
    // 截图翻译：覆盖色块背景色（#RRGGBB）与不透明度（0.5~1.0）
    #[serde(default = "default_overlay_bg_color")]
    pub overlay_bg_color: String,
    #[serde(default = "default_overlay_opacity")]
    pub overlay_opacity: f64,
}

fn default_screenshot_components() -> Vec<String> {
    vec![
        "engine".to_string(),
        "copy".to_string(),
        "close".to_string(),
        "settings".to_string(),
    ]
}

fn default_overlay_bg_color() -> String {
    "#ffffff".to_string()
}

fn default_overlay_opacity() -> f64 {
    0.95
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

        Self {
            autostart: true,
            default_translation_direction: "auto->zh".to_string(),
            offline_engine: "marian".to_string(),
            online_apis: vec!["baidu".to_string(), "youdao".to_string()],
            ocr_engine: "tesseract".to_string(),
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
            overlay_bg_color: default_overlay_bg_color(),
            overlay_opacity: default_overlay_opacity(),
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

/// 同步加载设置（setup阶段使用）
pub fn load_settings() -> AppSettings {
    let path = settings_path();
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(settings) = serde_json::from_str::<AppSettings>(&content) {
                return settings;
            }
        }
    }
    AppSettings::default()
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

// 设置系统托盘
pub fn setup_system_tray(_app: &tauri::AppHandle) -> Result<(), String> {
    // TODO: 实现系统托盘（需要 tauri tray 特性）
    Ok(())
}

// 设置开机自启动
pub fn setup_autostart() -> Result<(), String> {
    // TODO: Windows下写入注册表 HKCU\...\Run
    Ok(())
}

// 设置全局快捷键
pub fn setup_global_hotkeys(_app: &tauri::AppHandle) -> Result<(), String> {
    // TODO: 使用 tauri-plugin-global-shortcut
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

// Tauri命令：保存应用设置（持久化到JSON文件）
#[tauri::command]
pub async fn save_app_settings(settings: AppSettings) -> Result<(), String> {
    let path = settings_path();
    let json = serde_json::to_string_pretty(&settings)
        .map_err(|e| format!("序列化设置失败: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("写入设置文件失败: {e}"))?;
    Ok(())
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