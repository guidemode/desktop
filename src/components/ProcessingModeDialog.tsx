interface ProcessingModeDialogProps {
  isOpen: boolean
  sessionCount: number
  onSelectMode: (mode: 'core' | 'full') => void
  onCancel: () => void
}

export default function ProcessingModeDialog({
  isOpen,
  sessionCount,
  onSelectMode,
  onCancel,
}: ProcessingModeDialogProps) {
  if (!isOpen) return null

  return (
    <div className="modal modal-open" onClick={(e) => {
      if (e.target === e.currentTarget) {
        e.stopPropagation()
      }
    }}>
      <div className="modal-box max-w-2xl">
        <h3 className="font-bold text-lg mb-4">Choose Processing Mode</h3>

        <p className="text-sm text-base-content/70 mb-6">
          You are about to process {sessionCount} session{sessionCount !== 1 ? 's' : ''}.
          Choose how you want to process them:
        </p>

        <div className="space-y-4">
          {/* Core Metrics Only Option */}
          <button
            type="button"
            className="w-full p-4 border-2 border-base-300 rounded-lg hover:border-primary hover:bg-base-200 transition-all text-left"
            onClick={(e) => {
              e.preventDefault()
              e.stopPropagation()
              onSelectMode('core')
            }}
          >
            <div className="flex items-start gap-3">
              <div className="flex-shrink-0 mt-1">
                <svg className="w-6 h-6 text-primary" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 10V3L4 14h7v7l9-11h-7z" />
                </svg>
              </div>
              <div className="flex-1">
                <h4 className="font-semibold text-base mb-1">Core Metrics Only</h4>
                <p className="text-sm text-base-content/70 mb-2">
                  Quickly calculate performance, usage, quality, and engagement metrics for all sessions.
                </p>
                <ul className="text-xs text-base-content/60 space-y-1">
                  <li>✓ Fast processing (no delays)</li>
                  <li>✓ No AI API costs</li>
                  <li>✓ Re-processes ALL sessions</li>
                  <li>✓ Updates metrics immediately</li>
                </ul>
              </div>
            </div>
          </button>

          {/* Full Processing Option */}
          <button
            type="button"
            className="w-full p-4 border-2 border-base-300 rounded-lg hover:border-secondary hover:bg-base-200 transition-all text-left"
            onClick={(e) => {
              e.preventDefault()
              e.stopPropagation()
              onSelectMode('full')
            }}
          >
            <div className="flex items-start gap-3">
              <div className="flex-shrink-0 mt-1">
                <svg className="w-6 h-6 text-secondary" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z" />
                </svg>
              </div>
              <div className="flex-1">
                <h4 className="font-semibold text-base mb-1">Full Processing (Core + AI)</h4>
                <p className="text-sm text-base-content/70 mb-2">
                  Calculate core metrics plus AI-generated summaries, quality assessments, and phase analysis.
                </p>
                <ul className="text-xs text-base-content/60 space-y-1">
                  <li>✓ Complete session analysis</li>
                  <li>✓ AI summaries and insights</li>
                  <li>✓ Only processes unfinished sessions</li>
                  <li>⚠ Slower (2s delay between requests)</li>
                  <li>⚠ Uses AI API credits</li>
                </ul>
              </div>
            </div>
          </button>
        </div>

        <div className="modal-action mt-6">
          <button
            type="button"
            className="btn btn-sm btn-ghost"
            onClick={(e) => {
              e.preventDefault()
              e.stopPropagation()
              onCancel()
            }}
          >
            Cancel
          </button>
        </div>
      </div>
    </div>
  )
}
