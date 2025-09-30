import { useState } from 'react'
import { useParams, Navigate } from 'react-router-dom'
import { CODING_AGENTS } from '../types/providers'
import AgentConfig from '../components/Configuration/AgentConfig'
import LogViewer from '../components/LogViewer'
import { DocumentTextIcon, ChevronLeftIcon } from '@heroicons/react/24/outline'

function ProviderPage() {
  const { providerId } = useParams<{ providerId: string }>()
  const [showLogs, setShowLogs] = useState(false)

  const agent = CODING_AGENTS.find(a => a.id === providerId)

  if (!agent) {
    return <Navigate to="/overview" replace />
  }

  if (showLogs) {
    return (
      <div className="h-full flex flex-col">
        {/* Header */}
        <div className="p-4 border-b border-base-300 bg-base-100">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-3">
              <button
                onClick={() => setShowLogs(false)}
                className="btn btn-sm btn-ghost"
              >
                <ChevronLeftIcon className="w-5 h-5" />
                Back
              </button>
              <h1 className="text-2xl font-bold text-base-content">{agent.name} Logs</h1>
              <p className="text-sm text-base-content/70">View provider logs and system events</p>
            </div>
          </div>
        </div>

        {/* Logs Display - Full height with no padding */}
        <div className="flex-1 overflow-hidden">
          <LogViewer provider={providerId || ''} fullHeight />
        </div>
      </div>
    )
  }

  return (
    <div className="p-3">
      <AgentConfig
        agent={agent}
        headerActions={
          <button
            onClick={() => setShowLogs(true)}
            className="btn btn-sm btn-outline gap-2"
          >
            <DocumentTextIcon className="w-4 h-4" />
            Logs
          </button>
        }
      />
    </div>
  )
}

export default ProviderPage