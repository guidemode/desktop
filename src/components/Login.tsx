import React, { useState } from 'react'
import { Cog6ToothIcon } from '@heroicons/react/24/outline'
import { useAuth } from '../hooks/useAuth'

const DEFAULT_SERVER_URL = import.meta.env.VITE_SERVER_URL || (import.meta.env.MODE === 'production' ? 'https://guideai.dev' : 'http://localhost:3000')

export default function Login() {
  const [serverUrl, setServerUrl] = useState(DEFAULT_SERVER_URL)
  const [showServerUrl, setShowServerUrl] = useState(false)
  const { login, isLoggingIn } = useAuth()

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    login(serverUrl)
  }

  return (
    <div className="card bg-base-200 shadow-lg">
      <div className="card-body">
        {/* GuideAI Header */}
        <div className="flex flex-col items-center mb-6">
          <div className="flex items-center gap-3 mb-2">
            <div className="avatar">
              <div className="w-8 rounded">
                <img src="/logo-32-optimized.png" alt="GuideAI" className="w-full h-full object-contain" />
              </div>
            </div>
            <div>
              <h1 className="text-xl font-bold text-primary">GuideAI</h1>
              <p className="text-sm text-base-content/70">Desktop Manager</p>
            </div>
          </div>
          <h2 className="text-base font-medium text-base-content/80">Sign In to Continue</h2>
        </div>

        <form onSubmit={handleSubmit} className="space-y-3">
          {showServerUrl && (
            <div className="form-control">
              <label className="label">
                <span className="label-text">Server URL</span>
              </label>
              <input
                type="url"
                className="input input-bordered input-sm"
                value={serverUrl}
                onChange={(e) => setServerUrl(e.target.value)}
                placeholder="https://api.guideai.com"
                disabled={isLoggingIn}
              />
            </div>
          )}

          <div className="card-actions justify-center">
            <button
              type="submit"
              className="btn btn-primary"
              disabled={isLoggingIn || !serverUrl}
            >
              {isLoggingIn ? (
                <>
                  <span className="loading loading-spinner loading-sm"></span>
                  Signing In...
                </>
              ) : (
                'Sign In with GitHub'
              )}
            </button>
          </div>
        </form>

        <div className="text-center mt-2">
          <p className="text-sm text-base-content/70">
            This will open your browser to complete the OAuth flow
          </p>
        </div>

        {/* Settings Icon */}
        <div className="absolute top-4 right-4">
          <button
            type="button"
            onClick={() => setShowServerUrl(!showServerUrl)}
            className="btn btn-ghost btn-sm btn-circle"
            title="Settings"
            disabled={isLoggingIn}
          >
            <Cog6ToothIcon className="w-5 h-5" />
          </button>
        </div>
      </div>
    </div>
  )
}