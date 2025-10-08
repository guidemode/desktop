import { useEffect } from 'react'
import { listen } from '@tauri-apps/api/event'
import { useSessionActivityStore } from '../stores/sessionActivityStore'

/**
 * Hook to manage session activity tracking
 *
 * Listens to Tauri events from file watchers and marks sessions as active.
 * Automatically cleans up inactive sessions periodically.
 */
export function useSessionActivity() {
  const markSessionActive = useSessionActivityStore(state => state.markSessionActive)
  const cleanupInactiveSessions = useSessionActivityStore(state => state.cleanupInactiveSessions)

  useEffect(() => {
    // Listen for session update events from file watchers (real file changes)
    const unlistenUpdated = listen<string>('session-updated', (event) => {
      const sessionId = event.payload
      if (sessionId) {
        markSessionActive(sessionId)
      }
    })

    // NOTE: We do NOT listen to session-synced events because those are just
    // upload confirmations, not actual file changes. Syncing happens after
    // a session ends or during rescans, and doesn't mean the session is "live".

    // Cleanup inactive sessions every 30 seconds
    const cleanupInterval = setInterval(() => {
      cleanupInactiveSessions()
    }, 30000)

    return () => {
      unlistenUpdated.then(fn => fn())
      clearInterval(cleanupInterval)
    }
  }, [markSessionActive, cleanupInactiveSessions])
}
