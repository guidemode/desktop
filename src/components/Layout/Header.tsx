import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useAuth } from '../../hooks/useAuth'
import { useOnboarding } from '../../hooks/useOnboarding'
import { useUpdater } from '../../hooks/useUpdater'

function Header() {
  const { user, logout } = useAuth()
  const navigate = useNavigate()
  const { hasUpdate, latestVersion, checkForUpdates } = useUpdater()
  const { startTour } = useOnboarding()
  const [theme, setTheme] = useState(() => {
    return localStorage.getItem('theme') || 'guideai-light'
  })

  useEffect(() => {
    // Update document data-theme attribute
    document.documentElement.setAttribute('data-theme', theme)
    localStorage.setItem('theme', theme)
  }, [theme])

  useEffect(() => {
    // Check for updates on mount
    checkForUpdates()
  }, [checkForUpdates])

  const toggleTheme = () => {
    setTheme(prev => (prev === 'guideai-dark' ? 'guideai-light' : 'guideai-dark'))
  }

  const handleLogout = async () => {
    await logout()
  }

  const handleLoginClick = () => {
    navigate('/settings')
  }

  const handleStartTour = () => {
    // Navigate to dashboard first, then start tour
    navigate('/')
    // Delay to ensure navigation completes
    setTimeout(() => {
      startTour()
    }, 100)
  }

  return (
    <header className="bg-base-100 border-b border-base-300 px-3 py-2">
      <div className="flex items-center justify-between">
        {/* Brand */}
        <div className="flex items-center gap-0.5">
          <div className="avatar">
            <div className="w-8 rounded">
              <img
                src="/logo-44-optimized.png"
                alt="GuideAI"
                className="w-full h-full object-contain"
              />
            </div>
          </div>
          <div>
            <h1 className="text-xl font-bold text-primary">GuideAI</h1>
          </div>
        </div>

        {/* Theme Toggle and User Info */}
        <div className="flex items-center gap-2">
          {/* Update Available Notification */}
          {hasUpdate && (
            <button
              onClick={() => navigate('/settings', { state: { autoDownload: true } })}
              className="btn btn-warning btn-sm gap-1"
              title={`Update available: v${latestVersion}`}
            >
              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M7 16a4 4 0 01-.88-7.903A5 5 0 1115.9 6L16 6a5 5 0 011 9.9M15 13l-3-3m0 0l-3 3m3-3v12"
                />
              </svg>
              <span className="hidden sm:inline">Update</span>
            </button>
          )}

          {/* Docs Link */}
          <a
            href="https://docs.guideai.dev"
            target="_blank"
            rel="noopener noreferrer"
            className="btn btn-ghost btn-sm btn-circle"
            title="Documentation"
          >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M12 6.253v13m0-13C10.832 5.477 9.246 5 7.5 5S4.168 5.477 3 6.253v13C4.168 18.477 5.754 18 7.5 18s3.332.477 4.5 1.253m0-13C13.168 5.477 14.754 5 16.5 5c1.747 0 3.332.477 4.5 1.253v13C19.832 18.477 18.247 18 16.5 18c-1.746 0-3.332.477-4.5 1.253"
              />
            </svg>
          </a>

          {/* Help/Tour Button */}
          <button
            onClick={handleStartTour}
            className="btn btn-ghost btn-sm btn-circle"
            title="Take a tour"
          >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M8.228 9c.549-1.165 2.03-2 3.772-2 2.21 0 4 1.343 4 3 0 1.4-1.278 2.575-3.006 2.907-.542.104-.994.54-.994 1.093m0 3h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
              />
            </svg>
          </button>

          {/* Theme Toggle Button */}
          <button
            onClick={toggleTheme}
            className="btn btn-ghost btn-sm btn-circle"
            title={theme === 'guideai-dark' ? 'Switch to light mode' : 'Switch to dark mode'}
          >
            {theme === 'guideai-dark' ? (
              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M12 3v1m0 16v1m9-9h-1M4 12H3m15.364 6.364l-.707-.707M6.343 6.343l-.707-.707m12.728 0l-.707.707M6.343 17.657l-.707.707M16 12a4 4 0 11-8 0 4 4 0 018 0z"
                />
              </svg>
            ) : (
              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M20.354 15.354A9 9 0 018.646 3.646 9.003 9.003 0 0012 21a9.003 9.003 0 008.354-5.646z"
                />
              </svg>
            )}
          </button>

          {/* User Info or Login */}
          {user ? (
            <div className="flex items-center gap-2">
              <div className="text-right">
                <div className="text-sm font-medium">{user.name || user.username || 'User'}</div>
                <div className="text-xs text-base-content/70">@{user.username}</div>
              </div>
              <div className="avatar hidden sm:block">
                <div className="w-6 rounded-full">
                  {user.avatarUrl ? (
                    <img alt={user.name || user.username || 'User'} src={user.avatarUrl} />
                  ) : (
                    <div className="bg-primary text-primary-content w-full h-full flex items-center justify-center text-sm font-medium">
                      {(user.name || user.username || 'U').charAt(0).toUpperCase()}
                    </div>
                  )}
                </div>
              </div>
              <button
                onClick={handleLogout}
                className="btn btn-ghost btn-sm btn-circle text-error"
                title="Logout"
              >
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    strokeWidth={2}
                    d="M17 16l4-4m0 0l-4-4m4 4H7m6 4v1a3 3 0 01-3 3H6a3 3 0 01-3-3V7a3 3 0 013-3h4a3 3 0 013 3v1"
                  />
                </svg>
              </button>
            </div>
          ) : (
            <button onClick={handleLoginClick} className="btn btn-primary btn-sm">
              Login
            </button>
          )}
        </div>
      </div>
    </header>
  )
}

export default Header
