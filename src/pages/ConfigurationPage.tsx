import AgentConfig from '../components/Configuration/AgentConfig'
import { CODING_AGENTS } from '../types/providers'

function ConfigurationPage() {
  return (
    <div className="p-3">
      <div className="mb-3">
        <h1 className="text-lg font-bold text-base-content">Configuration</h1>
        <p className="text-sm text-base-content/70 mt-1">
          Configure your coding agents and monitoring settings
        </p>
      </div>

      <div className="space-y-3">
        {CODING_AGENTS.map(agent => (
          <AgentConfig key={agent.id} agent={agent} />
        ))}
      </div>
    </div>
  )
}

export default ConfigurationPage
