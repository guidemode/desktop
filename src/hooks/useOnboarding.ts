import { useOnboardingStore } from '../stores/onboardingStore'

/**
 * Convenience hook for accessing onboarding state and actions
 */
export function useOnboarding() {
  const {
    hasCompletedTour,
    isTourRunning,
    currentStepIndex,
    startTour,
    stopTour,
    completeTour,
    resetTour,
    setStepIndex,
  } = useOnboardingStore()

  return {
    hasCompletedTour,
    isTourRunning,
    currentStepIndex,
    startTour,
    stopTour,
    completeTour,
    resetTour,
    setStepIndex,
  }
}
