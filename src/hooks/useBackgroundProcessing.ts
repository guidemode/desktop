import { invoke } from '@tauri-apps/api/core'
import { useCallback, useEffect, useRef, useState } from 'react'
import { useSessionProcessing } from './useSessionProcessing'

/**
 * Background processing hook that can process sessions on demand
 * Does NOT run automatically - must be enabled explicitly
 */
export function useBackgroundProcessing() {
  const { processSession } = useSessionProcessing()
  const processingRef = useRef(false)
  const intervalRef = useRef<NodeJS.Timeout | null>(null)
  const [isEnabled, setIsEnabled] = useState(false)
  const [isProcessing, setIsProcessing] = useState(false)

  const checkForUnprocessedSessions = useCallback(async () => {
    if (processingRef.current) {
      return // Already processing
    }

    try {
      processingRef.current = true
      setIsProcessing(true)

      // Find sessions without metrics
      const unprocessedSessions: any[] = await invoke('execute_sql', {
        sql: `
          SELECT s.session_id, s.provider, s.file_path
          FROM agent_sessions s
          LEFT JOIN session_metrics m ON s.session_id = m.session_id
          WHERE m.id IS NULL
          ORDER BY s.created_at DESC
          LIMIT 5
        `,
        params: [],
      })

      if (unprocessedSessions.length > 0) {
        console.log(`Processing ${unprocessedSessions.length} unprocessed sessions...`)

        for (const session of unprocessedSessions) {
          try {
            // Get content using provider-specific logic
            const content: string = await invoke('get_session_content', {
              provider: session.provider,
              filePath: session.file_path,
              sessionId: session.session_id,
            })
            await processSession(session.session_id, session.provider, content, 'local')
            console.log(`✓ Processed session ${session.session_id}`)
          } catch (err) {
            console.error(`Failed to process session ${session.session_id}:`, err)
          }
        }
      }
    } catch (err) {
      console.error('Background processing error:', err)
    } finally {
      processingRef.current = false
      setIsProcessing(false)
    }
  }, [processSession])

  useEffect(() => {
    // Only start interval if enabled
    if (isEnabled) {
      checkForUnprocessedSessions() // Run immediately
      intervalRef.current = setInterval(checkForUnprocessedSessions, 10000) // Then every 10s
    }

    return () => {
      if (intervalRef.current) {
        clearInterval(intervalRef.current)
        intervalRef.current = null
      }
    }
  }, [isEnabled, checkForUnprocessedSessions])

  const enable = useCallback(() => setIsEnabled(true), [])
  const disable = useCallback(() => setIsEnabled(false), [])
  const processNow = useCallback(() => checkForUnprocessedSessions(), [checkForUnprocessedSessions])

  return {
    isEnabled,
    isProcessing,
    enable,
    disable,
    processNow,
  }
}
