import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { invoke } from '@tauri-apps/api/core'
import { useCallback, useState } from 'react'

export interface SessionInfo {
  provider: string
  project_name: string
  session_id: string
  file_path: string
  file_name: string
  file_size: number
  session_start_time?: string
}

export interface SessionSyncProgress {
  is_scanning: boolean
  is_syncing: boolean
  total_sessions: number
  synced_sessions: number
  current_provider: string
  current_project: string
  sessions_found: SessionInfo[]
  errors: string[]
  is_complete: boolean
  initial_queue_size?: number
  is_uploading: boolean
}

export function useSessionSync(providerId: string) {
  const [error, setError] = useState<string | null>(null)
  const queryClient = useQueryClient()

  // Query to get sync progress
  const { data: progress, refetch: refetchProgress } = useQuery({
    queryKey: ['session-sync-progress', providerId],
    queryFn: () => invoke<SessionSyncProgress>('get_session_sync_progress', { providerId }),
    refetchInterval: 1000, // Poll every second during active operations
    enabled: true,
  })

  // Scan sessions mutation
  const scanMutation = useMutation({
    mutationFn: () => invoke<SessionInfo[]>('scan_historical_sessions', { providerId }),
    onSuccess: () => {
      // Refresh progress to show scanned sessions
      refetchProgress()
      setError(null)
    },
    onError: err => {
      setError(err instanceof Error ? err.message : 'Failed to scan sessions')
    },
  })

  // Sync sessions mutation
  const syncMutation = useMutation({
    mutationFn: () => invoke<void>('sync_historical_sessions', { providerId }),
    onSuccess: () => {
      // Continue polling to track progress
      refetchProgress()
      setError(null)

      // Also invalidate upload queue status to show new uploads
      queryClient.invalidateQueries({ queryKey: ['upload-queue', 'status'] })
    },
    onError: err => {
      setError(err instanceof Error ? err.message : 'Failed to sync sessions')
    },
  })

  // Reset progress mutation
  const resetMutation = useMutation({
    mutationFn: () => invoke<void>('reset_session_sync_progress', { providerId }),
    onSuccess: () => {
      refetchProgress()
      setError(null)
    },
    onError: err => {
      setError(err instanceof Error ? err.message : 'Failed to reset progress')
    },
  })

  const scanSessions = useCallback(async () => {
    setError(null)
    try {
      await scanMutation.mutateAsync()
    } catch (_err) {
      // Error handling is done in mutation callbacks
    }
  }, [scanMutation])

  const syncSessions = useCallback(async () => {
    setError(null)
    try {
      await syncMutation.mutateAsync()
    } catch (_err) {
      // Error handling is done in mutation callbacks
    }
  }, [syncMutation])

  const resetProgress = useCallback(async () => {
    setError(null)
    try {
      await resetMutation.mutateAsync()
    } catch (_err) {
      // Error handling is done in mutation callbacks
    }
  }, [resetMutation])

  return {
    // Data
    progress,
    error,

    // Status flags
    isScanning: scanMutation.isPending || (progress?.is_scanning ?? false),
    isSyncing: syncMutation.isPending || (progress?.is_syncing ?? false),
    isResetting: resetMutation.isPending,

    // Actions
    scanSessions,
    syncSessions,
    resetProgress,

    // Utilities
    refetchProgress,
  }
}

export default useSessionSync
