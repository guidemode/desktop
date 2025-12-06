-- Rename projects table to repositories
ALTER TABLE projects RENAME TO repositories;

-- Rename columns in agent_sessions
ALTER TABLE agent_sessions RENAME COLUMN project_name TO repository_name;
ALTER TABLE agent_sessions RENAME COLUMN project_id TO repository_id;

-- Update indexes for repositories table
DROP INDEX IF EXISTS projects_name_idx;
DROP INDEX IF EXISTS projects_type_idx;
DROP INDEX IF EXISTS projects_updated_at_idx;
CREATE INDEX IF NOT EXISTS repositories_name_idx ON repositories(name);
CREATE INDEX IF NOT EXISTS repositories_type_idx ON repositories(type);
CREATE INDEX IF NOT EXISTS repositories_updated_at_idx ON repositories(updated_at);

-- Note: agent_sessions indexes don't reference the column names directly
-- so they don't need to be recreated
