// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![recursion_limit = "256"]

mod auth_server;
mod claude_files;
mod commands;
mod config;
mod context_files;
mod database;
mod error;
mod events;
mod file_watcher;
mod git_diff;
mod logging;
mod project_metadata;
mod providers;
mod shutdown;
mod types;
mod upload_queue;
mod validation;

use commands::{start_enabled_watchers, AppState};
use events::{DatabaseEventHandler, EventBus, FrontendEventHandler};
use file_watcher::start_config_file_watcher;
use shutdown::ShutdownCoordinator;
use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_sql::Builder::new()
                .add_migrations(
                    "sqlite:guideai.db",
                    vec![
                        tauri_plugin_sql::Migration {
                            version: 1,
                            description: "create_agent_sessions",
                            sql: include_str!("../migrations/001_create_agent_sessions.sql"),
                            kind: tauri_plugin_sql::MigrationKind::Up,
                        },
                        tauri_plugin_sql::Migration {
                            version: 2,
                            description: "create_session_metrics",
                            sql: include_str!("../migrations/002_create_session_metrics.sql"),
                            kind: tauri_plugin_sql::MigrationKind::Up,
                        },
                        tauri_plugin_sql::Migration {
                            version: 3,
                            description: "add_cwd_column",
                            sql: include_str!("../migrations/003_add_cwd_column.sql"),
                            kind: tauri_plugin_sql::MigrationKind::Up,
                        },
                        tauri_plugin_sql::Migration {
                            version: 4,
                            description: "add_sync_failed_reason",
                            sql: include_str!("../migrations/004_add_sync_failed_reason.sql"),
                            kind: tauri_plugin_sql::MigrationKind::Up,
                        },
                        tauri_plugin_sql::Migration {
                            version: 5,
                            description: "unique_session_id",
                            sql: include_str!("../migrations/005_unique_session_id.sql"),
                            kind: tauri_plugin_sql::MigrationKind::Up,
                        },
                        tauri_plugin_sql::Migration {
                            version: 6,
                            description: "unique_session_metrics",
                            sql: include_str!("../migrations/006_unique_session_id.sql"),
                            kind: tauri_plugin_sql::MigrationKind::Up,
                        },
                        tauri_plugin_sql::Migration {
                            version: 7,
                            description: "create_projects",
                            sql: include_str!("../migrations/007_create_projects.sql"),
                            kind: tauri_plugin_sql::MigrationKind::Up,
                        },
                        tauri_plugin_sql::Migration {
                            version: 8,
                            description: "add_project_foreign_key",
                            sql: include_str!("../migrations/008_add_project_foreign_key.sql"),
                            kind: tauri_plugin_sql::MigrationKind::Up,
                        },
                        tauri_plugin_sql::Migration {
                            version: 9,
                            description: "create_session_assessments",
                            sql: include_str!("../migrations/009_create_session_assessments.sql"),
                            kind: tauri_plugin_sql::MigrationKind::Up,
                        },
                        tauri_plugin_sql::Migration {
                            version: 10,
                            description: "add_phase_analysis",
                            sql: include_str!("../migrations/010_add_phase_analysis.sql"),
                            kind: tauri_plugin_sql::MigrationKind::Up,
                        },
                        tauri_plugin_sql::Migration {
                            version: 11,
                            description: "add_core_metrics_tracking",
                            sql: include_str!("../migrations/011_add_core_metrics_tracking.sql"),
                            kind: tauri_plugin_sql::MigrationKind::Up,
                        },
                        tauri_plugin_sql::Migration {
                            version: 12,
                            description: "add_category_improvement_tips",
                            sql: include_str!(
                                "../migrations/012_add_category_improvement_tips.sql"
                            ),
                            kind: tauri_plugin_sql::MigrationKind::Up,
                        },
                        tauri_plugin_sql::Migration {
                            version: 13,
                            description: "add_file_hash",
                            sql: include_str!("../migrations/013_add_file_hash.sql"),
                            kind: tauri_plugin_sql::MigrationKind::Up,
                        },
                        tauri_plugin_sql::Migration {
                            version: 14,
                            description: "add_git_tracking",
                            sql: include_str!("../migrations/014_add_git_tracking.sql"),
                            kind: tauri_plugin_sql::MigrationKind::Up,
                        },
                        tauri_plugin_sql::Migration {
                            version: 15,
                            description: "add_git_diff_metrics",
                            sql: include_str!("../migrations/015_add_git_diff_metrics.sql"),
                            kind: tauri_plugin_sql::MigrationKind::Up,
                        },
                        tauri_plugin_sql::Migration {
                            version: 16,
                            description: "add_git_diff_improvement_tips",
                            sql: include_str!(
                                "../migrations/016_add_git_diff_improvement_tips.sql"
                            ),
                            kind: tauri_plugin_sql::MigrationKind::Up,
                        },
                        tauri_plugin_sql::Migration {
                            version: 17,
                            description: "add_context_management_metrics",
                            sql: include_str!(
                                "../migrations/017_add_context_management_metrics.sql"
                            ),
                            kind: tauri_plugin_sql::MigrationKind::Up,
                        },
                        tauri_plugin_sql::Migration {
                            version: 18,
                            description: "update_context_metrics_structure",
                            sql: include_str!(
                                "../migrations/018_update_context_metrics_structure.sql"
                            ),
                            kind: tauri_plugin_sql::MigrationKind::Up,
                        },
                        tauri_plugin_sql::Migration {
                            version: 19,
                            description: "remove_per_message_tokens",
                            sql: include_str!("../migrations/019_remove_per_message_tokens.sql"),
                            kind: tauri_plugin_sql::MigrationKind::Up,
                        },
                        tauri_plugin_sql::Migration {
                            version: 20,
                            description: "remove_peak_context_tokens",
                            sql: include_str!("../migrations/020_remove_peak_context_tokens.sql"),
                            kind: tauri_plugin_sql::MigrationKind::Up,
                        },
                    ],
                )
                .build(),
        )
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            use tracing::{error, warn};

            // Initialize logging system
            if let Err(e) = logging::init_logging() {
                error!("Failed to initialize logging: {}", e);
            }

            // Initialize database
            if let Err(e) = database::init_database() {
                error!("Failed to initialize database: {}", e);
            }

            // Set app handle on database for event emission
            database::set_app_handle(app.handle().clone());

            // Create shutdown coordinator for graceful shutdown
            let shutdown = ShutdownCoordinator::new();

            // Create event bus (1000 event buffer)
            let event_bus = EventBus::new(1000);

            // Start event handlers with shutdown coordination
            let db_handler = DatabaseEventHandler::new(event_bus.clone(), shutdown.clone());
            db_handler.start();

            let frontend_handler = FrontendEventHandler::new(
                event_bus.clone(),
                app.handle().clone(),
                shutdown.clone(),
            );
            frontend_handler.start();

            // Initialize application state with event bus
            let app_state = AppState::new(event_bus);

            // Set app handle on upload queue for event emission
            app_state.upload_queue.set_app_handle(app.handle().clone());

            // Start enabled file watchers
            start_enabled_watchers(&app_state);

            app.manage(app_state);

            // Get reference to main window for config file watcher
            let main_window = app
                .get_webview_window("main")
                .ok_or("Main window not found")?;

            // Start config file watcher with main window for event emission
            match start_config_file_watcher(main_window.as_ref().window()) {
                Ok(_watcher) => {
                    // Store the watcher in app state so it doesn't get dropped
                    app.manage(_watcher);
                }
                Err(e) => {
                    warn!("Failed to start config file watcher: {}", e);
                    // Continue without file watcher - not critical for app functionality
                }
            }

            Ok(())
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
            commands::load_setup_instructions_command,
            commands::scan_projects_command,
            commands::check_directory_exists,
            commands::add_activity_log_command,
            commands::get_activity_logs_command,
            commands::start_claude_watcher,
            commands::stop_claude_watcher,
            commands::get_claude_watcher_status,
            commands::start_copilot_watcher,
            commands::stop_copilot_watcher,
            commands::get_copilot_watcher_status,
            commands::start_opencode_watcher,
            commands::stop_opencode_watcher,
            commands::get_opencode_watcher_status,
            commands::start_codex_watcher,
            commands::stop_codex_watcher,
            commands::get_codex_watcher_status,
            commands::start_gemini_watcher,
            commands::stop_gemini_watcher,
            commands::get_gemini_watcher_status,
            commands::get_upload_queue_status,
            commands::retry_failed_uploads,
            commands::clear_failed_uploads,
            commands::get_upload_queue_items,
            commands::retry_single_upload,
            commands::remove_queue_item,
            commands::get_provider_logs,
            commands::scan_historical_sessions,
            commands::sync_historical_sessions,
            commands::get_session_sync_progress,
            commands::reset_session_sync_progress,
            commands::execute_sql,
            commands::get_session_content,
            commands::clear_all_sessions,
            commands::clear_provider_sessions,
            commands::get_all_projects,
            commands::get_project_by_id,
            commands::open_folder_in_os,
            commands::quick_rate_session,
            commands::get_session_rating,
            commands::get_session_git_diff,
            commands::scan_context_files,
            commands::scan_claude_files,
            commands::log_updater_event_command
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
