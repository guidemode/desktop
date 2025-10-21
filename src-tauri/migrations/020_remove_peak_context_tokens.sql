-- Remove peak_context_tokens column (replaced by context_length in migration 018)
-- This column was kept temporarily for backward compatibility but is no longer needed.
-- We now use context_length which represents the current context size.

ALTER TABLE session_metrics DROP COLUMN peak_context_tokens;
