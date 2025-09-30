import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { invoke } from '@tauri-apps/api/tauri'

export interface OpenCodeWatcherStatus {
  is_running: boolean
  pending_uploads: number
  processing_uploads: number
  failed_uploads: number
}

export function useOpenCodeWatcherStatus() {
  return useQuery({
    queryKey: ['opencode-watcher-status'],
    queryFn: () => invoke<OpenCodeWatcherStatus>('get_opencode_watcher_status'),
    refetchInterval: 2000, // Poll every 2 seconds
  })
}

export function useStartOpenCodeWatcher() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (projects: string[]) =>
      invoke<void>('start_opencode_watcher', { projects }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['opencode-watcher-status'] })
    },
  })
}

export function useStopOpenCodeWatcher() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: () => invoke<void>('stop_opencode_watcher'),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['opencode-watcher-status'] })
    },
  })
}