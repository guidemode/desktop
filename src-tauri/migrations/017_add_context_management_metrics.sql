-- Add context management metrics columns for token tracking and compaction detection
-- These metrics help understand AI context usage, cache efficiency, and identify sessions
-- that hit context limits (requiring compaction)

-- Core token metrics
ALTER TABLE session_metrics ADD COLUMN total_input_tokens INTEGER;
ALTER TABLE session_metrics ADD COLUMN total_output_tokens INTEGER;
ALTER TABLE session_metrics ADD COLUMN total_cache_created INTEGER;
ALTER TABLE session_metrics ADD COLUMN total_cache_read INTEGER;
ALTER TABLE session_metrics ADD COLUMN peak_context_tokens INTEGER;
ALTER TABLE session_metrics ADD COLUMN context_window_size INTEGER;
ALTER TABLE session_metrics ADD COLUMN context_utilization_percent REAL;

-- Compaction tracking
ALTER TABLE session_metrics ADD COLUMN compact_event_count INTEGER;
ALTER TABLE session_metrics ADD COLUMN compact_event_steps TEXT; -- JSON array of step numbers where compaction occurred
ALTER TABLE session_metrics ADD COLUMN messages_until_first_compact INTEGER;

-- Efficiency metrics
ALTER TABLE session_metrics ADD COLUMN avg_tokens_per_message REAL;

-- Improvement tips for context management
ALTER TABLE session_metrics ADD COLUMN context_improvement_tips TEXT; -- JSON array of tips

-- Detailed per-message token data for visualization
ALTER TABLE session_metrics ADD COLUMN per_message_tokens TEXT; -- JSON array of token objects
-- Format: [{ step: number, input_tokens: number, output_tokens: number, cache_created: number, cache_read: number, cumulative_input: number, cumulative_output: number }]
