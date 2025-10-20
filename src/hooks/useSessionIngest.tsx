import { listen } from '@tauri-apps/api/event'
import { useEffect } from 'react'
import { type SessionData, insertSession, sessionExists } from '../services/sessionIngestion'

interface SessionDetectedPayload {
  provider: string
  project_name: string
  session_id: string
  file_name: string
  file_path: string
  file_size: number
  session_start_time?: number
  session_end_time?: number
  duration_ms?: number
}

/**
 * Hook that listens for session detection events from Rust and inserts them into local DB
 */
export function useSessionIngest() {
  useEffect(() => {
    let unlisten: (() => void) | null = null

    const setupListener = async () => {
      unlisten = await listen<SessionDetectedPayload>('session-detected', async event => {
        const payload = event.payload

        // Check if session already exists to avoid duplicates
        const exists = await sessionExists(payload.session_id, payload.file_name)
        if (exists) {
          return
        }

        // Convert to SessionData format
        const sessionData: SessionData = {
          provider: payload.provider,
          projectName: payload.project_name,
          sessionId: payload.session_id,
          fileName: payload.file_name,
          filePath: payload.file_path,
          fileSize: payload.file_size,
          sessionStartTime: payload.session_start_time,
          sessionEndTime: payload.session_end_time,
          durationMs: payload.duration_ms,
        }

        try {
          await insertSession(sessionData)
        } catch (error) {
          console.error(`Failed to insert session ${payload.session_id}:`, error)
        }
      })
    }

    setupListener()

    return () => {
      if (unlisten) {
        unlisten()
      }
    }
  }, [])
}
