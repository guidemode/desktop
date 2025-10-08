import { index, integer, real, sqliteTable, text } from 'drizzle-orm/sqlite-core'

// Wide fact table optimized for analytics
// One row per session with typed columns for all metrics
export const sessionMetrics = sqliteTable(
  'session_metrics',
  {
    // Primary key and dimensions
    id: text('id').primaryKey(),
    sessionId: text('session_id').notNull(),
    provider: text('provider').notNull(),
    timestamp: integer('timestamp', { mode: 'timestamp_ms' }).notNull(),

    // Performance metrics
    responseLatencyMs: real('response_latency_ms'),
    taskCompletionTimeMs: real('task_completion_time_ms'),
    performanceTotalResponses: integer('performance_total_responses'),

    // Usage metrics
    readWriteRatio: real('read_write_ratio'),
    inputClarityScore: real('input_clarity_score'),
    readOperations: integer('read_operations'),
    writeOperations: integer('write_operations'),
    totalUserMessages: integer('total_user_messages'),

    // Error metrics
    errorCount: integer('error_count'),
    errorTypes: text('error_types', { mode: 'json' }), // Array of strings
    lastErrorMessage: text('last_error_message'),
    recoveryAttempts: integer('recovery_attempts'),
    fatalErrors: integer('fatal_errors'),

    // Engagement metrics
    interruptionRate: real('interruption_rate'),
    sessionLengthMinutes: real('session_length_minutes'),
    totalInterruptions: integer('total_interruptions'),
    engagementTotalResponses: integer('engagement_total_responses'),

    // Quality metrics
    taskSuccessRate: real('task_success_rate'),
    iterationCount: integer('iteration_count'),
    processQualityScore: real('process_quality_score'),
    usedPlanMode: integer('used_plan_mode', { mode: 'boolean' }),
    usedTodoTracking: integer('used_todo_tracking', { mode: 'boolean' }),
    overTopAffirmations: integer('over_top_affirmations'),
    successfulOperations: integer('successful_operations'),
    totalOperations: integer('total_operations'),
    exitPlanModeCount: integer('exit_plan_mode_count'),
    todoWriteCount: integer('todo_write_count'),
    overTopAffirmationsPhrases: text('over_top_affirmations_phrases', { mode: 'json' }), // Array of strings

    // Improvement tips (category-specific)
    usageImprovementTips: text('usage_improvement_tips', { mode: 'json' }), // Array of strings
    errorImprovementTips: text('error_improvement_tips', { mode: 'json' }), // Array of strings
    engagementImprovementTips: text('engagement_improvement_tips', { mode: 'json' }), // Array of strings
    qualityImprovementTips: text('quality_improvement_tips', { mode: 'json' }), // Array of strings
    performanceImprovementTips: text('performance_improvement_tips', { mode: 'json' }), // Array of strings

    // Deprecated: kept for backward compatibility, use category-specific columns above
    improvementTips: text('improvement_tips', { mode: 'json' }), // Array of strings

    // Custom/rare metrics only (for extensibility)
    customMetrics: text('custom_metrics', { mode: 'json' }),

    // Standard warehouse fields
    createdAt: integer('created_at', { mode: 'timestamp_ms' }).notNull(),
  },
  table => ({
    // Core dimension indexes for filtering
    sessionIdx: index('session_metrics_session_idx').on(table.sessionId),
    providerIdx: index('session_metrics_provider_idx').on(table.provider),
    timestampIdx: index('session_metrics_timestamp_idx').on(table.timestamp),
  })
)

export type SessionMetric = typeof sessionMetrics.$inferSelect
export type NewSessionMetric = typeof sessionMetrics.$inferInsert
