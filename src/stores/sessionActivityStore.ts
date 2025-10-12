import { create } from 'zustand'

/**
 * Session Activity Store
 *
 * Tracks active sessions based on file watcher events from Tauri.
 * A session is considered "active" if:
 * 1. It has received activity (file watcher event) in the past 2 minutes, OR
 * 2. Its sessionEndTime is within the past 2 minutes (recently ended)
 */

interface ActiveSession {
  sessionId: string
  lastActivityTime: number
}

interface SessionActivityState {
  activeSessions: Map<string, ActiveSession>
  isTrackingEnabled: boolean // Global flag to disable tracking during rescans
  config: {
    activeSessionTimeout: number // milliseconds to consider a session "active"
  }
}

interface SessionActivityActions {
  markSessionActive: (sessionId: string) => void
  isSessionActive: (sessionId: string, sessionEndTime?: string | null) => boolean
  cleanupInactiveSessions: () => void
  clearAllActiveSessions: () => void
  setTrackingEnabled: (enabled: boolean) => void
  updateConfig: (config: Partial<SessionActivityState['config']>) => void
}

type SessionActivityStore = SessionActivityState & SessionActivityActions

const initialState: SessionActivityState = {
  activeSessions: new Map(),
  isTrackingEnabled: true,
  config: {
    activeSessionTimeout: 2 * 60 * 1000, // 2 minutes
  },
}

export const useSessionActivityStore = create<SessionActivityStore>()((set, get) => ({
  ...initialState,

  markSessionActive: (sessionId: string) => {
    const { isTrackingEnabled } = get()
    if (!isTrackingEnabled) {
      // Tracking is disabled, ignore this event
      return
    }

    set(state => {
      const newActiveSessions = new Map(state.activeSessions)
      newActiveSessions.set(sessionId, {
        sessionId,
        lastActivityTime: Date.now(),
      })
      return { activeSessions: newActiveSessions }
    })
  },

  isSessionActive: (sessionId: string, sessionEndTime?: string | null) => {
    const { activeSessions, config } = get()
    const now = Date.now()

    // Check if session has recent activity from file watcher events
    const session = activeSessions.get(sessionId)
    if (session) {
      const timeSinceActivity = now - session.lastActivityTime
      if (timeSinceActivity < config.activeSessionTimeout) {
        return true
      }
    }

    // Check if session ended within the last 2 minutes
    if (sessionEndTime) {
      const endTime = new Date(sessionEndTime).getTime()
      const timeSinceEnd = now - endTime
      if (timeSinceEnd < config.activeSessionTimeout && timeSinceEnd >= 0) {
        return true
      }
    }

    return false
  },

  cleanupInactiveSessions: () => {
    const { activeSessions, config } = get()
    const newActiveSessions = new Map(activeSessions)
    const now = Date.now()

    for (const [sessionId, session] of newActiveSessions.entries()) {
      if (now - session.lastActivityTime >= config.activeSessionTimeout) {
        newActiveSessions.delete(sessionId)
      }
    }

    if (newActiveSessions.size !== activeSessions.size) {
      set({ activeSessions: newActiveSessions })
    }
  },

  clearAllActiveSessions: () => {
    set({ activeSessions: new Map() })
  },

  setTrackingEnabled: (enabled: boolean) => {
    set({ isTrackingEnabled: enabled })
  },

  updateConfig: config => {
    set(state => ({
      config: { ...state.config, ...config },
    }))
  },
}))
