import { act, renderHook, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useSessionSync } from '../../src/hooks/useSessionSync'
import type { SessionSyncProgress } from '../../src/hooks/useSessionSync'

const mockInvoke = vi.fn()

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}))

const createProgress = (overrides: Partial<SessionSyncProgress> = {}) => ({
  is_scanning: false,
  is_syncing: false,
  total_sessions: 0,
  synced_sessions: 0,
  current_provider: '',
  current_project: '',
  sessions_found: [],
  errors: [],
  is_complete: false,
  is_uploading: false,
  ...overrides,
})

const createQueryClient = () =>
  new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  })

const renderSessionSyncHook = (providerId: string, client: QueryClient) =>
  renderHook(() => useSessionSync(providerId), {
    wrapper: ({ children }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>,
  })

describe('useSessionSync', () => {
  beforeEach(() => {
    mockInvoke.mockReset()
  })

  it('fetches progress on mount', async () => {
    const progress = createProgress({ total_sessions: 3, synced_sessions: 1 })

    mockInvoke.mockImplementation(async (command, args) => {
      expect(command).toBe('get_session_sync_progress')
      expect(args).toEqual({ providerId: 'claude-code' })
      return progress
    })

    const client = createQueryClient()
    try {
      const { result } = renderSessionSyncHook('claude-code', client)

      await waitFor(() => expect(result.current.progress).toEqual(progress))
      expect(result.current.isScanning).toBe(false)
      expect(result.current.isSyncing).toBe(false)
      expect(result.current.error).toBeNull()
    } finally {
      client.clear()
    }
  })

  it('scanSessions invokes scan command and refetches progress', async () => {
    const initialProgress = createProgress({ total_sessions: 0 })
    const afterScanProgress = createProgress({ total_sessions: 5, is_scanning: true })
    let progressCall = 0

    mockInvoke.mockImplementation(async (command, args) => {
      if (command === 'get_session_sync_progress') {
        progressCall += 1
        return progressCall === 1 ? initialProgress : afterScanProgress
      }

      if (command === 'scan_historical_sessions') {
        expect(args).toEqual({ providerId: 'claude-code' })
        return [{ session_id: 's-1' }]
      }

      throw new Error(`Unexpected command ${command}`)
    })

    const client = createQueryClient()
    try {
      const { result } = renderSessionSyncHook('claude-code', client)

      await waitFor(() => expect(result.current.progress).toEqual(initialProgress))

      await act(async () => {
        await result.current.scanSessions()
      })

      expect(mockInvoke).toHaveBeenCalledWith('scan_historical_sessions', {
        providerId: 'claude-code',
      })
      expect(progressCall).toBeGreaterThanOrEqual(2)

      await waitFor(() =>
        expect(result.current.progress?.total_sessions).toBe(afterScanProgress.total_sessions)
      )
      expect(result.current.error).toBeNull()
    } finally {
      client.clear()
    }
  })

  it('syncSessions invokes sync command and invalidates upload queue', async () => {
    const initialProgress = createProgress({ total_sessions: 2, synced_sessions: 1 })
    const afterSyncProgress = createProgress({ total_sessions: 2, synced_sessions: 2 })
    let progressCall = 0

    mockInvoke.mockImplementation(async (command, args) => {
      if (command === 'get_session_sync_progress') {
        progressCall += 1
        return progressCall === 1 ? initialProgress : afterSyncProgress
      }

      if (command === 'sync_historical_sessions') {
        expect(args).toEqual({ providerId: 'claude-code' })
        return undefined
      }

      throw new Error(`Unexpected command ${command}`)
    })

    const client = createQueryClient()
    const invalidateSpy = vi.spyOn(client, 'invalidateQueries')

    try {
      const { result } = renderSessionSyncHook('claude-code', client)

      await waitFor(() => expect(result.current.progress).toEqual(initialProgress))

      await act(async () => {
        await result.current.syncSessions()
      })

      expect(mockInvoke).toHaveBeenCalledWith('sync_historical_sessions', {
        providerId: 'claude-code',
      })
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['upload-queue', 'status'] })

      await waitFor(() =>
        expect(result.current.progress?.synced_sessions).toBe(afterSyncProgress.synced_sessions)
      )
      expect(result.current.error).toBeNull()
    } finally {
      invalidateSpy.mockRestore()
      client.clear()
    }
  })

  it('resetProgress surfaces errors from tauri command', async () => {
    mockInvoke.mockImplementation(async (command) => {
      if (command === 'get_session_sync_progress') {
        return createProgress()
      }

      if (command === 'reset_session_sync_progress') {
        throw new Error('reset failed')
      }

      throw new Error(`Unexpected command ${command}`)
    })

    const client = createQueryClient()
    try {
      const { result } = renderSessionSyncHook('claude-code', client)

      await waitFor(() => expect(result.current.progress).toBeDefined())

      await act(async () => {
        await result.current.resetProgress()
      })

      await waitFor(() => expect(result.current.error).toBe('reset failed'))
    } finally {
      client.clear()
    }
  })
})
