import { useAuth } from '../hooks/useAuth'

function SettingsPage() {
  const { user, logout } = useAuth()

  const handleLogout = async () => {
    if (confirm('Are you sure you want to logout?')) {
      await logout()
    }
  }

  return (
    <div className="p-6">
      <div className="mb-6">
        <h1 className="text-2xl font-bold text-base-content">Settings</h1>
        <p className="text-base-content/70 mt-1">
          Manage your account and application settings
        </p>
      </div>

      <div className="space-y-6">
        {/* Account Section */}
        <div className="card bg-base-100 shadow-sm border border-base-300">
          <div className="card-body">
            <h2 className="card-title">Account</h2>

            {user && (
              <div className="space-y-4">
                <div className="flex items-center gap-4">
                  <div className="avatar">
                    <div className="w-16 rounded-full">
                      {user.avatarUrl ? (
                        <img alt={user.name || user.username || 'User'} src={user.avatarUrl} />
                      ) : (
                        <div className="bg-primary text-primary-content w-full h-full flex items-center justify-center text-lg font-medium">
                          {(user.name || user.username || 'U').charAt(0).toUpperCase()}
                        </div>
                      )}
                    </div>
                  </div>
                  <div>
                    <div className="text-lg font-medium">{user.name || user.username || 'User'}</div>
                    <div className="text-sm text-base-content/70">@{user.username}</div>
                  </div>
                </div>

                <div className="divider"></div>

                <div className="space-y-2">
                  <div className="flex justify-between items-center">
                    <span className="text-base-content/70">Connected Server</span>
                    <div className="flex items-center gap-2">
                      <div className="w-2 h-2 bg-success rounded-full"></div>
                      <span className="text-sm font-mono">{user.serverUrl}</span>
                    </div>
                  </div>
                  {user.tenantName && (
                    <div className="flex justify-between">
                      <span className="text-base-content/70">Organization</span>
                      <span>{user.tenantName}</span>
                    </div>
                  )}
                </div>

                <div className="divider"></div>

                <button
                  onClick={handleLogout}
                  className="btn btn-error btn-outline"
                >
                  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      strokeWidth={2}
                      d="M17 16l4-4m0 0l-4-4m4 4H7m6 4v1a3 3 0 01-3 3H6a3 3 0 01-3-3V7a3 3 0 013-3h4a3 3 0 013 3v1"
                    />
                  </svg>
                  Logout
                </button>
              </div>
            )}
          </div>
        </div>

        {/* Application Settings */}
        <div className="card bg-base-100 shadow-sm border border-base-300">
          <div className="card-body">
            <h2 className="card-title">Application</h2>

            <div className="space-y-4">
              <div className="form-control">
                <label className="label">
                  <span className="label-text">Theme</span>
                </label>
                <select className="select select-bordered w-full max-w-xs">
                  <option value="guideai">GuideAI (Default)</option>
                  <option value="light">Light</option>
                  <option value="dark">Dark</option>
                </select>
              </div>

              <div className="form-control">
                <label className="cursor-pointer label justify-start gap-2">
                  <input type="checkbox" className="checkbox checkbox-primary" defaultChecked />
                  <span className="label-text">Start with system</span>
                </label>
              </div>

              <div className="form-control">
                <label className="cursor-pointer label justify-start gap-2">
                  <input type="checkbox" className="checkbox checkbox-primary" defaultChecked />
                  <span className="label-text">Show notifications</span>
                </label>
              </div>
            </div>
          </div>
        </div>

        {/* About Section */}
        <div className="card bg-base-100 shadow-sm border border-base-300">
          <div className="card-body">
            <h2 className="card-title">About</h2>

            <div className="space-y-2">
              <div className="flex justify-between">
                <span className="text-base-content/70">Version</span>
                <span>1.0.0</span>
              </div>
              <div className="flex justify-between">
                <span className="text-base-content/70">Build</span>
                <span>Desktop</span>
              </div>
              <div className="flex justify-between">
                <span className="text-base-content/70">Platform</span>
                <span>Tauri</span>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}

export default SettingsPage