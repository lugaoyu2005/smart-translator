#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod engines;
mod screenshot;
mod system;
mod translation;

use crate::translation::AppState;
use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            // 初始化系统模块（托盘、自启动、快捷键为后续功能）
            system::setup_system_tray(app.handle())?;
            system::setup_autostart()?;
            system::setup_global_hotkeys(app.handle())?;

            // 初始化翻译管理器状态
            let settings = system::load_settings();
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
            screenshot::capture_region_ocr,
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