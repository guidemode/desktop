import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { invoke } from '@tauri-apps/api/tauri'

export interface CodexWatcherStatus {
  is_running: boolean
  pending_uploads: number
  processing_uploads: number
  failed_uploads: number
}

export function useCodexWatcherStatus() {
  return useQuery({
    queryKey: ['codex-watcher-status'],
    queryFn: () => invoke<CodexWatcherStatus>('get_codex_watcher_status'),
    refetchInterval: 2000, // Poll every 2 seconds
  })
}

export function useStartCodexWatcher() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (projects: string[]) =>
      invoke<void>('start_codex_watcher', { projects }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['codex-watcher-status'] })
    },
  })
}

export function useStopCodexWatcher() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: () => invoke<void>('stop_codex_watcher'),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['codex-watcher-status'] })
    },
  })
}