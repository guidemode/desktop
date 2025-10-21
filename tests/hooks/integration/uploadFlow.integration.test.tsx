import type { ReactNode } from 'react'
import { renderHook, act, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useSessionSync } from '../../../src/../src/hooks/useSessionSync'
import { useUploadQueueStatus } from '../../../src/../src/hooks/useUploadQueue'

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

const withProvider =
  (client: QueryClient) =>
  ({ children }: { children: ReactNode }) =>
    <QueryClientProvider client={client}>{children}</QueryClientProvider>

describe('Upload flow integration', () => {
  beforeEach(() => {
    invoke.mockReset()
  })

  it('triggers upload queue refresh after syncing sessions', async () => {
    const initialProgress = {
      is_scanning: false,
      is_syncing: false,
      total_sessions: 3,
      synced_sessions: 1,
      current_provider: 'claude-code',
      current_project: 'proj-a',
      sessions_found: [],
      errors: [],
      is_complete: false,
      is_uploading: false,
    }
    const updatedProgress = {
      ...initialProgress,
      synced_sessions: 3,
      is_complete: true,
      is_uploading: true,
    }
    const initialQueue = { pending: 2, processing: 1, failed: 0, recent_uploads: [] }
    const refreshedQueue = {
      pending: 0,
      processing: 0,
      failed: 0,
      recent_uploads: [{ id: 'upload-1' }],
    }

    let progressCall = 0
    let queueCall = 0

    invoke.mockImplementation(async (command: string, args: unknown) => {
      switch (command) {
        case 'get_session_sync_progress': {
          progressCall += 1
          return progressCall === 1 ? initialProgress : updatedProgress
        }
        case 'sync_historical_sessions': {
          expect(args).toEqual({ providerId: 'claude-code' })
          return undefined
        }
        case 'get_upload_queue_status': {
          queueCall += 1
          return queueCall === 1 ? initialQueue : refreshedQueue
        }
        default:
          throw new Error(`Unexpected command: ${command}`)
      }
    })

    const client = createQueryClient()
    try {
      const { result } = renderHook(
        () => ({
          sync: useSessionSync('claude-code'),
          queue: useUploadQueueStatus(),
        }),
        {
          wrapper: withProvider(client),
        }
      )

      await waitFor(() => expect(result.current.sync.progress).toEqual(initialProgress))
      await waitFor(() => expect(result.current.queue.data?.pending).toBe(2))

      await act(async () => {
        await result.current.sync.syncSessions()
      })

      expect(invoke).toHaveBeenCalledWith('sync_historical_sessions', { providerId: 'claude-code' })

      await waitFor(() => expect(progressCall).toBeGreaterThanOrEqual(2))
      await waitFor(() => expect(queueCall).toBeGreaterThanOrEqual(2))
      await waitFor(() => expect(result.current.sync.progress).toEqual(updatedProgress))
      await waitFor(() => expect(result.current.queue.data?.pending).toBe(0))
    } finally {
      client.clear()
    }
  })
})

