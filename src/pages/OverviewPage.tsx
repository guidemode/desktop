import { DocumentTextIcon } from '@heroicons/react/24/outline'
import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import LogViewer from '../components/LogViewer'
import ProviderIcon from '../components/icons/ProviderIcon'
import { useProviderConfig } from '../hooks/useProviderConfig'
import { CODING_AGENTS } from '../types/providers'

function OverviewPage() {
  const navigate = useNavigate()

  return (
    <div className="p-3">
      <div className="mb-3">
        <h1 className="text-lg font-bold text-base-content">Overview</h1>
        <p className="text-sm text-base-content/70 mt-1">
          Overview of all your coding agents and their status
        </p>
      </div>

      <div className="grid gap-3">
        {CODING_AGENTS.map(agent => (
          <ProviderCard
            key={agent.id}
            agent={agent}
            onConfigure={() => navigate(`/provider/${agent.id}`)}
          />
        ))}
      </div>
    </div>
  )
}

interface ProviderCardProps {
  agent: (typeof CODING_AGENTS)[0]
  onConfigure: () => void
}

function ProviderCard({ agent, onConfigure }: ProviderCardProps) {
  const { data: config, isLoading } = useProviderConfig(agent.id)
  const [showLogs, setShowLogs] = useState(false)

  if (showLogs) {
    return (
      <div className="card bg-base-100 shadow-sm border border-base-300">
        <div className="card-body p-0">
          <div className="flex items-center justify-between p-3 border-b border-base-300">
            <div className="flex items-center gap-3">
              <div className="avatar placeholder">
                <div className="bg-base-200 rounded-lg w-8 h-8 flex items-center justify-center p-1.5">
                  <ProviderIcon providerId={agent.id} size={20} />
                </div>
              </div>
              <h3 className="text-base font-semibold">{agent.name} Logs</h3>
            </div>
            <button onClick={() => setShowLogs(false)} className="btn btn-sm btn-ghost">
              Back
            </button>
          </div>
          <div className="h-96 overflow-hidden">
            <LogViewer provider={agent.id} fullHeight />
          </div>
        </div>
      </div>
    )
  }

  return (
    <div className="card bg-base-100 shadow-sm border border-base-300">
      <div className="card-body">
        <div className="flex items-center justify-between">
          <button
            onClick={onConfigure}
            className="flex items-center gap-3 hover:opacity-70 transition-opacity"
          >
            <div className={'avatar placeholder'}>
              <div
                className={
                  'bg-base-200 rounded-lg w-10 h-10 flex items-center justify-center p-1.5'
                }
              >
                <ProviderIcon providerId={agent.id} size={28} />
              </div>
            </div>
            <div className="text-left">
              <h3 className="text-base font-semibold">{agent.name}</h3>
              <p className="text-sm text-base-content/70">{agent.description}</p>
            </div>
          </button>

          <div className="flex items-center gap-3">
            {isLoading ? (
              <span className="loading loading-spinner loading-sm" />
            ) : (
              <div className="flex items-center gap-2">
                <div
                  className={`w-2 h-2 rounded-full ${
                    config?.enabled ? 'bg-success' : 'bg-base-content/30'
                  }`}
                />
                <span className="text-sm text-base-content/70">
                  {config?.enabled ? 'Enabled' : 'Disabled'}
                </span>
              </div>
            )}
            <button className="btn btn-sm btn-ghost gap-2" onClick={() => setShowLogs(true)}>
              <DocumentTextIcon className="w-4 h-4" />
              Logs
            </button>
            <button className="btn btn-sm btn-primary" onClick={onConfigure}>
              Configure
            </button>
          </div>
        </div>

        {config?.enabled && (
          <div className="mt-3 pt-3 border-t border-base-300">
            <div className="text-xs text-base-content/70 space-y-1">
              <div>Home: {config.homeDirectory || agent.defaultHomeDirectory}</div>
              <div>
                Projects:{' '}
                {config.projectSelection === 'ALL'
                  ? 'All projects'
                  : `${config.selectedProjects.length} selected`}
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  )
}

export default OverviewPage
