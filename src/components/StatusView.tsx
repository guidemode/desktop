import { invoke } from '@tauri-apps/api'
import { useAuth } from '../hooks/useAuth'
import { useUploadQueueStatus } from '../hooks/useUploadQueue'
import { useProviderConfig, useSaveProviderConfig } from '../hooks/useProviderConfig'
import { CODING_AGENTS } from '../types/providers'
import { ArrowTopRightOnSquareIcon, DocumentTextIcon } from '@heroicons/react/24/outline'
import ProviderIcon from './icons/ProviderIcon'

function StatusView() {
  const { user } = useAuth()
  const { data: queueStatus } = useUploadQueueStatus()
  const { data: claudeCodeConfig } = useProviderConfig('claude-code')
  const { data: opencodeConfig } = useProviderConfig('opencode')
  const { data: codexConfig } = useProviderConfig('codex')
  const saveConfig = useSaveProviderConfig()

  const openMainWindow = async () => {
    try {
      await invoke('open_main_window')
    } catch (err) {
      console.error('Failed to open main window:', err)
    }
  }

  const toggleProvider = async (providerId: string, currentConfig: any) => {
    const newEnabled = !currentConfig?.enabled
    await saveConfig.mutateAsync({
      providerId,
      config: {
        ...currentConfig,
        enabled: newEnabled,
      },
    })
  }

  const totalQueueItems = (queueStatus?.pending || 0) + (queueStatus?.processing || 0) + (queueStatus?.failed || 0)

  const openMainWindowToProviderLogs = async (providerId: string) => {
    try {
      await invoke('open_main_window', { route: `/provider/${providerId}?showLogs=true` })
    } catch (err) {
      console.error('Failed to open main window:', err)
    }
  }

  const openProviderDetail = async (providerId: string) => {
    try {
      await invoke('open_main_window', { route: `/provider/${providerId}` })
    } catch (err) {
      console.error('Failed to open main window:', err)
    }
  }

  return (
    <div className="h-screen flex flex-col bg-base-100" data-theme="guideai">
      {/* Header with user info */}
      <div className="p-3 border-b border-base-300">
        <div className="flex items-center gap-2 mb-2">
          <div className="w-8 h-8 rounded-full bg-primary/20 flex items-center justify-center text-primary font-semibold text-sm">
            {user?.name?.[0]?.toUpperCase() || 'U'}
          </div>
          <div className="flex-1 min-w-0">
            <div className="text-sm font-medium truncate">{user?.name || 'User'}</div>
          </div>
        </div>
      </div>

      {/* Upload Queue Status */}
      <div className="p-3 border-b border-base-300">
        <div className="text-xs text-base-content/70 uppercase mb-2">Upload Queue</div>
        <div className="grid grid-cols-3 gap-2">
          <div className="text-center">
            <div className="text-lg font-bold">{queueStatus?.pending || 0}</div>
            <div className="text-xs text-base-content/60">Pending</div>
          </div>
          <div className="text-center">
            <div className="text-lg font-bold">{queueStatus?.processing || 0}</div>
            <div className="text-xs text-base-content/60">Processing</div>
          </div>
          <div className="text-center">
            <div className={`text-lg font-bold ${(queueStatus?.failed || 0) > 0 ? 'text-error' : ''}`}>
              {queueStatus?.failed || 0}
            </div>
            <div className="text-xs text-base-content/60">Failed</div>
          </div>
        </div>
        {totalQueueItems === 0 && (
          <div className="text-center text-xs text-base-content/60 mt-2">
            All uploads complete
          </div>
        )}
      </div>

      {/* Watcher Status */}
      <div className="p-3 border-b border-base-300">
        <div className="text-xs text-base-content/70 uppercase mb-2">Watchers</div>
        <div className="space-y-2">
          {CODING_AGENTS.map((agent) => {
            const config = agent.id === 'claude-code' ? claudeCodeConfig
              : agent.id === 'opencode' ? opencodeConfig
              : codexConfig
            const isEnabled = config?.enabled === true

            return (
              <div key={agent.id} className="flex items-center justify-between">
                <button
                  onClick={() => openProviderDetail(agent.id)}
                  className="flex items-center gap-2 hover:opacity-70 transition-opacity"
                >
                  <div className="flex-shrink-0 w-5 h-5 rounded overflow-hidden relative">
                    <ProviderIcon providerId={agent.id} size={20} />
                    <div className={`absolute bottom-0 right-0 w-2 h-2 rounded-full border border-base-100 ${
                      isEnabled ? 'bg-success' : 'bg-base-content/30'
                    }`} />
                  </div>
                  <span className="text-sm">{agent.name}</span>
                </button>
                <div className="flex items-center gap-1">
                  <button
                    onClick={() => openMainWindowToProviderLogs(agent.id)}
                    className="btn btn-xs btn-ghost gap-1"
                    title="Open logs in full window"
                  >
                    <DocumentTextIcon className="w-3 h-3" />
                  </button>
                  <button
                    onClick={() => toggleProvider(agent.id, config)}
                    disabled={saveConfig.isPending}
                    className={`btn btn-xs ${
                      isEnabled ? 'btn-error' : 'btn-success'
                    }`}
                  >
                    {isEnabled ? 'Disable' : 'Enable'}
                  </button>
                </div>
              </div>
            )
          })}
        </div>
      </div>

      {/* Actions */}
      <div className="flex-1 p-3 flex flex-col gap-2">
        <button
          onClick={openMainWindow}
          className="btn btn-primary btn-sm gap-2"
        >
          <ArrowTopRightOnSquareIcon className="w-4 h-4" />
          Open Full Window
        </button>
      </div>

      {/* Footer */}
      <div className="p-3 border-t border-base-300 text-center">
        <div className="text-xs text-base-content/50">GuideAI Desktop</div>
      </div>
    </div>
  )
}

export default StatusView