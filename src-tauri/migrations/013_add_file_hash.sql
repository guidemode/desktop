-- Add file_hash column for deduplication (v2 upload)
ALTER TABLE agent_sessions ADD COLUMN file_hash TEXT;

-- Create index for hash lookups
CREATE INDEX IF NOT EXISTS agent_sessions_file_hash_idx
ON agent_sessions(file_hash);
