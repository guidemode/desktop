import { getCurrentWindow } from '@tauri-apps/api/window'
import { useEffect, useState } from 'react'

type Theme = 'guidemode-light' | 'guidemode-dark'

/**
 * Hook to manage app theme with Tauri native window theme synchronization
 *
 * This hook:
 * 1. Syncs React theme state with localStorage
 * 2. Updates the document data-theme attribute for DaisyUI
 * 3. Calls Tauri's window.setTheme() to update native title bar appearance
 */
export function useTheme() {
  const [theme, setThemeState] = useState<Theme>(() => {
    const saved = localStorage.getItem('theme')
    return (saved === 'guidemode-dark' ? 'guidemode-dark' : 'guidemode-light') as Theme
  })

  useEffect(() => {
    const applyTheme = async () => {
      // Update document attribute for DaisyUI
      document.documentElement.setAttribute('data-theme', theme)

      // Save to localStorage
      localStorage.setItem('theme', theme)

      // Update native window theme (macOS 10.14+, Windows)
      try {
        const appWindow = getCurrentWindow()
        const nativeTheme = theme === 'guidemode-dark' ? 'dark' : 'light'
        await appWindow.setTheme(nativeTheme)
      } catch (error) {
        console.warn('Failed to set native window theme:', error)
        // Fallback gracefully - UI theme still works
      }
    }

    applyTheme()
  }, [theme])

  const setTheme = (newTheme: Theme) => {
    setThemeState(newTheme)
  }

  const toggleTheme = () => {
    setThemeState(prev => (prev === 'guidemode-dark' ? 'guidemode-light' : 'guidemode-dark'))
  }

  return {
    theme,
    setTheme,
    toggleTheme,
    isDark: theme === 'guidemode-dark',
  }
}
