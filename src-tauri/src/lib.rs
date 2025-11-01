// Library exports for integration tests
// This allows tests to access internal modules

#![recursion_limit = "256"]

pub mod claude_files;
pub mod config;
pub mod database;
pub mod error;
pub mod events;
pub mod logging;
pub mod project_metadata;
pub mod providers;
pub mod shutdown;
pub mod upload_queue;
pub mod validation;
