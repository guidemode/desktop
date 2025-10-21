import { ClaudeModelAdapter, GeminiModelAdapter } from '@guideai-dev/session-processing/ai-models'
import {
  IntentExtractionTask,
  QualityAssessmentTask,
  SessionPhaseAnalysisTask,
  SessionSummaryTask,
} from '@guideai-dev/session-processing/ai-models'
import type { ParsedSession } from '@guideai-dev/session-processing/processors'
import { invoke } from '@tauri-apps/api/core'
import { useCallback, useState } from 'react'
import { useConfigStore } from '../stores/configStore'
import { useAuth } from './useAuth'
import type { AiProcessingStep } from './useAiProcessingProgress'

interface AiProcessingResult {
  summary?: string
  qualityScore?: number
  metadata?: {
    'quality-assessment'?: {
      score: number
      reasoning: string
      strengths?: string[]
      improvements?: string[]
    }
    'intent-extraction'?: {
      taskType?: string
      primaryGoal?: string
      technologies?: string[]
      challenges?: string[]
      secondaryGoals?: string[]
    }
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
  const { user } = useAuth()

  const processSessionWithAi = useCallback(
    async (
      sessionId: string,
      parsedSession: ParsedSession,
      onProgressUpdate?: (step: AiProcessingStep | null) => void
    ): Promise<AiProcessingResult | null> => {
      setProcessing(true)
      setError(null)

      try {
        // Check for available API keys
        const claudeKey = getAiApiKey('claude')
        const geminiKey = getAiApiKey('gemini')

        if (!claudeKey && !geminiKey) {
          return null
        }

        // Prefer Claude if available, otherwise use Gemini
        const adapter = claudeKey
          ? new ClaudeModelAdapter({ apiKey: claudeKey })
          : new GeminiModelAdapter({ apiKey: geminiKey! })

        const result: AiProcessingResult = {
          metadata: {},
        }

        // Notify progress: Starting summary generation
        onProgressUpdate?.({
          name: 'Generating Summary',
          description: 'Creating AI-powered session summary',
          percentage: 20,
        })

        // Run Session Summary task
        try {
          const summaryTask = new SessionSummaryTask()
          const summaryResult = await adapter.executeTask(summaryTask, {
            sessionId,
            tenantId: user?.tenantId || 'local',
            userId: user?.username || 'local',
            provider: parsedSession.provider,
            session: parsedSession,
            user: user
              ? {
                  name: user.name || user.username,
                  username: user.username,
                  email: undefined,
                }
              : undefined,
          })

          if (summaryResult.success && summaryResult.output) {
            result.summary = summaryResult.output as string
          } else if (summaryResult.metadata?.error) {
            // Check if it's an authentication/API error (4xx client errors)
            const errorMsg = summaryResult.metadata.error.toLowerCase()
            // Check for HTTP 4xx errors or auth-related keywords
            if (
              errorMsg.includes('400') ||
              errorMsg.includes('401') ||
              errorMsg.includes('403') ||
              errorMsg.includes('invalid') ||
              errorMsg.includes('api key') ||
              errorMsg.includes('api_key') ||
              errorMsg.includes('unauthorized') ||
              errorMsg.includes('authentication') ||
              errorMsg.includes('credentials') ||
              errorMsg.includes('permission') ||
              errorMsg.match(/\b4\d{2}\b/)
            ) {
              // Match any 4xx HTTP status code
              throw new Error(summaryResult.metadata.error)
            }
            console.error('Summary task failed:', summaryResult.metadata.error)
          }
        } catch (err) {
          // Re-throw auth/API errors to surface them to the UI
          const errorMsg = err instanceof Error ? err.message.toLowerCase() : ''
          if (
            errorMsg.includes('400') ||
            errorMsg.includes('401') ||
            errorMsg.includes('403') ||
            errorMsg.includes('invalid') ||
            errorMsg.includes('api key') ||
            errorMsg.includes('api_key') ||
            errorMsg.includes('unauthorized') ||
            errorMsg.includes('authentication') ||
            errorMsg.includes('credentials') ||
            errorMsg.includes('permission') ||
            errorMsg.match(/\b4\d{2}\b/)
          ) {
            // Match any 4xx HTTP status code
            throw err
          }
          console.error('Summary task failed:', err)
          // Continue with quality assessment for other errors
        }

        // Notify progress: Starting intent extraction
        onProgressUpdate?.({
          name: 'Extracting Intent',
          description: 'Identifying goals and technologies',
          percentage: 40,
        })

        // Run Intent Extraction task
        try {
          const intentTask = new IntentExtractionTask()
          const intentResult = await adapter.executeTask(intentTask, {
            sessionId,
            tenantId: user?.tenantId || 'local',
            userId: user?.username || 'local',
            provider: parsedSession.provider,
            session: parsedSession,
            user: user
              ? {
                  name: user.name || user.username,
                  username: user.username,
                  email: undefined,
                }
              : undefined,
          })

          if (intentResult.success && intentResult.output) {
            result.metadata!['intent-extraction'] = intentResult.output as any
          } else if (intentResult.metadata?.error) {
            const errorMsg = intentResult.metadata.error.toLowerCase()
            if (
              errorMsg.includes('400') ||
              errorMsg.includes('401') ||
              errorMsg.includes('403') ||
              errorMsg.includes('invalid') ||
              errorMsg.includes('api key') ||
              errorMsg.includes('api_key') ||
              errorMsg.includes('unauthorized') ||
              errorMsg.includes('authentication') ||
              errorMsg.includes('credentials') ||
              errorMsg.includes('permission') ||
              errorMsg.match(/\b4\d{2}\b/)
            ) {
              throw new Error(intentResult.metadata.error)
            }
            console.error('Intent extraction failed:', intentResult.metadata.error)
          }
        } catch (err) {
          const errorMsg = err instanceof Error ? err.message.toLowerCase() : ''
          if (
            errorMsg.includes('400') ||
            errorMsg.includes('401') ||
            errorMsg.includes('403') ||
            errorMsg.includes('invalid') ||
            errorMsg.includes('api key') ||
            errorMsg.includes('api_key') ||
            errorMsg.includes('unauthorized') ||
            errorMsg.includes('authentication') ||
            errorMsg.includes('credentials') ||
            errorMsg.includes('permission') ||
            errorMsg.match(/\b4\d{2}\b/)
          ) {
            throw err
          }
          console.error('Intent extraction task failed:', err)
        }

        // Notify progress: Starting quality assessment
        onProgressUpdate?.({
          name: 'Assessing Quality',
          description: 'Evaluating session quality and effectiveness',
          percentage: 60,
        })

        // Run Quality Assessment task
        try {
          const qualityTask = new QualityAssessmentTask()
          const qualityResult = await adapter.executeTask(qualityTask, {
            sessionId,
            tenantId: user?.tenantId || 'local',
            userId: user?.username || 'local',
            provider: parsedSession.provider,
            session: parsedSession,
            user: user
              ? {
                  name: user.name || user.username,
                  username: user.username,
                  email: undefined,
                }
              : undefined,
          })

          if (qualityResult.success && qualityResult.output) {
            const assessment = qualityResult.output as any
            result.qualityScore = assessment.score
            result.metadata!['quality-assessment'] = assessment
          } else if (qualityResult.metadata?.error) {
            // Check if it's an authentication/API error (4xx client errors)
            const errorMsg = qualityResult.metadata.error.toLowerCase()
            // Check for HTTP 4xx errors or auth-related keywords
            if (
              errorMsg.includes('400') ||
              errorMsg.includes('401') ||
              errorMsg.includes('403') ||
              errorMsg.includes('invalid') ||
              errorMsg.includes('api key') ||
              errorMsg.includes('api_key') ||
              errorMsg.includes('unauthorized') ||
              errorMsg.includes('authentication') ||
              errorMsg.includes('credentials') ||
              errorMsg.includes('permission') ||
              errorMsg.match(/\b4\d{2}\b/)
            ) {
              // Match any 4xx HTTP status code
              throw new Error(qualityResult.metadata.error)
            }
            console.error('Quality assessment failed:', qualityResult.metadata.error)
          }
        } catch (err) {
          // Re-throw auth/API errors to surface them to the UI
          const errorMsg = err instanceof Error ? err.message.toLowerCase() : ''
          if (
            errorMsg.includes('400') ||
            errorMsg.includes('401') ||
            errorMsg.includes('403') ||
            errorMsg.includes('invalid') ||
            errorMsg.includes('api key') ||
            errorMsg.includes('api_key') ||
            errorMsg.includes('unauthorized') ||
            errorMsg.includes('authentication') ||
            errorMsg.includes('credentials') ||
            errorMsg.includes('permission') ||
            errorMsg.match(/\b4\d{2}\b/)
          ) {
            // Match any 4xx HTTP status code
            throw err
          }
          console.error('Quality assessment task failed:', err)
          // Continue even if quality assessment fails for other errors
        }

        // Notify progress: Starting phase analysis
        onProgressUpdate?.({
          name: 'Analyzing Phases',
          description: 'Breaking down session into distinct phases',
          percentage: 80,
        })

        // Run Phase Analysis task
        try {
          const phaseAnalysisTask = new SessionPhaseAnalysisTask()
          const phaseAnalysisResult = await adapter.executeTask(phaseAnalysisTask, {
            sessionId,
            tenantId: user?.tenantId || 'local',
            userId: user?.username || 'local',
            provider: parsedSession.provider,
            session: parsedSession,
            user: user
              ? {
                  name: user.name || user.username,
                  username: user.username,
                  email: undefined,
                }
              : undefined,
          })

          if (phaseAnalysisResult.success && phaseAnalysisResult.output) {
            result.phaseAnalysis = phaseAnalysisResult.output as any
          } else if (phaseAnalysisResult.metadata?.error) {
            // Check if it's an authentication/API error (4xx client errors)
            const errorMsg = phaseAnalysisResult.metadata.error.toLowerCase()
            // Check for HTTP 4xx errors or auth-related keywords
            if (
              errorMsg.includes('400') ||
              errorMsg.includes('401') ||
              errorMsg.includes('403') ||
              errorMsg.includes('invalid') ||
              errorMsg.includes('api key') ||
              errorMsg.includes('api_key') ||
              errorMsg.includes('unauthorized') ||
              errorMsg.includes('authentication') ||
              errorMsg.includes('credentials') ||
              errorMsg.includes('permission') ||
              errorMsg.match(/\b4\d{2}\b/)
            ) {
              // Match any 4xx HTTP status code
              throw new Error(phaseAnalysisResult.metadata.error)
            }
            console.error('Phase analysis failed:', phaseAnalysisResult.metadata.error)
          }
        } catch (err) {
          // Re-throw auth/API errors to surface them to the UI
          const errorMsg = err instanceof Error ? err.message.toLowerCase() : ''
          if (
            errorMsg.includes('400') ||
            errorMsg.includes('401') ||
            errorMsg.includes('403') ||
            errorMsg.includes('invalid') ||
            errorMsg.includes('api key') ||
            errorMsg.includes('api_key') ||
            errorMsg.includes('unauthorized') ||
            errorMsg.includes('authentication') ||
            errorMsg.includes('credentials') ||
            errorMsg.includes('permission') ||
            errorMsg.match(/\b4\d{2}\b/)
          ) {
            // Match any 4xx HTTP status code
            throw err
          }
          console.error('Phase analysis task failed:', err)
          // Continue even if phase analysis fails for other errors
        }

        // Store AI results in database
        if (result.summary || result.qualityScore !== undefined || result.phaseAnalysis) {
          await storeAiResults(sessionId, result)
        }

        // Notify progress: Complete
        onProgressUpdate?.({
          name: 'Complete',
          description: 'Processing finished successfully',
          percentage: 100,
        })

        return result
      } catch (err) {
        const errorMessage = err instanceof Error ? err.message : 'Unknown error'
        setError(errorMessage)
        console.error('AI Processing failed:', err)

        // Re-throw auth/API errors so they show in the UI
        const errorMsg = errorMessage.toLowerCase()
        if (
          errorMsg.includes('400') ||
          errorMsg.includes('401') ||
          errorMsg.includes('403') ||
          errorMsg.includes('invalid') ||
          errorMsg.includes('api key') ||
          errorMsg.includes('api_key') ||
          errorMsg.includes('unauthorized') ||
          errorMsg.includes('authentication') ||
          errorMsg.includes('credentials') ||
          errorMsg.includes('permission') ||
          errorMsg.match(/\b4\d{2}\b/)
        ) {
          // Match any 4xx HTTP status code
          throw err
        }

        return null
      } finally {
        setProcessing(false)
      }
    },
    [getAiApiKey, user]
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
    // Update agent_sessions table with AI results, mark processing as completed,
    // and reset synced_to_server to trigger a new upload with AI data
    await invoke('execute_sql', {
      sql: `
        UPDATE agent_sessions
        SET
          ai_model_summary = ?,
          ai_model_quality_score = ?,
          ai_model_metadata = ?,
          ai_model_phase_analysis = ?,
          processing_status = 'completed',
          processed_at = ?,
          synced_to_server = 0
        WHERE session_id = ?
      `,
      params: [
        results.summary || null,
        results.qualityScore ?? null,
        results.metadata && Object.keys(results.metadata).length > 0
          ? JSON.stringify(results.metadata)
          : null,
        results.phaseAnalysis ? JSON.stringify(results.phaseAnalysis) : null,
        Date.now(),
        sessionId,
      ],
    })
  } catch (err) {
    console.error('Failed to store AI results:', err)
    throw err
  }
}
