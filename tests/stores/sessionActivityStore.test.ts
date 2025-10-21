import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest'
import { useSessionActivityStore } from '../../src/stores/sessionActivityStore'

const DEFAULT_TIMEOUT = 2 * 60 * 1000

const resetStore = () => {
	useSessionActivityStore.setState({
		activeSessions: new Map(),
		isTrackingEnabled: true,
		config: {
			activeSessionTimeout: DEFAULT_TIMEOUT,
		},
	})
}

describe('useSessionActivityStore', () => {
	beforeEach(() => {
		resetStore()
	})

	afterEach(() => {
		vi.useRealTimers()
		resetStore()
	})

	describe('markSessionActive', () => {
		it('marks a session active when tracking is enabled', () => {
			const { markSessionActive, isSessionActive } = useSessionActivityStore.getState()

			markSessionActive('session-1')

			expect(isSessionActive('session-1')).toBe(true)
		})

		it('updates timestamp to current time on each mark', () => {
			vi.useFakeTimers()
			const { markSessionActive, activeSessions } = useSessionActivityStore.getState()

			const time1 = Date.now()
			markSessionActive('session-1')
			const firstTimestamp = useSessionActivityStore.getState().activeSessions.get('session-1')
				?.lastActivityTime

			vi.advanceTimersByTime(1000)

			const time2 = Date.now()
			markSessionActive('session-1')
			const secondTimestamp = useSessionActivityStore.getState().activeSessions.get('session-1')
				?.lastActivityTime

			expect(secondTimestamp).toBeGreaterThan(firstTimestamp!)
		})

		it('ignores session activity when tracking is disabled', () => {
			const { setTrackingEnabled, markSessionActive, isSessionActive } =
				useSessionActivityStore.getState()

			setTrackingEnabled(false)
			markSessionActive('session-1')

			expect(isSessionActive('session-1')).toBe(false)
			expect(useSessionActivityStore.getState().activeSessions.size).toBe(0)
		})
	})

	describe('isSessionActive', () => {
		it('returns true for recent activity (<2 min)', () => {
			vi.useFakeTimers()
			const { markSessionActive, isSessionActive } = useSessionActivityStore.getState()

			markSessionActive('session-1')

			vi.advanceTimersByTime(60_000) // 1 minute

			expect(isSessionActive('session-1')).toBe(true)
		})

		it('returns true for recent end time (<2 min)', () => {
			vi.useFakeTimers()
			const now = new Date()
			const recentEndTime = new Date(now.getTime() - 60_000).toISOString() // 1 minute ago

			const { isSessionActive } = useSessionActivityStore.getState()

			expect(isSessionActive('session-1', recentEndTime)).toBe(true)
		})

		it('returns false for old activity (>2 min)', () => {
			vi.useFakeTimers()
			const { markSessionActive, isSessionActive } = useSessionActivityStore.getState()

			markSessionActive('session-1')

			vi.advanceTimersByTime(121_000) // 2 minutes + 1 second

			expect(isSessionActive('session-1')).toBe(false)
		})

		it('returns false for old end time (>2 min)', () => {
			vi.useFakeTimers()
			const now = new Date()
			const oldEndTime = new Date(now.getTime() - 121_000).toISOString() // 2 min + 1 sec ago

			const { isSessionActive } = useSessionActivityStore.getState()

			expect(isSessionActive('session-1', oldEndTime)).toBe(false)
		})

		it('returns false for future end time', () => {
			const futureEndTime = new Date(Date.now() + 60_000).toISOString() // 1 minute in future

			const { isSessionActive } = useSessionActivityStore.getState()

			expect(isSessionActive('session-1', futureEndTime)).toBe(false)
		})
	})

	describe('cleanupInactiveSessions', () => {
		it('removes only stale sessions', () => {
			vi.useFakeTimers()
			const store = useSessionActivityStore.getState()

			store.updateConfig({ activeSessionTimeout: 1_000 })

			store.markSessionActive('session-1')
			vi.advanceTimersByTime(500)
			store.markSessionActive('session-2')

			vi.advanceTimersByTime(600) // session-1 is now 1100ms old, session-2 is 600ms old

			store.cleanupInactiveSessions()

			expect(useSessionActivityStore.getState().isSessionActive('session-1')).toBe(false)
			expect(useSessionActivityStore.getState().isSessionActive('session-2')).toBe(true)
		})

		it('keeps active sessions', () => {
			vi.useFakeTimers()
			const store = useSessionActivityStore.getState()

			store.updateConfig({ activeSessionTimeout: 2_000 })
			store.markSessionActive('session-1')
			store.markSessionActive('session-2')

			vi.advanceTimersByTime(1_000) // Both still active

			store.cleanupInactiveSessions()

			expect(useSessionActivityStore.getState().activeSessions.size).toBe(2)
		})
	})

	describe('time-based state transitions', () => {
		it('transitions from active to inactive after timeout', () => {
			vi.useFakeTimers()
			const store = useSessionActivityStore.getState()

			store.updateConfig({ activeSessionTimeout: 1_000 })
			store.markSessionActive('session-1')

			expect(store.isSessionActive('session-1')).toBe(true)

			vi.advanceTimersByTime(500)
			expect(useSessionActivityStore.getState().isSessionActive('session-1')).toBe(true)

			vi.advanceTimersByTime(600) // Total 1100ms

			expect(useSessionActivityStore.getState().isSessionActive('session-1')).toBe(false)
		})
	})

	describe('clearAllActiveSessions', () => {
		it('removes all active sessions', () => {
			const { markSessionActive, clearAllActiveSessions } = useSessionActivityStore.getState()

			markSessionActive('session-1')
			markSessionActive('session-2')
			markSessionActive('session-3')

			expect(useSessionActivityStore.getState().activeSessions.size).toBe(3)

			clearAllActiveSessions()

			expect(useSessionActivityStore.getState().activeSessions.size).toBe(0)
		})
	})

	describe('updateConfig', () => {
		it('updates config values', () => {
			const { updateConfig } = useSessionActivityStore.getState()

			updateConfig({ activeSessionTimeout: 5000 })

			expect(useSessionActivityStore.getState().config.activeSessionTimeout).toBe(5000)
		})
	})
})
