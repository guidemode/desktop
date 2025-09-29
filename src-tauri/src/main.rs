// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod auth_server;
mod commands;
mod config;
mod file_watcher;
mod logging;
mod providers;
mod upload_queue;

use commands::{AppState, start_enabled_watchers};
use file_watcher::start_config_file_watcher;
use tauri::{
    CustomMenuItem, LogicalPosition, LogicalSize, Manager, Size, SystemTray, SystemTrayEvent,
    SystemTrayMenu,
};
use tauri_plugin_positioner::{Position, WindowExt};

fn main() {
    let quit = CustomMenuItem::new("quit".to_string(), "Quit").accelerator("Cmd+Q");
    let system_tray_menu = SystemTrayMenu::new().add_item(quit);

    tauri::Builder::default()
        .plugin(tauri_plugin_positioner::init())
        .setup(|app| {
            // Initialize logging system
            if let Err(e) = logging::init_logging() {
                eprintln!("Failed to initialize logging: {}", e);
            }

            // Initialize application state
            let app_state = AppState::new();

            // Start enabled file watchers
            start_enabled_watchers(&app_state);

            app.manage(app_state);

            // Initialize file watcher for config changes
            let window = app.get_window("main").unwrap();

            // Hide window on startup for menubar-only behavior
            window.hide().unwrap();

            if let Ok(Some(monitor)) = window.current_monitor() {
                let scale_factor = monitor.scale_factor();
                let size = monitor.size().to_logical::<f64>(scale_factor);
                let position = monitor.position().to_logical::<f64>(scale_factor);

                let logical_width = (size.width * 0.9).round();
                let logical_height = (size.height * 0.9).round();

                let centered_x = position.x + (size.width - logical_width) / 2.0;
                let centered_y = position.y + (size.height - logical_height) / 2.0;

                let _ = window.set_size(Size::Logical(LogicalSize::new(
                    logical_width,
                    logical_height,
                )));
                let _ = window.set_position(tauri::Position::Logical(LogicalPosition::new(
                    centered_x, centered_y,
                )));
            }

            match start_config_file_watcher(window.clone()) {
                Ok(_watcher) => {
                    // Store the watcher in app state so it doesn't get dropped
                    app.manage(_watcher);
                }
                Err(e) => {
                    eprintln!("Failed to start config file watcher: {}", e);
                    // Continue without file watcher - not critical for app functionality
                }
            }

            Ok(())
        })
        .system_tray(SystemTray::new().with_menu(system_tray_menu))
        .on_system_tray_event(|app, event| {
            tauri_plugin_positioner::on_tray_event(app, &event);
            match event {
                SystemTrayEvent::LeftClick {
                    position: _,
                    size: _,
                    ..
                } => {
                    let window = app.get_window("main").unwrap();
                    let _ = window.move_window(Position::TrayCenter);

                    if window.is_visible().unwrap() {
                        window.hide().unwrap();
                    } else {
                        window.show().unwrap();
                        window.set_focus().unwrap();
                    }
                }
                SystemTrayEvent::MenuItemClick { id, .. } => match id.as_str() {
                    "quit" => {
                        std::process::exit(0);
                    }
                    _ => {}
                },
                _ => {}
            }
        })
        .on_window_event(|event| match event.event() {
            tauri::WindowEvent::Focused(is_focused) => {
                // Hide immediately when focus is lost since this is a menubar-only app
                if !is_focused {
                    event.window().hide().unwrap();
                }
            }
            tauri::WindowEvent::CloseRequested { api, .. } => {
                // keep app running when close button is pressed; hide instead
                api.prevent_close();
                event.window().hide().unwrap();
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            commands::load_config_command,
            commands::save_config_command,
            commands::clear_config_command,
            commands::login_command,
            commands::logout_command,
            commands::load_provider_config_command,
            commands::save_provider_config_command,
            commands::delete_provider_config_command,
            commands::scan_projects_command,
            commands::add_activity_log_command,
            commands::get_activity_logs_command,
            commands::start_claude_watcher,
            commands::stop_claude_watcher,
            commands::get_claude_watcher_status,
            commands::start_opencode_watcher,
            commands::stop_opencode_watcher,
            commands::get_opencode_watcher_status,
            commands::get_upload_queue_status,
            commands::retry_failed_uploads,
            commands::clear_failed_uploads,
            commands::get_provider_logs,
            commands::scan_historical_sessions,
            commands::sync_historical_sessions,
            commands::get_session_sync_progress,
            commands::reset_session_sync_progress
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
