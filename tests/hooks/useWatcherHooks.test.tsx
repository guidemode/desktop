import type { ReactNode } from 'react'
import { renderHook, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import {
  useClaudeWatcherStatus,
  useStartClaudeWatcher,
  useStopClaudeWatcher,
  useProviderLogs,
} from '../../src/hooks/useClaudeWatcher'
import {
  useCodexWatcherStatus,
  useStartCodexWatcher,
  useStopCodexWatcher,
} from '../../src/hooks/useCodexWatcher'
import {
  useCopilotWatcherStatus,
  useStartCopilotWatcher,
  useStopCopilotWatcher,
} from '../../src/hooks/useCopilotWatcher'
import {
  useOpenCodeWatcherStatus,
  useStartOpenCodeWatcher,
  useStopOpenCodeWatcher,
} from '../../src/hooks/useOpenCodeWatcher'

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

describe('Claude watcher hooks', () => {
  beforeEach(() => {
    invoke.mockReset()
  })

  it('fetches Claude watcher status', async () => {
    const status = { is_running: true, pending_uploads: 1, processing_uploads: 0, failed_uploads: 2 }
    invoke.mockResolvedValue(status)

    const client = createQueryClient()
    try {
      const { result } = renderHook(() => useClaudeWatcherStatus(), {
        wrapper: withProvider(client),
      })

      await waitFor(() => expect(result.current.data).toEqual(status))
      expect(invoke).toHaveBeenCalledWith('get_claude_watcher_status')
    } finally {
      client.clear()
    }
  })

  it('starts Claude watcher and invalidates status', async () => {
    invoke.mockResolvedValue(undefined)

    const client = createQueryClient()
    const invalidateSpy = vi.spyOn(client, 'invalidateQueries')

    try {
      const { result } = renderHook(() => useStartClaudeWatcher(), {
        wrapper: withProvider(client),
      })

      await result.current.mutateAsync(['project-a'])

      expect(invoke).toHaveBeenCalledWith('start_claude_watcher', { projects: ['project-a'] })
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['claude-watcher-status'] })
    } finally {
      invalidateSpy.mockRestore()
      client.clear()
    }
  })

  it('stops Claude watcher and invalidates status', async () => {
    invoke.mockResolvedValue(undefined)

    const client = createQueryClient()
    const invalidateSpy = vi.spyOn(client, 'invalidateQueries')

    try {
      const { result } = renderHook(() => useStopClaudeWatcher(), {
        wrapper: withProvider(client),
      })

      await result.current.mutateAsync()

      expect(invoke).toHaveBeenCalledWith('stop_claude_watcher')
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['claude-watcher-status'] })
    } finally {
      invalidateSpy.mockRestore()
      client.clear()
    }
  })

  it('fetches provider logs', async () => {
    const logs = [{ timestamp: '2024', level: 'INFO', provider: 'claude', message: 'started' }]
    invoke.mockResolvedValue(logs)

    const client = createQueryClient()
    try {
      const { result } = renderHook(() => useProviderLogs('claude', 50), {
        wrapper: withProvider(client),
      })

      await waitFor(() => expect(result.current.data).toEqual(logs))
      expect(invoke).toHaveBeenCalledWith('get_provider_logs', {
        provider: 'claude',
        maxLines: 50,
      })
    } finally {
      client.clear()
    }
  })
})

const testWatcherHooks = (
  label: string,
  statusHook: () => ReturnType<typeof useCodexWatcherStatus>,
  startHook: () => ReturnType<typeof useStartCodexWatcher>,
  stopHook: () => ReturnType<typeof useStopCodexWatcher>,
  statusCommand: string,
  startCommand: string,
  stopCommand: string
) => {
  describe(`${label} watcher hooks`, () => {
    beforeEach(() => {
      invoke.mockReset()
    })

    it(`fetches ${label} watcher status`, async () => {
      const status = { is_running: false, pending_uploads: 0, processing_uploads: 0, failed_uploads: 0 }
      invoke.mockResolvedValue(status)

      const client = createQueryClient()
      try {
        const { result } = renderHook(statusHook, {
          wrapper: withProvider(client),
        })

          await waitFor(() => expect(result.current.data).toEqual(status))
        expect(invoke).toHaveBeenCalledWith(statusCommand)
      } finally {
        client.clear()
      }
    })

    it(`starts ${label} watcher and invalidates status`, async () => {
      invoke.mockResolvedValue(undefined)

      const client = createQueryClient()
      const invalidateSpy = vi.spyOn(client, 'invalidateQueries')

      try {
        const { result } = renderHook(startHook, {
          wrapper: withProvider(client),
        })

        await result.current.mutateAsync(['proj-x'])

        expect(invoke).toHaveBeenCalledWith(startCommand, { projects: ['proj-x'] })
        expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: [`${label}-watcher-status`] })
      } finally {
        invalidateSpy.mockRestore()
        client.clear()
      }
    })

    it(`stops ${label} watcher and invalidates status`, async () => {
      invoke.mockResolvedValue(undefined)

      const client = createQueryClient()
      const invalidateSpy = vi.spyOn(client, 'invalidateQueries')

      try {
        const { result } = renderHook(stopHook, {
          wrapper: withProvider(client),
        })

        await result.current.mutateAsync()

        expect(invoke).toHaveBeenCalledWith(stopCommand)
        expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: [`${label}-watcher-status`] })
      } finally {
        invalidateSpy.mockRestore()
        client.clear()
      }
    })
  })
}

testWatcherHooks(
  'codex',
  useCodexWatcherStatus,
  useStartCodexWatcher,
  useStopCodexWatcher,
  'get_codex_watcher_status',
  'start_codex_watcher',
  'stop_codex_watcher'
)

testWatcherHooks(
  'copilot',
  useCopilotWatcherStatus,
  useStartCopilotWatcher,
  useStopCopilotWatcher,
  'get_copilot_watcher_status',
  'start_copilot_watcher',
  'stop_copilot_watcher'
)

testWatcherHooks(
  'opencode',
  useOpenCodeWatcherStatus,
  useStartOpenCodeWatcher,
  useStopOpenCodeWatcher,
  'get_opencode_watcher_status',
  'start_opencode_watcher',
  'stop_opencode_watcher'
)

