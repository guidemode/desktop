-- Add category-specific improvement tips columns
-- Previously all tips were stored in a single improvement_tips column
-- Now we separate them by metric category for better UI organization

ALTER TABLE session_metrics ADD COLUMN usage_improvement_tips TEXT;
ALTER TABLE session_metrics ADD COLUMN error_improvement_tips TEXT;
ALTER TABLE session_metrics ADD COLUMN engagement_improvement_tips TEXT;
ALTER TABLE session_metrics ADD COLUMN quality_improvement_tips TEXT;
ALTER TABLE session_metrics ADD COLUMN performance_improvement_tips TEXT;

-- Migrate existing data from improvement_tips to quality_improvement_tips
-- (since historically improvement_tips came from the quality processor)
UPDATE session_metrics SET quality_improvement_tips = improvement_tips WHERE improvement_tips IS NOT NULL;

-- Note: We keep the old improvement_tips column for backward compatibility
-- It can be deprecated in a future migration
