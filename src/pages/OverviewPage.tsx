import { CODING_AGENTS } from '../types/providers'
import { useProviderConfig } from '../hooks/useProviderConfig'
import { useNavigate } from 'react-router-dom'
import ProviderIcon from '../components/icons/ProviderIcon'

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
        {CODING_AGENTS.map((agent) => (
          <ProviderCard key={agent.id} agent={agent} onConfigure={() => navigate(`/provider/${agent.id}`)} />
        ))}
      </div>
    </div>
  )
}

interface ProviderCardProps {
  agent: typeof CODING_AGENTS[0]
  onConfigure: () => void
}

function ProviderCard({ agent, onConfigure }: ProviderCardProps) {
  const { data: config, isLoading } = useProviderConfig(agent.id)

  return (
    <div className="card bg-base-100 shadow-sm border border-base-300">
      <div className="card-body">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            <div className={`avatar placeholder`}>
              <div className={`bg-base-200 rounded-lg w-10 h-10 flex items-center justify-center p-1.5`}>
                <ProviderIcon providerId={agent.id} size={28} />
              </div>
            </div>
            <div>
              <h3 className="text-base font-semibold">{agent.name}</h3>
              <p className="text-sm text-base-content/70">{agent.description}</p>
            </div>
          </div>

          <div className="flex items-center gap-3">
            {isLoading ? (
              <span className="loading loading-spinner loading-sm"></span>
            ) : (
              <div className="flex items-center gap-2">
                <div className={`w-2 h-2 rounded-full ${
                  config?.enabled ? 'bg-success' : 'bg-base-content/30'
                }`} />
                <span className="text-sm text-base-content/70">
                  {config?.enabled ? 'Enabled' : 'Disabled'}
                </span>
              </div>
            )}
            <button
              className="btn btn-sm btn-primary"
              onClick={onConfigure}
            >
              Configure
            </button>
          </div>
        </div>

        {config?.enabled && (
          <div className="mt-3 pt-3 border-t border-base-300">
            <div className="text-xs text-base-content/70 space-y-1">
              <div>Home: {config.homeDirectory || agent.defaultHomeDirectory}</div>
              <div>
                Projects: {config.projectSelection === 'ALL'
                  ? 'All projects'
                  : `${config.selectedProjects.length} selected`
                }
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  )
}

export default OverviewPage