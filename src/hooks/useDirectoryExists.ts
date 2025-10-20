import { useQuery } from '@tanstack/react-query'
import { invoke } from '@tauri-apps/api/core'

export function useDirectoryExists(path: string | undefined, enabled = true) {
  return useQuery({
    queryKey: ['directory-exists', path],
    queryFn: () => invoke<boolean>('check_directory_exists', { path }),
    enabled: enabled && !!path,
    staleTime: 30 * 1000, // 30 seconds
  })
}
