import { index, integer, sqliteTable, text } from 'drizzle-orm/sqlite-core'
import { agentSessions } from './agent-sessions'

export const sessionAssessments = sqliteTable(
  'session_assessments',
  {
    id: text('id').primaryKey(),
    sessionId: text('session_id')
      .notNull()
      .references(() => agentSessions.sessionId, { onDelete: 'cascade' }),
    provider: text('provider').notNull(),

    // Assessment data (JSON stored as TEXT in SQLite)
    responses: text('responses', { mode: 'json' }).notNull(), // Record<questionId, AssessmentAnswer>
    surveyType: text('survey_type'), // 'short' | 'standard' | 'full'
    durationSeconds: integer('duration_seconds'), // Duration in seconds
    rating: text('rating'), // 'thumbs_up' | 'meh' | 'thumbs_down'

    // Timestamps (stored as INTEGER milliseconds since epoch)
    completedAt: integer('completed_at', { mode: 'timestamp_ms' }).notNull(),
    createdAt: integer('created_at', { mode: 'timestamp_ms' }).notNull(),
  },
  (table) => ({
    // Indexes for common queries
    sessionIdx: index('session_assessments_session_idx').on(table.sessionId),
    providerIdx: index('session_assessments_provider_idx').on(table.provider),
    completedAtIdx: index('session_assessments_completed_at_idx').on(table.completedAt),
    ratingIdx: index('session_assessments_rating_idx').on(table.rating),
  })
)

export type SessionAssessment = typeof sessionAssessments.$inferSelect
export type NewSessionAssessment = typeof sessionAssessments.$inferInsert
