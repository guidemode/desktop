import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { invoke } from '@tauri-apps/api/core'
import type { Project, ProviderConfig } from '../types/providers'

export function useProviderConfig(providerId: string) {
  return useQuery({
    queryKey: ['providerConfig', providerId],
    queryFn: () => invoke<ProviderConfig>('load_provider_config_command', { providerId }),
    staleTime: 5 * 60 * 1000, // 5 minutes
  })
}

export function useSaveProviderConfig() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ providerId, config }: { providerId: string; config: ProviderConfig }) =>
      invoke('save_provider_config_command', { providerId, config }),
    onSuccess: (_, { providerId, config }) => {
      // Update React Query cache immediately for instant UI updates
      queryClient.setQueryData(['providerConfig', providerId], config)
      // Also invalidate to ensure fresh data on next fetch
      queryClient.invalidateQueries({ queryKey: ['providerConfig', providerId] })
    },
  })
}

export function useDeleteProviderConfig() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (providerId: string) => invoke('delete_provider_config_command', { providerId }),
    onSuccess: (_, providerId) => {
      queryClient.invalidateQueries({ queryKey: ['providerConfig', providerId] })
    },
  })
}

export function useScanProjects(providerId: string, directory: string) {
  return useQuery({
    queryKey: ['projects', providerId, directory],
    queryFn: () => invoke<Project[]>('scan_projects_command', { providerId, directory }),
    enabled: !!directory && !!providerId,
    staleTime: 2 * 60 * 1000, // 2 minutes
  })
}
