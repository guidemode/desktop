import type { ReactNode } from 'react'
import { renderHook, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import {
  useDeleteProviderConfig,
  useProviderConfig,
  useSaveProviderConfig,
  useScanProjects,
} from './useProviderConfig'

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

describe('provider config hooks', () => {
  beforeEach(() => {
    invoke.mockReset()
  })

  it('loads provider config data', async () => {
    const config = { enabled: true, homeDirectory: '~/.claude', projectSelection: 'ALL' as const, selectedProjects: [], lastScanned: null, syncMode: 'Metrics Only' as const }
    invoke.mockResolvedValue(config)

    const client = createQueryClient()
    try {
      const { result } = renderHook(() => useProviderConfig('claude'), {
        wrapper: withProvider(client),
      })

      await waitFor(() => expect(result.current.data).toEqual(config))
      expect(invoke).toHaveBeenCalledWith('load_provider_config_command', { providerId: 'claude' })
    } finally {
      client.clear()
    }
  })

  it('saves provider config and invalidates cache', async () => {
    invoke.mockResolvedValue(undefined)

    const client = createQueryClient()
    const invalidateSpy = vi.spyOn(client, 'invalidateQueries')

    try {
      const { result } = renderHook(() => useSaveProviderConfig(), {
        wrapper: withProvider(client),
      })

      const testConfig = { enabled: true, homeDirectory: '~/.claude', projectSelection: 'ALL' as const, selectedProjects: [], lastScanned: null, syncMode: 'Metrics Only' as const }
      await result.current.mutateAsync({
        providerId: 'claude',
        config: testConfig,
      })

      expect(invoke).toHaveBeenCalledWith('save_provider_config_command', {
        providerId: 'claude',
        config: testConfig,
      })
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['providerConfig', 'claude'] })
    } finally {
      invalidateSpy.mockRestore()
      client.clear()
    }
  })

  it('deletes provider config and invalidates cache', async () => {
    invoke.mockResolvedValue(undefined)

    const client = createQueryClient()
    const invalidateSpy = vi.spyOn(client, 'invalidateQueries')

    try {
      const { result } = renderHook(() => useDeleteProviderConfig(), {
        wrapper: withProvider(client),
      })

      await result.current.mutateAsync('copilot')

      expect(invoke).toHaveBeenCalledWith('delete_provider_config_command', {
        providerId: 'copilot',
      })
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['providerConfig', 'copilot'] })
    } finally {
      invalidateSpy.mockRestore()
      client.clear()
    }
  })

  it('scan projects is disabled when providerId or directory missing', async () => {
    const client = createQueryClient()
    try {
      renderHook(() => useScanProjects('', ''), { wrapper: withProvider(client) })

      expect(invoke).not.toHaveBeenCalled()
    } finally {
      client.clear()
    }

    invoke.mockReset()
    invoke.mockResolvedValue([{ name: 'project-a' }])

    const client2 = createQueryClient()
    try {
      const { result } = renderHook(() => useScanProjects('opencode', '/projects'), {
        wrapper: withProvider(client2),
      })

      await waitFor(() => expect(result.current.data).toEqual([{ name: 'project-a' }]))
      expect(invoke).toHaveBeenCalledWith('scan_projects_command', {
        providerId: 'opencode',
        directory: '/projects',
      })
    } finally {
      client2.clear()
    }
  })
})
