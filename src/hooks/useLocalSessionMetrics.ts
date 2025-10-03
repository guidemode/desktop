import { useQuery } from '@tanstack/react-query'
import { invoke } from '@tauri-apps/api/core'
import type { SessionMetricsUI } from '@guideai-dev/session-processing/ui'

interface UseLocalSessionMetricsResult {
  metrics: SessionMetricsUI | null
  loading: boolean
  error: string | null
}

async function fetchSessionMetrics(sessionId: string): Promise<SessionMetricsUI | null> {
  // Fetch metrics from local database
  const result: any[] = await invoke('execute_sql', {
    sql: `
      SELECT *
      FROM session_metrics
      WHERE session_id = ?
      ORDER BY created_at DESC
      LIMIT 1
    `,
    params: [sessionId],
  })

  if (result.length === 0) {
    return null
  }

  const row = result[0]

  // Transform database row to SessionMetricsUI format
  const metricsData: SessionMetricsUI = {
    createdAt: row.created_at ? new Date(row.created_at).toISOString() : undefined,
    usage: row.read_write_ratio !== null || row.input_clarity_score !== null ? {
      readWriteRatio: row.read_write_ratio?.toString(),
      inputClarityScore: row.input_clarity_score?.toString(),
      improvementTips: row.improvement_tips ? row.improvement_tips.split('\n').filter(Boolean) : [],
    } : undefined,
    error: row.error_count !== null ? {
      errorCount: row.error_count,
      fatalErrors: row.fatal_errors || 0,
      recoveryAttempts: row.recovery_attempts || 0,
      errorTypes: row.error_types ? row.error_types.split(',') : [],
      lastErrorMessage: row.last_error_message || undefined,
      improvementTips: [],
    } : undefined,
    engagement: row.interruption_rate !== null || row.session_length_minutes !== null ? {
      interruptionRate: row.interruption_rate?.toString(),
      sessionLengthMinutes: row.session_length_minutes?.toString(),
      improvementTips: [],
    } : undefined,
    quality: row.task_success_rate !== null ? {
      taskSuccessRate: row.task_success_rate?.toString(),
      iterationCount: row.iteration_count || 0,
      processQualityScore: row.process_quality_score?.toString(),
      usedPlanMode: row.used_plan_mode === 1,
      exitPlanModeCount: row.exit_plan_mode_count || 0,
      usedTodoTracking: row.used_todo_tracking === 1,
      todoWriteCount: row.todo_write_count || 0,
      overTopAffirmations: row.over_top_affirmations || 0,
      overTopAffirmationsPhrases: row.over_top_affirmations_phrases ? row.over_top_affirmations_phrases.split(',') : [],
      improvementTips: row.improvement_tips ? row.improvement_tips.split('\n').filter(Boolean) : [],
    } : undefined,
    performance: row.response_latency_ms !== null || row.task_completion_time_ms !== null ? {
      responseLatencyMs: row.response_latency_ms?.toString(),
      taskCompletionTimeMs: row.task_completion_time_ms?.toString(),
      improvementTips: [],
    } : undefined,
  }

  return metricsData
}

/**
 * Hook to fetch session metrics from local database
 * Returns null if no metrics have been processed locally
 */
export function useLocalSessionMetrics(
  sessionId: string | undefined
): UseLocalSessionMetricsResult {
  const { data: metrics, isLoading: loading, error } = useQuery({
    queryKey: ['session-metrics', sessionId],
    queryFn: () => fetchSessionMetrics(sessionId!),
    enabled: !!sessionId,
  })

  return {
    metrics: metrics ?? null,
    loading,
    error: error ? (error as Error).message : null,
  }
}
