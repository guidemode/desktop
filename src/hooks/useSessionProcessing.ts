import type { ProcessorContext, ProcessorResult } from '@guideai-dev/session-processing/processors'
import type {
  EngagementMetrics,
  ErrorMetrics,
  GitDiffMetrics,
  PerformanceMetrics,
  QualityMetrics,
  UsageMetrics,
} from '@guideai-dev/types'
import { invoke } from '@tauri-apps/api/core'
import { useCallback, useState } from 'react'

interface SessionMetricsRow {
  id: string
  session_id: string
  provider: string
  timestamp: number
  // Performance metrics
  response_latency_ms?: number
  task_completion_time_ms?: number
  performance_total_responses?: number
  // Usage metrics
  read_write_ratio?: number
  input_clarity_score?: number
  read_operations?: number
  write_operations?: number
  total_user_messages?: number
  // Error metrics
  error_count?: number
  error_types?: string
  last_error_message?: string
  recovery_attempts?: number
  fatal_errors?: number
  // Engagement metrics
  interruption_rate?: number
  session_length_minutes?: number
  total_interruptions?: number
  engagement_total_responses?: number
  // Quality metrics
  task_success_rate?: number
  iteration_count?: number
  process_quality_score?: number
  used_plan_mode?: number
  used_todo_tracking?: number
  over_top_affirmations?: number
  successful_operations?: number
  total_operations?: number
  exit_plan_mode_count?: number
  todo_write_count?: number
  over_top_affirmations_phrases?: string
  // Improvement tips (category-specific)
  usage_improvement_tips?: string
  error_improvement_tips?: string
  engagement_improvement_tips?: string
  quality_improvement_tips?: string
  performance_improvement_tips?: string
  improvement_tips?: string // Deprecated - backward compatibility
  // Git diff metrics (desktop-only)
  git_total_files_changed?: number
  git_lines_added?: number
  git_lines_removed?: number
  git_lines_modified?: number
  git_net_lines_changed?: number
  git_lines_read_per_line_changed?: number
  git_reads_per_file_changed?: number
  git_lines_changed_per_minute?: number
  git_lines_changed_per_tool_use?: number
  total_lines_read?: number
  git_diff_improvement_tips?: string
  // Custom metrics
  custom_metrics?: string
  created_at: number
}

export function useSessionProcessing() {
  const [processing, setProcessing] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const processSession = useCallback(
    async (sessionId: string, provider: string, content: string, userId = 'local') => {
      setProcessing(true)
      setError(null)

      try {
        // Dynamic import to avoid bundling issues
        const { ProcessorRegistry } = await import('@guideai-dev/session-processing/processors')
        const registry = new ProcessorRegistry()

        // Get processor for provider
        const processor = registry.getProcessor(provider)
        if (!processor) {
          throw new Error(`No processor found for provider: ${provider}`)
        }

        // Fetch git diff data to include in processing (desktop only)
        let gitDiffData = undefined
        try {
          // Get session details from database to get cwd and commit hashes
          const sessionDetails = await invoke<any[]>('execute_sql', {
            sql: 'SELECT cwd, first_commit_hash, latest_commit_hash, session_start_time, session_end_time FROM agent_sessions WHERE session_id = ?',
            params: [sessionId],
          })

          if (sessionDetails[0]?.cwd && sessionDetails[0]?.first_commit_hash) {
            const session = sessionDetails[0]

            // Determine if session is active (no end time)
            const isActive = !session.session_end_time

            // Fetch git diff using same command as Session Changes tab
            const fileDiffs = await invoke<any[]>('get_session_git_diff', {
              cwd: session.cwd,
              firstCommitHash: session.first_commit_hash,
              latestCommitHash: session.latest_commit_hash,
              isActive,
              sessionStartTime: session.session_start_time || null,
              sessionEndTime: session.session_end_time || null,
            })

            // Transform to format expected by git diff processor
            if (fileDiffs && fileDiffs.length > 0) {
              gitDiffData = {
                files: fileDiffs.map((f: any) => ({
                  path: f.new_path,
                  stats: f.stats,
                })),
                isUnstaged: isActive,
              }
            }
          }
        } catch (err) {
          console.warn('Failed to fetch git diff for metrics processing:', err)
          // Continue without git diff data - processor will return empty metrics
        }

        // Create processing context
        const context: ProcessorContext = {
          sessionId,
          tenantId: 'local', // Desktop uses 'local' as tenant
          userId,
          provider,
          gitDiffData, // Pass git diff data if available
        }

        // Process metrics
        const results = await processor.processMetrics(content, context)

        // Store metrics in local database
        await storeMetrics(sessionId, provider, results)

        return results
      } catch (err) {
        const errorMessage = err instanceof Error ? err.message : 'Unknown error'
        console.error(`Failed to process session ${sessionId}:`, errorMessage)
        setError(errorMessage)
        throw err
      } finally {
        setProcessing(false)
      }
    },
    []
  )

  return {
    processSession,
    processing,
    error,
  }
}

/**
 * Convert processor results to desktop database format
 */
function mapResultsToRow(
  sessionId: string,
  provider: string,
  results: ProcessorResult[]
): SessionMetricsRow {
  const row: SessionMetricsRow = {
    id: crypto.randomUUID(),
    session_id: sessionId,
    provider,
    timestamp: Date.now(),
    created_at: Date.now(),
  }

  for (const result of results) {
    const metrics = result.metrics

    switch (result.metricType) {
      case 'performance': {
        const perf = metrics as PerformanceMetrics
        row.response_latency_ms = perf.response_latency_ms
        row.task_completion_time_ms = perf.task_completion_time_ms
        row.performance_total_responses = perf.metadata?.total_responses
        row.performance_improvement_tips = perf.metadata?.improvement_tips?.join('\n')
        break
      }

      case 'usage': {
        const usage = metrics as UsageMetrics
        row.read_write_ratio = usage.read_write_ratio
        row.input_clarity_score = usage.input_clarity_score
        row.read_operations = usage.metadata?.read_operations
        row.write_operations = usage.metadata?.write_operations
        row.total_user_messages = usage.metadata?.total_user_messages
        row.usage_improvement_tips = usage.metadata?.improvement_tips?.join('\n')
        break
      }

      case 'error': {
        const errors = metrics as ErrorMetrics
        row.error_count = errors.error_count
        row.error_types = errors.error_types?.join(',')
        row.last_error_message = errors.last_error_message
        row.recovery_attempts = errors.recovery_attempts
        row.fatal_errors = errors.fatal_errors
        row.error_improvement_tips = errors.metadata?.improvement_tips?.join('\n')
        break
      }

      case 'engagement': {
        const engagement = metrics as EngagementMetrics
        row.interruption_rate = engagement.interruption_rate
        row.session_length_minutes = engagement.session_length_minutes
        row.total_interruptions = engagement.metadata?.total_interruptions
        row.engagement_total_responses = engagement.metadata?.total_responses
        row.engagement_improvement_tips = engagement.metadata?.improvement_tips?.join('\n')
        break
      }

      case 'quality': {
        const quality = metrics as QualityMetrics
        row.task_success_rate = quality.task_success_rate
        row.iteration_count = quality.iteration_count
        row.process_quality_score = quality.process_quality_score
        row.used_plan_mode = quality.used_plan_mode ? 1 : 0
        row.used_todo_tracking = quality.used_todo_tracking ? 1 : 0
        row.over_top_affirmations = quality.over_top_affirmations
        row.successful_operations = quality.metadata?.successful_operations
        row.total_operations = quality.metadata?.total_operations
        row.exit_plan_mode_count = quality.metadata?.exit_plan_mode_count
        row.todo_write_count = quality.metadata?.todo_write_count
        row.over_top_affirmations_phrases =
          quality.metadata?.over_top_affirmations_phrases?.join(',')
        row.quality_improvement_tips = quality.metadata?.improvement_tips?.join('\n')
        // Keep backward compatibility
        row.improvement_tips = quality.metadata?.improvement_tips?.join('\n')
        break
      }

      case 'git-diff': {
        const gitDiff = metrics as GitDiffMetrics
        row.git_total_files_changed = gitDiff.git_total_files_changed
        row.git_lines_added = gitDiff.git_lines_added
        row.git_lines_removed = gitDiff.git_lines_removed
        row.git_lines_modified = gitDiff.git_lines_modified
        row.git_net_lines_changed = gitDiff.git_net_lines_changed
        row.git_lines_read_per_line_changed = gitDiff.git_lines_read_per_line_changed
        row.git_reads_per_file_changed = gitDiff.git_reads_per_file_changed
        row.git_lines_changed_per_minute = gitDiff.git_lines_changed_per_minute
        row.git_lines_changed_per_tool_use = gitDiff.git_lines_changed_per_tool_use
        row.total_lines_read = gitDiff.total_lines_read
        row.git_diff_improvement_tips = gitDiff.metadata?.improvement_tips?.join('\n')
        break
      }
    }
  }

  return row
}

/**
 * Store metrics in local SQLite database
 */
async function storeMetrics(
  sessionId: string,
  provider: string,
  results: ProcessorResult[]
): Promise<void> {
  const row = mapResultsToRow(sessionId, provider, results)

  // Use Tauri SQL plugin to insert or replace metrics (upsert based on session_id unique constraint)
  await invoke('execute_sql', {
    sql: `
      INSERT OR REPLACE INTO session_metrics (
        id, session_id, provider, timestamp,
        response_latency_ms, task_completion_time_ms, performance_total_responses,
        read_write_ratio, input_clarity_score, read_operations, write_operations, total_user_messages,
        error_count, error_types, last_error_message, recovery_attempts, fatal_errors,
        interruption_rate, session_length_minutes, total_interruptions, engagement_total_responses,
        task_success_rate, iteration_count, process_quality_score,
        used_plan_mode, used_todo_tracking, over_top_affirmations,
        successful_operations, total_operations, exit_plan_mode_count, todo_write_count,
        over_top_affirmations_phrases,
        usage_improvement_tips, error_improvement_tips, engagement_improvement_tips,
        quality_improvement_tips, performance_improvement_tips, improvement_tips,
        git_total_files_changed, git_lines_added, git_lines_removed, git_lines_modified,
        git_net_lines_changed, git_lines_read_per_line_changed, git_reads_per_file_changed,
        git_lines_changed_per_minute, git_lines_changed_per_tool_use, total_lines_read,
        git_diff_improvement_tips,
        created_at
      ) VALUES (
        ?, ?, ?, ?,
        ?, ?, ?,
        ?, ?, ?, ?, ?,
        ?, ?, ?, ?, ?,
        ?, ?, ?, ?,
        ?, ?, ?,
        ?, ?, ?,
        ?, ?, ?, ?,
        ?,
        ?, ?, ?,
        ?, ?, ?,
        ?, ?, ?, ?,
        ?, ?, ?,
        ?, ?, ?,
        ?,
        ?
      )
    `,
    params: [
      row.id,
      row.session_id,
      row.provider,
      row.timestamp,
      row.response_latency_ms ?? null,
      row.task_completion_time_ms ?? null,
      row.performance_total_responses ?? null,
      row.read_write_ratio ?? null,
      row.input_clarity_score ?? null,
      row.read_operations ?? null,
      row.write_operations ?? null,
      row.total_user_messages ?? null,
      row.error_count ?? null,
      row.error_types ?? null,
      row.last_error_message ?? null,
      row.recovery_attempts ?? null,
      row.fatal_errors ?? null,
      row.interruption_rate ?? null,
      row.session_length_minutes ?? null,
      row.total_interruptions ?? null,
      row.engagement_total_responses ?? null,
      row.task_success_rate ?? null,
      row.iteration_count ?? null,
      row.process_quality_score ?? null,
      row.used_plan_mode ?? null,
      row.used_todo_tracking ?? null,
      row.over_top_affirmations ?? null,
      row.successful_operations ?? null,
      row.total_operations ?? null,
      row.exit_plan_mode_count ?? null,
      row.todo_write_count ?? null,
      row.over_top_affirmations_phrases ?? null,
      row.usage_improvement_tips ?? null,
      row.error_improvement_tips ?? null,
      row.engagement_improvement_tips ?? null,
      row.quality_improvement_tips ?? null,
      row.performance_improvement_tips ?? null,
      row.improvement_tips ?? null,
      row.git_total_files_changed ?? null,
      row.git_lines_added ?? null,
      row.git_lines_removed ?? null,
      row.git_lines_modified ?? null,
      row.git_net_lines_changed ?? null,
      row.git_lines_read_per_line_changed ?? null,
      row.git_reads_per_file_changed ?? null,
      row.git_lines_changed_per_minute ?? null,
      row.git_lines_changed_per_tool_use ?? null,
      row.total_lines_read ?? null,
      row.git_diff_improvement_tips ?? null,
      row.created_at,
    ],
  })

  // Update the core_metrics_status in agent_sessions table and reset sync flag to trigger upload
  await invoke('execute_sql', {
    sql: `
      UPDATE agent_sessions
      SET core_metrics_status = 'completed',
          core_metrics_processed_at = ?,
          synced_to_server = 0
      WHERE session_id = ?
    `,
    params: [Date.now(), sessionId],
  })
}
