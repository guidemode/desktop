import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { useEffect } from 'react'

export interface User {
  username: string
  serverUrl: string
  tenantId?: string
  tenantName?: string
  name?: string
  avatarUrl?: string
}

export interface GuideModeConfig {
  apiKey?: string
  serverUrl?: string
  username?: string
  name?: string
  avatarUrl?: string
  tenantId?: string
  tenantName?: string
}

export function useAuth() {
  const queryClient = useQueryClient()

  const { data: config, isLoading } = useQuery({
    queryKey: ['auth', 'config'],
    queryFn: async (): Promise<GuideModeConfig> => {
      return await invoke('load_config_command')
    },
  })

  // Listen for config file changes from the file watcher
  useEffect(() => {
    let unlisten: (() => void) | undefined

    const setupListener = async () => {
      try {
        unlisten = await listen('config-changed', () => {
          // Invalidate and refetch the config when the file changes
          queryClient.invalidateQueries({ queryKey: ['auth', 'config'] })
        })
      } catch (error) {
        console.error('Failed to set up config change listener:', error)
      }
    }

    setupListener()

    return () => {
      if (unlisten) {
        unlisten()
      }
    }
  }, [queryClient])

  const loginMutation = useMutation({
    mutationFn: async (serverUrl: string) => {
      return await invoke('login_command', { serverUrl })
    },
    onSuccess: async () => {
      // Invalidate and immediately refetch to ensure fresh state
      await queryClient.invalidateQueries({ queryKey: ['auth'] })
      await queryClient.refetchQueries({ queryKey: ['auth', 'config'] })
    },
  })

  const logoutMutation = useMutation({
    mutationFn: async () => {
      return await invoke('logout_command')
    },
    onSuccess: async () => {
      // Invalidate and immediately refetch to ensure fresh state
      await queryClient.invalidateQueries({ queryKey: ['auth'] })
      await queryClient.refetchQueries({ queryKey: ['auth', 'config'] })
    },
  })

  const user: User | null =
    config?.apiKey && config?.username
      ? {
          username: config.username,
          serverUrl: config.serverUrl || import.meta.env.VITE_SERVER_URL || 'http://localhost:3000',
          tenantId: config.tenantId,
          tenantName: config.tenantName,
          name: config.name,
          avatarUrl: config.avatarUrl,
        }
      : null

  return {
    user,
    config,
    isLoading,
    login: loginMutation.mutate,
    logout: logoutMutation.mutate,
    isLoggingIn: loginMutation.isPending,
    isLoggingOut: logoutMutation.isPending,
  }
}
