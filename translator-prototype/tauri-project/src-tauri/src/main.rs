#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod engines;
mod screenshot;
mod system;
mod terms;
mod translation;

use crate::translation::AppState;
use tauri::Manager;

fn main() {
    tauri::Builder::default()
        // 系统集成插件：开机自启动 + 全局快捷键
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        // 主窗口点 × 隐藏到托盘（退出走托盘菜单）
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(|app| {
            // 先加载设置（自启动/快捷键/引擎都依赖它）
            let settings = system::load_settings();

            // 托盘创建失败视为致命（否则应用启动即隐身，无法交互）
            system::setup_system_tray(app.handle())?;

            // 自启动/快捷键失败不阻断启动，仅记录
            if let Err(e) = system::apply_autostart(app.handle(), settings.autostart) {
                eprintln!("[启动] 应用自启动设置失败: {e}");
            }
            if let Err(e) = system::register_hotkeys(app.handle(), &settings) {
                eprintln!("[启动] 注册全局快捷键失败: {e}");
            }

            // 初始化翻译管理器状态（术语库从 terms.json 加载）
            let manager = translation::build_manager(&settings);
            app.manage(AppState {
                manager: tokio::sync::Mutex::new(manager),
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            translation::translate_text,
            translation::translate_lines,
            translation::get_supported_languages,
            translation::list_engines,
            translation::set_term_priority,
            translation::reload_engines,
            terms::list_terms,
            terms::add_term,
            terms::delete_term,
            terms::delete_term_translation,
            screenshot::capture_region_ocr,
            screenshot::capture_region_store,
            screenshot::ocr_stored_capture,
            screenshot::capture_region,
            screenshot::perform_ocr,
            screenshot::capture_screenshot,
            screenshot::get_extract_button_info,
            system::get_network_status,
            system::get_app_settings,
            system::save_app_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
