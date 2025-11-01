/**
 * Database Mapping Utilities
 *
 * Converts snake_case SQL query results to camelCase TypeScript types.
 * This ensures type safety and consistency across the desktop app.
 */

import type { SessionMetricsData } from '@guideai-dev/types'

/**
 * SQL Row Types (snake_case to match database column names)
 */

export interface AgentSessionRow {
  id: string
  provider: string
  project_name: string
  project_id: string | null
  session_id: string
  file_name: string
  file_path: string
  file_size: number
  session_start_time: number | null
  session_end_time: number | null
  duration_ms: number | null
  processing_status: string
  error_message: string | null
  created_at: number
  updated_at: number
  file_hash: string | null
  cwd: string | null
  first_commit_hash: string | null
  last_commit_hash: string | null
  commit_count: number | null
  branch_name: string | null
  total_input_tokens: number | null
  total_output_tokens: number | null
  total_cache_read_tokens: number | null
  total_cache_creation_tokens: number | null
  tool_use_count: number | null
  bash_count: number | null
  edit_count: number | null
  read_count: number | null
  write_count: number | null
  has_user_feedback: number
  user_rating: number | null
  user_rating_reason: string | null
  ai_summary_rating: number | null
  ai_summary_rating_reason: string | null
  ai_insight: string | null
  quick_rating: string | null
  core_metrics_status: string | null
}

export interface SessionMetricsRow {
  id: string
  session_id: string
  provider: string
  timestamp: number
  response_latency_ms: number | null
  task_completion_time_ms: number | null
  read_write_ratio: number | null
  input_clarity_score: number | null
  prompt_tokens: number | null
  completion_tokens: number | null
  cache_creation_tokens: number | null
  cache_read_tokens: number | null
  error_count: number | null
  context_window_usage: number | null
  edit_operations: number | null
  function_calls: number | null
  successful_operations: number | null
  failed_operations: number | null
  files_modified: number | null
  message_count: number | null
  user_engagement_score: number | null
  session_outcome: string | null
  task_complexity: string | null
  tool_switches: number | null
  average_thinking_time_ms: number | null
  retry_count: number | null
  rollback_count: number | null
  user_interruptions: number | null
  task_completion_status: string | null
  ai_summary_quality: number | null
  created_at: number
  has_git_diff: number
  git_diff_data: string | null
}

export interface ProjectRow {
  id: string
  name: string
  path: string
  last_session_time: number | null
  session_count: number
  created_at: number
  updated_at: number
}

/**
 * Mapper Functions
 */

/**
 * Maps a SQL agent session row to a desktop-specific session type
 * Returns fields compatible with AgentSession plus desktop-specific extensions
 */
export function mapAgentSessionRow(row: AgentSessionRow) {
  // Helper to safely convert timestamp (milliseconds) to ISO string
  const toISOString = (timestamp: number | null): string | null => {
    if (!timestamp) return null
    try {
      return new Date(timestamp).toISOString()
    } catch {
      return null
    }
  }

  return {
    id: row.id,
    provider: row.provider,
    projectName: row.project_name,
    projectId: row.project_id,
    sessionId: row.session_id,
    fileName: row.file_name,
    filePath: row.file_path,
    fileSize: row.file_size,
    sessionStartTime: toISOString(row.session_start_time),
    sessionEndTime: toISOString(row.session_end_time),
    durationMs: row.duration_ms,
    processingStatus: row.processing_status as 'pending' | 'completed' | 'failed',
    createdAt: toISOString(row.created_at) ?? new Date().toISOString(),
    cwd: row.cwd,
    firstCommitHash: row.first_commit_hash,
    gitBranch: row.branch_name,
    // Desktop-specific fields (not in shared AgentSession type)
    updatedAt: toISOString(row.updated_at) ?? new Date().toISOString(),
    fileHash: row.file_hash,
    lastCommitHash: row.last_commit_hash,
    commitCount: row.commit_count,
    totalInputTokens: row.total_input_tokens,
    totalOutputTokens: row.total_output_tokens,
    totalCacheReadTokens: row.total_cache_read_tokens,
    totalCacheCreationTokens: row.total_cache_creation_tokens,
    toolUseCount: row.tool_use_count,
    bashCount: row.bash_count,
    editCount: row.edit_count,
    readCount: row.read_count,
    writeCount: row.write_count,
    hasUserFeedback: row.has_user_feedback === 1,
    userRating: row.user_rating,
    userRatingReason: row.user_rating_reason,
    aiSummaryRating: row.ai_summary_rating,
    aiSummaryRatingReason: row.ai_summary_rating_reason,
    aiInsight: row.ai_insight,
    quickRating: row.quick_rating,
    errorMessage: row.error_message,
    coreMetricsStatus: row.core_metrics_status,
  }
}

/**
 * Maps a SQL session metrics row to the TypeScript SessionMetricsData type
 */
export function mapSessionMetricsRow(row: SessionMetricsRow): SessionMetricsData {
  // Helper to safely convert timestamp (milliseconds) to ISO string
  const toISOString = (timestamp: number | null): string | null => {
    if (!timestamp) return null
    try {
      return new Date(timestamp).toISOString()
    } catch {
      return null
    }
  }

  return {
    id: row.id,
    sessionId: row.session_id,
    provider: row.provider,
    timestamp: toISOString(row.timestamp) ?? new Date().toISOString(),
    createdAt: toISOString(row.created_at) ?? new Date().toISOString(),

    // Usage metrics
    usage:
      row.read_write_ratio !== null || row.input_clarity_score !== null
        ? {
            readWriteRatio: row.read_write_ratio?.toString(),
            inputClarityScore: row.input_clarity_score ?? undefined,
            promptTokens: row.prompt_tokens ?? undefined,
            completionTokens: row.completion_tokens ?? undefined,
            cacheCreationTokens: row.cache_creation_tokens ?? undefined,
            cacheReadTokens: row.cache_read_tokens ?? undefined,
            contextWindowUsage: row.context_window_usage ?? undefined,
          }
        : undefined,

    // Error metrics
    error:
      row.error_count !== null || row.retry_count !== null
        ? {
            errorCount: row.error_count ?? undefined,
            retryCount: row.retry_count ?? undefined,
            rollbackCount: row.rollback_count ?? undefined,
          }
        : undefined,

    // Performance metrics
    performance:
      row.response_latency_ms !== null || row.task_completion_time_ms !== null
        ? {
            responseLatencyMs: row.response_latency_ms ?? undefined,
            taskCompletionTimeMs: row.task_completion_time_ms ?? undefined,
            averageThinkingTimeMs: row.average_thinking_time_ms ?? undefined,
          }
        : undefined,

    // Quality metrics
    quality:
      row.ai_summary_quality !== null || row.session_outcome !== null
        ? {
            aiSummaryQuality: row.ai_summary_quality ?? undefined,
            sessionOutcome: row.session_outcome ?? undefined,
            taskCompletionStatus: row.task_completion_status ?? undefined,
            taskComplexity: row.task_complexity ?? undefined,
          }
        : undefined,

    // Engagement metrics
    engagement:
      row.user_engagement_score !== null || row.message_count !== null
        ? {
            userEngagementScore: row.user_engagement_score ?? undefined,
            messageCount: row.message_count ?? undefined,
            userInterruptions: row.user_interruptions ?? undefined,
          }
        : undefined,

    // Operations
    editOperations: row.edit_operations ?? undefined,
    functionCalls: row.function_calls ?? undefined,
    successfulOperations: row.successful_operations ?? undefined,
    failedOperations: row.failed_operations ?? undefined,
    filesModified: row.files_modified ?? undefined,
    toolSwitches: row.tool_switches ?? undefined,

    // Git diff
    hasGitDiff: row.has_git_diff === 1,
    gitDiffData: row.git_diff_data ? JSON.parse(row.git_diff_data) : undefined,
  }
}

/**
 * Maps a SQL project row to a local project type
 */
export interface LocalProject {
  id: string
  name: string
  path: string
  lastSessionTime: string | null
  sessionCount: number
  createdAt: string
  updatedAt: string
}

export function mapProjectRow(row: ProjectRow): LocalProject {
  // Helper to safely convert timestamp (milliseconds) to ISO string
  const toISOString = (timestamp: number | null): string | null => {
    if (!timestamp) return null
    try {
      return new Date(timestamp).toISOString()
    } catch {
      return null
    }
  }

  return {
    id: row.id,
    name: row.name,
    path: row.path,
    lastSessionTime: toISOString(row.last_session_time),
    sessionCount: row.session_count,
    createdAt: toISOString(row.created_at) ?? new Date().toISOString(),
    updatedAt: toISOString(row.updated_at) ?? new Date().toISOString(),
  }
}
