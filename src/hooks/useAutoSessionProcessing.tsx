import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { useEffect } from 'react'
import { useSessionProcessing } from './useSessionProcessing'

/**
 * Hook that listens for session-completed events from Rust and automatically processes metrics
 * This enables "Metrics Only" mode to work correctly by ensuring metrics are generated
 * before sessions are uploaded
 */
export function useAutoSessionProcessing() {
  const { processSession } = useSessionProcessing()

  useEffect(() => {
    let unlisten: (() => void) | null = null
    const processingQueue = new Set<string>() // Track sessions currently being processed

    const setupListener = async () => {
      unlisten = await listen<string>('session-completed', async event => {
        const sessionId = event.payload

        // Skip if already processing this session
        if (processingQueue.has(sessionId)) {
          return
        }

        processingQueue.add(sessionId)

        try {
          // Fetch session details from database
          const sessionResult: any[] = await invoke('execute_sql', {
            sql: `SELECT provider, file_path, session_id
                  FROM agent_sessions
                  WHERE session_id = ?
                  LIMIT 1`,
            params: [sessionId],
          })

          if (sessionResult.length === 0) {
            console.error(`Session ${sessionId} not found in database`)
            return
          }

          const session = sessionResult[0]

          // Get session content using provider-specific logic
          const content: string = await invoke('get_session_content', {
            provider: session.provider,
            filePath: session.file_path,
            sessionId: session.session_id,
          })

          // Process metrics
          await processSession(session.session_id, session.provider, content, 'local')
        } catch (error) {
          console.error(`Failed to auto-process session ${sessionId}:`, error)
        } finally {
          processingQueue.delete(sessionId)
        }
      })
    }

    setupListener()

    return () => {
      if (unlisten) {
        unlisten()
      }
    }
  }, [processSession])
}
