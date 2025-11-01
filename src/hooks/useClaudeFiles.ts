import { useQuery } from '@tanstack/react-query'
import { invoke } from '@tauri-apps/api/core'

export type ClaudeFileType = 'command' | 'skill' | 'config' | 'other'

export interface ClaudeMetadata {
  name?: string
  description?: string
}

export interface ClaudeFile {
  file_name: string
  file_path: string
  relative_path: string
  content: string
  size: number
  file_type: ClaudeFileType
  metadata?: ClaudeMetadata
}

/**
 * Hook to fetch Claude configuration files (.claude folder contents)
 * @param cwd - Current working directory (project root)
 * @param enabled - Whether to enable the query (default: true when cwd is provided)
 */
export function useClaudeFiles(cwd: string | null | undefined, enabled = true) {
  return useQuery<ClaudeFile[]>({
    queryKey: ['claude-files', cwd],
    queryFn: async () => {
      if (!cwd) {
        return []
      }

      try {
        const files = await invoke<ClaudeFile[]>('scan_claude_files', { cwd })
        return files || []
      } catch (error) {
        console.error('Failed to scan Claude files:', error)
        return []
      }
    },
    enabled: enabled && !!cwd,
    staleTime: 5 * 60 * 1000, // 5 minutes
    gcTime: 10 * 60 * 1000, // 10 minutes (formerly cacheTime)
  })
}
