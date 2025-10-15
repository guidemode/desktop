import { useState, useEffect } from 'react'
import { useParams, Navigate, useSearchParams } from 'react-router-dom'
import { CODING_AGENTS } from '../types/providers'
import AgentConfig from '../components/Configuration/AgentConfig'
import LogViewer from '../components/LogViewer'
import { DocumentTextIcon } from '@heroicons/react/24/outline'

function ProviderPage() {
  const { providerId } = useParams<{ providerId: string }>()
  const [searchParams] = useSearchParams()
  const [showLogs, setShowLogs] = useState(false)

  // Check if we should show logs based on query parameter
  useEffect(() => {
    if (searchParams.get('showLogs') === 'true') {
      setShowLogs(true)
    }
  }, [searchParams])

  const agent = CODING_AGENTS.find(a => a.id === providerId)

  if (!agent) {
    return <Navigate to="/overview" replace />
  }

  if (showLogs) {
    return (
      <div className="-mx-6 -mt-6 h-[calc(100vh-60px)] flex flex-col overflow-hidden max-w-none">
        <LogViewer provider={providerId || ''} fullHeight showBackButton onBack={() => setShowLogs(false)} providerName={agent.name} />
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
            data-tour="provider-logs-button"
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