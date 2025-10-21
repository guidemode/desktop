-- Remove per_message_tokens column from session_metrics
-- This column stored detailed per-message token data as JSON which could get very large.
-- Token usage graphs will be computed on-demand from the session transcript instead.

-- SQLite doesn't support DROP COLUMN directly in older versions,
-- but modern SQLite (3.35.0+, 2021) supports it.
-- Tauri uses a modern SQLite version, so this should work.

ALTER TABLE session_metrics DROP COLUMN per_message_tokens;
