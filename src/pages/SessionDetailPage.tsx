import { useParams, useNavigate } from 'react-router-dom'
import { useState, useEffect } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-shell'
import { useAuth } from '../hooks/useAuth'
import { useToast } from '../hooks/useToast'
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
  RatingBadge,
  PhaseTimeline,
  SessionDetailHeader,
  type SessionPhaseAnalysis,
} from '@guideai-dev/session-processing/ui'
import type { SessionRating } from '@guideai-dev/session-processing/ui'
import ProviderIcon from '../components/icons/ProviderIcon'
import { useQuickRating } from '../hooks/useQuickRating'
import {
  ClockIcon,
  ChartBarIcon,
  ArrowUpIcon,
  ArrowDownIcon,
  Cog6ToothIcon,
  FolderIcon,
  ChatBubbleLeftRightIcon,
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
  ai_model_phase_analysis: string | null
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
    sql: `SELECT s.*, a.rating as assessment_rating
          FROM agent_sessions s
          LEFT JOIN session_assessments a ON s.session_id = a.session_id
          WHERE s.session_id = ? LIMIT 1`,
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
  const [reverseOrder, setReverseOrder] = useState(() => {
    const saved = localStorage.getItem('sessionMessageOrder')
    return saved === 'newest-first'
  })
  const [showSettings, setShowSettings] = useState(false)
  const [showMetaMessages, setShowMetaMessages] = useState(false)
  const [processingAi, setProcessingAi] = useState(false)
  const { processSessionWithAi, hasApiKey } = useAiProcessing()
  const { processSession: processMetrics } = useSessionProcessing()
  const toast = useToast()
  const quickRatingMutation = useQuickRating()

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

  // Parse phase analysis if available
  const phaseAnalysis: SessionPhaseAnalysis | null = session?.ai_model_phase_analysis
    ? JSON.parse(session.ai_model_phase_analysis)
    : null

  // Tab state - default to transcript
  const [activeTab, setActiveTab] = useState<'phase-timeline' | 'transcript' | 'metrics'>('transcript')

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

      toast.success('Session queued for upload. Check the Upload Queue page for status.')
    } catch (err) {
      console.error('Failed to queue session for upload:', err)
      toast.error('Failed to queue session: ' + (err as Error).message)
    }
  }

  // Handle quick rating
  const handleQuickRate = async (rating: SessionRating) => {
    if (!sessionId) return

    try {
      await quickRatingMutation.mutateAsync({ sessionId, rating })
      // Invalidate session metadata to refresh rating display
      queryClient.invalidateQueries({ queryKey: ['session-metadata', sessionId] })
      toast.success('Rating saved!')
    } catch (err) {
      console.error('Failed to rate session:', err)
      toast.error('Failed to save rating: ' + (err as Error).message)
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
        toast.success('Processing complete! Metrics calculated and AI summary generated.')
      } else {
        toast.success('Metrics calculated! AI processing skipped (no API key configured).')
      }
    } catch (err) {
      console.error('Failed to process:', err)
      toast.error('Failed to process: ' + (err as Error).message)
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

  // Handler for opening folder in OS
  const handleCwdClick = async (path: string) => {
    try {
      await invoke('open_folder_in_os', { path })
    } catch (err) {
      console.error('Failed to open folder:', err)
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

  const messages = timeline?.items.filter(item => !isTimelineGroup(item)) || []
  const messageCount = messages.length

  return (
    <div className="space-y-4">
      {/* Page Header */}
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

      {/* Session Detail Header - Shared across all tabs */}
      {session && (
        <SessionDetailHeader
          session={{
            provider: session.provider,
            projectName: session.project_name,
            sessionStartTime: session.session_start_time ? new Date(session.session_start_time).toISOString() : null,
            durationMs: session.duration_ms,
            fileSize: session.file_size,
            cwd: session.cwd || undefined,
            project: project ? {
              name: project.name,
              gitRemoteUrl: project.github_repo || undefined,
              cwd: undefined,
            } : undefined,
          }}
          messageCount={messageCount}
          rating={((session as any).assessment_rating as SessionRating) || null}
          onRate={handleQuickRate}
          onProcessSession={handleProcessWithAi}
          processingStatus={(session as any).ai_model_summary ? 'completed' : 'pending'}
          isProcessing={processingAi}
          onCwdClick={session.cwd ? handleCwdClick : undefined}
          syncStatus={{
            synced: session.synced_to_server === 1,
            failed: !!session.sync_failed_reason,
            reason: session.sync_failed_reason || undefined,
            onSync: handleSyncSession,
            onShowError: (error) => toast.error(error, 10000),
          }}
          ProviderIcon={ProviderIcon}
        />
      )}

      {/* Tabs Navigation */}
      <div className="tabs tabs-bordered">
        <button
          className={`tab tab-lg gap-2 ${
            activeTab === 'transcript'
              ? 'tab-active bg-base-100 text-primary font-semibold border-b-2 border-primary'
              : 'bg-base-200 hover:bg-base-300'
          }`}
          onClick={() => setActiveTab('transcript')}
          title="Transcript"
        >
          <ChatBubbleLeftRightIcon className="w-5 h-5" />
          <span className="hidden md:inline">Transcript</span>
        </button>
        {phaseAnalysis && (
          <button
            className={`tab tab-lg gap-2 ${
              activeTab === 'phase-timeline'
                ? 'tab-active bg-base-100 text-primary font-semibold border-b-2 border-primary'
                : 'bg-base-200 hover:bg-base-300'
            }`}
            onClick={() => setActiveTab('phase-timeline')}
            title="Timeline"
          >
            <ClockIcon className="w-5 h-5" />
            <span className="hidden md:inline">Timeline</span>
          </button>
        )}
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

      {/* Tab Content */}
      <div>
        {activeTab === 'transcript' && (
          <div className="space-y-4">
            {/* Transcript Controls */}
            <div className="flex items-center justify-between gap-2">
              <div className="flex items-center gap-2">
                <button
                  onClick={() => setReverseOrder(!reverseOrder)}
                  className={`btn btn-sm gap-1.5 ${reverseOrder ? 'btn-primary' : 'btn-ghost'}`}
                >
                  {reverseOrder ? <ArrowUpIcon className="w-3.5 h-3.5" /> : <ArrowDownIcon className="w-3.5 h-3.5" />}
                  <span className="text-xs">{reverseOrder ? 'Newest First' : 'Oldest First'}</span>
                </button>
                <button
                  onClick={() => setShowSettings(!showSettings)}
                  className={`btn btn-sm gap-1.5 ${showSettings ? 'btn-primary' : 'btn-ghost'}`}
                >
                  <Cog6ToothIcon className="w-3.5 h-3.5" />
                  <span className="text-xs">Settings</span>
                </button>
              </div>
            </div>

            {/* Settings Dropdown */}
            {showSettings && (
              <>
                <div className="fixed inset-0 z-10" onClick={() => setShowSettings(false)} />
                <div className="absolute right-0 md:right-4 top-auto mt-2 w-full md:w-80 bg-base-100 border border-base-300 rounded-lg shadow-lg z-20 p-4">
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

            {/* Timeline Messages */}
            {contentLoading ? (
              <div className="flex items-center justify-center h-64">
                <span className="loading loading-spinner loading-lg"></span>
              </div>
            ) : contentError ? (
              <div className="alert alert-error">
                <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                </svg>
                <span>Failed to load session content: {contentError}</span>
              </div>
            ) : (
              <div className="overflow-auto">
                {renderTimeline()}
              </div>
            )}
          </div>
        )}
        {activeTab === 'phase-timeline' && phaseAnalysis && (
          <PhaseTimeline phaseAnalysis={phaseAnalysis} />
        )}
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
