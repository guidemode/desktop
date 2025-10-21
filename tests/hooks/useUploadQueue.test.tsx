import type { ReactNode } from 'react'
import { renderHook, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import {
  useClearAllFailed,
  useRemoveQueueItem,
  useRetryAllFailed,
  useRetryUpload,
  useUploadQueueItems,
  useUploadQueueStatus,
} from '../../src/hooks/useUploadQueue'

const invoke = vi.fn()

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invoke(...args),
}))

const createQueryClient = () =>
  new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  })

const withProvider = (client: QueryClient) => {
  return ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  )
}

describe('upload queue hooks', () => {
  beforeEach(() => {
    invoke.mockReset()
  })

  it('useUploadQueueItems fetches queue items', async () => {
    const queue = { pending: [], failed: [] }
    invoke.mockResolvedValue(queue)

    const client = createQueryClient()
    try {
      const { result } = renderHook(() => useUploadQueueItems(), {
        wrapper: withProvider(client),
      })

      await waitFor(() => expect(result.current.data).toEqual(queue))
      expect(invoke).toHaveBeenCalledWith('get_upload_queue_items')
    } finally {
      client.clear()
    }
  })

  it('useUploadQueueStatus fetches queue status', async () => {
    const status = { pending: 1, processing: 0, failed: 2, recent_uploads: [] }
    invoke.mockResolvedValue(status)

    const client = createQueryClient()
    try {
      const { result } = renderHook(() => useUploadQueueStatus(), {
        wrapper: withProvider(client),
      })

      await waitFor(() => expect(result.current.data).toEqual(status))
      expect(invoke).toHaveBeenCalledWith('get_upload_queue_status')
    } finally {
      client.clear()
    }
  })

  it('mutations call backend and invalidate queue queries', async () => {
    invoke.mockResolvedValue(undefined)

    const client = createQueryClient()
    const invalidateSpy = vi.spyOn(client, 'invalidateQueries')

    try {
      const retryUpload = renderHook(() => useRetryUpload(), { wrapper: withProvider(client) })
      await retryUpload.result.current.mutateAsync('item-1')
      expect(invoke).toHaveBeenCalledWith('retry_single_upload', { itemId: 'item-1' })

      const removeItem = renderHook(() => useRemoveQueueItem(), {
        wrapper: withProvider(client),
      })
      await removeItem.result.current.mutateAsync('item-2')
      expect(invoke).toHaveBeenCalledWith('remove_queue_item', { itemId: 'item-2' })

      const retryAll = renderHook(() => useRetryAllFailed(), { wrapper: withProvider(client) })
      await retryAll.result.current.mutateAsync()
      expect(invoke).toHaveBeenCalledWith('retry_failed_uploads')

      const clearAll = renderHook(() => useClearAllFailed(), { wrapper: withProvider(client) })
      await clearAll.result.current.mutateAsync()
      expect(invoke).toHaveBeenCalledWith('clear_failed_uploads')

      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['upload-queue'] })
    } finally {
      invalidateSpy.mockRestore()
      client.clear()
    }
  })
})
