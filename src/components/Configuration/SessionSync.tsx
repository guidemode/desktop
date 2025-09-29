import { useState } from 'react'
import { ArrowPathIcon, CloudArrowUpIcon, ExclamationTriangleIcon } from '@heroicons/react/24/outline'
import { CheckCircleIcon, XCircleIcon } from '@heroicons/react/24/solid'
import { formatDistanceToNow } from 'date-fns'
import { useSessionSync } from '../../hooks/useSessionSync'
import { CodingAgent } from '../../types/providers'

interface SessionSyncProps {
  agent: CodingAgent
}

function SessionSync({ agent }: SessionSyncProps) {
  const {
    scanSessions,
    syncSessions,
    resetProgress,
    progress,
    isScanning,
    isSyncing,
    error
  } = useSessionSync(agent.id)

  const [showDetails, setShowDetails] = useState(false)

  const handleScan = async () => {
    await scanSessions()
  }

  const handleSync = async () => {
    if (progress && progress.sessions_found.length > 0) {
      await syncSessions()
    }
  }

  const handleReset = async () => {
    await resetProgress()
    setShowDetails(false)
  }

  const hasScannedSessions = progress && progress.sessions_found.length > 0
  const isComplete = progress?.is_complete || false
  const hasErrors = progress?.errors && progress.errors.length > 0
  const isUploading = progress?.is_uploading || false

  return (
    <div className="card bg-base-100 shadow-sm border border-base-300">
      <div className="card-body">
        <div className="flex items-center justify-between mb-3">
          <div className="flex items-center gap-2">
            <CloudArrowUpIcon className="w-5 h-5 text-primary" />
            <h4 className="text-base font-semibold">Historical Session Sync</h4>
          </div>

          {isComplete && (
            <CheckCircleIcon className="w-5 h-5 text-success" />
          )}
        </div>

        <p className="text-sm text-base-content/70 mb-4">
          Upload your historical {agent.name} sessions to GuideAI for analytics and insights.
          Only sessions from selected projects will be included.
        </p>

        {/* Progress Section */}
        {progress && (
          <div className="bg-base-200 rounded-lg p-3 mb-4">
            {/* Progress Stats */}
            <div className="grid grid-cols-2 gap-4 mb-3">
              <div>
                <div className="text-sm font-medium">Sessions To Sync</div>
                <div className="text-lg font-bold text-primary">
                  {progress.total_sessions}
                </div>
              </div>
              <div>
                <div className="text-sm font-medium">Synced</div>
                <div className="text-lg font-bold text-success">
                  {progress.synced_sessions}
                </div>
              </div>
            </div>

            {/* Progress Bar */}
            {hasScannedSessions && (
              <div className="w-full">
                <div className="flex justify-between text-xs mb-1">
                  <span>Progress</span>
                  <span>{Math.round((progress.synced_sessions / progress.total_sessions) * 100)}%</span>
                </div>
                <progress
                  className="progress progress-primary w-full"
                  value={progress.synced_sessions}
                  max={progress.total_sessions}
                />
              </div>
            )}

            {/* Current Status */}
            {(isScanning || isSyncing || isUploading) && (
              <div className="flex items-center gap-2 mt-2">
                <span className="loading loading-spinner loading-xs"></span>
                <span className="text-sm">
                  {isScanning && `Scanning ${progress.current_provider}...`}
                  {isSyncing && progress.current_project && `Queueing ${progress.current_project}...`}
                  {isSyncing && !progress.current_project && 'Queueing sessions...'}
                  {isUploading && 'Uploading sessions...'}
                </span>
              </div>
            )}

            {/* Errors */}
            {hasErrors && (
              <div className="alert alert-warning mt-3">
                <ExclamationTriangleIcon className="w-4 h-4" />
                <div>
                  <div className="font-bold">Some issues occurred</div>
                  <div className="text-sm">
                    {progress.errors.length} error(s) during sync
                  </div>
                </div>
                <button
                  className="btn btn-sm btn-ghost"
                  onClick={() => setShowDetails(!showDetails)}
                >
                  {showDetails ? 'Hide' : 'Show'} Details
                </button>
              </div>
            )}

            {/* Error Details */}
            {showDetails && hasErrors && (
              <div className="mt-3">
                <div className="text-sm font-medium mb-2">Error Details:</div>
                <div className="max-h-32 overflow-y-auto text-xs space-y-1">
                  {progress.errors.map((error, index) => (
                    <div key={index} className="bg-base-300 p-2 rounded">
                      {error}
                    </div>
                  ))}
                </div>
              </div>
            )}

            {/* Session Details */}
            {showDetails && hasScannedSessions && (
              <div className="mt-3">
                <div className="text-sm font-medium mb-2">
                  Sessions Found ({progress.sessions_found.length}):
                </div>
                <div className="max-h-40 overflow-y-auto text-xs space-y-1">
                  {progress.sessions_found.map((session, index) => (
                    <div key={index} className="bg-base-300 p-2 rounded flex justify-between">
                      <span className="truncate flex-1">{session.project_name}/{session.file_name}</span>
                      <span className="text-base-content/60 ml-2">
                        {session.session_start_time && formatDistanceToNow(new Date(session.session_start_time), { addSuffix: true })}
                      </span>
                    </div>
                  ))}
                </div>
              </div>
            )}
          </div>
        )}

        {/* Error Message */}
        {error && (
          <div className="alert alert-error mb-4">
            <XCircleIcon className="w-4 h-4" />
            <span>{error}</span>
          </div>
        )}

        {/* Action Buttons */}
        <div className="flex gap-2">
          {!hasScannedSessions ? (
            <button
              className="btn btn-primary"
              onClick={handleScan}
              disabled={isScanning || isSyncing}
            >
              {isScanning ? (
                <>
                  <span className="loading loading-spinner loading-sm"></span>
                  Scanning...
                </>
              ) : (
                <>
                  <ArrowPathIcon className="w-4 h-4" />
                  Scan for Sessions
                </>
              )}
            </button>
          ) : !isComplete ? (
            <>
              <button
                className="btn btn-success"
                onClick={handleSync}
                disabled={isScanning || isSyncing || isUploading}
              >
                {(isSyncing || isUploading) ? (
                  <>
                    <span className="loading loading-spinner loading-sm"></span>
                    {isSyncing ? 'Queueing...' : 'Uploading...'}
                  </>
                ) : (
                  <>
                    <CloudArrowUpIcon className="w-4 h-4" />
                    Sync {progress.total_sessions} Sessions
                  </>
                )}
              </button>
              <button
                className="btn btn-ghost"
                onClick={handleReset}
                disabled={isScanning || isSyncing || isUploading}
              >
                Cancel
              </button>
            </>
          ) : (
            <div className="flex items-center justify-between w-full">
              <div className="flex items-center gap-2 text-success">
                <CheckCircleIcon className="w-5 h-5" />
                <span className="font-medium">
                  Successfully synced {progress.synced_sessions} sessions!
                </span>
              </div>
              <button
                className="btn btn-sm btn-ghost"
                onClick={handleReset}
              >
                Reset
              </button>
            </div>
          )}

          {hasScannedSessions && !isComplete && (
            <button
              className="btn btn-sm btn-ghost"
              onClick={() => setShowDetails(!showDetails)}
            >
              {showDetails ? 'Hide' : 'Show'} Details
            </button>
          )}
        </div>

        {/* Help Text */}
        <div className="text-xs text-base-content/60 mt-3">
          This will scan your local {agent.name} directories for session files and upload them to GuideAI.
          Duplicate sessions will be automatically detected and skipped.
        </div>
      </div>
    </div>
  )
}

export default SessionSync