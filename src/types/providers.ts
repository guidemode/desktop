export type ProjectSelection = 'ALL' | 'SELECTED'

export type SyncMode = 'Nothing' | 'Metrics Only' | 'Transcript and Metrics'

export interface Project {
  name: string
  path: string
  lastModified: string
}

export interface ProviderConfig {
  enabled: boolean
  homeDirectory: string
  projectSelection: ProjectSelection
  selectedProjects: string[]
  lastScanned: string | null
  syncMode: SyncMode
}

export interface CodingAgent {
  id: string
  name: string
  description: string
  defaultHomeDirectory: string
  icon: string
  color: string
  setupInstructionsFile?: string
}

/**
 * Operational status of a provider, derived from configuration and watcher state.
 *
 * - 'not-installed': Provider directory does not exist (provider not installed)
 * - 'disabled': Provider not active (not configured, disabled, or watcher not running)
 * - 'local-only': Sessions processed locally without cloud sync (Privacy-first mode)
 * - 'metrics-only': Only metadata synced (duration, counts), no transcript data
 * - 'full-sync': Full session data synced to cloud for analytics
 */
export type ProviderStatus =
  | 'not-installed'
  | 'disabled'
  | 'local-only'
  | 'metrics-only'
  | 'full-sync'

/**
 * Combined provider metadata and real-time status for display.
 */
export interface ProviderInfo {
  /**
   * Unique provider identifier (e.g., 'claude-code')
   */
  id: string

  /**
   * Human-readable display name (e.g., 'Claude Code')
   */
  name: string

  /**
   * Current operational status
   */
  status: ProviderStatus

  /**
   * Whether status is being loaded (initial fetch only)
   */
  isLoading: boolean

  /**
   * Error during status calculation, if any
   */
  error: Error | null
}

/**
 * Type guard to validate ProviderStatus values
 */
export function isValidProviderStatus(value: unknown): value is ProviderStatus {
  return (
    typeof value === 'string' &&
    ['not-installed', 'disabled', 'local-only', 'metrics-only', 'full-sync'].includes(value)
  )
}

// Platform-specific default paths
const PLATFORM_DEFAULTS: Record<string, Record<string, string>> = {
  'claude-code': {
    win32: '~/.claude', // Works with WSL/Git Bash on Windows
    darwin: '~/.claude',
    linux: '~/.claude',
  },
  'github-copilot': {
    win32: '~/.copilot', // Works with WSL/Git Bash on Windows
    darwin: '~/.copilot',
    linux: '~/.copilot',
  },
  opencode: {
    win32: '%LOCALAPPDATA%/opencode', // Windows: C:\Users\<user>\AppData\Local\opencode
    darwin: '~/.local/share/opencode',
    linux: '~/.local/share/opencode',
  },
  codex: {
    win32: '~/.codex', // Works with WSL/Git Bash on Windows
    darwin: '~/.codex',
    linux: '~/.codex',
  },
  'gemini-code': {
    win32: '~/.gemini', // Works with WSL/Git Bash on Windows
    darwin: '~/.gemini',
    linux: '~/.gemini',
  },
}

// Get platform-specific default home directory
function getPlatformDefault(agentId: string): string {
  // Detect platform - use window.navigator.platform or userAgent as fallback
  const platform = navigator.platform.toLowerCase()
  let os = 'linux' // default

  if (platform.includes('win')) {
    os = 'win32'
  } else if (platform.includes('mac')) {
    os = 'darwin'
  }

  return PLATFORM_DEFAULTS[agentId]?.[os] || PLATFORM_DEFAULTS[agentId]?.linux || '~'
}

export const CODING_AGENTS: CodingAgent[] = [
  {
    id: 'claude-code',
    name: 'Claude Code',
    description: 'AI assistant for coding with Claude',
    defaultHomeDirectory: getPlatformDefault('claude-code'),
    icon: 'M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5',
    color: 'from-orange-500 to-red-500',
    setupInstructionsFile: 'claude-code.md',
  },
  {
    id: 'github-copilot',
    name: 'GitHub Copilot',
    description: 'GitHub Copilot CLI assistant',
    defaultHomeDirectory: getPlatformDefault('github-copilot'),
    icon: 'M8 0c4.42 0 8 3.58 8 8a8.013 8.013 0 0 1-5.45 7.59c-.4.08-.55-.17-.55-.38 0-.27.01-1.13.01-2.2 0-.75-.25-1.23-.54-1.48 1.78-.2 3.65-.88 3.65-3.95 0-.88-.31-1.59-.82-2.15.08-.2.36-1.02-.08-2.12 0 0-.67-.22-2.2.82-.64-.18-1.32-.27-2-.27-.68 0-1.36.09-2 .27-1.53-1.03-2.2-.82-2.2-.82-.44 1.1-.16 1.92-.08 2.12-.51.56-.82 1.28-.82 2.15 0 3.06 1.86 3.75 3.64 3.95-.23.2-.44.55-.51 1.07-.46.21-1.61.55-2.33-.66-.15-.24-.6-.83-1.23-.82-.67.01-.27.38.01.53.34.19.73.9.82 1.13.16.45.68 1.31 2.69.94 0 .67.01 1.3.01 1.49 0 .21-.15.45-.55.38A7.995 7.995 0 0 1 0 8c0-4.42 3.58-8 8-8Z',
    color: 'from-gray-700 to-gray-900',
    setupInstructionsFile: 'github-copilot.md',
  },
  {
    id: 'opencode',
    name: 'OpenCode',
    description: 'Open source coding assistant',
    defaultHomeDirectory: getPlatformDefault('opencode'),
    icon: 'M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-2 15l-5-5 5-5v3h4v4h-4v3z',
    color: 'from-green-600 to-blue-600',
    setupInstructionsFile: 'opencode.md',
  },
  {
    id: 'codex',
    name: 'OpenAI Codex',
    description: 'OpenAI Codex integration',
    defaultHomeDirectory: getPlatformDefault('codex'),
    icon: 'M12 2l3.09 6.26L22 9l-5.91 3.74L18 22l-6-4.74L6 22l1.91-9.26L2 9l6.91-.74L12 2z',
    color: 'from-emerald-500 to-teal-600',
    setupInstructionsFile: 'codex.md',
  },
  {
    id: 'gemini-code',
    name: 'Gemini Code',
    description: 'Google Gemini AI coding assistant',
    defaultHomeDirectory: getPlatformDefault('gemini-code'),
    icon: 'M12 2 L13.5 8.5 L20 10 L13.5 11.5 L12 18 L10.5 11.5 L4 10 L10.5 8.5 Z',
    color: 'from-blue-500 to-purple-600',
    setupInstructionsFile: 'gemini-code.md',
  },
]
