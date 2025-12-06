import { index, integer, sqliteTable, text } from 'drizzle-orm/sqlite-core'

export const processingStatusEnum = ['pending', 'processing', 'completed', 'failed'] as const
export const assessmentStatusEnum = [
  'not_started',
  'rating_only',
  'in_progress',
  'completed',
] as const

export const agentSessions = sqliteTable(
  'agent_sessions',
  {
    id: text('id').primaryKey(),
    provider: text('provider').notNull(), // e.g., 'claude-code', 'opencode', 'codex'
    projectName: text('project_name').notNull(),
    sessionId: text('session_id').notNull(), // UUID from provider
    fileName: text('file_name').notNull(),
    filePath: text('file_path').notNull(), // Local file path instead of R2
    fileSize: integer('file_size').notNull(), // Size in bytes
    sessionStartTime: integer('session_start_time', { mode: 'timestamp_ms' }), // Timestamp
    sessionEndTime: integer('session_end_time', { mode: 'timestamp_ms' }), // Timestamp
    durationMs: integer('duration_ms'), // Duration in milliseconds
    processingStatus: text('processing_status', { enum: processingStatusEnum }).default('pending'),
    queuedAt: integer('queued_at', { mode: 'timestamp_ms' }), // When queued for processing
    processedAt: integer('processed_at', { mode: 'timestamp_ms' }),
    assessmentStatus: text('assessment_status', { enum: assessmentStatusEnum }).default(
      'not_started'
    ),
    assessmentCompletedAt: integer('assessment_completed_at', { mode: 'timestamp_ms' }),
    repositoryId: text('repository_id'), // Optional reference to repositories
    // AI Model fields
    aiModelSummary: text('ai_model_summary'), // Generated summary from AI model
    aiModelQualityScore: integer('ai_model_quality_score'), // Quality assessment score (0-100)
    aiModelMetadata: text('ai_model_metadata', { mode: 'json' }), // Structured AI outputs (intents, assessments, etc.)
    aiModelPhaseAnalysis: text('ai_model_phase_analysis', { mode: 'json' }), // Phase analysis breakdown of session
    // Git tracking fields (session-specific, not project-level)
    gitBranch: text('git_branch'), // Git branch at session start
    firstCommitHash: text('first_commit_hash'), // First commit hash (SHA-1)
    latestCommitHash: text('latest_commit_hash'), // Latest commit hash during session
    // Sync fields
    syncedToServer: integer('synced_to_server', { mode: 'boolean' }).default(false),
    syncedAt: integer('synced_at', { mode: 'timestamp_ms' }),
    serverSessionId: text('server_session_id'), // ID from server if uploaded
    createdAt: integer('created_at', { mode: 'timestamp_ms' }).notNull(),
    uploadedAt: integer('uploaded_at', { mode: 'timestamp_ms' }).notNull(),
  },
  table => ({
    // Index for querying sessions by provider
    providerIdx: index('agent_sessions_provider_idx').on(table.provider),
    // Index for session-based queries
    sessionIdx: index('agent_sessions_session_idx').on(table.sessionId),
    // Index for time-based queries
    createdAtIdx: index('agent_sessions_created_at_idx').on(table.createdAt),
    uploadedAtIdx: index('agent_sessions_uploaded_at_idx').on(table.uploadedAt),
    // Index for file size analytics
    fileSizeIdx: index('agent_sessions_file_size_idx').on(table.fileSize),
    // Index for processing status queries
    processingStatusIdx: index('agent_sessions_processing_status_idx').on(table.processingStatus),
    // Index for assessment status queries
    assessmentStatusIdx: index('agent_sessions_assessment_status_idx').on(table.assessmentStatus),
    // Index for repository-based queries
    repositoryIdx: index('agent_sessions_repository_idx').on(table.repositoryId),
    // Index for git branch queries (for future PR matching)
    gitBranchIdx: index('agent_sessions_git_branch_idx').on(table.gitBranch),
    // Index for sync status
    syncIdx: index('agent_sessions_sync_idx').on(table.syncedToServer),
  })
)

export type AgentSession = typeof agentSessions.$inferSelect
export type NewAgentSession = typeof agentSessions.$inferInsert
