import { useState, useCallback } from 'react'

export interface AiProcessingStep {
  name: string
  description: string
  percentage: number
}

export interface AiProcessingProgress {
  currentStep: AiProcessingStep | null
  isProcessing: boolean
}

/**
 * Hook for tracking AI processing progress through multiple steps
 */
export function useAiProcessingProgress() {
  const [progress, setProgress] = useState<AiProcessingProgress>({
    currentStep: null,
    isProcessing: false,
  })

  const updateProgress = useCallback((step: AiProcessingStep | null) => {
    setProgress({
      currentStep: step,
      isProcessing: step !== null,
    })
  }, [])

  const reset = useCallback(() => {
    setProgress({
      currentStep: null,
      isProcessing: false,
    })
  }, [])

  return {
    progress,
    updateProgress,
    reset,
  }
}

/**
 * Predefined AI processing steps
 */
export const AI_PROCESSING_STEPS = {
  METRICS: {
    name: 'Calculating Metrics',
    description: 'Analyzing session performance and quality metrics',
    percentage: 0,
  },
  SUMMARY: {
    name: 'Generating Summary',
    description: 'Creating AI-powered session summary',
    percentage: 25,
  },
  QUALITY: {
    name: 'Assessing Quality',
    description: 'Evaluating session quality and effectiveness',
    percentage: 50,
  },
  PHASE_ANALYSIS: {
    name: 'Analyzing Phases',
    description: 'Breaking down session into distinct phases',
    percentage: 75,
  },
  COMPLETE: {
    name: 'Complete',
    description: 'Processing finished successfully',
    percentage: 100,
  },
} as const
