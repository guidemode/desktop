import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { invoke } from '@tauri-apps/api/tauri'

export interface UploadItem {
  id: string
  provider: string
  project_name: string
  file_path: string
  file_name: string
  queued_at: string
  retry_count: number
  next_retry_at?: string
  last_error?: string
  file_hash?: number
  file_size: number
  session_id?: string
  content?: string
}

export interface QueueItems {
  pending: UploadItem[]
  failed: UploadItem[]
}

export interface UploadStatus {
  pending: number
  processing: number
  failed: number
  recent_uploads: UploadItem[]
}

export function useUploadQueueItems() {
  return useQuery<QueueItems>({
    queryKey: ['upload-queue', 'items'],
    queryFn: async () => {
      return await invoke('get_upload_queue_items')
    },
    refetchInterval: 3000, // Poll every 3 seconds
  })
}

export function useUploadQueueStatus() {
  return useQuery<UploadStatus>({
    queryKey: ['upload-queue', 'status'],
    queryFn: async () => {
      return await invoke('get_upload_queue_status')
    },
    refetchInterval: 3000, // Poll every 3 seconds
  })
}

export function useRetryUpload() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: async (itemId: string) => {
      return await invoke('retry_single_upload', { itemId })
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['upload-queue'] })
    },
  })
}

export function useRemoveQueueItem() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: async (itemId: string) => {
      return await invoke('remove_queue_item', { itemId })
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['upload-queue'] })
    },
  })
}

export function useRetryAllFailed() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: async () => {
      return await invoke('retry_failed_uploads')
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['upload-queue'] })
    },
  })
}

export function useClearAllFailed() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: async () => {
      return await invoke('clear_failed_uploads')
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['upload-queue'] })
    },
  })
}