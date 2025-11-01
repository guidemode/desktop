import { useQueryClient } from '@tanstack/react-query'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { useEffect, useRef } from 'react'
import { useConfigStore } from '../stores/configStore'
import { useSessionProcessing } from './useSessionProcessing'

// SQL result type for session query
interface SessionRow {
  provider: string
  file_path: string
  session_id: string
  core_metrics_status: string | null
}

/**
 * Hook that listens for session-updated events and processes core metrics
 * with debouncing to avoid processing on every file change.
 *
 * Waits for configurable delay (default 10s) of inactivity before processing.
 *
 * IMPORTANT: This hook ALWAYS re-processes metrics on session updates, even if
 * core_metrics_status is 'completed'. This ensures metrics stay up-to-date for
 * live/active sessions as they evolve. The debouncing prevents excessive processing.
 */
export function useDebouncedCoreMetrics() {
  const { processSession } = useSessionProcessing()
  const { systemConfig } = useConfigStore()
  const queryClient = useQueryClient()
  const debounceMap = useRef<Map<string, NodeJS.Timeout>>(new Map())
  const processingQueue = useRef<Set<string>>(new Set())

  useEffect(() => {
    let unlisten: (() => void) | null = null

    const setupListener = async () => {
      unlisten = await listen<string>('session-updated', async event => {
        const sessionId = event.payload

        // Get debounce delay from config (default 10 seconds)
        const debounceMs = (systemConfig?.coreMetricsDebounceSeconds || 10) * 1000

        // Clear existing timeout for this session
        const existingTimeout = debounceMap.current.get(sessionId)
        if (existingTimeout) {
          clearTimeout(existingTimeout)
        }

        // Set new timeout
        const newTimeout = setTimeout(async () => {
          // Skip if already processing this session
          if (processingQueue.current.has(sessionId)) {
            return
          }

          processingQueue.current.add(sessionId)

          try {
            // Fetch session details from database (returns snake_case SQL column names)
            const sessionResult = await invoke<SessionRow[]>('execute_sql', {
              sql: `SELECT provider, file_path, session_id, core_metrics_status
                    FROM agent_sessions
                    WHERE session_id = ?
                    LIMIT 1`,
              params: [sessionId],
            })

            if (sessionResult.length === 0) {
              console.error(`Session ${sessionId} not found in database`)
              return
            }

            const sessionRow = sessionResult[0]

            // Get session content using provider-specific logic
            const content: string = await invoke('get_session_content', {
              provider: sessionRow.provider,
              filePath: sessionRow.file_path,
              sessionId: sessionRow.session_id,
            })

            // Process core metrics (will update existing metrics if already processed)
            await processSession(sessionRow.session_id, sessionRow.provider, content, 'local')

            // Invalidate query cache to show updated metrics immediately
            await queryClient.invalidateQueries({ queryKey: ['local-sessions'] })
            await queryClient.invalidateQueries({ queryKey: ['session-metrics', sessionId] })
            await queryClient.invalidateQueries({ queryKey: ['session-metadata', sessionId] })
          } catch (error) {
            console.error(`Failed to process session ${sessionId}:`, error)
          } finally {
            processingQueue.current.delete(sessionId)
            debounceMap.current.delete(sessionId)
          }
        }, debounceMs)

        debounceMap.current.set(sessionId, newTimeout)
      })
    }

    setupListener()

    return () => {
      // Cleanup: clear all timeouts
      debounceMap.current.forEach(timeout => clearTimeout(timeout))
      debounceMap.current.clear()

      if (unlisten) {
        unlisten()
      }
    }
  }, [processSession, queryClient, systemConfig?.coreMetricsDebounceSeconds])
}
