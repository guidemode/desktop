import { Link, useNavigate } from 'react-router-dom'
import { useAuth } from '../hooks/useAuth'
import { useLocalSessions } from '../hooks/useLocalSessions'
import { SessionCard } from '@guideai-dev/session-processing/ui'
import ProviderIcon from '../components/icons/ProviderIcon'
import { useSessionActivity } from '../hooks/useSessionActivity'
import { useSessionActivityStore } from '../stores/sessionActivityStore'
import { useClaudeWatcherStatus } from '../hooks/useClaudeWatcher'
import { useOpenCodeWatcherStatus } from '../hooks/useOpenCodeWatcher'
import { useCodexWatcherStatus } from '../hooks/useCodexWatcher'

function DashboardPage() {
  const navigate = useNavigate()
  const { user } = useAuth()
  const { sessions, loading } = useLocalSessions()

  // Get watcher statuses
  const { data: claudeStatus } = useClaudeWatcherStatus()
  const { data: opencodeStatus } = useOpenCodeWatcherStatus()
  const { data: codexStatus } = useCodexWatcherStatus()

  // Track session activity
  useSessionActivity()
  const isSessionActive = useSessionActivityStore(state => state.isSessionActive)

  // Calculate total duration in minutes/hours/days
  const totalDurationMs = sessions.reduce((total, session) => {
    return total + (session.durationMs || 0)
  }, 0)

  const formatDuration = (ms: number) => {
    const minutes = Math.floor(ms / 60000)
    const hours = Math.floor(minutes / 60)
    const days = Math.floor(hours / 24)

    if (days > 0) {
      return { value: days, unit: 'days', subValue: hours % 24, subUnit: 'hrs' }
    } else if (hours > 0) {
      return { value: hours, unit: 'hours', subValue: minutes % 60, subUnit: 'min' }
    } else {
      return { value: minutes, unit: 'minutes', subValue: null, subUnit: null }
    }
  }

  const duration = formatDuration(totalDurationMs)

  // Get active providers
  const activeProviders = [
    { id: 'claude-code', name: 'Claude Code', isRunning: claudeStatus?.is_running },
    { id: 'opencode', name: 'OpenCode', isRunning: opencodeStatus?.is_running },
    { id: 'codex', name: 'Codex', isRunning: codexStatus?.is_running },
  ].filter(p => p.isRunning)

  // Get latest 5 sessions
  const latestSessions = sessions.slice(0, 5)

  const handleViewSession = (sessionId: string) => {
    navigate(`/sessions/${sessionId}`)
  }

  return (
    <div className="p-6">
      <div className="mb-6">
        <h1 className="text-2xl font-bold text-base-content">Dashboard</h1>
        <p className="text-sm text-base-content/70 mt-1">
          Welcome to GuideAI Desktop Manager
        </p>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
        {/* Session Stats Card */}
        <div className="card bg-base-100 shadow-sm border border-base-300">
          <div className="card-body">
            <h2 className="card-title text-base">Total Sessions</h2>
            <p className="text-3xl font-bold">{sessions.length}</p>
            <div className="divider my-2"></div>
            <div className="text-sm text-base-content/70 mb-1">Total Duration</div>
            {totalDurationMs > 0 ? (
              <div className="flex items-baseline gap-2">
                <p className="text-2xl font-bold">{duration.value}</p>
                <p className="text-base text-base-content/70">{duration.unit}</p>
                {duration.subValue !== null && (
                  <>
                    <p className="text-lg font-semibold">{duration.subValue}</p>
                    <p className="text-sm text-base-content/70">{duration.subUnit}</p>
                  </>
                )}
              </div>
            ) : (
              <p className="text-2xl font-bold">0 min</p>
            )}
          </div>
        </div>

        {/* Active Providers Card */}
        <div className="card bg-base-100 shadow-sm border border-base-300">
          <div className="card-body">
            <h2 className="card-title text-base">Active Providers</h2>
            {activeProviders.length > 0 ? (
              <div className="flex flex-col gap-3 mt-2">
                {activeProviders.map(provider => (
                  <div key={provider.id} className="flex items-center gap-3">
                    <ProviderIcon providerId={provider.id} size={32} />
                    <span className="text-base font-medium">{provider.name}</span>
                    <div className="w-2 h-2 bg-success rounded-full ml-auto" title="Running" />
                  </div>
                ))}
              </div>
            ) : (
              <div className="flex flex-col items-center justify-center py-4">
                <p className="text-base-content/50 text-sm">No active watchers</p>
                <Link to="/settings" className="link link-primary text-xs mt-1">
                  Configure providers
                </Link>
              </div>
            )}
          </div>
        </div>

        {/* Sync Status Card */}
        <div className="card bg-base-100 shadow-sm border border-base-300">
          <div className="card-body">
            <h2 className="card-title text-base">Sync Status</h2>
            <p className="text-lg font-semibold">
              {user ? (
                <span className="text-success">Connected</span>
              ) : (
                <span className="text-base-content/50">Offline</span>
              )}
            </p>
            {user ? (
              <div className="text-sm text-base-content/70">
                <p>{user.serverUrl}</p>
                {user.tenantName && <p className="font-medium">{user.tenantName}</p>}
              </div>
            ) : (
              <Link to="/settings" className="link link-primary text-sm">
                Login to sync
              </Link>
            )}
          </div>
        </div>
      </div>

      {/* Latest Sessions */}
      <div className="mt-6">
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-xl font-bold text-base-content">Latest Sessions</h2>
          {sessions.length > 0 && (
            <Link to="/sessions" className="link link-primary text-sm">
              View all →
            </Link>
          )}
        </div>
        {loading ? (
          <div className="flex items-center justify-center py-12">
            <span className="loading loading-spinner loading-lg" />
          </div>
        ) : latestSessions.length === 0 ? (
          <div className="card bg-base-100 shadow-sm border border-base-300">
            <div className="card-body">
              <p className="text-sm text-base-content/70">No sessions yet</p>
            </div>
          </div>
        ) : (
          <div className="grid gap-4">
            {latestSessions.map((session) => (
              <SessionCard
                key={session.sessionId as string}
                session={{
                  ...session,
                  aiModelQualityScore: session.aiModelQualityScore,
                }}
                isActive={isSessionActive(session.sessionId as string)}
                onViewSession={() => handleViewSession(session.sessionId as string)}
                ProviderIcon={({ providerId, size }) => (
                  <ProviderIcon providerId={providerId} size={size} />
                )}
                LinkComponent={({ to, children }) => <Link to={to}>{children}</Link>}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  )
}

export default DashboardPage
