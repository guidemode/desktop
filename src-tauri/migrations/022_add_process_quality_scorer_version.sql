-- Records which version of the deterministic process-quality scorer produced
-- process_quality_score. Scores from different versions are not comparable, so anything
-- that trends or aggregates the score needs to be able to group by it.
ALTER TABLE session_metrics ADD COLUMN process_quality_scorer_version TEXT;
