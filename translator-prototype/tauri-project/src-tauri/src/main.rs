#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod engines;
mod screenshot;
mod selection;
mod system;
mod terms;
mod translation;

use crate::translation::AppState;
use tauri::Manager;

fn main() {
    tauri::Builder::default()
        // 单实例：二次启动唤起已有实例的主窗口（必须为第一个插件）
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            system::show_main_window(app);
        }))
        // 系统集成插件：开机自启动 + 全局快捷键
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        // 主窗口点 × 隐藏到托盘（退出走托盘菜单）；隐藏前记住窗口尺寸
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    // 记住当前窗口尺寸（逻辑像素），供"上次大小"模式下次启动恢复
                    if let (Ok(size), Ok(scale)) = (window.inner_size(), window.scale_factor()) {
                        if scale > 0.0 {
                            let mut settings = system::load_settings();
                            settings.window_last_width = (size.width as f64 / scale).round() as u32;
                            settings.window_last_height = (size.height as f64 / scale).round() as u32;
                            let _ = system::save_settings(&settings);
                        }
                    }
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(|app| {
            // 先加载设置（自启动/快捷键/引擎都依赖它）
            let settings = system::load_settings();

            // 主窗口尺寸：按"上次大小/固定大小"模式应用
            if let Some(main_win) = app.get_webview_window("main") {
                let (w, h) = match settings.window_size_mode.as_str() {
                    "fixed" => (settings.window_fixed_width, settings.window_fixed_height),
                    _ => (settings.window_last_width, settings.window_last_height),
                };
                if w >= 400 && h >= 300 {
                    let _ = main_win.set_size(tauri::LogicalSize::new(w as f64, h as f64));
                }
            }

            // 托盘创建失败视为致命（否则应用启动即隐身，无法交互）
            system::setup_system_tray(app.handle())?;

            // 自启动/快捷键失败不阻断启动，仅记录
            if let Err(e) = system::apply_autostart(app.handle(), settings.autostart) {
                eprintln!("[启动] 应用自启动设置失败: {e}");
            }
            if let Err(e) = system::register_hotkeys(app.handle(), &settings) {
                eprintln!("[启动] 注册全局快捷键失败: {e}");
            }

            // 截图窗口预热：show→hide 一次，强制 WebView2 完成全屏透明合成
            // 初始化，消除首次触发截图的卡顿。预热期间前端收到事件后暂停渲染，
            // 窗口完全透明，用户无感知
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                use tauri::Emitter;
                std::thread::sleep(std::time::Duration::from_millis(1500));
                if let Some(win) = handle.get_webview_window("screenshot") {
                    let _ = win.emit("screenshot-warmup", ());
                    let _ = win.show();
                    std::thread::sleep(std::time::Duration::from_millis(350));
                    let _ = win.hide();
                    let _ = win.emit("screenshot-warmup-done", ());
                }
            });

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
            system::open_external,
            system::get_app_settings,
            system::save_app_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
