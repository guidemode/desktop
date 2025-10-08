-- Add core_metrics_status tracking to separate core metrics from AI processing
-- This allows core metrics to be processed quickly with debouncing,
-- while AI processing can be delayed (e.g., 10 minutes after session ends)

ALTER TABLE agent_sessions ADD COLUMN core_metrics_status TEXT DEFAULT 'pending';
ALTER TABLE agent_sessions ADD COLUMN core_metrics_processed_at INTEGER;

-- Create index for efficient querying
CREATE INDEX IF NOT EXISTS agent_sessions_core_metrics_idx ON agent_sessions(core_metrics_status);

-- Note: processing_status, queued_at, processed_at remain for AI processing tracking
