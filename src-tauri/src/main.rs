#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bt;
mod builder;
mod commands;
mod model;
mod pipeline;
mod scripts;
mod ssh;
mod store;
mod vault;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, WindowEvent};

struct QuitFlag(Arc<AtomicBool>);

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let log_dir = dir.join("logs");
            std::fs::create_dir_all(&log_dir)?;
            let conn = rusqlite::Connection::open(dir.join("data.db"))?;
            store::init(&conn)?;
            app.manage(commands::AppState {
                db: Arc::new(Mutex::new(conn)),
                deploy: Arc::new(Mutex::new(None)),
                log_dir,
            });
            app.manage(QuitFlag(Arc::new(AtomicBool::new(false))));

            // 系统托盘：显示主界面 / 退出
            let show = MenuItem::with_id(app, "show", "显示主界面", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出云梭", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("云梭 — 一键打包部署")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "quit" => {
                        app.state::<QuitFlag>().0.store(true, Ordering::Relaxed);
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                })
                .build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                let force_quit = app.state::<QuitFlag>().0.load(Ordering::Relaxed);
                let to_tray = app
                    .state::<commands::AppState>()
                    .db
                    .lock()
                    .map(|db| store::get_setting(&db, "close_to_tray", "1") == "1")
                    .unwrap_or(true);
                if !force_quit && to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_servers,
            commands::save_server,
            commands::delete_server,
            commands::test_server,
            commands::test_server_draft,
            commands::list_projects,
            commands::save_project,
            commands::delete_project,
            commands::detect_project,
            commands::get_profile,
            commands::save_profile,
            commands::list_environments,
            commands::get_setting,
            commands::set_setting,
            commands::save_text_file,
            commands::open_log_dir,
            commands::cleanup_old_logs,
            commands::start_deploy,
            commands::cancel_deploy,
            commands::rollback,
            commands::list_history,
            commands::get_history_log,
        ])
        .run(tauri::generate_context!())
        .expect("error while running DeployPilot");
}