#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod engines;
mod history;
mod offline_mt;
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

            // 同步内置符号纠错开关到 OCR 纠错链路
            crate::engines::BUILTIN_SYMBOLS_ON.store(
                settings.builtin_symbols_enabled,
                std::sync::atomic::Ordering::Relaxed,
            );


            // 离线翻译下载进度推送给前端（测试等无GUI场景不注入则静默跳过）
            let _ = offline_mt::APP_HANDLE.set(app.handle().clone());

            // 主窗口尺寸：按"上次大小/固定大小"模式应用
            if let Some(main_win) = app.get_webview_window("main") {
                let (w, h) = match settings.window_size_mode.as_str() {
                    "fixed" => (settings.window_fixed_width, settings.window_fixed_height),
                    _ => (settings.window_last_width, settings.window_last_height),
                };
                if w >= 400 && h >= 300 {
                    let _ = main_win.set_size(tauri::LogicalSize::new(w as f64, h as f64));
                }
                // 启动行为：显示主界面在前台 / 隐藏在托盘（默认）
                if settings.startup_show_window {
                    let _ = main_win.show();
                    let _ = main_win.set_focus();
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

            // 截图窗口：置穿透 + TOOLWINDOW（不进 Alt+Tab）。不在此处显示——
            // 全屏窗口显隐会强制桌面重绘（文件管理器闪烁），预热由前端首帧后
            // 调用 preheat_screenshot 在屏幕外完成
            if let Some(win) = app.get_webview_window("screenshot") {
                let _ = win.set_ignore_cursor_events(true);
                #[cfg(target_os = "windows")]
                unsafe {
                    use windows::Win32::UI::WindowsAndMessaging::{
                        GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_TOOLWINDOW,
                    };
                    if let Ok(hwnd) = win.hwnd() {
                        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style | WS_EX_TOOLWINDOW.0 as isize);
                    }
                }
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
            screenshot::capture_region_store,
            screenshot::ocr_stored_capture,
            screenshot::capture_region,
            system::get_network_status,
            system::open_external,
            system::get_app_settings,
            system::save_app_settings,
            history::list_history,
            history::delete_history_entry,
            history::clear_history,
            system::trigger_screenshot_cmd,
            system::preheat_screenshot,
            system::poll_esc,
            system::set_hotkeys_suspended,
            system::download_offline_model,
            terms::list_packages,
            terms::save_package,
            terms::delete_package,
            terms::import_package_text,
            terms::list_schemes,
            terms::get_active_scheme,
            terms::add_scheme,
            terms::rename_scheme,
            terms::delete_scheme,
            terms::activate_scheme,
            terms::set_scheme_packages,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
