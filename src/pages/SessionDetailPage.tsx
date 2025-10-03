import { useParams, useNavigate } from 'react-router-dom'
import { useState, useEffect } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-shell'
import { useAuth } from '../hooks/useAuth'
import ProviderIcon from '../components/icons/ProviderIcon'
import { useLocalSessionContent } from '../hooks/useLocalSessionContent'
import { useLocalSessionMetrics } from '../hooks/useLocalSessionMetrics'
import { useAiProcessing } from '../hooks/useAiProcessing'
import { useSessionProcessing } from '../hooks/useSessionProcessing'
import { useSessionActivity } from '../hooks/useSessionActivity'
import { useSessionActivityStore } from '../stores/sessionActivityStore'
import {
  TimelineMessage,
  TimelineGroup,
  isTimelineGroup,
  MetricsOverview,
} from '@guideai-dev/session-processing/ui'
import {
  ClockIcon,
  ChartBarIcon,
  ArrowUpIcon,
  ArrowDownIcon,
  Cog6ToothIcon,
  FolderIcon,
} from '@heroicons/react/24/outline'

interface AgentSession {
  id: string
  provider: string
  project_name: string
  session_id: string
  file_name: string
  file_path: string
  file_size: number
  session_start_time: number | null
  session_end_time: number | null
  duration_ms: number | null
  processing_status: string
  synced_to_server: number
  synced_at: number | null
  server_session_id: string | null
  created_at: number
  uploaded_at: number | null
  cwd: string | null
  sync_failed_reason: string | null
}

interface LocalProject {
  id: string
  name: string
  github_repo: string | null
  cwd: string
  type: string
}

// Fetch function for session metadata
async function fetchSessionMetadata(sessionId: string): Promise<AgentSession | null> {
  const result = await invoke<any[]>('execute_sql', {
    sql: 'SELECT * FROM agent_sessions WHERE session_id = ? LIMIT 1',
    params: [sessionId],
  })

  if (result.length === 0) {
    throw new Error('Session not found')
  }

  return result[0]
}

// Fetch project for session
async function fetchSessionProject(sessionId: string): Promise<LocalProject | null> {
  const result = await invoke<any[]>('execute_sql', {
    sql: `SELECT p.* FROM projects p
          JOIN agent_sessions s ON p.id = s.project_id
          WHERE s.session_id = ? LIMIT 1`,
    params: [sessionId],
  })

  if (result.length === 0) {
    return null
  }

  return result[0]
}

export default function SessionDetailPage() {
  const { sessionId } = useParams<{ sessionId: string }>()
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const { user } = useAuth()
  const [activeTab, setActiveTab] = useState<'timeline' | 'metrics'>('timeline')
  const [reverseOrder, setReverseOrder] = useState(() => {
    const saved = localStorage.getItem('sessionMessageOrder')
    return saved === 'newest-first'
  })
  const [showSettings, setShowSettings] = useState(false)
  const [showMetaMessages, setShowMetaMessages] = useState(false)
  const [processingAi, setProcessingAi] = useState(false)
  const { processSessionWithAi, hasApiKey } = useAiProcessing()
  const { processSession: processMetrics } = useSessionProcessing()

  // Track session activity from file watchers
  useSessionActivity()
  const isSessionActive = useSessionActivityStore(state => state.isSessionActive)

  // Fetch session metadata with TanStack Query
  const { data: session, isLoading: loading, error } = useQuery({
    queryKey: ['session-metadata', sessionId],
    queryFn: () => fetchSessionMetadata(sessionId!),
    enabled: !!sessionId,
  })

  // Fetch project for session
  const { data: project } = useQuery({
    queryKey: ['session-project', sessionId],
    queryFn: () => fetchSessionProject(sessionId!),
    enabled: !!sessionId,
  })

  // Handle sync session click
  const handleSyncSession = async () => {
    if (!user) {
      navigate('/')
      return
    }

    if (!sessionId) return

    try {
      // Clear the sync_failed_reason to allow retry
      await invoke('execute_sql', {
        sql: 'UPDATE agent_sessions SET sync_failed_reason = NULL WHERE session_id = ?',
        params: [sessionId],
      })

      alert('Session queued for upload. Check the Upload Queue page for status.')
    } catch (err) {
      console.error('Failed to queue session for upload:', err)
      alert('Failed to queue session: ' + (err as Error).message)
    }
  }

  // Save message order preference to localStorage
  useEffect(() => {
    localStorage.setItem('sessionMessageOrder', reverseOrder ? 'newest-first' : 'oldest-first')
  }, [reverseOrder])

  // Listen for sync events and invalidate queries
  useEffect(() => {
    if (!sessionId) return

    const invalidateSessionData = () => {
      // Invalidate both metadata and content
      queryClient.invalidateQueries({ queryKey: ['session-metadata', sessionId] })
      queryClient.invalidateQueries({ queryKey: ['session-content', sessionId] })
    }

    const unlistenSynced = listen('session-synced', (event) => {
      if (event.payload === sessionId) {
        console.log('[SessionDetailPage] Session synced event received, invalidating queries...')
        invalidateSessionData()
      }
    })

    const unlistenFailed = listen('session-sync-failed', (event) => {
      if (event.payload === sessionId) {
        console.log('[SessionDetailPage] Session sync failed event received, invalidating queries...')
        invalidateSessionData()
      }
    })

    const unlistenUpdated = listen('session-updated', (event) => {
      if (event.payload === sessionId) {
        console.log('[SessionDetailPage] Session updated event received, invalidating queries...')
        invalidateSessionData()
      }
    })

    return () => {
      unlistenSynced.then(fn => fn())
      unlistenFailed.then(fn => fn())
      unlistenUpdated.then(fn => fn())
    }
  }, [sessionId, queryClient])

  // Load session content and parse into timeline
  const {
    timeline,
    loading: contentLoading,
    error: contentError,
  } = useLocalSessionContent(
    session?.session_id,
    session?.provider,
    session?.file_path
  )

  // Load local metrics if available
  const {
    metrics,
    loading: metricsLoading,
  } = useLocalSessionMetrics(session?.session_id)

  // Handle AI processing
  const handleProcessWithAi = async () => {
    if (!session || !timeline) return

    setProcessingAi(true)
    try {
      // Parse session content
      const { ProcessorRegistry } = await import('@guideai-dev/session-processing/processors')
      const registry = new ProcessorRegistry()
      const processor = registry.getProcessor(session.provider)

      if (!processor) {
        throw new Error(`No processor found for provider: ${session.provider}`)
      }

      const content = await invoke<string>('get_session_content', {
        provider: session.provider,
        filePath: (session as any).file_path,
        sessionId: session.session_id,
      })
      const parsedSession = processor.parseSession(content)

      // Step 1: Calculate metrics (always)
      console.log('[Processing] Calculating metrics...')
      await processMetrics(session.session_id, session.provider, content, 'local')

      // Step 2: Process with AI if API key available
      if (hasApiKey()) {
        console.log('[Processing] Running AI processing...')
        await processSessionWithAi(session.session_id, parsedSession)
      } else {
        console.log('[Processing] Skipping AI processing - no API key configured')
      }

      // Reload session to show AI results
      await invoke<any[]>('execute_sql', {
        sql: 'SELECT * FROM agent_sessions WHERE session_id = ? LIMIT 1',
        params: [sessionId],
      })

      // Invalidate all relevant queries to refresh the UI
      await queryClient.invalidateQueries({ queryKey: ['session-metadata', sessionId] })
      await queryClient.invalidateQueries({ queryKey: ['session-content', sessionId] })
      await queryClient.invalidateQueries({ queryKey: ['session-metrics', sessionId] })
      await queryClient.invalidateQueries({ queryKey: ['local-sessions'] })

      if (hasApiKey()) {
        alert('✓ Processing complete! Metrics calculated and AI summary generated.')
      } else {
        alert('✓ Metrics calculated! AI processing skipped (no API key configured).')
      }
    } catch (err) {
      console.error('Failed to process:', err)
      alert('Failed to process: ' + (err as Error).message)
    } finally {
      setProcessingAi(false)
    }
  }

  const formatDate = (timestamp: number | null) => {
    if (!timestamp) return 'N/A'
    return new Date(timestamp).toLocaleString()
  }

  const formatFileSize = (bytes: number) => {
    if (bytes === 0) return '0 B'
    const k = 1024
    const sizes = ['B', 'KB', 'MB', 'GB']
    const i = Math.floor(Math.log(bytes) / Math.log(k))
    return `${parseFloat((bytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`
  }

  const formatDuration = (durationMs: number | null) => {
    if (!durationMs) return 'N/A'
    const seconds = Math.floor(durationMs / 1000)
    const minutes = Math.floor(seconds / 60)
    const hours = Math.floor(minutes / 60)

    if (hours > 0) {
      return `${hours}h ${minutes % 60}m ${seconds % 60}s`
    } else if (minutes > 0) {
      return `${minutes}m ${seconds % 60}s`
    } else {
      return `${seconds}s`
    }
  }

  const renderTimeline = () => {
    if (!timeline) return null

    // Filter out meta messages if setting is disabled
    let filteredItems = timeline.items
    if (!showMetaMessages) {
      filteredItems = filteredItems.filter((item) => {
        if (isTimelineGroup(item)) {
          // Keep group if neither message is meta
          return item.messages.every(msg => msg.originalMessage.type !== 'meta')
        } else {
          // Keep single message if not meta
          return item.originalMessage.type !== 'meta'
        }
      })
    }

    // Apply reverse order if requested
    const orderedItems = reverseOrder
      ? [...filteredItems].reverse()
      : filteredItems

    return (
      <div>
        {orderedItems.map((item) => {
          if (isTimelineGroup(item)) {
            return <TimelineGroup key={item.id} group={item} />
          } else {
            return <TimelineMessage key={item.id} message={item} />
          }
        })}
      </div>
    )
  }

  const renderTimelineTab = () => {
    if (contentLoading) {
      return (
        <div className="flex items-center justify-center h-full">
          <span className="loading loading-spinner loading-lg"></span>
        </div>
      )
    }

    if (contentError) {
      return (
        <div className="alert alert-error m-4">
          <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
          <span>Failed to load session content: {contentError}</span>
        </div>
      )
    }

    if (!timeline) return null

    const messages = timeline.items.filter(item => !isTimelineGroup(item))
    const messageCount = messages.length

    return (
      <div className="space-y-4">
        <div className="card bg-base-100 border border-base-300">
          <div className="card-body p-4">
            {/* Header with avatar, username, project, time on left and action buttons on right */}
            <div className="mb-2">
              <div className="flex flex-col md:flex-row md:items-start md:justify-between gap-2 mb-2">
                {/* Left: Project - Start Time - Sync Status */}
                {session && (
                  <div className="flex items-center gap-2 flex-wrap">
                    <span className="font-medium text-base md:text-lg">{session.project_name}</span>
                    <span className="text-base-content/50 text-sm">•</span>
                    <span className="text-sm text-base-content/70">{formatDate(session.session_start_time)}</span>

                    {/* Sync Status Icon */}
                    {session.sync_failed_reason ? (
                      <div
                        className="tooltip tooltip-bottom cursor-pointer hover:scale-110 transition-transform"
                        data-tip="Sync failed - Click to view error"
                        onClick={() => alert(session.sync_failed_reason)}
                      >
                        <svg className="w-4 h-4 text-error" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
                        </svg>
                      </div>
                    ) : session.synced_to_server === 1 ? (
                      <div className="tooltip tooltip-bottom" data-tip="Synced to server">
                        <svg className="w-4 h-4 text-success" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
                        </svg>
                      </div>
                    ) : (
                      <div
                        className="tooltip tooltip-bottom cursor-pointer hover:scale-110 transition-transform"
                        data-tip="Click to sync to server"
                        onClick={handleSyncSession}
                      >
                        <svg className="w-4 h-4 text-base-content/30" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M7 16a4 4 0 01-.88-7.903A5 5 0 1115.9 6L16 6a5 5 0 011 9.9M15 13l-3-3m0 0l-3 3m3-3v12" />
                        </svg>
                      </div>
                    )}
                  </div>
                )}

                {/* Right: Action Buttons (desktop only) */}
                <div className="hidden md:flex items-center gap-1.5 flex-shrink-0">
                  <button
                    onClick={() => setReverseOrder(!reverseOrder)}
                    className={`btn btn-xs gap-1.5 ${reverseOrder ? 'btn-primary' : 'btn-ghost'}`}
                  >
                    {reverseOrder ? <ArrowUpIcon className="w-3.5 h-3.5" /> : <ArrowDownIcon className="w-3.5 h-3.5" />}
                    <span className="hidden lg:inline text-xs">{reverseOrder ? 'Newest First' : 'Oldest First'}</span>
                  </button>
                  <button
                    onClick={() => setShowSettings(!showSettings)}
                    className={`btn btn-xs gap-1.5 ${showSettings ? 'btn-primary' : 'btn-ghost'}`}
                  >
                    <Cog6ToothIcon className="w-3.5 h-3.5" />
                    <span className="hidden lg:inline text-xs">Settings</span>
                  </button>
                </div>
              </div>

              {/* Mobile: Actions Sheet */}
              <div className="md:hidden mb-2 flex gap-1.5">
                <button
                  onClick={() => setReverseOrder(!reverseOrder)}
                  className={`btn btn-xs gap-1.5 ${reverseOrder ? 'btn-primary' : 'btn-ghost'}`}
                >
                  {reverseOrder ? <ArrowUpIcon className="w-3.5 h-3.5" /> : <ArrowDownIcon className="w-3.5 h-3.5" />}
                  <span className="text-xs">{reverseOrder ? 'Newest First' : 'Oldest First'}</span>
                </button>
                <button
                  onClick={() => setShowSettings(!showSettings)}
                  className={`btn btn-xs gap-1.5 ${showSettings ? 'btn-primary' : 'btn-ghost'}`}
                >
                  <Cog6ToothIcon className="w-3.5 h-3.5" />
                  <span className="text-xs">Settings</span>
                </button>
              </div>

              {/* Settings Dropdown */}
              {showSettings && (
                <>
                  <div className="fixed inset-0 z-10" onClick={() => setShowSettings(false)} />
                  <div className="absolute right-0 md:right-4 top-24 md:top-auto md:mt-2 w-full md:w-80 bg-base-100 border border-base-300 rounded-lg shadow-lg z-20 p-4">
                    <h3 className="text-sm font-semibold mb-3">Timeline Settings</h3>
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={showMetaMessages}
                        onChange={(e) => setShowMetaMessages(e.target.checked)}
                        className="checkbox checkbox-sm checkbox-primary"
                      />
                      <span className="text-sm">Show meta messages</span>
                    </label>
                    <p className="text-xs text-base-content/60 mt-2">
                      Meta messages are internal system messages that provide context but are not part of the main conversation.
                    </p>
                  </div>
                </>
              )}
            </div>

            {/* Session Stats Grid - Responsive */}
            {session && (
              <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-2">
                <div className="stat bg-base-200 rounded-lg p-2.5">
                  <div className="stat-title text-xs">Provider</div>
                  <div className="stat-value text-sm flex items-center gap-1.5">
                    <ProviderIcon providerId={session.provider} size={16} />
                    {session.provider}
                  </div>
                </div>
                <div className="stat bg-base-200 rounded-lg p-2.5">
                  <div className="stat-title text-xs">Duration</div>
                  <div className="stat-value text-sm">{formatDuration(session.duration_ms)}</div>
                </div>
                <div className="stat bg-base-200 rounded-lg p-2.5">
                  <div className="stat-title text-xs">Messages</div>
                  <div className="stat-value text-sm">{messageCount}</div>
                </div>
                <div className="stat bg-base-200 rounded-lg p-2.5">
                  <div className="stat-title text-xs">Size</div>
                  <div className="stat-value text-sm">{formatFileSize(session.file_size)}</div>
                </div>
              </div>
            )}

            {/* Project and Working Directory - 2 columns below stats */}
            {(project || session?.cwd) && (
              <div className="mt-2 grid grid-cols-1 lg:grid-cols-2 gap-2">
                {/* Project Info */}
                {project && (
                  <div className="stat bg-base-200 rounded-lg p-2.5">
                    <div className="stat-title text-xs mb-1">Project</div>
                    <div className="flex items-center justify-between gap-2">
                      <div className="stat-value text-sm">
                        {project.name}
                      </div>
                      {project.github_repo && (
                        <button
                          type="button"
                          onClick={(e) => {
                            e.preventDefault()
                            e.stopPropagation()
                            open(project.github_repo!.replace(/\.git$/, ''))
                          }}
                          className="flex items-center gap-1.5 text-xs text-base-content/60 hover:text-primary transition-colors flex-shrink-0"
                          title={project.github_repo.replace(/^https?:\/\/github\.com\//, '').replace(/\.git$/, '')}
                        >
                          <svg className="w-3 h-3" fill="currentColor" viewBox="0 0 24 24">
                            <path
                              fillRule="evenodd"
                              d="M12 0C5.374 0 0 5.373 0 12c0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23A11.509 11.509 0 0112 5.803c1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576C20.566 21.797 24 17.3 24 12c0-6.627-5.373-12-12-12z"
                              clipRule="evenodd"
                            />
                          </svg>
                          <span className="hidden sm:inline truncate max-w-[150px]">
                            {project.github_repo.replace(/^https?:\/\/github\.com\//, '').replace(/\.git$/, '')}
                          </span>
                        </button>
                      )}
                    </div>
                  </div>
                )}

                {/* Working Directory */}
                {session?.cwd && (
                  <div className="stat bg-base-200 rounded-lg p-2.5">
                    <div className="stat-title text-xs flex items-center gap-1.5">
                      <FolderIcon className="w-3.5 h-3.5" />
                      Working Directory
                    </div>
                    <div className="stat-value text-sm mt-1">
                      <button
                        onClick={async () => {
                          try {
                            await invoke('open_folder_in_os', { path: session.cwd })
                          } catch (err) {
                            console.error('Failed to open folder:', err)
                          }
                        }}
                        className="text-left hover:text-primary transition-colors font-mono break-all"
                        title="Click to open in Finder"
                      >
                        {session.cwd}
                      </button>
                    </div>
                  </div>
                )}
              </div>
            )}
          </div>
        </div>

        <div className="overflow-auto">
          {renderTimeline()}
        </div>
      </div>
    )
  }

  if (loading) {
    return (
      <div className="flex items-center justify-center min-h-[400px]">
        <span className="loading loading-spinner loading-lg" />
      </div>
    )
  }

  if (error || !session) {
    return (
      <div className="space-y-6">
        <button onClick={() => navigate('/sessions')} className="btn btn-sm btn-ghost">
          ← Back to Sessions
        </button>
        <div className="alert alert-error">
          <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
            />
          </svg>
          <span>{error ? (error as Error).message : 'Session not found'}</span>
        </div>
      </div>
    )
  }

  return (
    <div className="space-y-4">
      {/* Header */}
      <div>
        <div className="flex items-start justify-between">
          <div>
            <div className="flex items-center gap-3">
              <h1 className="text-2xl font-bold">Session Detail</h1>
              {sessionId && isSessionActive(sessionId) && (
                <span className="badge badge-success gap-1.5 animate-pulse">
                  <span className="relative flex h-2 w-2 items-center justify-center">
                    <span className="animate-ping absolute h-full w-full rounded-full bg-white opacity-75"></span>
                    <span className="relative rounded-full h-2 w-2 bg-white"></span>
                  </span>
                  <span>LIVE</span>
                </span>
              )}
            </div>
            <button
              onClick={() => navigate('/sessions')}
              className="btn btn-sm btn-ghost mt-3 pl-0"
            >
              <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 19l-7-7 7-7" />
              </svg>
              Back to Sessions
            </button>
          </div>

          {/* Tabs Navigation - Top Right */}
          <div className="tabs tabs-bordered">
            <button
              className={`tab tab-lg gap-2 ${
                activeTab === 'timeline'
                  ? 'tab-active bg-base-100 text-primary font-semibold border-b-2 border-primary'
                  : 'bg-base-200 hover:bg-base-300'
              }`}
              onClick={() => setActiveTab('timeline')}
              title="Timeline"
            >
              <ClockIcon className="w-5 h-5" />
              <span className="hidden md:inline">Timeline</span>
            </button>
            <button
              className={`tab tab-lg gap-2 ${
                activeTab === 'metrics'
                  ? 'tab-active bg-base-100 text-primary font-semibold border-b-2 border-primary'
                  : 'bg-base-200 hover:bg-base-300'
              }`}
              onClick={() => setActiveTab('metrics')}
              title="Metrics"
            >
              <ChartBarIcon className="w-5 h-5" />
              <span className="hidden md:inline">Metrics</span>
            </button>
          </div>
        </div>
      </div>

      {/* Content Area */}
      <div>
        {activeTab === 'timeline' && renderTimelineTab()}
        {activeTab === 'metrics' && (
          <MetricsOverview
            sessionId={session.session_id}
            metrics={metrics}
            isLoading={metricsLoading}
            error={null}
            onProcessSession={
              (session as any).ai_model_summary || (session as any).ai_model_quality_score
                ? undefined
                : handleProcessWithAi
            }
            isProcessing={processingAi}
            aiModelSummary={(session as any).ai_model_summary}
            aiModelQualityScore={(session as any).ai_model_quality_score}
            aiModelMetadata={
              (session as any).ai_model_metadata
                ? JSON.parse((session as any).ai_model_metadata)
                : undefined
            }
          />
        )}
      </div>
    </div>
  )
}
