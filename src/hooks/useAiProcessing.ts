import { useState, useCallback } from 'react'
import { useConfigStore } from '../stores/configStore'
import { ClaudeModelAdapter, GeminiModelAdapter } from '@guideai-dev/session-processing/ai-models'
import { SessionSummaryTask, QualityAssessmentTask, SessionPhaseAnalysisTask } from '@guideai-dev/session-processing/ai-models'
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
  phaseAnalysis?: {
    phases: any[]
    totalPhases: number
    totalSteps: number
    sessionDurationMs: number
    pattern: string
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
          } else if (summaryResult.metadata?.error) {
            // Check if it's an authentication/API error (4xx client errors)
            const errorMsg = summaryResult.metadata.error.toLowerCase()
            // Check for HTTP 4xx errors or auth-related keywords
            if (errorMsg.includes('400') || errorMsg.includes('401') || errorMsg.includes('403') ||
                errorMsg.includes('invalid') || errorMsg.includes('api key') || errorMsg.includes('api_key') ||
                errorMsg.includes('unauthorized') || errorMsg.includes('authentication') ||
                errorMsg.includes('credentials') || errorMsg.includes('permission') ||
                errorMsg.match(/\b4\d{2}\b/)) { // Match any 4xx HTTP status code
              throw new Error(summaryResult.metadata.error)
            }
            console.error('[AI Processing] Summary task failed:', summaryResult.metadata.error)
          }
        } catch (err) {
          // Re-throw auth/API errors to surface them to the UI
          const errorMsg = err instanceof Error ? err.message.toLowerCase() : ''
          if (errorMsg.includes('400') || errorMsg.includes('401') || errorMsg.includes('403') ||
              errorMsg.includes('invalid') || errorMsg.includes('api key') || errorMsg.includes('api_key') ||
              errorMsg.includes('unauthorized') || errorMsg.includes('authentication') ||
              errorMsg.includes('credentials') || errorMsg.includes('permission') ||
              errorMsg.match(/\b4\d{2}\b/)) { // Match any 4xx HTTP status code
            throw err
          }
          console.error('[AI Processing] Summary task failed:', err)
          // Continue with quality assessment for other errors
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
          } else if (qualityResult.metadata?.error) {
            // Check if it's an authentication/API error (4xx client errors)
            const errorMsg = qualityResult.metadata.error.toLowerCase()
            // Check for HTTP 4xx errors or auth-related keywords
            if (errorMsg.includes('400') || errorMsg.includes('401') || errorMsg.includes('403') ||
                errorMsg.includes('invalid') || errorMsg.includes('api key') || errorMsg.includes('api_key') ||
                errorMsg.includes('unauthorized') || errorMsg.includes('authentication') ||
                errorMsg.includes('credentials') || errorMsg.includes('permission') ||
                errorMsg.match(/\b4\d{2}\b/)) { // Match any 4xx HTTP status code
              throw new Error(qualityResult.metadata.error)
            }
            console.error('[AI Processing] Quality assessment failed:', qualityResult.metadata.error)
          }
        } catch (err) {
          // Re-throw auth/API errors to surface them to the UI
          const errorMsg = err instanceof Error ? err.message.toLowerCase() : ''
          if (errorMsg.includes('400') || errorMsg.includes('401') || errorMsg.includes('403') ||
              errorMsg.includes('invalid') || errorMsg.includes('api key') || errorMsg.includes('api_key') ||
              errorMsg.includes('unauthorized') || errorMsg.includes('authentication') ||
              errorMsg.includes('credentials') || errorMsg.includes('permission') ||
              errorMsg.match(/\b4\d{2}\b/)) { // Match any 4xx HTTP status code
            throw err
          }
          console.error('[AI Processing] Quality assessment task failed:', err)
          // Continue even if quality assessment fails for other errors
        }

        // Run Phase Analysis task
        try {
          const phaseAnalysisTask = new SessionPhaseAnalysisTask()
          const phaseAnalysisResult = await adapter.executeTask(phaseAnalysisTask, {
            sessionId,
            tenantId: 'local',
            userId: 'local',
            provider: parsedSession.provider,
            session: parsedSession,
          })

          if (phaseAnalysisResult.success && phaseAnalysisResult.output) {
            result.phaseAnalysis = phaseAnalysisResult.output as any
            console.log('[AI Processing] Phase analysis:', phaseAnalysisResult.output)
          } else if (phaseAnalysisResult.metadata?.error) {
            // Check if it's an authentication/API error (4xx client errors)
            const errorMsg = phaseAnalysisResult.metadata.error.toLowerCase()
            // Check for HTTP 4xx errors or auth-related keywords
            if (errorMsg.includes('400') || errorMsg.includes('401') || errorMsg.includes('403') ||
                errorMsg.includes('invalid') || errorMsg.includes('api key') || errorMsg.includes('api_key') ||
                errorMsg.includes('unauthorized') || errorMsg.includes('authentication') ||
                errorMsg.includes('credentials') || errorMsg.includes('permission') ||
                errorMsg.match(/\b4\d{2}\b/)) { // Match any 4xx HTTP status code
              throw new Error(phaseAnalysisResult.metadata.error)
            }
            console.error('[AI Processing] Phase analysis failed:', phaseAnalysisResult.metadata.error)
          }
        } catch (err) {
          // Re-throw auth/API errors to surface them to the UI
          const errorMsg = err instanceof Error ? err.message.toLowerCase() : ''
          if (errorMsg.includes('400') || errorMsg.includes('401') || errorMsg.includes('403') ||
              errorMsg.includes('invalid') || errorMsg.includes('api key') || errorMsg.includes('api_key') ||
              errorMsg.includes('unauthorized') || errorMsg.includes('authentication') ||
              errorMsg.includes('credentials') || errorMsg.includes('permission') ||
              errorMsg.match(/\b4\d{2}\b/)) { // Match any 4xx HTTP status code
            throw err
          }
          console.error('[AI Processing] Phase analysis task failed:', err)
          // Continue even if phase analysis fails for other errors
        }

        // Store AI results in database
        if (result.summary || result.qualityScore !== undefined || result.phaseAnalysis) {
          await storeAiResults(sessionId, result)
        }

        return result
      } catch (err) {
        const errorMessage = err instanceof Error ? err.message : 'Unknown error'
        setError(errorMessage)
        console.error('[AI Processing] Failed:', err)

        // Re-throw auth/API errors so they show in the UI
        const errorMsg = errorMessage.toLowerCase()
        if (errorMsg.includes('400') || errorMsg.includes('401') || errorMsg.includes('403') ||
            errorMsg.includes('invalid') || errorMsg.includes('api key') || errorMsg.includes('api_key') ||
            errorMsg.includes('unauthorized') || errorMsg.includes('authentication') ||
            errorMsg.includes('credentials') || errorMsg.includes('permission') ||
            errorMsg.match(/\b4\d{2}\b/)) { // Match any 4xx HTTP status code
          throw err
        }

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
          ai_model_metadata = ?,
          ai_model_phase_analysis = ?
        WHERE session_id = ?
      `,
      params: [
        results.summary || null,
        results.qualityScore ?? null,
        results.qualityMetadata ? JSON.stringify(results.qualityMetadata) : null,
        results.phaseAnalysis ? JSON.stringify(results.phaseAnalysis) : null,
        sessionId,
      ],
    })

    console.log(`[AI Processing] Stored AI results for session ${sessionId}`)
  } catch (err) {
    console.error('[AI Processing] Failed to store AI results:', err)
    throw err
  }
}
