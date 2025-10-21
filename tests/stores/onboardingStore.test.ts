import { beforeEach, describe, expect, it } from 'vitest'
import { useOnboardingStore } from '../../src/stores/onboardingStore'

describe('useOnboardingStore', () => {
	beforeEach(() => {
		// Reset store to initial state
		useOnboardingStore.setState({
			hasCompletedTour: false,
			isTourRunning: false,
			currentStepIndex: 0,
		})
	})

	describe('Initial state', () => {
		it('has correct default values', () => {
			const state = useOnboardingStore.getState()

			expect(state.hasCompletedTour).toBe(false)
			expect(state.isTourRunning).toBe(false)
			expect(state.currentStepIndex).toBe(0)
		})
	})

	describe('startTour', () => {
		it('sets isTourRunning to true', () => {
			const { startTour } = useOnboardingStore.getState()

			startTour()

			expect(useOnboardingStore.getState().isTourRunning).toBe(true)
		})

		it('does not change other state values', () => {
			const { startTour } = useOnboardingStore.getState()

			startTour()

			expect(useOnboardingStore.getState().hasCompletedTour).toBe(false)
			expect(useOnboardingStore.getState().currentStepIndex).toBe(0)
		})

		it('can restart tour after stopping', () => {
			const { startTour, stopTour } = useOnboardingStore.getState()

			startTour()
			expect(useOnboardingStore.getState().isTourRunning).toBe(true)

			stopTour()
			expect(useOnboardingStore.getState().isTourRunning).toBe(false)

			startTour()
			expect(useOnboardingStore.getState().isTourRunning).toBe(true)
		})
	})

	describe('stopTour', () => {
		it('sets isTourRunning to false', () => {
			useOnboardingStore.setState({ isTourRunning: true })

			const { stopTour } = useOnboardingStore.getState()
			stopTour()

			expect(useOnboardingStore.getState().isTourRunning).toBe(false)
		})

		it('preserves current step index', () => {
			useOnboardingStore.setState({ isTourRunning: true, currentStepIndex: 3 })

			const { stopTour } = useOnboardingStore.getState()
			stopTour()

			expect(useOnboardingStore.getState().currentStepIndex).toBe(3)
		})

		it('does not mark tour as completed', () => {
			useOnboardingStore.setState({ isTourRunning: true })

			const { stopTour } = useOnboardingStore.getState()
			stopTour()

			expect(useOnboardingStore.getState().hasCompletedTour).toBe(false)
		})
	})

	describe('completeTour', () => {
		it('marks tour as completed', () => {
			const { completeTour } = useOnboardingStore.getState()

			completeTour()

			expect(useOnboardingStore.getState().hasCompletedTour).toBe(true)
		})

		it('stops the tour', () => {
			useOnboardingStore.setState({ isTourRunning: true })

			const { completeTour } = useOnboardingStore.getState()
			completeTour()

			expect(useOnboardingStore.getState().isTourRunning).toBe(false)
		})

		it('resets step index to 0', () => {
			useOnboardingStore.setState({ currentStepIndex: 5 })

			const { completeTour } = useOnboardingStore.getState()
			completeTour()

			expect(useOnboardingStore.getState().currentStepIndex).toBe(0)
		})

		it('sets all completion state correctly', () => {
			useOnboardingStore.setState({
				isTourRunning: true,
				currentStepIndex: 7,
			})

			const { completeTour } = useOnboardingStore.getState()
			completeTour()

			const state = useOnboardingStore.getState()
			expect(state.hasCompletedTour).toBe(true)
			expect(state.isTourRunning).toBe(false)
			expect(state.currentStepIndex).toBe(0)
		})
	})

	describe('resetTour', () => {
		it('marks tour as not completed', () => {
			useOnboardingStore.setState({ hasCompletedTour: true })

			const { resetTour } = useOnboardingStore.getState()
			resetTour()

			expect(useOnboardingStore.getState().hasCompletedTour).toBe(false)
		})

		it('starts the tour', () => {
			const { resetTour } = useOnboardingStore.getState()

			resetTour()

			expect(useOnboardingStore.getState().isTourRunning).toBe(true)
		})

		it('resets step index to 0', () => {
			useOnboardingStore.setState({ currentStepIndex: 5 })

			const { resetTour } = useOnboardingStore.getState()
			resetTour()

			expect(useOnboardingStore.getState().currentStepIndex).toBe(0)
		})

		it('allows replaying tour after completion', () => {
			useOnboardingStore.setState({
				hasCompletedTour: true,
				isTourRunning: false,
				currentStepIndex: 0,
			})

			const { resetTour } = useOnboardingStore.getState()
			resetTour()

			const state = useOnboardingStore.getState()
			expect(state.hasCompletedTour).toBe(false)
			expect(state.isTourRunning).toBe(true)
			expect(state.currentStepIndex).toBe(0)
		})
	})

	describe('setStepIndex', () => {
		it('updates current step index', () => {
			const { setStepIndex } = useOnboardingStore.getState()

			setStepIndex(3)

			expect(useOnboardingStore.getState().currentStepIndex).toBe(3)
		})

		it('allows setting to 0', () => {
			useOnboardingStore.setState({ currentStepIndex: 5 })

			const { setStepIndex } = useOnboardingStore.getState()
			setStepIndex(0)

			expect(useOnboardingStore.getState().currentStepIndex).toBe(0)
		})

		it('allows forward progression', () => {
			const { setStepIndex } = useOnboardingStore.getState()

			setStepIndex(1)
			expect(useOnboardingStore.getState().currentStepIndex).toBe(1)

			setStepIndex(2)
			expect(useOnboardingStore.getState().currentStepIndex).toBe(2)

			setStepIndex(3)
			expect(useOnboardingStore.getState().currentStepIndex).toBe(3)
		})

		it('allows backward progression (for review)', () => {
			useOnboardingStore.setState({ currentStepIndex: 5 })

			const { setStepIndex } = useOnboardingStore.getState()
			setStepIndex(3)

			expect(useOnboardingStore.getState().currentStepIndex).toBe(3)
		})

		it('does not change other state values', () => {
			useOnboardingStore.setState({
				hasCompletedTour: true,
				isTourRunning: false,
			})

			const { setStepIndex } = useOnboardingStore.getState()
			setStepIndex(5)

			expect(useOnboardingStore.getState().hasCompletedTour).toBe(true)
			expect(useOnboardingStore.getState().isTourRunning).toBe(false)
		})
	})

	describe('State machine transitions', () => {
		it('handles full tour flow', () => {
			const {
				startTour,
				setStepIndex,
				completeTour,
			} = useOnboardingStore.getState()

			// Start tour
			startTour()
			expect(useOnboardingStore.getState().isTourRunning).toBe(true)

			// Progress through steps
			setStepIndex(1)
			setStepIndex(2)
			setStepIndex(3)

			// Complete tour
			completeTour()
			const state = useOnboardingStore.getState()
			expect(state.hasCompletedTour).toBe(true)
			expect(state.isTourRunning).toBe(false)
			expect(state.currentStepIndex).toBe(0)
		})

		it('handles tour interruption and resume', () => {
			const { startTour, setStepIndex, stopTour } = useOnboardingStore.getState()

			// Start and progress
			startTour()
			setStepIndex(3)

			// Stop mid-tour
			stopTour()
			expect(useOnboardingStore.getState().currentStepIndex).toBe(3)

			// Resume
			startTour()
			expect(useOnboardingStore.getState().isTourRunning).toBe(true)
			expect(useOnboardingStore.getState().currentStepIndex).toBe(3) // Should preserve step
		})
	})

	describe('Persistence', () => {
		it('persists state across store access', () => {
			const { completeTour } = useOnboardingStore.getState()

			completeTour()

			// Access store again
			const state = useOnboardingStore.getState()
			expect(state.hasCompletedTour).toBe(true)
		})
	})
})
