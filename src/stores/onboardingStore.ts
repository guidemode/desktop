import { create } from 'zustand'
import { persist } from 'zustand/middleware'

interface OnboardingState {
  // Whether the user has completed the tour
  hasCompletedTour: boolean
  // Whether the tour is currently running
  isTourRunning: boolean
  // Current step index (for resuming)
  currentStepIndex: number
  // Actions
  startTour: () => void
  stopTour: () => void
  completeTour: () => void
  resetTour: () => void
  setStepIndex: (index: number) => void
}

export const useOnboardingStore = create<OnboardingState>()(
  persist(
    (set) => ({
      hasCompletedTour: false,
      isTourRunning: false,
      currentStepIndex: 0,

      startTour: () => set({ isTourRunning: true }),

      stopTour: () => set({ isTourRunning: false }),

      completeTour: () => set({
        hasCompletedTour: true,
        isTourRunning: false,
        currentStepIndex: 0
      }),

      resetTour: () => set({
        hasCompletedTour: false,
        currentStepIndex: 0,
        isTourRunning: true
      }),

      setStepIndex: (index: number) => set({ currentStepIndex: index }),
    }),
    {
      name: 'guideai-onboarding-storage',
    }
  )
)
