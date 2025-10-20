import { useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { useProviderLogs } from '../hooks/useClaudeWatcher'
import type { LogEntry } from '../hooks/useClaudeWatcher'

interface LogViewerProps {
  provider: string
  title?: string
  defaultMaxLines?: number
  fullHeight?: boolean
  showBackButton?: boolean
  onBack?: () => void
  providerName?: string
}

function LogViewer({
  provider,
  title,
  defaultMaxLines = 100,
  fullHeight = false,
  showBackButton = false,
  onBack,
  providerName,
}: LogViewerProps) {
  const [maxLines, setMaxLines] = useState(defaultMaxLines)
  const queryClient = useQueryClient()

  const { data: logs = [], isLoading, error } = useProviderLogs(provider, maxLines)

  const handleRefresh = () => {
    queryClient.invalidateQueries({ queryKey: ['provider-logs', provider, maxLines] })
  }

  const getLevelColor = (level: string) => {
    switch (level.toLowerCase()) {
      case 'error':
        return 'text-red-400'
      case 'warn':
      case 'warning':
        return 'text-yellow-400'
      case 'info':
        return 'text-blue-400'
      case 'debug':
        return 'text-gray-500'
      default:
        return 'text-gray-300'
    }
  }

  const formatTimestamp = (timestamp: string) => {
    const date = new Date(timestamp)
    return date.toISOString().replace('T', ' ').slice(0, 19)
  }

  const formatLevel = (level: string) => {
    return level.toUpperCase().padEnd(5)
  }

  if (fullHeight) {
    return (
      <div className="h-full flex flex-col overflow-hidden">
        {/* Controls */}
        <div className="flex items-center justify-between p-4 border-b border-base-300 bg-base-100 shrink-0">
          <div className="flex items-center gap-3">
            {showBackButton && onBack && (
              <button onClick={onBack} className="btn btn-sm btn-ghost">
                <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    strokeWidth={2}
                    d="M15 19l-7-7 7-7"
                  />
                </svg>
                Back
              </button>
            )}
            {(title || providerName) && (
              <h1 className="text-2xl font-bold text-base-content">
                {providerName ? `${providerName} Logs` : title}
              </h1>
            )}
            {isLoading && <span className="loading loading-spinner loading-sm" />}
          </div>

          <div className="flex items-center gap-2">
            <select
              className="select select-bordered select-sm"
              value={maxLines}
              onChange={e => setMaxLines(Number(e.target.value))}
            >
              <option value={50}>50 lines</option>
              <option value={100}>100 lines</option>
              <option value={200}>200 lines</option>
              <option value={500}>500 lines</option>
            </select>

            <button className="btn btn-sm btn-primary" onClick={handleRefresh}>
              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
                />
              </svg>
              Refresh
            </button>
          </div>
        </div>

        {/* Error display */}
        {error && (
          <div className="alert alert-error m-4 shrink-0">
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
              />
            </svg>
            <span>Failed to load logs: {String(error)}</span>
          </div>
        )}

        {/* Logs display - Full height */}
        {!error && (
          <>
            <div className="bg-black flex-1 overflow-y-auto p-4 min-h-0">
              {logs.length === 0 ? (
                <div className="text-center text-gray-400 py-8 font-mono">
                  {isLoading ? 'Loading logs...' : 'No logs available'}
                </div>
              ) : (
                <div className="font-mono text-xs leading-tight">
                  {logs.map((log: LogEntry, index: number) => (
                    <div key={index} className="hover:bg-gray-900 px-1 py-0.5 text-gray-200">
                      <span className="text-gray-400 inline-block w-36 shrink-0">
                        {formatTimestamp(log.timestamp)}
                      </span>
                      <span
                        className={`inline-block w-12 shrink-0 font-medium ${getLevelColor(log.level)}`}
                      >
                        {formatLevel(log.level)}
                      </span>
                      <span className="text-gray-300 inline-block w-24 shrink-0 truncate">
                        {log.provider}
                      </span>
                      <span className="text-gray-100">
                        {log.message}
                        {log.details && (
                          <span className="text-gray-500 ml-2">{JSON.stringify(log.details)}</span>
                        )}
                      </span>
                    </div>
                  ))}
                </div>
              )}
            </div>

            {logs.length > 0 && (
              <div className="text-xs text-base-content/70 px-4 py-2 bg-base-100 border-t border-base-300 shrink-0">
                Showing {logs.length} most recent log entries
              </div>
            )}
          </>
        )}
      </div>
    )
  }

  return (
    <div className="space-y-2">
      {/* Controls */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          {title && <h2 className="text-lg font-semibold">{title}</h2>}
          {isLoading && <span className="loading loading-spinner loading-sm" />}
        </div>

        <div className="flex items-center gap-2">
          <select
            className="select select-bordered select-sm"
            value={maxLines}
            onChange={e => setMaxLines(Number(e.target.value))}
          >
            <option value={50}>50 lines</option>
            <option value={100}>100 lines</option>
            <option value={200}>200 lines</option>
            <option value={500}>500 lines</option>
          </select>

          <button className="btn btn-sm btn-primary" onClick={handleRefresh}>
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
              />
            </svg>
            Refresh
          </button>
        </div>
      </div>

      {/* Error display */}
      {error && (
        <div className="alert alert-error">
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
            />
          </svg>
          <span>Failed to load logs: {String(error)}</span>
        </div>
      )}

      {/* Logs display */}
      {!error && (
        <>
          <div className="bg-black rounded-lg p-4 max-h-96 overflow-y-auto">
            {logs.length === 0 ? (
              <div className="text-center text-gray-400 py-8 font-mono">
                {isLoading ? 'Loading logs...' : 'No logs available'}
              </div>
            ) : (
              <div className="font-mono text-xs leading-tight">
                {logs.map((log: LogEntry, index: number) => (
                  <div key={index} className="hover:bg-gray-900 px-1 py-0.5 text-gray-200">
                    <span className="text-gray-400 inline-block w-36 shrink-0">
                      {formatTimestamp(log.timestamp)}
                    </span>
                    <span
                      className={`inline-block w-12 shrink-0 font-medium ${getLevelColor(log.level)}`}
                    >
                      {formatLevel(log.level)}
                    </span>
                    <span className="text-gray-300 inline-block w-24 shrink-0 truncate">
                      {log.provider}
                    </span>
                    <span className="text-gray-100">
                      {log.message}
                      {log.details && (
                        <span className="text-gray-500 ml-2">{JSON.stringify(log.details)}</span>
                      )}
                    </span>
                  </div>
                ))}
              </div>
            )}
          </div>

          {logs.length > 0 && (
            <div className="text-xs text-base-content/70">
              Showing {logs.length} most recent log entries
            </div>
          )}
        </>
      )}
    </div>
  )
}

export default LogViewer
