import { useAuth } from '../../hooks/useAuth'
import { useNavigate } from 'react-router-dom'
import { useState, useEffect } from 'react'

function Header() {
  const { user, logout } = useAuth()
  const navigate = useNavigate()
  const [theme, setTheme] = useState(() => {
    return localStorage.getItem('theme') || 'guideai-light'
  })

  useEffect(() => {
    // Update document data-theme attribute
    document.documentElement.setAttribute('data-theme', theme)
    localStorage.setItem('theme', theme)
  }, [theme])

  const toggleTheme = () => {
    setTheme(prev => prev === 'guideai-dark' ? 'guideai-light' : 'guideai-dark')
  }

  const handleLogout = async () => {
    await logout()
  }

  const handleLoginClick = () => {
    navigate('/settings')
  }

  return (
    <header className="bg-base-100 border-b border-base-300 px-3 py-2">
      <div className="flex items-center justify-between">
        {/* Brand */}
        <div className="flex items-center gap-2">
          <div className="avatar">
            <div className="w-6 rounded">
              <img src="/logo-32-optimized.png" alt="GuideAI" className="w-full h-full object-contain" />
            </div>
          </div>
          <div>
            <h1 className="text-base font-bold text-primary">GuideAI</h1>
          </div>
        </div>

        {/* Theme Toggle and User Info */}
        <div className="flex items-center gap-2">
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
            <button
              onClick={handleLoginClick}
              className="btn btn-primary btn-sm"
            >
              Login
            </button>
          )}
        </div>
      </div>
    </header>
  )
}

export default Header