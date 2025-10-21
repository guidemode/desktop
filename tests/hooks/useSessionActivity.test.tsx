import { renderHook } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useSessionActivity } from '../../src/hooks/useSessionActivity'
import { useSessionActivityStore } from '../../src/stores/sessionActivityStore'

const listen = vi.fn()

vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: unknown[]) => listen(...args),
}))

describe('useSessionActivity', () => {
  let unlistenSpy: ReturnType<typeof vi.fn>
  let eventHandler: ((event: { payload: string }) => void) | null = null
  let markSessionActiveSpy: ReturnType<typeof vi.spyOn>
  let cleanupInactiveSessionsSpy: ReturnType<typeof vi.spyOn>

  beforeEach(() => {
    vi.useFakeTimers()

    useSessionActivityStore.setState(state => ({
      ...state,
      activeSessions: new Map(),
      isTrackingEnabled: true,
      config: {
        ...state.config,
        activeSessionTimeout: 2 * 60 * 1000,
      },
    }))

    markSessionActiveSpy = vi.spyOn(useSessionActivityStore.getState(), 'markSessionActive')
    cleanupInactiveSessionsSpy = vi.spyOn(useSessionActivityStore.getState(), 'cleanupInactiveSessions')

    unlistenSpy = vi.fn()
    eventHandler = null

    listen.mockImplementation(async (_event, handler) => {
      eventHandler = handler as typeof eventHandler
      return () => {
        unlistenSpy()
      }
    })
  })

  afterEach(() => {
    markSessionActiveSpy.mockRestore()
    cleanupInactiveSessionsSpy.mockRestore()
    vi.runOnlyPendingTimers()
    vi.useRealTimers()
    vi.clearAllMocks()
  })

  it('marks sessions active when session-updated events arrive and cleans up periodically', async () => {
    const { unmount } = renderHook(() => useSessionActivity())

    expect(listen).toHaveBeenCalledWith('session-updated', expect.any(Function))
    expect(eventHandler).toBeTruthy()

    eventHandler?.({ payload: 'session-123' })
    expect(markSessionActiveSpy).toHaveBeenCalledWith('session-123')

    vi.advanceTimersByTime(30000)
    expect(cleanupInactiveSessionsSpy).toHaveBeenCalled()

    unmount()

    await Promise.resolve()
    expect(unlistenSpy).toHaveBeenCalled()
  })
})
