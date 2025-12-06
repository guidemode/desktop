import { invoke } from '@tauri-apps/api/core'
import { useCallback, useEffect, useState } from 'react'

// Keep LocalProject as an alias for backward compatibility with components
export interface LocalProject {
  id: string
  name: string
  githubRepo: string | null
  cwd: string
  type: string
  createdAt: number
  updatedAt: number
  sessionCount: number
}

// New interface name for repository terminology
export type LocalRepository = LocalProject

/**
 * Fetch all repositories from the local database
 */
export function useLocalProjects() {
  const [projects, setProjects] = useState<LocalRepository[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    try {
      setLoading(true)
      setError(null)
      const result = await invoke<LocalRepository[]>('get_all_repositories')
      setProjects(result)
    } catch (err) {
      console.error('Failed to fetch repositories:', err)
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    refresh()
  }, [refresh])

  return { projects, loading, error, refresh }
}

/**
 * Fetch a single repository by ID
 */
export function useLocalProject(projectId: string | undefined) {
  const [project, setProject] = useState<LocalRepository | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!projectId) {
      setLoading(false)
      return
    }

    const fetchProject = async () => {
      try {
        setLoading(true)
        setError(null)
        const result = await invoke<LocalRepository | null>('get_repository_by_id', {
          repositoryId: projectId,
        })
        setProject(result)
      } catch (err) {
        console.error('Failed to fetch repository:', err)
        setError(err instanceof Error ? err.message : String(err))
      } finally {
        setLoading(false)
      }
    }

    fetchProject()
  }, [projectId])

  return { project, loading, error }
}
