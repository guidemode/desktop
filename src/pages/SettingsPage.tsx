import { useAuth } from '../hooks/useAuth'
import { useConfigStore } from '../stores/configStore'
import { useUpdater } from '../hooks/useUpdater'
import { useOnboarding } from '../hooks/useOnboarding'
import Login from '../components/Login'
import { useState } from 'react'
import { useNavigate } from 'react-router-dom'

function SettingsPage() {
  const navigate = useNavigate()
  const { user, logout } = useAuth()
  const { aiApiKeys, setAiApiKey, deleteAiApiKey, systemConfig, updateSystemConfig } = useConfigStore()
  const { resetTour, hasCompletedTour } = useOnboarding()
  const {
    hasUpdate,
    currentVersion,
    latestVersion,
    isChecking,
    isDownloading,
    isInstalling,
    downloadProgress,
    error,
    checkForUpdates,
    downloadAndInstall,
    isUpToDate,
  } = useUpdater()
  const [showClaudeKey, setShowClaudeKey] = useState(false)
  const [showGeminiKey, setShowGeminiKey] = useState(false)
  const [claudeKey, setClaudeKey] = useState(aiApiKeys.claude || '')
  const [geminiKey, setGeminiKey] = useState(aiApiKeys.gemini || '')

  const handleLogout = async () => {
    await logout()
  }

  const handleRestartTour = () => {
    navigate('/')
    // Delay to ensure navigation completes
    setTimeout(() => {
      resetTour()
    }, 100)
  }

  return (
    <div className="p-6">
      <div className="mb-6 flex items-start justify-between">
        <div>
          <h1 className="text-2xl font-bold text-base-content">Settings</h1>
          <p className="text-base-content/70 mt-1">
            Manage your account and application settings
          </p>
        </div>
        <button
          onClick={() => navigate('/logs')}
          className="btn btn-outline btn-sm gap-2"
        >
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
          </svg>
          Logs
        </button>
      </div>

      <div className="space-y-6">
        {/* Account Section */}
        <div className="card bg-base-100 shadow-sm border border-base-300">
          <div className="card-body">
            <h2 className="card-title">Account</h2>

            {user ? (
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
            ) : (
              <div className="py-4">
                <p className="text-sm text-base-content/70 mb-4">
                  Login to enable cloud sync features and access your sessions across devices.
                </p>
                <Login />
              </div>
            )}
          </div>
        </div>

        {/* AI Processing Settings */}
        <div className="card bg-base-100 shadow-sm border border-base-300" data-tour="ai-processing">
          <div className="card-body">
            <h2 className="card-title">AI Processing</h2>
            <p className="text-sm text-base-content/70 mb-4">
              Configure API keys to enable AI-powered session summaries and quality assessments.
              Keys are stored locally and never sent to GuideAI servers.
            </p>

            <div className="space-y-6">
              {/* Claude API Key */}
              <div className="form-control">
                <label className="label">
                  <span className="label-text font-medium">Claude API Key</span>
                  <span className="label-text-alt text-xs">For session summaries & quality scores</span>
                </label>
                <div className="flex gap-2">
                  <div className="flex-1 relative">
                    <input
                      type={showClaudeKey ? 'text' : 'password'}
                      placeholder="sk-ant-..."
                      className="input input-bordered w-full pr-20"
                      value={claudeKey}
                      onChange={(e) => setClaudeKey(e.target.value)}
                    />
                    <button
                      type="button"
                      className="absolute right-2 top-1/2 -translate-y-1/2 btn btn-ghost btn-sm btn-square"
                      onClick={() => setShowClaudeKey(!showClaudeKey)}
                    >
                      {showClaudeKey ? (
                        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13.875 18.825A10.05 10.05 0 0112 19c-4.478 0-8.268-2.943-9.543-7a9.97 9.97 0 011.563-3.029m5.858.908a3 3 0 114.243 4.243M9.878 9.878l4.242 4.242M9.88 9.88l-3.29-3.29m7.532 7.532l3.29 3.29M3 3l3.59 3.59m0 0A9.953 9.953 0 0112 5c4.478 0 8.268 2.943 9.543 7a10.025 10.025 0 01-4.132 5.411m0 0L21 21" />
                        </svg>
                      ) : (
                        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
                        </svg>
                      )}
                    </button>
                  </div>
                  <button
                    className="btn btn-primary"
                    onClick={() => setAiApiKey('claude', claudeKey)}
                    disabled={!claudeKey || claudeKey === aiApiKeys.claude}
                  >
                    Save
                  </button>
                  {aiApiKeys.claude && (
                    <button
                      className="btn btn-error btn-outline"
                      onClick={() => {
                        deleteAiApiKey('claude')
                        setClaudeKey('')
                      }}
                    >
                      Clear
                    </button>
                  )}
                </div>
                {aiApiKeys.claude && (
                  <label className="label">
                    <span className="label-text-alt text-success">✓ Claude API key configured</span>
                  </label>
                )}
              </div>

              {/* Gemini API Key */}
              <div className="form-control">
                <label className="label">
                  <span className="label-text font-medium">Gemini API Key</span>
                  <span className="label-text-alt text-xs">Alternative to Claude</span>
                </label>
                <div className="flex gap-2">
                  <div className="flex-1 relative">
                    <input
                      type={showGeminiKey ? 'text' : 'password'}
                      placeholder="AIza..."
                      className="input input-bordered w-full pr-20"
                      value={geminiKey}
                      onChange={(e) => setGeminiKey(e.target.value)}
                    />
                    <button
                      type="button"
                      className="absolute right-2 top-1/2 -translate-y-1/2 btn btn-ghost btn-sm btn-square"
                      onClick={() => setShowGeminiKey(!showGeminiKey)}
                    >
                      {showGeminiKey ? (
                        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13.875 18.825A10.05 10.05 0 0112 19c-4.478 0-8.268-2.943-9.543-7a9.97 9.97 0 011.563-3.029m5.858.908a3 3 0 114.243 4.243M9.878 9.878l4.242 4.242M9.88 9.88l-3.29-3.29m7.532 7.532l3.29 3.29M3 3l3.59 3.59m0 0A9.953 9.953 0 0112 5c4.478 0 8.268 2.943 9.543 7a10.025 10.025 0 01-4.132 5.411m0 0L21 21" />
                        </svg>
                      ) : (
                        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
                        </svg>
                      )}
                    </button>
                  </div>
                  <button
                    className="btn btn-primary"
                    onClick={() => setAiApiKey('gemini', geminiKey)}
                    disabled={!geminiKey || geminiKey === aiApiKeys.gemini}
                  >
                    Save
                  </button>
                  {aiApiKeys.gemini && (
                    <button
                      className="btn btn-error btn-outline"
                      onClick={() => {
                        deleteAiApiKey('gemini')
                        setGeminiKey('')
                      }}
                    >
                      Clear
                    </button>
                  )}
                </div>
                {aiApiKeys.gemini && (
                  <label className="label">
                    <span className="label-text-alt text-success">✓ Gemini API key configured</span>
                  </label>
                )}
              </div>

              <div className="alert alert-info">
                <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                </svg>
                <div className="text-sm">
                  <p className="font-medium">How to get API keys:</p>
                  <ul className="mt-1 space-y-1">
                    <li>• Claude: <a href="https://console.anthropic.com" target="_blank" rel="noopener noreferrer" className="link">console.anthropic.com</a></li>
                    <li>• Gemini: <a href="https://makersuite.google.com/app/apikey" target="_blank" rel="noopener noreferrer" className="link">makersuite.google.com</a></li>
                  </ul>
                </div>
              </div>
            </div>
          </div>
        </div>

        {/* Help & Tour Section */}
        <div className="card bg-base-100 shadow-sm border border-base-300">
          <div className="card-body">
            <h2 className="card-title">Help & Tour</h2>
            <p className="text-sm text-base-content/70 mb-4">
              Take a guided tour of GuideAI to learn how to configure providers, sync sessions, and view analytics.
            </p>

            <div className="flex items-center justify-between p-4 bg-base-200 rounded-lg">
              <div>
                <div className="font-medium">Onboarding Tour</div>
                <div className="text-sm text-base-content/60 mt-1">
                  {hasCompletedTour
                    ? 'Restart the tour to see how to use GuideAI'
                    : 'Start the tour to learn about GuideAI features'}
                </div>
              </div>
              <button
                onClick={handleRestartTour}
                className="btn btn-primary"
              >
                <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    strokeWidth={2}
                    d="M8.228 9c.549-1.165 2.03-2 3.772-2 2.21 0 4 1.343 4 3 0 1.4-1.278 2.575-3.006 2.907-.542.104-.994.54-.994 1.093m0 3h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
                  />
                </svg>
                {hasCompletedTour ? 'Restart Tour' : 'Start Tour'}
              </button>
            </div>
          </div>
        </div>

        {/* Metrics Processing Timing */}
        <div className="card bg-base-100 shadow-sm border border-base-300">
          <div className="card-body">
            <h2 className="card-title">Metrics Processing</h2>
            <p className="text-sm text-base-content/70 mb-4">
              Configure when metrics are processed for your sessions.
            </p>

            <div className="space-y-6">
              {/* Core Metrics Debounce */}
              <div className="form-control">
                <label className="label">
                  <span className="label-text font-medium">Core Metrics Debounce</span>
                  <span className="label-text-alt text-xs">{systemConfig.coreMetricsDebounceSeconds}s</span>
                </label>
                <input
                  type="range"
                  min="5"
                  max="60"
                  value={systemConfig.coreMetricsDebounceSeconds}
                  onChange={(e) => updateSystemConfig({ coreMetricsDebounceSeconds: parseInt(e.target.value) })}
                  className="range range-primary"
                  step="5"
                />
                <div className="w-full flex justify-between text-xs px-2 text-base-content/50">
                  <span>5s</span>
                  <span>30s</span>
                  <span>60s</span>
                </div>
                <label className="label">
                  <span className="label-text-alt">Wait time after file activity stops before processing core metrics</span>
                </label>
              </div>

              {/* AI Processing Delay */}
              <div className="form-control">
                <label className="label">
                  <span className="label-text font-medium">AI Processing Delay</span>
                  <span className="label-text-alt text-xs">{systemConfig.aiProcessingDelayMinutes}m</span>
                </label>
                <input
                  type="range"
                  min="1"
                  max="60"
                  value={systemConfig.aiProcessingDelayMinutes}
                  onChange={(e) => updateSystemConfig({ aiProcessingDelayMinutes: parseInt(e.target.value) })}
                  className="range range-primary"
                  step="1"
                />
                <div className="w-full flex justify-between text-xs px-2 text-base-content/50">
                  <span>1m</span>
                  <span>30m</span>
                  <span>60m</span>
                </div>
                <label className="label">
                  <span className="label-text-alt">Wait time after session ends before processing AI summaries (requires API key)</span>
                </label>
              </div>

              <div className="alert alert-info">
                <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                </svg>
                <div className="text-sm">
                  <p><strong>Core Metrics:</strong> Basic statistics (performance, usage, errors) processed locally without AI.</p>
                  <p className="mt-1"><strong>AI Processing:</strong> Advanced summaries and quality scores generated using your configured AI API.</p>
                </div>
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
                <span>{currentVersion}</span>
              </div>
              {hasUpdate && (
                <div className="flex justify-between items-center">
                  <span className="text-base-content/70">Latest Version</span>
                  <span className="text-warning font-medium">{latestVersion}</span>
                </div>
              )}
              <div className="flex justify-between">
                <span className="text-base-content/70">Build</span>
                <span>Desktop</span>
              </div>
              <div className="flex justify-between">
                <span className="text-base-content/70">Platform</span>
                <span>Tauri</span>
              </div>
            </div>

            {error && (
              <>
                <div className="divider"></div>
                <div className="alert alert-error">
                  <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                  </svg>
                  <div>
                    <h3 className="font-bold">Update Error</h3>
                    <div className="text-sm">{error}</div>
                  </div>
                </div>
              </>
            )}

            {isUpToDate && (
              <>
                <div className="divider"></div>
                <div className="alert alert-success">
                  <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
                  </svg>
                  <div>
                    <h3 className="font-bold">App is Up to Date</h3>
                    <div className="text-sm">You're running the latest version {currentVersion}</div>
                  </div>
                </div>
              </>
            )}

            {hasUpdate && (
              <>
                <div className="divider"></div>
                <div className="alert alert-success">
                  <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M7 16a4 4 0 01-.88-7.903A5 5 0 1115.9 6L16 6a5 5 0 011 9.9M15 13l-3-3m0 0l-3 3m3-3v12" />
                  </svg>
                  <div className="flex-1">
                    <h3 className="font-bold">Update Available</h3>
                    <div className="text-sm">Version {latestVersion} is now available. Click below to download and install.</div>
                  </div>
                </div>

                {isDownloading || isInstalling ? (
                  <div className="space-y-2">
                    <div className="flex justify-between text-sm">
                      <span>{isInstalling ? 'Installing...' : 'Downloading...'}</span>
                      <span>{downloadProgress}%</span>
                    </div>
                    <progress className="progress progress-success w-full" value={downloadProgress} max="100"></progress>
                  </div>
                ) : (
                  <button
                    onClick={downloadAndInstall}
                    className="btn btn-success btn-block"
                    disabled={isDownloading || isInstalling}
                  >
                    <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
                    </svg>
                    Download and Install Update
                  </button>
                )}
              </>
            )}

            {!hasUpdate && !isChecking && (
              <>
                <div className="divider"></div>
                <button
                  onClick={checkForUpdates}
                  className="btn btn-outline btn-block"
                  disabled={isChecking}
                >
                  {isChecking ? (
                    <>
                      <span className="loading loading-spinner loading-sm"></span>
                      Checking for updates...
                    </>
                  ) : (
                    <>
                      <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
                      </svg>
                      Check for Updates
                    </>
                  )}
                </button>
              </>
            )}
          </div>
        </div>
      </div>
    </div>
  )
}

export default SettingsPage