import { BuildingOfficeIcon, KeyIcon, ServerIcon, UserIcon } from '@heroicons/react/24/outline'
import { type User, useAuth } from '../hooks/useAuth'

interface UserInfoProps {
  user: User
}

export default function UserInfo({ user }: UserInfoProps) {
  const { logout, isLoggingOut, config } = useAuth()

  const handleLogout = () => {
    logout()
  }

  return (
    <div className="card bg-base-200 shadow-lg">
      <div className="card-body">
        <h2 className="card-title justify-center mb-4">
          <UserIcon className="w-5 h-5" />
          Account Info
        </h2>

        <div className="space-y-4">
          <div className="flex items-center gap-3">
            <UserIcon className="w-4 h-4 text-primary" />
            <div>
              <div className="font-medium">{user.username}</div>
              <div className="text-sm text-base-content/70">Username</div>
            </div>
          </div>

          <div className="flex items-center gap-3">
            <ServerIcon className="w-4 h-4 text-primary" />
            <div>
              <div className="font-medium">{user.serverUrl}</div>
              <div className="text-sm text-base-content/70">Server</div>
            </div>
          </div>

          {user.tenantName && (
            <div className="flex items-center gap-3">
              <BuildingOfficeIcon className="w-4 h-4 text-primary" />
              <div>
                <div className="font-medium">{user.tenantName}</div>
                <div className="text-sm text-base-content/70">Organization</div>
              </div>
            </div>
          )}

          {config?.apiKey && (
            <div className="flex items-center gap-3">
              <KeyIcon className="w-4 h-4 text-primary" />
              <div>
                <div className="font-mono text-sm">{config.apiKey.substring(0, 12)}...</div>
                <div className="text-sm text-base-content/70">API Key</div>
              </div>
            </div>
          )}
        </div>

        <div className="card-actions justify-center mt-6">
          <button
            onClick={handleLogout}
            className="btn btn-outline btn-error"
            disabled={isLoggingOut}
          >
            {isLoggingOut ? (
              <>
                <span className="loading loading-spinner loading-sm" />
                Signing Out...
              </>
            ) : (
              'Sign Out'
            )}
          </button>
        </div>
      </div>
    </div>
  )
}
