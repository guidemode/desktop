import { useState, useEffect } from 'react'
import { CodingAgent, ProviderConfig } from '../../types/providers'
import { useProviderConfig, useSaveProviderConfig, useScanProjects } from '../../hooks/useProviderConfig'
import {
  useClaudeWatcherStatus,
  useStartClaudeWatcher,
  useStopClaudeWatcher
} from '../../hooks/useClaudeWatcher'
import {
  useOpenCodeWatcherStatus,
  useStartOpenCodeWatcher,
  useStopOpenCodeWatcher
} from '../../hooks/useOpenCodeWatcher'
import {
  useCodexWatcherStatus,
  useStartCodexWatcher,
  useStopCodexWatcher
} from '../../hooks/useCodexWatcher'
import { formatDistanceToNow } from 'date-fns'
import SessionSync from './SessionSync'
import ProviderIcon from '../icons/ProviderIcon'

interface AgentConfigProps {
  agent: CodingAgent
  headerActions?: React.ReactNode
}

function AgentConfig({ agent, headerActions }: AgentConfigProps) {
  const { data: config, isLoading: configLoading } = useProviderConfig(agent.id)
  const { mutate: saveConfig, isPending: saving } = useSaveProviderConfig()

  // Watcher hooks - conditional based on provider
  const { data: claudeWatcherStatus } = useClaudeWatcherStatus()
  const { mutate: startClaudeWatcher, isPending: startingClaudeWatcher } = useStartClaudeWatcher()
  const { mutate: stopClaudeWatcher, isPending: stoppingClaudeWatcher } = useStopClaudeWatcher()

  const { data: opencodeWatcherStatus } = useOpenCodeWatcherStatus()
  const { mutate: startOpenCodeWatcher, isPending: startingOpenCodeWatcher } = useStartOpenCodeWatcher()
  const { mutate: stopOpenCodeWatcher, isPending: stoppingOpenCodeWatcher } = useStopOpenCodeWatcher()

  const { data: codexWatcherStatus } = useCodexWatcherStatus()
  const { mutate: startCodexWatcher, isPending: startingCodexWatcher } = useStartCodexWatcher()
  const { mutate: stopCodexWatcher, isPending: stoppingCodexWatcher } = useStopCodexWatcher()

  // Get the appropriate status and functions for the current provider
  const watcherStatus = agent.id === 'claude-code' ? claudeWatcherStatus :
                        agent.id === 'opencode' ? opencodeWatcherStatus :
                        agent.id === 'codex' ? codexWatcherStatus : undefined
  const startWatcher = agent.id === 'claude-code' ? startClaudeWatcher :
                       agent.id === 'opencode' ? startOpenCodeWatcher :
                       agent.id === 'codex' ? startCodexWatcher : undefined
  const stopWatcher = agent.id === 'claude-code' ? stopClaudeWatcher :
                      agent.id === 'opencode' ? stopOpenCodeWatcher :
                      agent.id === 'codex' ? stopCodexWatcher : undefined
  const startingWatcher = agent.id === 'claude-code' ? startingClaudeWatcher :
                          agent.id === 'opencode' ? startingOpenCodeWatcher :
                          agent.id === 'codex' ? startingCodexWatcher : false
  const stoppingWatcher = agent.id === 'claude-code' ? stoppingClaudeWatcher :
                          agent.id === 'opencode' ? stoppingOpenCodeWatcher :
                          agent.id === 'codex' ? stoppingCodexWatcher : false

  const [localConfig, setLocalConfig] = useState<ProviderConfig>({
    enabled: false,
    homeDirectory: agent.defaultHomeDirectory,
    projectSelection: 'ALL',
    selectedProjects: [],
    lastScanned: null
  })

  const effectiveHomeDirectory = localConfig.homeDirectory || agent.defaultHomeDirectory

  const { data: projects = [], isLoading: projectsLoading } = useScanProjects(
    agent.id,
    localConfig.enabled ? effectiveHomeDirectory : ''
  )

  useEffect(() => {
    if (config) {
      setLocalConfig({
        ...config,
        homeDirectory: config.homeDirectory || agent.defaultHomeDirectory,
      })
    }
  }, [config])


  const handleEnabledChange = (enabled: boolean) => {
    const newConfig = { ...localConfig, enabled }

    // Non-destructive disable: only stop watching, preserve all other config
    if (!enabled) {
      // Stop watching when disabling
      if (watcherStatus?.is_running && stopWatcher) {
        stopWatcher()
      }
    } else if (!newConfig.homeDirectory) {
      newConfig.homeDirectory = agent.defaultHomeDirectory
    }
    setLocalConfig(newConfig)

    // Always save config to preserve settings when disabled
    saveConfig({ providerId: agent.id, config: newConfig })

    if (enabled) {
      // Auto-start watching when enabling (if we can)
      if (canStartWatcher && !watcherStatus?.is_running && startWatcher) {
        const projectsToWatch = newConfig.projectSelection === 'ALL'
          ? projects.map(p => p.name)
          : newConfig.selectedProjects
        if (projectsToWatch.length > 0) {
          startWatcher(projectsToWatch)
        }
      }
    }
  }

  const handleConfigChange = (updates: Partial<ProviderConfig>) => {
    const newConfig = {
      ...localConfig,
      ...updates,
    }

    if (!newConfig.homeDirectory) {
      newConfig.homeDirectory = agent.defaultHomeDirectory
    }

    setLocalConfig(newConfig)
    if (newConfig.enabled) {
      saveConfig({ providerId: agent.id, config: newConfig })

      // Auto-start watching if not running and we can
      if (!watcherStatus?.is_running && startWatcher) {
        const projectsToWatch = newConfig.projectSelection === 'ALL'
          ? projects.map(p => p.name)
          : newConfig.selectedProjects
        if (projectsToWatch.length > 0) {
          startWatcher(projectsToWatch)
        }
      }
    }
  }

  const handleProjectToggle = (projectName: string) => {
    const isSelected = localConfig.selectedProjects.includes(projectName)
    const selectedProjects = isSelected
      ? localConfig.selectedProjects.filter(p => p !== projectName)
      : [...localConfig.selectedProjects, projectName]

    handleConfigChange({ selectedProjects })
  }

  // Watcher control functions
  const handleStartWatcher = () => {
    if (!startWatcher) return

    const projectsToWatch = localConfig.projectSelection === 'ALL'
      ? projects.map(p => p.name)
      : localConfig.selectedProjects

    startWatcher(projectsToWatch)
  }

  const handleStopWatcher = () => {
    if (!stopWatcher) return
    stopWatcher()
  }

  const isConfigLoading = configLoading || saving
  const isWatcherBusy = startingWatcher || stoppingWatcher
  const canStartWatcher = localConfig.enabled && startWatcher !== undefined &&
    (localConfig.projectSelection === 'ALL' || localConfig.selectedProjects.length > 0)

  // Note: Autostart has been moved to Rust code at application startup
  // This prevents the restart loop issue and is more reliable
  // The watcher will start automatically when the app starts if the provider is enabled

  return (
    <div className="card bg-base-100 shadow-sm border border-base-300">
      <div className="card-body">
        {/* Header */}
        <div className="flex items-start justify-between">
          <div className="flex items-center gap-2">
            <div className={`avatar placeholder`}>
              <div className={`bg-base-200 rounded-lg w-8 h-8 flex items-center justify-center p-1`}>
                <ProviderIcon providerId={agent.id} size={24} />
              </div>
            </div>
            <div>
              <h3 className="text-base font-semibold">{agent.name}</h3>
              <p className="text-sm text-base-content/70">{agent.description}</p>
            </div>
          </div>

          <div className="flex items-center gap-2">
            {headerActions}
            <div className="form-control">
              <label className="label cursor-pointer gap-2">
                <span className="label-text">Enabled</span>
                <input
                  type="checkbox"
                  className="toggle toggle-primary"
                  checked={localConfig.enabled}
                  onChange={(e) => handleEnabledChange(e.target.checked)}
                  disabled={isConfigLoading}
                />
              </label>
            </div>
          </div>
        </div>

        {/* Configuration */}
        {localConfig.enabled && (
          <div className="space-y-3 mt-3">
            {/* Home Directory */}
            <div className="form-control">
              <label className="label">
                <span className="label-text">Home Directory</span>
              </label>
              <input
                type="text"
                className="input input-bordered"
                value={localConfig.homeDirectory}
                onChange={(e) => handleConfigChange({ homeDirectory: e.target.value })}
                placeholder={agent.defaultHomeDirectory}
                disabled={isConfigLoading}
              />
            </div>

            {/* Project Selection */}
            <div className="form-control">
              <label className="label">
                <span className="label-text">Projects</span>
              </label>
              <div className="space-y-1">
                <label className="cursor-pointer label justify-start gap-2">
                  <input
                    type="radio"
                    name={`projects-${agent.id}`}
                    className="radio radio-primary"
                    checked={localConfig.projectSelection === 'ALL'}
                    onChange={() => handleConfigChange({ projectSelection: 'ALL' })}
                    disabled={isConfigLoading}
                  />
                  <span className="label-text">Monitor all projects</span>
                </label>
                <label className="cursor-pointer label justify-start gap-2">
                  <input
                    type="radio"
                    name={`projects-${agent.id}`}
                    className="radio radio-primary"
                    checked={localConfig.projectSelection === 'SELECTED'}
                    onChange={() => handleConfigChange({ projectSelection: 'SELECTED' })}
                    disabled={isConfigLoading}
                  />
                  <span className="label-text">Monitor selected projects only</span>
                </label>
              </div>
            </div>

            {/* Project List */}
            {localConfig.projectSelection === 'SELECTED' && (
              <div className="form-control">
                <label className="label">
                  <span className="label-text">Available Projects</span>
                  {projectsLoading && <span className="loading loading-spinner loading-xs"></span>}
                </label>
                <div className="max-h-48 overflow-y-auto border border-base-300 rounded-lg p-2">
                  {projects.length === 0 ? (
                    <div className="text-sm text-base-content/70 text-center py-4">
                      {projectsLoading ? 'Scanning projects...' : 'No projects found'}
                    </div>
                  ) : (
                    <div className="grid grid-cols-1 sm:grid-cols-2 gap-1 text-xs">
                      {projects.map((project) => {
                        const modifiedDate = new Date(project.lastModified)
                        const modifiedLabel = Number.isNaN(modifiedDate.getTime())
                          ? 'Unknown'
                          : formatDistanceToNow(modifiedDate, { addSuffix: true })

                        const isSelected = localConfig.selectedProjects.includes(project.name)

                        return (
                          <label
                            key={project.name}
                            className={`cursor-pointer flex items-center gap-2 px-2 py-1 rounded hover:bg-base-200 ${
                              isSelected ? 'bg-base-200' : ''
                            }`}
                          >
                            <input
                              type="checkbox"
                              className="checkbox checkbox-primary checkbox-xs"
                              checked={isSelected}
                              onChange={() => handleProjectToggle(project.name)}
                              disabled={isConfigLoading}
                            />
                            <span className="truncate flex-1">{project.name}</span>
                            <span className="text-[11px] text-base-content/60 shrink-0">
                              {modifiedLabel}
                            </span>
                          </label>
                        )
                      })}
                    </div>
                  )}
                </div>
              </div>
            )}

            {/* File Watching Controls (for all providers) */}
            {startWatcher !== undefined && (
              <div className="form-control">
                <label className="label">
                  <span className="label-text">File Watching</span>
                </label>
                <div className="bg-base-200 rounded-lg p-3 space-y-2">
                  {/* Watcher Status */}
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      <div className={`w-2 h-2 rounded-full ${
                        watcherStatus?.is_running ? 'bg-success' : 'bg-base-content/30'
                      }`} />
                      <span className="text-sm">
                        {watcherStatus?.is_running ? 'Watching files' : 'Not watching'}
                      </span>
                    </div>

                    {watcherStatus?.is_running ? (
                      <button
                        className="btn btn-sm btn-outline btn-warning"
                        onClick={handleStopWatcher}
                        disabled={isWatcherBusy}
                      >
                        {stoppingWatcher ? (
                          <>
                            <span className="loading loading-spinner loading-xs"></span>
                            Pausing...
                          </>
                        ) : (
                          'Pause Watching'
                        )}
                      </button>
                    ) : (
                      <button
                        className="btn btn-sm btn-primary"
                        onClick={handleStartWatcher}
                        disabled={!canStartWatcher || isWatcherBusy}
                      >
                        {startingWatcher ? (
                          <>
                            <span className="loading loading-spinner loading-xs"></span>
                            Resuming...
                          </>
                        ) : (
                          'Resume Watching'
                        )}
                      </button>
                    )}
                  </div>


                  {/* Help text */}
                  {!canStartWatcher && !watcherStatus?.is_running && (
                    <div className="text-xs text-base-content/60">
                      {!localConfig.enabled
                        ? 'Enable the provider to start file watching'
                        : localConfig.projectSelection === 'SELECTED' && localConfig.selectedProjects.length === 0
                        ? 'Select at least one project to watch'
                        : 'Configure your projects above to start watching'}
                    </div>
                  )}
                </div>
              </div>
            )}

            {/* Status */}
            {localConfig.lastScanned && (
              <div className="text-xs text-base-content/70">
                Last scanned: {formatDistanceToNow(new Date(localConfig.lastScanned))} ago
              </div>
            )}
          </div>
        )}

        {/* Historical Session Sync */}
        {localConfig.enabled && (
          <div className="mt-4">
            <SessionSync agent={agent} />
          </div>
        )}

        {/* Loading indicator */}
        {isConfigLoading && (
          <div className="flex items-center justify-center py-4">
            <span className="loading loading-spinner loading-sm"></span>
          </div>
        )}
      </div>
    </div>
  )
}

export default AgentConfig
