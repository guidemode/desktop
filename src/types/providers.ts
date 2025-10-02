export type ProjectSelection = 'ALL' | 'SELECTED'

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
}

export interface CodingAgent {
  id: string
  name: string
  description: string
  defaultHomeDirectory: string
  icon: string
  color: string
}

// Platform-specific default paths
const PLATFORM_DEFAULTS: Record<string, Record<string, string>> = {
  'claude-code': {
    win32: '~/.claude', // Works with WSL/Git Bash on Windows
    darwin: '~/.claude',
    linux: '~/.claude'
  },
  'opencode': {
    win32: '%LOCALAPPDATA%/opencode', // Windows: C:\Users\<user>\AppData\Local\opencode
    darwin: '~/.local/share/opencode',
    linux: '~/.local/share/opencode'
  },
  'codex': {
    win32: '~/.codex', // Works with WSL/Git Bash on Windows
    darwin: '~/.codex',
    linux: '~/.codex'
  }
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

  return PLATFORM_DEFAULTS[agentId]?.[os] || PLATFORM_DEFAULTS[agentId]?.['linux'] || '~'
}

export const CODING_AGENTS: CodingAgent[] = [
  {
    id: 'claude-code',
    name: 'Claude Code',
    description: 'AI assistant for coding with Claude',
    defaultHomeDirectory: getPlatformDefault('claude-code'),
    icon: 'M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5',
    color: 'from-orange-500 to-red-500'
  },
  {
    id: 'opencode',
    name: 'OpenCode',
    description: 'Open source coding assistant',
    defaultHomeDirectory: getPlatformDefault('opencode'),
    icon: 'M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-2 15l-5-5 5-5v3h4v4h-4v3z',
    color: 'from-green-600 to-blue-600'
  },
  {
    id: 'codex',
    name: 'OpenAI Codex',
    description: 'OpenAI Codex integration',
    defaultHomeDirectory: getPlatformDefault('codex'),
    icon: 'M12 2l3.09 6.26L22 9l-5.91 3.74L18 22l-6-4.74L6 22l1.91-9.26L2 9l6.91-.74L12 2z',
    color: 'from-emerald-500 to-teal-600'
  }
]
