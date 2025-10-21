import type { SessionMetricsUI } from '@guideai-dev/session-processing/ui'
import { useQuery } from '@tanstack/react-query'
import { invoke } from '@tauri-apps/api/core'

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
    usage:
      row.read_write_ratio !== null || row.input_clarity_score !== null
        ? {
            readWriteRatio: row.read_write_ratio?.toString(),
            inputClarityScore: row.input_clarity_score?.toString(),
            improvementTips: row.usage_improvement_tips
              ? row.usage_improvement_tips.split('\n').filter(Boolean)
              : [],
          }
        : undefined,
    error:
      row.error_count !== null
        ? {
            errorCount: row.error_count,
            fatalErrors: row.fatal_errors || 0,
            recoveryAttempts: row.recovery_attempts || 0,
            errorTypes: row.error_types ? row.error_types.split(',') : [],
            lastErrorMessage: row.last_error_message || undefined,
            improvementTips: row.error_improvement_tips
              ? row.error_improvement_tips.split('\n').filter(Boolean)
              : [],
          }
        : undefined,
    engagement:
      row.interruption_rate !== null || row.session_length_minutes !== null
        ? {
            interruptionRate: row.interruption_rate?.toString(),
            sessionLengthMinutes: row.session_length_minutes?.toString(),
            improvementTips: row.engagement_improvement_tips
              ? row.engagement_improvement_tips.split('\n').filter(Boolean)
              : [],
          }
        : undefined,
    quality:
      row.task_success_rate !== null
        ? {
            taskSuccessRate: row.task_success_rate?.toString(),
            iterationCount: row.iteration_count || 0,
            processQualityScore: row.process_quality_score?.toString(),
            usedPlanMode: row.used_plan_mode === 1,
            exitPlanModeCount: row.exit_plan_mode_count || 0,
            usedTodoTracking: row.used_todo_tracking === 1,
            todoWriteCount: row.todo_write_count || 0,
            overTopAffirmations: row.over_top_affirmations || 0,
            overTopAffirmationsPhrases: row.over_top_affirmations_phrases
              ? row.over_top_affirmations_phrases.split(',')
              : [],
            improvementTips: row.quality_improvement_tips
              ? row.quality_improvement_tips.split('\n').filter(Boolean)
              : [],
          }
        : undefined,
    performance:
      row.response_latency_ms !== null || row.task_completion_time_ms !== null
        ? {
            responseLatencyMs: row.response_latency_ms?.toString(),
            taskCompletionTimeMs: row.task_completion_time_ms?.toString(),
            improvementTips: row.performance_improvement_tips
              ? row.performance_improvement_tips.split('\n').filter(Boolean)
              : [],
          }
        : undefined,
    gitDiff:
      row.git_total_files_changed !== null
        ? {
            totalFiles: row.git_total_files_changed,
            linesAdded: row.git_lines_added,
            linesRemoved: row.git_lines_removed,
            linesModified: row.git_lines_modified,
            netLines: row.git_net_lines_changed,
            linesReadPerChanged: row.git_lines_read_per_line_changed?.toString(),
            readsPerFile: row.git_reads_per_file_changed?.toString(),
            linesPerMinute: row.git_lines_changed_per_minute?.toString(),
            linesPerTool: row.git_lines_changed_per_tool_use?.toString(),
            improvementTips: row.git_diff_improvement_tips
              ? row.git_diff_improvement_tips.split('\n').filter(Boolean)
              : [],
          }
        : undefined,
    context:
      row.total_input_tokens !== null
        ? {
            totalInputTokens: row.total_input_tokens,
            totalOutputTokens: row.total_output_tokens,
            totalCacheCreated: row.total_cache_created,
            totalCacheRead: row.total_cache_read,
            contextLength: row.context_length,
            contextWindowSize: row.context_window_size,
            contextUtilizationPercent: row.context_utilization_percent,
            compactEventCount: row.compact_event_count,
            compactEventSteps: row.compact_event_steps
              ? JSON.parse(row.compact_event_steps)
              : [],
            avgTokensPerMessage: row.avg_tokens_per_message,
            messagesUntilFirstCompact: row.messages_until_first_compact,
            improvementTips: row.context_improvement_tips
              ? JSON.parse(row.context_improvement_tips)
              : [],
          }
        : undefined,
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
  const {
    data: metrics,
    isLoading: loading,
    error,
  } = useQuery({
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
