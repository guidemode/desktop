import { useState, useCallback } from 'react'
import { useConfigStore } from '../stores/configStore'
import { ClaudeModelAdapter, GeminiModelAdapter } from '@guideai-dev/session-processing/ai-models'
import { SessionSummaryTask, QualityAssessmentTask } from '@guideai-dev/session-processing/ai-models'
import type { ParsedSession } from '@guideai-dev/session-processing/processors'
import { invoke } from '@tauri-apps/api/core'

interface AiProcessingResult {
  summary?: string
  qualityScore?: number
  qualityMetadata?: {
    score: number
    reasoning: string
    strengths?: string[]
    improvements?: string[]
  }
}

export function useAiProcessing() {
  const [processing, setProcessing] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const { getAiApiKey } = useConfigStore()

  const processSessionWithAi = useCallback(
    async (sessionId: string, parsedSession: ParsedSession): Promise<AiProcessingResult | null> => {
      setProcessing(true)
      setError(null)

      try {
        // Check for available API keys
        const claudeKey = getAiApiKey('claude')
        const geminiKey = getAiApiKey('gemini')

        if (!claudeKey && !geminiKey) {
          console.log('[AI Processing] No API keys configured, skipping AI processing')
          return null
        }

        // Prefer Claude if available, otherwise use Gemini
        const adapter = claudeKey
          ? new ClaudeModelAdapter({ apiKey: claudeKey })
          : new GeminiModelAdapter({ apiKey: geminiKey! })

        console.log(`[AI Processing] Using ${adapter.name} for session ${sessionId}`)

        const result: AiProcessingResult = {}

        // Run Session Summary task
        try {
          const summaryTask = new SessionSummaryTask()
          const summaryResult = await adapter.executeTask(summaryTask, {
            sessionId,
            tenantId: 'local',
            userId: 'local',
            provider: parsedSession.provider,
            session: parsedSession,
          })

          if (summaryResult.success && summaryResult.output) {
            result.summary = summaryResult.output as string
            console.log('[AI Processing] Summary generated:', result.summary)
          }
        } catch (err) {
          console.error('[AI Processing] Summary task failed:', err)
          // Continue with quality assessment even if summary fails
        }

        // Run Quality Assessment task
        try {
          const qualityTask = new QualityAssessmentTask()
          const qualityResult = await adapter.executeTask(qualityTask, {
            sessionId,
            tenantId: 'local',
            userId: 'local',
            provider: parsedSession.provider,
            session: parsedSession,
          })

          if (qualityResult.success && qualityResult.output) {
            const assessment = qualityResult.output as any
            result.qualityScore = assessment.score
            result.qualityMetadata = assessment
            console.log('[AI Processing] Quality assessment:', assessment)
          }
        } catch (err) {
          console.error('[AI Processing] Quality assessment task failed:', err)
          // Continue even if quality assessment fails
        }

        // Store AI results in database
        if (result.summary || result.qualityScore !== undefined) {
          await storeAiResults(sessionId, result)
        }

        return result
      } catch (err) {
        const errorMessage = err instanceof Error ? err.message : 'Unknown error'
        setError(errorMessage)
        console.error('[AI Processing] Failed:', err)
        return null
      } finally {
        setProcessing(false)
      }
    },
    [getAiApiKey]
  )

  return {
    processSessionWithAi,
    processing,
    error,
    hasApiKey: () => {
      const claudeKey = getAiApiKey('claude')
      const geminiKey = getAiApiKey('gemini')
      return !!(claudeKey || geminiKey)
    },
  }
}

/**
 * Store AI processing results in the database
 */
async function storeAiResults(sessionId: string, results: AiProcessingResult): Promise<void> {
  try {
    // Update agent_sessions table with AI results
    await invoke('execute_sql', {
      sql: `
        UPDATE agent_sessions
        SET
          ai_model_summary = ?,
          ai_model_quality_score = ?,
          ai_model_metadata = ?
        WHERE session_id = ?
      `,
      params: [
        results.summary || null,
        results.qualityScore ?? null,
        results.qualityMetadata ? JSON.stringify(results.qualityMetadata) : null,
        sessionId,
      ],
    })

    console.log(`[AI Processing] Stored AI results for session ${sessionId}`)
  } catch (err) {
    console.error('[AI Processing] Failed to store AI results:', err)
    throw err
  }
}
