import { getVersion } from '@tauri-apps/api/app'
import { invoke } from '@tauri-apps/api/core'
import { relaunch } from '@tauri-apps/plugin-process'
import { type Update, check } from '@tauri-apps/plugin-updater'
import { useCallback, useEffect, useState } from 'react'

// Helper function to log updater events to persistent storage
async function logUpdaterEvent(level: string, message: string, details?: Record<string, unknown>) {
  try {
    await invoke('log_updater_event_command', {
      level,
      message,
      details: details ? details : null,
    })
  } catch (error) {
    console.error('Failed to log updater event:', error)
  }
}

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
  isUpToDate: boolean
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
  const [isUpToDate, setIsUpToDate] = useState(false)

  // Load current app version on mount
  useEffect(() => {
    getVersion()
      .then(version => {
        setState(prev => ({ ...prev, currentVersion: version }))
        logUpdaterEvent('INFO', `App version loaded: ${version}`)
      })
      .catch(err => {
        const errorMsg = err instanceof Error ? err.message : String(err)
        logUpdaterEvent('ERROR', 'Failed to get app version', { error: errorMsg })
      })
  }, [])

  const checkForUpdates = useCallback(async () => {
    setState(prev => ({ ...prev, isChecking: true, error: null }))

    try {
      // Log the expected endpoint URL for debugging
      const platform = navigator.platform.toLowerCase()
      const arch =
        navigator.userAgent.includes('x64') || navigator.userAgent.includes('x86_64')
          ? 'x86_64'
          : navigator.userAgent.includes('aarch64') || navigator.userAgent.includes('arm64')
            ? 'aarch64'
            : 'x86_64'

      let target = 'unknown'
      if (platform.includes('mac') || platform.includes('darwin')) {
        target = 'darwin-universal'
      } else if (platform.includes('linux')) {
        target = `linux-${arch}`
      } else if (platform.includes('win')) {
        target = `windows-${arch}`
      }

      const expectedUrl = `https://install.guideai.dev/desktop/${target}/latest.json`
      await logUpdaterEvent('INFO', 'Checking for updates', {
        platform,
        arch,
        target,
        endpoint: expectedUrl,
      })

      const update = await check()

      if (update) {
        await logUpdaterEvent('INFO', 'Update available', {
          currentVersion: update.currentVersion,
          latestVersion: update.version,
        })
        setState(prev => ({
          ...prev,
          isChecking: false,
          hasUpdate: true,
          currentVersion: update.currentVersion,
          latestVersion: update.version,
        }))
        setUpdateInstance(update)
      } else {
        await logUpdaterEvent('INFO', 'No update available - app is up to date')
        setIsUpToDate(true)
        setState(prev => ({
          ...prev,
          isChecking: false,
          hasUpdate: false,
          error: null,
        }))
        // Reset isUpToDate after 3 seconds
        setTimeout(() => setIsUpToDate(false), 3000)
      }
    } catch (error) {
      const errorDetails = {
        message: error instanceof Error ? error.message : String(error),
        stack: error instanceof Error ? error.stack : undefined,
        type: error instanceof Error ? error.constructor.name : typeof error,
      }
      await logUpdaterEvent('ERROR', 'Failed to check for updates', errorDetails)
      setState(prev => ({
        ...prev,
        isChecking: false,
        error: error instanceof Error ? error.message : 'Failed to check for updates',
      }))
    }
  }, [])

  const downloadAndInstall = useCallback(async () => {
    if (!updateInstance) {
      await logUpdaterEvent('ERROR', 'Cannot download and install: No update available')
      setState(prev => ({ ...prev, error: 'No update available' }))
      return
    }

    await logUpdaterEvent('INFO', 'Starting download and install', {
      version: updateInstance.version,
    })
    setState(prev => ({ ...prev, isDownloading: true, error: null, downloadProgress: 0 }))

    try {
      // Download the update with progress tracking
      let totalDownloaded = 0
      let chunkCount = 0
      await logUpdaterEvent('INFO', 'Calling downloadAndInstall on update instance')

      await updateInstance.downloadAndInstall(event => {
        switch (event.event) {
          case 'Started':
            logUpdaterEvent('INFO', 'Download started', {
              contentLength: event.data.contentLength,
            })
            setState(prev => ({ ...prev, isDownloading: true }))
            break
          case 'Progress':
            // Track chunks downloaded (we don't have total size, so show indeterminate progress)
            chunkCount++
            totalDownloaded += event.data.chunkLength
            // Log every 10th chunk to avoid spam
            if (chunkCount % 10 === 0) {
              logUpdaterEvent(
                'INFO',
                `Download progress: ${chunkCount} chunks, ${totalDownloaded} bytes`
              )
            }
            setState(prev => ({
              ...prev,
              downloadProgress: 50, // Indeterminate progress
            }))
            break
          case 'Finished':
            logUpdaterEvent('INFO', 'Download finished, starting installation', {
              totalDownloaded,
              chunkCount,
            })
            setState(prev => ({
              ...prev,
              isDownloading: false,
              isInstalling: true,
              downloadProgress: 100,
            }))
            break
        }
      })

      await logUpdaterEvent('INFO', 'downloadAndInstall completed successfully')

      // Update installed successfully, relaunch the app
      await logUpdaterEvent('INFO', 'Installation complete, preparing to relaunch app', {
        version: updateInstance.version,
        totalDownloaded,
      })
      setState(prev => ({ ...prev, isInstalling: false }))

      try {
        await logUpdaterEvent('INFO', 'Calling relaunch() from @tauri-apps/plugin-process')
        await relaunch()
        // This line should never execute if relaunch succeeds
        await logUpdaterEvent('WARN', 'Relaunch returned without error but app is still running')
      } catch (relaunchError) {
        // Catch specific relaunch errors
        const relaunchDetails = {
          message: relaunchError instanceof Error ? relaunchError.message : String(relaunchError),
          stack: relaunchError instanceof Error ? relaunchError.stack : undefined,
          type:
            relaunchError instanceof Error ? relaunchError.constructor.name : typeof relaunchError,
          errorCode: (relaunchError as any).code,
          errorName: (relaunchError as any).name,
        }
        await logUpdaterEvent('ERROR', 'Relaunch failed with error', relaunchDetails)
        throw relaunchError // Re-throw to be caught by outer catch
      }
    } catch (error) {
      const errorDetails = {
        message: error instanceof Error ? error.message : String(error),
        stack: error instanceof Error ? error.stack : undefined,
        type: error instanceof Error ? error.constructor.name : typeof error,
        version: updateInstance.version,
      }
      await logUpdaterEvent('ERROR', 'Failed to download and install update', errorDetails)
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
    isUpToDate,
  }
}
