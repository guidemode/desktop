import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useToastStore } from '../../src/stores/toastStore'
import type { Toast } from '../../src/stores/toastStore'

describe('useToastStore', () => {
	beforeEach(() => {
		// Reset store to initial state
		useToastStore.setState({
			toasts: [],
		})
		vi.useFakeTimers()
	})

	afterEach(() => {
		vi.useRealTimers()
	})

	describe('Initial state', () => {
		it('starts with empty toasts array', () => {
			const state = useToastStore.getState()

			expect(state.toasts).toEqual([])
		})
	})

	describe('addToast', () => {
		it('adds toast to queue', () => {
			const { addToast } = useToastStore.getState()

			addToast({
				type: 'success',
				message: 'Operation successful',
			})

			const state = useToastStore.getState()
			expect(state.toasts).toHaveLength(1)
			expect(state.toasts[0].type).toBe('success')
			expect(state.toasts[0].message).toBe('Operation successful')
		})

		it('generates unique ID for each toast', () => {
			const { addToast } = useToastStore.getState()

			addToast({ type: 'info', message: 'Toast 1' })
			addToast({ type: 'info', message: 'Toast 2' })

			const state = useToastStore.getState()
			expect(state.toasts[0].id).toBeDefined()
			expect(state.toasts[1].id).toBeDefined()
			expect(state.toasts[0].id).not.toBe(state.toasts[1].id)
		})

		it('supports all toast types', () => {
			const { addToast } = useToastStore.getState()

			addToast({ type: 'success', message: 'Success' })
			addToast({ type: 'error', message: 'Error' })
			addToast({ type: 'info', message: 'Info' })
			addToast({ type: 'warning', message: 'Warning' })

			const state = useToastStore.getState()
			expect(state.toasts).toHaveLength(4)
			expect(state.toasts.map(t => t.type)).toEqual(['success', 'error', 'info', 'warning'])
		})

		it('auto-dismisses after default duration (5 seconds)', () => {
			const { addToast } = useToastStore.getState()

			addToast({ type: 'info', message: 'Test' })

			expect(useToastStore.getState().toasts).toHaveLength(1)

			// Fast-forward 5 seconds
			vi.advanceTimersByTime(5000)

			expect(useToastStore.getState().toasts).toHaveLength(0)
		})

		it('auto-dismisses after custom duration', () => {
			const { addToast } = useToastStore.getState()

			addToast({ type: 'info', message: 'Test', duration: 2000 })

			expect(useToastStore.getState().toasts).toHaveLength(1)

			// Should still be visible after 1 second
			vi.advanceTimersByTime(1000)
			expect(useToastStore.getState().toasts).toHaveLength(1)

			// Should be dismissed after 2 seconds
			vi.advanceTimersByTime(1000)
			expect(useToastStore.getState().toasts).toHaveLength(0)
		})

		it('handles multiple toasts with different durations', () => {
			const { addToast } = useToastStore.getState()

			addToast({ type: 'info', message: 'Short', duration: 1000 })
			addToast({ type: 'info', message: 'Medium', duration: 3000 })
			addToast({ type: 'info', message: 'Long', duration: 5000 })

			expect(useToastStore.getState().toasts).toHaveLength(3)

			// After 1 second, short toast should be gone
			vi.advanceTimersByTime(1000)
			expect(useToastStore.getState().toasts).toHaveLength(2)

			// After 3 seconds total, medium toast should be gone
			vi.advanceTimersByTime(2000)
			expect(useToastStore.getState().toasts).toHaveLength(1)
			expect(useToastStore.getState().toasts[0].message).toBe('Long')

			// After 5 seconds total, all should be gone
			vi.advanceTimersByTime(2000)
			expect(useToastStore.getState().toasts).toHaveLength(0)
		})
	})

	describe('removeToast', () => {
		it('removes toast by ID', () => {
			const { addToast, removeToast } = useToastStore.getState()

			addToast({ type: 'info', message: 'Test' })
			const toastId = useToastStore.getState().toasts[0].id

			removeToast(toastId)

			expect(useToastStore.getState().toasts).toHaveLength(0)
		})

		it('only removes specified toast', () => {
			const { addToast, removeToast } = useToastStore.getState()

			addToast({ type: 'info', message: 'Toast 1' })
			addToast({ type: 'info', message: 'Toast 2' })
			addToast({ type: 'info', message: 'Toast 3' })

			const toasts = useToastStore.getState().toasts
			const middleToastId = toasts[1].id

			removeToast(middleToastId)

			const remaining = useToastStore.getState().toasts
			expect(remaining).toHaveLength(2)
			expect(remaining[0].message).toBe('Toast 1')
			expect(remaining[1].message).toBe('Toast 3')
		})

		it('handles removal of non-existent toast gracefully', () => {
			const { addToast, removeToast } = useToastStore.getState()

			addToast({ type: 'info', message: 'Test' })

			removeToast('non-existent-id')

			expect(useToastStore.getState().toasts).toHaveLength(1)
		})

		it('allows manual dismissal before auto-dismiss', () => {
			const { addToast, removeToast } = useToastStore.getState()

			addToast({ type: 'info', message: 'Test', duration: 5000 })
			const toastId = useToastStore.getState().toasts[0].id

			// Manually remove after 1 second
			vi.advanceTimersByTime(1000)
			removeToast(toastId)

			expect(useToastStore.getState().toasts).toHaveLength(0)

			// Advance to when auto-dismiss would have triggered
			vi.advanceTimersByTime(4000)

			// Should still be empty (no error from trying to remove already-removed toast)
			expect(useToastStore.getState().toasts).toHaveLength(0)
		})
	})

	describe('clearAll', () => {
		it('removes all toasts', () => {
			const { addToast, clearAll } = useToastStore.getState()

			addToast({ type: 'success', message: 'Toast 1' })
			addToast({ type: 'error', message: 'Toast 2' })
			addToast({ type: 'warning', message: 'Toast 3' })

			expect(useToastStore.getState().toasts).toHaveLength(3)

			clearAll()

			expect(useToastStore.getState().toasts).toHaveLength(0)
		})

		it('works when toasts array is already empty', () => {
			const { clearAll } = useToastStore.getState()

			clearAll()

			expect(useToastStore.getState().toasts).toHaveLength(0)
		})

		it('clears toasts before they auto-dismiss', () => {
			const { addToast, clearAll } = useToastStore.getState()

			addToast({ type: 'info', message: 'Test', duration: 5000 })

			vi.advanceTimersByTime(1000)

			clearAll()

			expect(useToastStore.getState().toasts).toHaveLength(0)

			// Advance to when auto-dismiss would have triggered
			vi.advanceTimersByTime(4000)

			// Should still be empty
			expect(useToastStore.getState().toasts).toHaveLength(0)
		})
	})

	describe('Toast queue management', () => {
		it('maintains insertion order', () => {
			const { addToast } = useToastStore.getState()

			addToast({ type: 'info', message: 'First' })
			addToast({ type: 'info', message: 'Second' })
			addToast({ type: 'info', message: 'Third' })

			const toasts = useToastStore.getState().toasts
			expect(toasts[0].message).toBe('First')
			expect(toasts[1].message).toBe('Second')
			expect(toasts[2].message).toBe('Third')
		})

		it('handles rapid toast additions', () => {
			const { addToast } = useToastStore.getState()

			// Add 10 toasts rapidly
			for (let i = 0; i < 10; i++) {
				addToast({ type: 'info', message: `Toast ${i}` })
			}

			expect(useToastStore.getState().toasts).toHaveLength(10)
		})

		it('auto-dismisses toasts in order', () => {
			const { addToast } = useToastStore.getState()

			// Add toasts with same duration
			addToast({ type: 'info', message: 'First', duration: 1000 })
			vi.advanceTimersByTime(100)
			addToast({ type: 'info', message: 'Second', duration: 1000 })
			vi.advanceTimersByTime(100)
			addToast({ type: 'info', message: 'Third', duration: 1000 })

			// After 1000ms from first toast
			vi.advanceTimersByTime(800)
			let toasts = useToastStore.getState().toasts
			expect(toasts).toHaveLength(2)
			expect(toasts[0].message).toBe('Second')

			// After 1000ms from second toast
			vi.advanceTimersByTime(100)
			toasts = useToastStore.getState().toasts
			expect(toasts).toHaveLength(1)
			expect(toasts[0].message).toBe('Third')

			// After 1000ms from third toast
			vi.advanceTimersByTime(100)
			expect(useToastStore.getState().toasts).toHaveLength(0)
		})
	})

	describe('Toast priorities (via type)', () => {
		it('can identify toast by type', () => {
			const { addToast } = useToastStore.getState()

			addToast({ type: 'error', message: 'Critical error' })
			addToast({ type: 'info', message: 'Info message' })

			const toasts = useToastStore.getState().toasts
			const errorToast = toasts.find(t => t.type === 'error')
			const infoToast = toasts.find(t => t.type === 'info')

			expect(errorToast?.message).toBe('Critical error')
			expect(infoToast?.message).toBe('Info message')
		})
	})

	describe('Edge cases', () => {
		it('handles empty message', () => {
			const { addToast } = useToastStore.getState()

			addToast({ type: 'info', message: '' })

			expect(useToastStore.getState().toasts[0].message).toBe('')
		})

		it('handles very long messages', () => {
			const { addToast } = useToastStore.getState()

			const longMessage = 'A'.repeat(1000)
			addToast({ type: 'info', message: longMessage })

			expect(useToastStore.getState().toasts[0].message).toBe(longMessage)
		})

		it('handles duration of 0', () => {
			const { addToast } = useToastStore.getState()

			addToast({ type: 'info', message: 'Test', duration: 0 })

			expect(useToastStore.getState().toasts).toHaveLength(1)

			// Should be removed after timer fires (even with 0 duration)
			vi.runAllTimers()
			expect(useToastStore.getState().toasts).toHaveLength(0)
		})
	})
})
