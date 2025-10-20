import type { AgentSession } from '@guideai-dev/types'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { invoke } from '@tauri-apps/api/core'
import { useEffect, useState } from 'react'

interface SessionWithMetrics extends AgentSession {
  filePath: string | null
  syncedToServer?: boolean
  syncFailedReason?: string | null
  cwd?: string | null
  metrics?: {
    // Performance
    response_latency_ms?: number
    task_completion_time_ms?: number
    // Usage
    read_write_ratio?: number
    input_clarity_score?: number
    // Quality
    task_success_rate?: number
    iteration_count?: number
    process_quality_score?: number
    used_plan_mode?: boolean
    used_todo_tracking?: boolean
    // Engagement
    interruption_rate?: number
    session_length_minutes?: number
    // Error
    error_count?: number
    fatal_errors?: number
  }
}

export type DateFilterOption =
  | 'all'
  | 'last24hrs'
  | 'today'
  | 'yesterday'
  | 'this-week'
  | 'last-week'
  | 'range'

export interface DateRange {
  from: string
  to: string
}

export interface DateFilterValue {
  option: DateFilterOption
  range?: DateRange
}

interface SessionFilters {
  provider?: string
  projectId?: string
  dateFilter?: DateFilterValue
}

function buildDateWhereClause(dateFilter: DateFilterValue): { clause: string; params: any[] } {
  if (dateFilter.option === 'all') {
    return { clause: '', params: [] }
  }

  const now = new Date()

  switch (dateFilter.option) {
    case 'last24hrs': {
      const twentyFourHoursAgo = now.getTime() - 24 * 60 * 60 * 1000
      return {
        clause:
          'AND (s.session_end_time >= ? OR (s.session_end_time IS NULL AND s.session_start_time >= ?))',
        params: [twentyFourHoursAgo, twentyFourHoursAgo],
      }
    }
    case 'today': {
      const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime()
      return {
        clause:
          'AND (s.session_end_time >= ? OR (s.session_end_time IS NULL AND s.session_start_time >= ?))',
        params: [startOfToday, startOfToday],
      }
    }
    case 'yesterday': {
      const startOfYesterday = new Date(
        now.getFullYear(),
        now.getMonth(),
        now.getDate() - 1
      ).getTime()
      const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime()
      return {
        clause:
          'AND ((s.session_end_time >= ? AND s.session_end_time < ?) OR (s.session_end_time IS NULL AND s.session_start_time >= ? AND s.session_start_time < ?))',
        params: [startOfYesterday, startOfToday, startOfYesterday, startOfToday],
      }
    }
    case 'this-week': {
      const startOfWeek = new Date(now)
      startOfWeek.setDate(now.getDate() - now.getDay())
      startOfWeek.setHours(0, 0, 0, 0)
      const startOfWeekMs = startOfWeek.getTime()
      return {
        clause:
          'AND (s.session_end_time >= ? OR (s.session_end_time IS NULL AND s.session_start_time >= ?))',
        params: [startOfWeekMs, startOfWeekMs],
      }
    }
    case 'last-week': {
      const startOfLastWeek = new Date(now)
      startOfLastWeek.setDate(now.getDate() - now.getDay() - 7)
      startOfLastWeek.setHours(0, 0, 0, 0)
      const startOfThisWeek = new Date(now)
      startOfThisWeek.setDate(now.getDate() - now.getDay())
      startOfThisWeek.setHours(0, 0, 0, 0)
      const startOfLastWeekMs = startOfLastWeek.getTime()
      const startOfThisWeekMs = startOfThisWeek.getTime()
      return {
        clause:
          'AND ((s.session_end_time >= ? AND s.session_end_time < ?) OR (s.session_end_time IS NULL AND s.session_start_time >= ? AND s.session_start_time < ?))',
        params: [startOfLastWeekMs, startOfThisWeekMs, startOfLastWeekMs, startOfThisWeekMs],
      }
    }
    case 'range': {
      if (!dateFilter.range) {
        return { clause: '', params: [] }
      }
      const fromDate = new Date(dateFilter.range.from)
      fromDate.setHours(0, 0, 0, 0)
      const toDate = new Date(dateFilter.range.to)
      toDate.setHours(23, 59, 59, 999)
      const fromMs = fromDate.getTime()
      const toMs = toDate.getTime()
      return {
        clause:
          'AND ((s.session_end_time >= ? AND s.session_end_time <= ?) OR (s.session_end_time IS NULL AND s.session_start_time >= ? AND s.session_start_time <= ?))',
        params: [fromMs, toMs, fromMs, toMs],
      }
    }
    default:
      return { clause: '', params: [] }
  }
}

async function fetchSessions(filters: SessionFilters = {}): Promise<SessionWithMetrics[]> {
  const { provider, projectId, dateFilter } = filters

  // Build WHERE clause conditions
  const whereConditions: string[] = []
  const params: any[] = []

  if (provider) {
    whereConditions.push('s.provider = ?')
    params.push(provider)
  }

  if (projectId) {
    whereConditions.push('s.project_id = ?')
    params.push(projectId)
  }

  // Add date filter
  if (dateFilter) {
    const dateWhere = buildDateWhereClause(dateFilter)
    if (dateWhere.clause) {
      whereConditions.push(dateWhere.clause.replace('AND ', ''))
      params.push(...dateWhere.params)
    }
  }

  const whereClause = whereConditions.length > 0 ? `WHERE ${whereConditions.join(' AND ')}` : ''

  // Build query
  const query = `
    SELECT
      s.*,
      s.project_id,
      COALESCE(p.name, s.project_name) as project_name,
      m.response_latency_ms,
      m.task_completion_time_ms,
      m.read_write_ratio,
      m.input_clarity_score,
      m.task_success_rate,
      m.iteration_count,
      m.process_quality_score,
      m.used_plan_mode,
      m.used_todo_tracking,
      m.interruption_rate,
      m.session_length_minutes,
      m.error_count,
      m.fatal_errors,
      a.rating as assessment_rating
    FROM agent_sessions s
    LEFT JOIN session_metrics m ON s.session_id = m.session_id
    LEFT JOIN session_assessments a ON s.session_id = a.session_id
    LEFT JOIN projects p ON s.project_id = p.id
    ${whereClause}
    ORDER BY s.session_end_time DESC NULLS LAST
  `

  const result: any[] = await invoke('execute_sql', {
    sql: query,
    params,
  })

  console.log(`[useLocalSessions] Loaded ${result.length} sessions from database`)

  // Transform results to include metrics as nested object
  const sessionsWithMetrics: SessionWithMetrics[] = result.map(row => {
    const hasMetrics =
      row.response_latency_ms !== null ||
      row.task_completion_time_ms !== null ||
      row.read_write_ratio !== null

    if (!hasMetrics && row.core_metrics_status === 'completed') {
      console.warn(
        `[useLocalSessions] ⚠️  Session ${row.session_id} has core_metrics_status='completed' but NO metrics in LEFT JOIN result!`
      )
    }

    const metrics = hasMetrics
      ? {
          response_latency_ms: row.response_latency_ms,
          task_completion_time_ms: row.task_completion_time_ms,
          read_write_ratio: row.read_write_ratio,
          input_clarity_score: row.input_clarity_score,
          task_success_rate: row.task_success_rate,
          iteration_count: row.iteration_count,
          process_quality_score: row.process_quality_score,
          used_plan_mode: row.used_plan_mode === 1,
          used_todo_tracking: row.used_todo_tracking === 1,
          interruption_rate: row.interruption_rate,
          session_length_minutes: row.session_length_minutes,
          error_count: row.error_count,
          fatal_errors: row.fatal_errors,
        }
      : undefined

    return {
      id: row.id,
      sessionId: row.session_id,
      provider: row.provider,
      fileName: row.file_name || '',
      filePath: row.file_path,
      userId: '', // Local database doesn't have user info
      username: '', // Local database doesn't have user info
      projectName: row.project_name || 'Unknown Project',
      projectId: row.project_id || null,
      // Timestamps are stored as milliseconds in SQLite
      sessionStartTime: row.session_start_time
        ? new Date(row.session_start_time).toISOString()
        : null,
      sessionEndTime: row.session_end_time ? new Date(row.session_end_time).toISOString() : null,
      fileSize: row.file_size || 0,
      durationMs: row.duration_ms || null,
      processingStatus: row.processing_status || 'pending',
      processedAt: row.processed_at ? new Date(row.processed_at).toISOString() : null,
      coreMetricsStatus: row.core_metrics_status || 'pending',
      coreMetricsProcessedAt: row.core_metrics_processed_at
        ? new Date(row.core_metrics_processed_at).toISOString()
        : null,
      assessmentStatus: row.assessment_status || 'not_started',
      assessmentCompletedAt: row.assessment_completed_at
        ? new Date(row.assessment_completed_at).toISOString()
        : null,
      assessmentRating: row.assessment_rating || null,
      aiModelSummary: row.ai_model_summary || null,
      aiModelQualityScore: row.ai_model_quality_score || null,
      aiModelMetadata: row.ai_model_metadata ? JSON.parse(row.ai_model_metadata) : null,
      aiModelPhaseAnalysis: row.ai_model_phase_analysis
        ? JSON.parse(row.ai_model_phase_analysis)
        : null,
      createdAt: row.created_at ? new Date(row.created_at).toISOString() : new Date().toISOString(),
      uploadedAt: row.uploaded_at
        ? new Date(row.uploaded_at).toISOString()
        : new Date().toISOString(),
      syncedToServer: row.synced_to_server === 1,
      syncFailedReason: row.sync_failed_reason || null,
      cwd: row.cwd || null,
      gitBranch: row.git_branch || null,
      firstCommitHash: row.first_commit_hash || null,
      latestCommitHash: row.latest_commit_hash || null,
      metrics,
    }
  })

  return sessionsWithMetrics
}

export function useLocalSessions(filters?: SessionFilters) {
  const {
    data: sessions = [],
    isLoading: loading,
    error,
    refetch,
  } = useQuery({
    queryKey: ['local-sessions', filters?.provider, filters?.projectId, filters?.dateFilter],
    queryFn: () => fetchSessions(filters),
  })

  return {
    sessions,
    loading,
    error: error ? (error as Error).message : null,
    refresh: refetch,
  }
}

// Hook to invalidate sessions cache from anywhere
export function useInvalidateSessions() {
  const queryClient = useQueryClient()

  return () => {
    queryClient.invalidateQueries({ queryKey: ['local-sessions'] })
  }
}

/**
 * Hook to get a single session with its content and metrics
 */
export function useLocalSession(sessionId: string) {
  const [session, setSession] = useState<SessionWithMetrics | null>(null)
  const [content, setContent] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    loadSession()
  }, [sessionId])

  const loadSession = async () => {
    setLoading(true)
    setError(null)

    try {
      // Load session with metrics
      const sessionResult: any[] = await invoke('execute_sql', {
        sql: `
          SELECT
            s.*,
            COALESCE(p.name, s.project_name) as project_name,
            m.response_latency_ms,
            m.task_completion_time_ms,
            m.read_write_ratio,
            m.input_clarity_score,
            m.task_success_rate,
            m.iteration_count,
            m.process_quality_score,
            m.used_plan_mode,
            m.used_todo_tracking,
            m.interruption_rate,
            m.session_length_minutes,
            m.error_count,
            m.fatal_errors,
            m.improvement_tips
          FROM agent_sessions s
          LEFT JOIN session_metrics m ON s.session_id = m.session_id
          LEFT JOIN projects p ON s.project_id = p.id
          WHERE s.session_id = ?
        `,
        params: [sessionId],
      })

      if (sessionResult.length === 0) {
        throw new Error('Session not found')
      }

      const row = sessionResult[0]

      const metrics =
        row.response_latency_ms !== null ||
        row.task_completion_time_ms !== null ||
        row.read_write_ratio !== null
          ? {
              response_latency_ms: row.response_latency_ms,
              task_completion_time_ms: row.task_completion_time_ms,
              read_write_ratio: row.read_write_ratio,
              input_clarity_score: row.input_clarity_score,
              task_success_rate: row.task_success_rate,
              iteration_count: row.iteration_count,
              process_quality_score: row.process_quality_score,
              used_plan_mode: row.used_plan_mode === 1,
              used_todo_tracking: row.used_todo_tracking === 1,
              interruption_rate: row.interruption_rate,
              session_length_minutes: row.session_length_minutes,
              error_count: row.error_count,
              fatal_errors: row.fatal_errors,
              improvement_tips: row.improvement_tips ? row.improvement_tips.split('\n') : [],
            }
          : undefined

      const sessionData = {
        id: row.id,
        sessionId: row.session_id,
        provider: row.provider,
        fileName: row.file_name || '',
        userId: '', // Local database doesn't have user info
        username: '', // Local database doesn't have user info
        projectName: row.project_name || 'Unknown Project',
        projectId: row.project_id || null,
        // Timestamps are stored as milliseconds in SQLite
        sessionStartTime: row.session_start_time
          ? new Date(row.session_start_time).toISOString()
          : null,
        sessionEndTime: row.session_end_time ? new Date(row.session_end_time).toISOString() : null,
        fileSize: row.file_size || 0,
        durationMs: row.duration_ms || null,
        processingStatus: row.processing_status || 'pending',
        processedAt: row.processed_at ? new Date(row.processed_at).toISOString() : null,
        coreMetricsStatus: row.core_metrics_status || 'pending',
        coreMetricsProcessedAt: row.core_metrics_processed_at
          ? new Date(row.core_metrics_processed_at).toISOString()
          : null,
        assessmentStatus: row.assessment_status || 'not_started',
        assessmentCompletedAt: row.assessment_completed_at
          ? new Date(row.assessment_completed_at).toISOString()
          : null,
        assessmentRating: row.assessment_rating || null,
        aiModelSummary: row.ai_model_summary || null,
        aiModelQualityScore: row.ai_model_quality_score || null,
        aiModelMetadata: row.ai_model_metadata ? JSON.parse(row.ai_model_metadata) : null,
        aiModelPhaseAnalysis: row.ai_model_phase_analysis
          ? JSON.parse(row.ai_model_phase_analysis)
          : null,
        createdAt: row.created_at
          ? new Date(row.created_at).toISOString()
          : new Date().toISOString(),
        uploadedAt: row.uploaded_at
          ? new Date(row.uploaded_at).toISOString()
          : new Date().toISOString(),
        syncedToServer: row.synced_to_server === 1,
        syncFailedReason: row.sync_failed_reason || null,
        filePath: row.file_path,
        cwd: row.cwd || null,
        gitBranch: row.git_branch || null,
        firstCommitHash: row.first_commit_hash || null,
        latestCommitHash: row.latest_commit_hash || null,
        metrics,
      }

      setSession(sessionData)

      // Load content from file
      const fileContent: string = await invoke('read_session_file', {
        filePath: row.file_path,
      })
      setContent(fileContent)
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to load session'
      setError(errorMessage)
      console.error('Error loading session:', err)
    } finally {
      setLoading(false)
    }
  }

  return {
    session,
    content,
    loading,
    error,
    refresh: loadSession,
  }
}
