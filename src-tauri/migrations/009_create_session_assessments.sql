-- Create session_assessments table for user survey responses
-- SQLite version - adapted from server PostgreSQL schema

CREATE TABLE IF NOT EXISTS session_assessments (
  id TEXT PRIMARY KEY NOT NULL,
  session_id TEXT NOT NULL,
  provider TEXT NOT NULL,

  -- Assessment data (stored as JSON text in SQLite)
  responses TEXT NOT NULL, -- JSON: Record<questionId, AssessmentAnswer>
  survey_type TEXT, -- 'short' | 'standard' | 'full'
  duration_seconds REAL, -- Duration in seconds
  rating TEXT, -- 'thumbs_up' | 'meh' | 'thumbs_down'

  -- Timestamps (stored as INTEGER milliseconds since epoch)
  completed_at INTEGER NOT NULL,
  created_at INTEGER NOT NULL DEFAULT (unixepoch() * 1000),

  -- Foreign key to agent_sessions
  FOREIGN KEY (session_id) REFERENCES agent_sessions (session_id) ON DELETE CASCADE
);

-- Indexes for common queries
CREATE INDEX IF NOT EXISTS session_assessments_session_idx ON session_assessments (session_id);
CREATE INDEX IF NOT EXISTS session_assessments_provider_idx ON session_assessments (provider);
CREATE INDEX IF NOT EXISTS session_assessments_completed_at_idx ON session_assessments (completed_at);
CREATE INDEX IF NOT EXISTS session_assessments_rating_idx ON session_assessments (rating);
