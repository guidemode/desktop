//! Redaction module for stripping secrets and PII from JSONL content before upload.
//!
//! Uses the same patterns as the TypeScript implementation in
//! `packages/session-processing/src/redaction/patterns.json`.
//! Patterns are embedded at compile time via `include_str!`.

mod engine;

pub use engine::{redact_jsonl_content, RedactedContent, RedactionStats};
