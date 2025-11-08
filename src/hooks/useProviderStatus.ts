import { useMemo } from 'react'
import type { ProviderConfig, ProviderStatus } from '../types/providers'
import { CODING_AGENTS } from '../types/providers'
import { useClaudeWatcherStatus } from './useClaudeWatcher'
import { useCodexWatcherStatus } from './useCodexWatcher'
import { useCopilotWatcherStatus } from './useCopilotWatcher'
import { useCursorWatcherStatus } from './useCursorWatcher'
import { useDirectoryExists } from './useDirectoryExists'
import { useGeminiWatcherStatus } from './useGeminiWatcher'
import { useOpenCodeWatcherStatus } from './useOpenCodeWatcher'
import { useProviderConfig } from './useProviderConfig'

interface UseProviderStatusResult {
  status: ProviderStatus
  isLoading: boolean
  error: Error | null
  refetch: () => Promise<void>
}

/**
 * Calculate provider operational status from config data and directory existence
 * Note: Status reflects the provider's CONFIGURED mode, not whether the watcher is currently running.
 * The watcher can be paused/stopped temporarily without changing the provider's operational mode.
 */
function calculateProviderStatus(
  config: ProviderConfig | undefined,
  directoryExists: boolean | undefined
): ProviderStatus {
  // Check if directory exists first - this takes priority
  if (directoryExists === false) {
    return 'not-installed'
  }

  // No config or explicitly disabled
  if (!config || !config.enabled) {
    return 'disabled'
  }

  // Provider is enabled - determine mode from syncMode
  switch (config.syncMode) {
    case 'Nothing':
      return 'local-only'
    case 'Metrics Only':
      return 'metrics-only'
    case 'Transcript and Metrics':
      return 'full-sync'
    default:
      console.warn(`Invalid syncMode: ${config.syncMode}. Defaulting to disabled.`)
      return 'disabled'
  }
}

/**
 * Hook to compute provider status from watcher and config data
 */
export function useProviderStatus(providerId: string): UseProviderStatusResult {
  // Get watcher status based on provider ID
  const claudeWatcher = useClaudeWatcherStatus()
  const copilotWatcher = useCopilotWatcherStatus()
  const cursorWatcher = useCursorWatcherStatus()
  const opencodeWatcher = useOpenCodeWatcherStatus()
  const codexWatcher = useCodexWatcherStatus()
  const geminiWatcher = useGeminiWatcherStatus()

  // Select the appropriate watcher based on provider ID
  const watcherQuery = useMemo(() => {
    switch (providerId) {
      case 'claude-code':
        return claudeWatcher
      case 'github-copilot':
        return copilotWatcher
      case 'cursor':
        return cursorWatcher
      case 'opencode':
        return opencodeWatcher
      case 'codex':
        return codexWatcher
      case 'gemini-code':
        return geminiWatcher
      default:
        return {
          data: undefined,
          isLoading: false,
          error: new Error(`Unknown provider: ${providerId}`),
          refetch: async () => {},
        }
    }
  }, [
    providerId,
    claudeWatcher,
    copilotWatcher,
    cursorWatcher,
    opencodeWatcher,
    codexWatcher,
    geminiWatcher,
  ])

  // Get provider config from React Query (single source of truth)
  const {
    data: config,
    isLoading: configLoading,
    error: configError,
    refetch: refetchConfig,
  } = useProviderConfig(providerId)

  // Get agent to access default home directory
  const agent = useMemo(() => CODING_AGENTS.find(a => a.id === providerId), [providerId])
  const effectiveHomeDirectory = config?.homeDirectory || agent?.defaultHomeDirectory

  // Check if home directory exists
  const { data: directoryExists } = useDirectoryExists(effectiveHomeDirectory)

  // Compute status
  const status = useMemo(() => {
    return calculateProviderStatus(config, directoryExists)
  }, [config, directoryExists])

  // isLoading true only during initial fetch of either watcher or config
  const isLoading = (watcherQuery.isLoading && !watcherQuery.data) || (configLoading && !config)

  // Refetch function
  const refetch = async () => {
    await refetchConfig()
    await watcherQuery.refetch()
  }

  return {
    status,
    isLoading,
    error: (watcherQuery.error || configError) as Error | null,
    refetch,
  }
}
