-- Add ai_model_phase_analysis column to agent_sessions table
-- This stores the structured phase analysis result from AI processing
ALTER TABLE agent_sessions ADD COLUMN ai_model_phase_analysis TEXT;
