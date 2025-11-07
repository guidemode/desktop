//! Claude Code provider module
//!
//! Handles conversion of native Claude Code JSONL files to canonical format.
//!
//! ## Key Responsibilities
//!
//! - **Filter system events**: Remove file-history-snapshot, summary, and system events
//! - **Add provider field**: Inject `provider: "claude-code"` into all messages
//! - **Handle nullable fields**: Clean up `parentUuid: null` and similar fields
//! - **Preserve conversational flow**: Keep only user, assistant, and meta messages

pub mod converter;
pub mod converter_utils;
pub mod scanner;
pub mod types;

// Re-export main types
pub use converter_utils::convert_to_canonical_file;
pub use scanner::scan_projects;
