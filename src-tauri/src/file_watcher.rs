use crate::config::get_config_file_path;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;
use tauri::Window;

pub struct ConfigFileWatcher {
    _watcher: RecommendedWatcher,
    _thread_handle: std::thread::JoinHandle<()>,
}

impl ConfigFileWatcher {
    pub fn new(window: Window) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let config_file_path =
            get_config_file_path().map_err(|e| format!("Failed to get config file path: {}", e))?;

        // Create a channel to receive file system events
        let (tx, rx) = mpsc::channel();

        // Create the file watcher
        let mut watcher = RecommendedWatcher::new(
            tx,
            Config::default().with_poll_interval(Duration::from_secs(1)),
        )?;

        // Watch the config file (or its parent directory if the file doesn't exist yet)
        let watch_path = if config_file_path.exists() {
            config_file_path.clone()
        } else {
            config_file_path
                .parent()
                .ok_or("Could not determine config directory")?
                .to_path_buf()
        };

        watcher.watch(&watch_path, RecursiveMode::NonRecursive)?;

        // Start a background thread to handle file events
        let thread_handle = std::thread::spawn(move || {
            let mut debounce_timer: Option<std::time::Instant> = None;
            const DEBOUNCE_DURATION: Duration = Duration::from_millis(500);

            for result in rx {
                match result {
                    Ok(event) => {
                        if Self::is_config_file_event(&event, &config_file_path) {
                            // Debounce rapid file changes
                            let now = std::time::Instant::now();
                            if let Some(last_event) = debounce_timer {
                                if now.duration_since(last_event) < DEBOUNCE_DURATION {
                                    debounce_timer = Some(now);
                                    continue;
                                }
                            }
                            debounce_timer = Some(now);

                            // Emit event to frontend
                            if let Err(e) = window.emit("config-changed", ()) {
                                eprintln!("Failed to emit config-changed event: {}", e);
                            }
                        }
                    }
                    Err(error) => {
                        eprintln!("File watcher error: {:?}", error);

                        // Try to restart the watcher after a delay
                        std::thread::sleep(Duration::from_secs(5));

                        // Note: In a production system, you might want to implement
                        // a more sophisticated restart mechanism here
                    }
                }
            }
        });

        Ok(ConfigFileWatcher {
            _watcher: watcher,
            _thread_handle: thread_handle,
        })
    }

    fn is_config_file_event(event: &Event, config_file_path: &Path) -> bool {
        // Check if this event is related to our config file
        match &event.kind {
            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
                // Check if any of the paths in the event match our config file
                event.paths.iter().any(|path| {
                    path == config_file_path
                        || (path.is_dir()
                            && path == config_file_path.parent().unwrap_or(Path::new("")))
                })
            }
            _ => false,
        }
    }
}

// Implement Drop to ensure proper cleanup
impl Drop for ConfigFileWatcher {
    fn drop(&mut self) {
        // The watcher and thread will be cleaned up automatically
        // when they go out of scope
    }
}

pub fn start_config_file_watcher(
    window: Window,
) -> Result<ConfigFileWatcher, Box<dyn std::error::Error + Send + Sync>> {
    ConfigFileWatcher::new(window)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_is_config_file_event() {
        let temp_dir = tempdir().unwrap();
        let config_file = temp_dir.path().join("config.json");

        // Create a test event for file modification
        let event = Event {
            kind: EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Content,
            )),
            paths: vec![config_file.clone()],
            attrs: Default::default(),
        };

        assert!(ConfigFileWatcher::is_config_file_event(
            &event,
            &config_file
        ));

        // Test with unrelated file
        let other_file = temp_dir.path().join("other.txt");
        let event2 = Event {
            kind: EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Content,
            )),
            paths: vec![other_file],
            attrs: Default::default(),
        };

        assert!(!ConfigFileWatcher::is_config_file_event(
            &event2,
            &config_file
        ));
    }
}
