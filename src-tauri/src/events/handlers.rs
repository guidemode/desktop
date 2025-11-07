use super::{EventBus, SessionEvent, SessionEventPayload};
use crate::database;
use crate::logging::{log_error, log_info};
use crate::shutdown::ShutdownCoordinator;
use tauri::Emitter;
use tokio::sync::broadcast;

/// Handler that writes events to database
pub struct DatabaseEventHandler {
    event_bus: EventBus,
    shutdown: ShutdownCoordinator,
}

impl DatabaseEventHandler {
    pub fn new(event_bus: EventBus, shutdown: ShutdownCoordinator) -> Self {
        Self {
            event_bus,
            shutdown,
        }
    }

    pub fn start(self) {
        tauri::async_runtime::spawn(async move {
            let mut rx = self.event_bus.subscribe();
            let mut shutdown_rx = self.shutdown.subscribe();

            loop {
                tokio::select! {
                    result = rx.recv() => {
                        match result {
                            Ok(event) => {
                                if let Err(e) = self.handle_event(&event) {
                                    log_error(&event.provider, &format!("Database handler error: {}", e))
                                        .unwrap_or_default();
                                }
                            }
                            Err(broadcast::error::RecvError::Closed) => {
                                log_info("events", "Database handler stopped (event bus closed)").unwrap_or_default();
                                break;
                            }
                            Err(broadcast::error::RecvError::Lagged(n)) => {
                                log_error("events", &format!("Database handler lagged {} events", n))
                                    .unwrap_or_default();
                            }
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        log_info("events", "Database handler gracefully shutting down").unwrap_or_default();
                        break;
                    }
                }
            }
        });
    }

    fn handle_event(&self, event: &SessionEvent) -> Result<(), String> {
        match &event.payload {
            SessionEventPayload::SessionChanged {
                session_id,
                project_name,
                file_path,
                file_size,
            } => {
                // Use db_helpers which does:
                // - Smart insert-or-update (tries insert, falls back to update)
                // - Extracts CWD, git info, and timing from file
                // - Links session to project
                crate::providers::common::db_helpers::insert_session_immediately(
                    &event.provider,
                    project_name,
                    session_id,
                    file_path,
                    *file_size,
                    None, // file_hash will be calculated during upload
                )
                .map_err(|e| e.to_string())?;
            }

            SessionEventPayload::Completed {
                session_id,
                start_time,
                end_time,
                ..
            } => {
                // Update with timing information
                database::update_session(
                    session_id,
                    "", // file_name not changed
                    "", // file_path not changed
                    0, // file_size not changed
                    None,
                    Some(*start_time),
                    Some(*end_time),
                    None,
                    None,
                    None,
                )
                .map_err(|e| e.to_string())?;
            }

            SessionEventPayload::Failed { session_id, reason } => {
                database::mark_session_sync_failed(session_id, reason)
                    .map_err(|e| e.to_string())?;
            }
        }

        Ok(())
    }
}

/// Handler that emits events to frontend
pub struct FrontendEventHandler {
    event_bus: EventBus,
    app_handle: tauri::AppHandle,
    shutdown: ShutdownCoordinator,
}

impl FrontendEventHandler {
    pub fn new(
        event_bus: EventBus,
        app_handle: tauri::AppHandle,
        shutdown: ShutdownCoordinator,
    ) -> Self {
        Self {
            event_bus,
            app_handle,
            shutdown,
        }
    }

    pub fn start(self) {
        tauri::async_runtime::spawn(async move {
            let mut rx = self.event_bus.subscribe();
            let mut shutdown_rx = self.shutdown.subscribe();

            loop {
                tokio::select! {
                    result = rx.recv() => {
                        match result {
                            Ok(event) => {
                                // Emit different events based on payload type
                                match &event.payload {
                                    SessionEventPayload::SessionChanged { session_id, .. } => {
                                        let _ = self.app_handle.emit("session-updated", session_id);
                                    }

                                    SessionEventPayload::Completed { session_id, .. } => {
                                        let _ = self.app_handle.emit("session-completed", session_id);
                                    }

                                    _ => {}
                                }
                            }
                            Err(broadcast::error::RecvError::Closed) => {
                                log_info("events", "Frontend handler stopped (event bus closed)").unwrap_or_default();
                                break;
                            }
                            Err(_) => continue,
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        log_info("events", "Frontend handler gracefully shutting down").unwrap_or_default();
                        break;
                    }
                }
            }
        });
    }
}
