import { v4 as uuidv4 } from 'uuid'
import { getDatabase } from '../db/client'

export interface SessionData {
  provider: string
  projectName: string
  sessionId: string
  fileName: string
  filePath: string
  fileSize: number
  sessionStartTime?: number
  sessionEndTime?: number
  durationMs?: number
}

/**
 * Insert a new session into the local database
 */
export async function insertSession(sessionData: SessionData): Promise<string> {
  const db = getDatabase()
  const id = uuidv4()
  const now = Date.now()

  const query = `
    INSERT INTO agent_sessions (
      id, provider, project_name, session_id, file_name, file_path, file_size,
      session_start_time, session_end_time, duration_ms,
      processing_status, synced_to_server,
      created_at, uploaded_at
    ) VALUES (
      ?, ?, ?, ?, ?, ?, ?,
      ?, ?, ?,
      'pending', 0,
      ?, ?
    )
  `

  await db.execute(query, [
    id,
    sessionData.provider,
    sessionData.projectName,
    sessionData.sessionId,
    sessionData.fileName,
    sessionData.filePath,
    sessionData.fileSize,
    sessionData.sessionStartTime || null,
    sessionData.sessionEndTime || null,
    sessionData.durationMs || null,
    now,
    now,
  ])

  console.log(`✓ Inserted session ${sessionData.sessionId} into local database`)
  return id
}

/**
 * Check if a session already exists in the database
 */
export async function sessionExists(sessionId: string, fileName: string): Promise<boolean> {
  const db = getDatabase()

  const result = await db.select<Array<{ count: number }>>(
    'SELECT COUNT(*) as count FROM agent_sessions WHERE session_id = ? AND file_name = ?',
    [sessionId, fileName]
  )

  return result[0]?.count > 0
}

/**
 * Insert session metrics into the local database
 */
export async function insertSessionMetrics(
  sessionId: string,
  provider: string,
  metrics: any
): Promise<string> {
  const db = getDatabase()
  const id = uuidv4()
  const now = Date.now()

  const query = `
    INSERT OR REPLACE INTO session_metrics (
      id, session_id, provider, timestamp,
      response_latency_ms, task_completion_time_ms, performance_total_responses,
      read_write_ratio, input_clarity_score, read_operations, write_operations, total_user_messages,
      error_count, error_types, last_error_message, recovery_attempts, fatal_errors,
      interruption_rate, session_length_minutes, total_interruptions, engagement_total_responses,
      task_success_rate, iteration_count, process_quality_score,
      used_plan_mode, used_todo_tracking, over_top_affirmations,
      successful_operations, total_operations, exit_plan_mode_count, todo_write_count,
      over_top_affirmations_phrases, improvement_tips, custom_metrics,
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
      ?, ?, ?,
      ?, ?, ?, ?,
      ?, ?, ?,
      ?, ?, ?,
      ?,
      ?
    )
  `

  await db.execute(query, [
    id,
    sessionId,
    provider,
    now,
    metrics.responseLatencyMs || null,
    metrics.taskCompletionTimeMs || null,
    metrics.performanceTotalResponses || null,
    metrics.readWriteRatio || null,
    metrics.inputClarityScore || null,
    metrics.readOperations || null,
    metrics.writeOperations || null,
    metrics.totalUserMessages || null,
    metrics.errorCount || null,
    metrics.errorTypes ? JSON.stringify(metrics.errorTypes) : null,
    metrics.lastErrorMessage || null,
    metrics.recoveryAttempts || null,
    metrics.fatalErrors || null,
    metrics.interruptionRate || null,
    metrics.sessionLengthMinutes || null,
    metrics.totalInterruptions || null,
    metrics.engagementTotalResponses || null,
    metrics.taskSuccessRate || null,
    metrics.iterationCount || null,
    metrics.processQualityScore || null,
    metrics.usedPlanMode ? 1 : 0,
    metrics.usedTodoTracking ? 1 : 0,
    metrics.overTopAffirmations || null,
    metrics.successfulOperations || null,
    metrics.totalOperations || null,
    metrics.exitPlanModeCount || null,
    metrics.todoWriteCount || null,
    metrics.overTopAffirmationsPhrases ? JSON.stringify(metrics.overTopAffirmationsPhrases) : null,
    metrics.improvementTips ? JSON.stringify(metrics.improvementTips) : null,
    metrics.customMetrics ? JSON.stringify(metrics.customMetrics) : null,
    metrics.gitTotalFilesChanged || null,
    metrics.gitLinesAdded || null,
    metrics.gitLinesRemoved || null,
    metrics.gitLinesModified || null,
    metrics.gitNetLinesChanged || null,
    metrics.gitLinesReadPerLineChanged || null,
    metrics.gitReadsPerFileChanged || null,
    metrics.gitLinesChangedPerMinute || null,
    metrics.gitLinesChangedPerToolUse || null,
    metrics.totalLinesRead || null,
    metrics.gitDiffImprovementTips ? JSON.stringify(metrics.gitDiffImprovementTips) : null,
    now,
  ])

  console.log(`✓ Inserted metrics for session ${sessionId} into local database`)
  return id
}

/**
 * Get all sessions from the local database
 */
export async function getAllSessions(filters?: {
  provider?: string
  limit?: number
  offset?: number
}): Promise<any[]> {
  const db = getDatabase()

  let query = 'SELECT * FROM agent_sessions'
  const params: any[] = []

  if (filters?.provider) {
    query += ' WHERE provider = ?'
    params.push(filters.provider)
  }

  query += ' ORDER BY created_at DESC'

  if (filters?.limit) {
    query += ' LIMIT ?'
    params.push(filters.limit)
  }

  if (filters?.offset) {
    query += ' OFFSET ?'
    params.push(filters.offset)
  }

  return await db.select(query, params)
}

/**
 * Get session metrics for a specific session
 */
export async function getSessionMetrics(sessionId: string): Promise<any> {
  const db = getDatabase()

  const results = (await db.select('SELECT * FROM session_metrics WHERE session_id = ?', [
    sessionId,
  ])) as any[]

  return results[0] || null
}
