import type { AiProcessingStep } from '../hooks/useAiProcessingProgress'

interface AiProcessingProgressProps {
  step: AiProcessingStep
  compact?: boolean
}

/**
 * Component to display AI processing progress with step name and progress bar
 */
export function AiProcessingProgress({ step, compact = false }: AiProcessingProgressProps) {
  if (compact) {
    return (
      <div className="flex items-center gap-2">
        <span className="loading loading-spinner loading-xs"></span>
        <span className="text-xs">{step.name}</span>
        <span className="text-xs text-base-content/50">{step.percentage}%</span>
      </div>
    )
  }

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <span className="loading loading-spinner loading-sm"></span>
          <span className="text-sm font-medium">{step.name}</span>
        </div>
        <span className="text-sm font-semibold text-primary">{step.percentage}%</span>
      </div>
      <progress
        className="progress progress-primary w-full"
        value={step.percentage}
        max="100"
      ></progress>
      <p className="text-xs text-base-content/60">{step.description}</p>
    </div>
  )
}
