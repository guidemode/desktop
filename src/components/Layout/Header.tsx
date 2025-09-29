import { useAuth } from '../../hooks/useAuth'

function Header() {
  const { user, logout } = useAuth()

  const handleLogout = async () => {
    await logout()
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
            <p className="text-xs text-base-content/70 hidden sm:block">Desktop Manager</p>
          </div>
        </div>

        {/* User Info */}
        {user && (
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
        )}
      </div>
    </header>
  )
}

export default Header