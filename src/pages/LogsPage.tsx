import { useState } from 'react'
import { useProviderLogs } from '../hooks/useClaudeWatcher'

function LogsPage() {
  const [selectedProvider, setSelectedProvider] = useState('app')
  const [maxLines, setMaxLines] = useState(100)

  const { data: logs = [], isLoading, error } = useProviderLogs(selectedProvider, maxLines)

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

  return (
    <div className="-mx-6 -mt-6 h-[calc(100vh-60px)] flex flex-col overflow-hidden max-w-none">
      {/* Header with Controls */}
      <div className="flex items-center justify-between p-4 border-b border-base-300 shrink-0">
        <div>
          <h1 className="text-2xl font-bold text-base-content">Logs</h1>
          <p className="text-sm text-base-content/70">View provider logs and system events</p>
        </div>

        <div className="flex gap-2 items-center">
          <select
            className="select select-bordered select-sm"
            value={selectedProvider}
            onChange={(e) => setSelectedProvider(e.target.value)}
          >
            <optgroup label="System Logs">
              <option value="app">Application</option>
              <option value="system">System</option>
              <option value="database">Database</option>
              <option value="upload-queue">Upload Queue</option>
            </optgroup>
            <optgroup label="Provider Logs">
              <option value="claude-code">Claude Code</option>
              <option value="opencode">OpenCode</option>
              <option value="codex">Codex</option>
            </optgroup>
          </select>

          <select
            className="select select-bordered select-sm"
            value={maxLines}
            onChange={(e) => setMaxLines(Number(e.target.value))}
          >
            <option value={50}>50 lines</option>
            <option value={100}>100 lines</option>
            <option value={200}>200 lines</option>
            <option value={500}>500 lines</option>
          </select>

          <button
            className="btn btn-primary btn-sm"
            onClick={() => window.location.reload()}
          >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
            </svg>
            Refresh
          </button>
        </div>
      </div>

      {/* Error display */}
      {error && (
        <div className="alert alert-error m-4 shrink-0">
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
          <span>Failed to load logs: {String(error)}</span>
        </div>
      )}

      {/* Logs Display - Scrollable */}
      {!error && (
        <div className="flex-1 bg-black overflow-y-auto p-4 min-h-0">
          {logs.length === 0 ? (
            <div className="text-center text-gray-400 py-8 font-mono">
              {isLoading ? 'Loading logs...' : 'No logs available'}
            </div>
          ) : (
            <div className="font-mono text-xs leading-tight">
              {logs.map((log, index) => (
                <div key={index} className="hover:bg-gray-900 px-1 py-0.5 text-gray-200">
                  <span className="text-gray-400 inline-block w-36 shrink-0">
                    {formatTimestamp(log.timestamp)}
                  </span>
                  <span className={`inline-block w-12 shrink-0 font-medium ${getLevelColor(log.level)}`}>
                    {formatLevel(log.level)}
                  </span>
                  <span className="text-gray-300 inline-block w-24 shrink-0 truncate">
                    {log.provider}
                  </span>
                  <span className="text-gray-100">
                    {log.message}
                    {log.details && (
                      <span className="text-gray-500 ml-2">
                        {JSON.stringify(log.details)}
                      </span>
                    )}
                  </span>
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {/* Footer */}
      {logs.length > 0 && (
        <div className="text-xs text-base-content/70 px-4 py-2 bg-base-100 border-t border-base-300 shrink-0">
          Showing {logs.length} most recent log entries
        </div>
      )}
    </div>
  )
}

export default LogsPage