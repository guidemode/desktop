import { useEffect, useRef } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { useQueryClient } from '@tanstack/react-query'
import { useAiProcessing } from './useAiProcessing'
import { useConfigStore } from '../stores/configStore'
import { ProcessorRegistry } from '@guideai-dev/session-processing/processors'

/**
 * Hook that runs background task to process AI for completed sessions
 * after a configurable delay (default 10 minutes after session ends).
 *
 * Runs every minute to check for eligible sessions.
 */
export function useDelayedAiProcessing() {
  const { processSessionWithAi, hasApiKey } = useAiProcessing()
  const { systemConfig } = useConfigStore()
  const queryClient = useQueryClient()
  const processingQueue = useRef<Set<string>>(new Set())
  const intervalRef = useRef<NodeJS.Timeout | null>(null)

  useEffect(() => {
    // Only run if API key is configured
    if (!hasApiKey()) {
      return
    }

    const processEligibleSessions = async () => {
      try {
        // Get delay from config (default 10 minutes)
        const delayMinutes = systemConfig?.aiProcessingDelayMinutes || 10
        const delayMs = delayMinutes * 60 * 1000
        const nowMs = Date.now()

        // Only process sessions within the last hour to avoid processing very old sessions
        const maxAgeMs = 60 * 60 * 1000 // 1 hour
        const minSessionEndTime = nowMs - maxAgeMs

        // Query for eligible sessions:
        // - core_metrics_status = 'completed' (metrics already processed)
        // - processing_status = 'pending' (AI not yet processed)
        // - session_end_time IS NOT NULL (session has ended)
        // - (now - session_end_time) > delay (minimum wait time)
        // - session_end_time > (now - 1 hour) (maximum age window)
        const eligibleSessions: any[] = await invoke('execute_sql', {
          sql: `
            SELECT session_id, provider, file_path, session_end_time
            FROM agent_sessions
            WHERE core_metrics_status = 'completed'
              AND processing_status = 'pending'
              AND session_end_time IS NOT NULL
              AND (? - session_end_time) > ?
              AND session_end_time > ?
            LIMIT 10
          `,
          params: [nowMs, delayMs, minSessionEndTime],
        })

        if (eligibleSessions.length === 0) {
          return
        }

        // Process each eligible session
        for (const session of eligibleSessions) {
          // Skip if already processing
          if (processingQueue.current.has(session.session_id)) {
            continue
          }

          processingQueue.current.add(session.session_id)

          try {
            // Get session content
            const content: string = await invoke('get_session_content', {
              provider: session.provider,
              filePath: session.file_path,
              sessionId: session.session_id,
            })

            // Parse session using processor registry
            const registry = new ProcessorRegistry()
            const processor = registry.getProcessor(session.provider)

            if (!processor) {
              console.error(`No processor found for provider: ${session.provider}`)
              continue
            }

            const parsedSession = processor.parseSession(content)

            // Process AI tasks
            await processSessionWithAi(session.session_id, parsedSession)

            // Invalidate query cache to show updated AI results immediately
            await queryClient.invalidateQueries({ queryKey: ['local-sessions'] })
            await queryClient.invalidateQueries({ queryKey: ['session-metrics', session.session_id] })
            await queryClient.invalidateQueries({ queryKey: ['session-metadata', session.session_id] })
          } catch (error) {
            console.error(`Failed to process AI for session ${session.session_id}:`, error)

            // Mark as failed if it's an auth error, otherwise leave as pending to retry
            const errorMsg = error instanceof Error ? error.message.toLowerCase() : ''
            const isAuthError = errorMsg.includes('401') || errorMsg.includes('403') ||
                               errorMsg.includes('api key') || errorMsg.includes('unauthorized')

            if (isAuthError) {
              // Mark as failed so we don't keep retrying with bad credentials
              await invoke('execute_sql', {
                sql: `UPDATE agent_sessions SET processing_status = 'failed' WHERE session_id = ?`,
                params: [session.session_id],
              })
            }
          } finally {
            processingQueue.current.delete(session.session_id)
          }
        }
      } catch (error) {
        console.error('Error in delayed AI processing:', error)
      }
    }

    // Run immediately on mount
    processEligibleSessions()

    // Then run every minute
    intervalRef.current = setInterval(processEligibleSessions, 60 * 1000)

    return () => {
      if (intervalRef.current) {
        clearInterval(intervalRef.current)
      }
    }
  }, [processSessionWithAi, hasApiKey, queryClient, systemConfig?.aiProcessingDelayMinutes])
}
