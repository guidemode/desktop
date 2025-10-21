-- Update context management metrics structure to match reference implementation
-- Changes peak_context_tokens to context_length to accurately represent the metric
-- This represents the most recent message's full context (input_tokens + cache_read + cache_creation)
-- rather than a peak value.

-- Rename peak_context_tokens to context_length for accuracy
-- SQLite doesn't support ALTER COLUMN RENAME, so we need to recreate
-- Note: Since this is early development and the column was just added in migration 017,
-- we can directly update the column name. In production with user data, we'd need a more
-- careful migration strategy.

-- For now, we'll add a new column and copy data if it exists
ALTER TABLE session_metrics ADD COLUMN context_length INTEGER;

-- Copy existing peak_context_tokens data to new context_length column
UPDATE session_metrics
SET context_length = peak_context_tokens
WHERE peak_context_tokens IS NOT NULL;

-- Note: We keep peak_context_tokens for backward compatibility during development
-- In a production release, we would drop it after ensuring all data is migrated
