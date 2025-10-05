import { useState, useCallback } from 'react'
import { check, Update } from '@tauri-apps/plugin-updater'
import { relaunch } from '@tauri-apps/plugin-process'

interface UpdaterState {
  isChecking: boolean
  isDownloading: boolean
  isInstalling: boolean
  hasUpdate: boolean
  currentVersion: string | null
  latestVersion: string | null
  error: string | null
  downloadProgress: number
}

interface UseUpdaterReturn extends UpdaterState {
  checkForUpdates: () => Promise<void>
  downloadAndInstall: () => Promise<void>
}

/**
 * Hook to manage app updates using Tauri's built-in updater
 */
export function useUpdater(): UseUpdaterReturn {
  const [state, setState] = useState<UpdaterState>({
    isChecking: false,
    isDownloading: false,
    isInstalling: false,
    hasUpdate: false,
    currentVersion: null,
    latestVersion: null,
    error: null,
    downloadProgress: 0,
  })

  const [updateInstance, setUpdateInstance] = useState<Update | null>(null)

  const checkForUpdates = useCallback(async () => {
    setState(prev => ({ ...prev, isChecking: true, error: null }))

    try {
      const update = await check()

      if (update) {
        setState(prev => ({
          ...prev,
          isChecking: false,
          hasUpdate: true,
          currentVersion: update.currentVersion,
          latestVersion: update.version,
        }))
        setUpdateInstance(update)
      } else {
        setState(prev => ({
          ...prev,
          isChecking: false,
          hasUpdate: false,
          error: null,
        }))
      }
    } catch (error) {
      console.error('Failed to check for updates:', error)
      setState(prev => ({
        ...prev,
        isChecking: false,
        error: error instanceof Error ? error.message : 'Failed to check for updates',
      }))
    }
  }, [])

  const downloadAndInstall = useCallback(async () => {
    if (!updateInstance) {
      setState(prev => ({ ...prev, error: 'No update available' }))
      return
    }

    setState(prev => ({ ...prev, isDownloading: true, error: null, downloadProgress: 0 }))

    try {
      // Download the update with progress tracking
      let totalDownloaded = 0
      await updateInstance.downloadAndInstall((event) => {
        switch (event.event) {
          case 'Started':
            setState(prev => ({ ...prev, isDownloading: true }))
            break
          case 'Progress':
            // Track chunks downloaded (we don't have total size, so show indeterminate progress)
            totalDownloaded += event.data.chunkLength
            setState(prev => ({
              ...prev,
              downloadProgress: 50, // Indeterminate progress
            }))
            break
          case 'Finished':
            setState(prev => ({
              ...prev,
              isDownloading: false,
              isInstalling: true,
              downloadProgress: 100,
            }))
            break
        }
      })

      // Update installed successfully, relaunch the app
      setState(prev => ({ ...prev, isInstalling: false }))
      await relaunch()
    } catch (error) {
      console.error('Failed to download and install update:', error)
      setState(prev => ({
        ...prev,
        isDownloading: false,
        isInstalling: false,
        error: error instanceof Error ? error.message : 'Failed to download and install update',
      }))
    }
  }, [updateInstance])

  return {
    ...state,
    checkForUpdates,
    downloadAndInstall,
  }
}
