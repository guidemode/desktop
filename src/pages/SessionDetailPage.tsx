import {
  MetricsOverview,
  PhaseTimeline,
  SessionDetailHeader,
  type SessionPhaseAnalysis,
  VirtualizedMessageList,
  isTimelineGroup,
} from '@guideai-dev/session-processing/ui'
import type { SessionRating } from '@guideai-dev/session-processing/ui'
import {
  ArrowDownIcon,
  ArrowUpIcon,
  ChartBarIcon,
  ChatBubbleLeftRightIcon,
  ClockIcon,
  Cog6ToothIcon,
  DocumentTextIcon,
} from '@heroicons/react/24/outline'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { useEffect, useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { AiProcessingProgress } from '../components/AiProcessingProgress'
import { SessionChangesTab } from '../components/SessionChangesTab'
import { SessionContextTab } from '../components/SessionContextTab'
import ProviderIcon from '../components/icons/ProviderIcon'
import { useAiProcessing } from '../hooks/useAiProcessing'
import { useAiProcessingProgress } from '../hooks/useAiProcessingProgress'
import { useAuth } from '../hooks/useAuth'
import { useLocalSessionContent } from '../hooks/useLocalSessionContent'
import { useLocalSessionMetrics } from '../hooks/useLocalSessionMetrics'
import { useQuickRating } from '../hooks/useQuickRating'
import { useSessionActivity } from '../hooks/useSessionActivity'
import { useSessionProcessing } from '../hooks/useSessionProcessing'
import { useToast } from '../hooks/useToast'
import { useSessionActivityStore } from '../stores/sessionActivityStore'

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
  git_branch: string | null
  first_commit_hash: string | null
  latest_commit_hash: string | null
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
  const { progress, updateProgress, reset: resetProgress } = useAiProcessingProgress()
  const toast = useToast()
  const quickRatingMutation = useQuickRating()
  const [hasPendingChanges, setHasPendingChanges] = useState(false)

  // Track session activity from file watchers
  useSessionActivity()
  const isSessionActive = useSessionActivityStore(state => state.isSessionActive)

  // Tab state - default to transcript
  const [activeTab, setActiveTab] = useState<
    'phase-timeline' | 'transcript' | 'metrics' | 'changes' | 'context'
  >('transcript')

  // Fetch session metadata with TanStack Query
  const {
    data: session,
    isLoading: loading,
    error,
  } = useQuery({
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

  // Helper to check if we should show unstaged changes
  // Shows unstaged if: no commits during session AND session ended within 48 hours
  const shouldShowUnstaged = (session: AgentSession | null | undefined): boolean => {
    if (!session) return false

    const noCommitsDuringSession = session.first_commit_hash === session.latest_commit_hash
    if (!noCommitsDuringSession) return false

    if (!session.session_end_time) return false

    const now = Date.now()
    const timeSinceEnd = now - session.session_end_time
    const fortyEightHours = 48 * 60 * 60 * 1000

    return timeSinceEnd < fortyEightHours && timeSinceEnd >= 0
  }

  // Fetch git diff stats for tab badge (only when Changes tab is NOT active)
  const { data: gitDiffStats } = useQuery({
    queryKey: ['session-git-diff-stats', sessionId, shouldShowUnstaged(session)],
    queryFn: async () => {
      if (!session?.cwd || !session?.first_commit_hash) {
        return null
      }
      const diffs = await invoke<any[]>('get_session_git_diff', {
        cwd: session.cwd,
        firstCommitHash: session.first_commit_hash,
        latestCommitHash: session.latest_commit_hash || null,
        isActive: shouldShowUnstaged(session),
      })
      const stats = diffs.reduce(
        (acc: { additions: number; deletions: number }, file: any) => ({
          additions: acc.additions + (file.stats?.additions || 0),
          deletions: acc.deletions + (file.stats?.deletions || 0),
        }),
        { additions: 0, deletions: 0 }
      )
      return stats
    },
    enabled: !!session?.cwd && !!session?.first_commit_hash && activeTab !== 'changes',
  })

  // Fetch context file stats for tab badge (only when Context tab is NOT active)
  const { data: contextStats } = useQuery({
    queryKey: ['session-context-stats', sessionId, session?.cwd],
    queryFn: async () => {
      if (!session?.cwd) {
        return null
      }
      const files = await invoke<any[]>('scan_context_files', { cwd: session.cwd })
      const totalSize = files.reduce((sum: number, file: any) => sum + (file.size || 0), 0)
      return {
        fileCount: files.length,
        totalSize,
      }
    },
    enabled: !!session?.cwd && activeTab !== 'context',
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

      toast.success('Session queued for upload. Check the Upload Queue page for status.')
    } catch (err) {
      console.error('Failed to queue session for upload:', err)
      toast.error(`Failed to queue session: ${(err as Error).message}`)
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
      toast.error(`Failed to save rating: ${(err as Error).message}`)
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
      // Invalidate metadata and content
      queryClient.invalidateQueries({ queryKey: ['session-metadata', sessionId] })
      queryClient.invalidateQueries({ queryKey: ['session-content', sessionId] })

      // For git diff changes, set pending flag instead of invalidating
      // This prevents the Changes tab from refreshing and losing scroll/expansion state
      setHasPendingChanges(true)

      // Still invalidate git diff stats for the tab badge
      queryClient.invalidateQueries({ queryKey: ['session-git-diff-stats', sessionId] })
    }

    let unlistenSynced: (() => void) | undefined
    let unlistenFailed: (() => void) | undefined
    let unlistenUpdated: (() => void) | undefined

    listen('session-synced', event => {
      if (event.payload === sessionId) {
        invalidateSessionData()
      }
    }).then(fn => {
      unlistenSynced = fn
    })

    listen('session-sync-failed', event => {
      if (event.payload === sessionId) {
        invalidateSessionData()
      }
    }).then(fn => {
      unlistenFailed = fn
    })

    listen('session-updated', event => {
      if (event.payload === sessionId) {
        invalidateSessionData()
      }
    }).then(fn => {
      unlistenUpdated = fn
    })

    return () => {
      unlistenSynced?.()
      unlistenFailed?.()
      unlistenUpdated?.()
    }
  }, [sessionId, queryClient])

  // Load session content and parse into timeline
  const {
    timeline,
    fileContent,
    loading: contentLoading,
    error: contentError,
  } = useLocalSessionContent(session?.session_id, session?.provider, session?.file_path)

  // Load local metrics if available
  const { metrics, loading: metricsLoading } = useLocalSessionMetrics(session?.session_id)

  // Handle AI processing
  const handleProcessWithAi = async () => {
    if (!session || !timeline) return

    setProcessingAi(true)
    resetProgress()

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
      const parsedSession = processor.parseSession(content, session.provider)

      // Step 1: Calculate metrics (always)
      updateProgress({
        name: 'Calculating Metrics',
        description: 'Analyzing session performance and quality metrics',
        percentage: 0,
      })
      await processMetrics(session.session_id, session.provider, content, 'local')

      // Step 2: Process with AI if API key available
      if (hasApiKey()) {
        await processSessionWithAi(session.session_id, parsedSession, updateProgress)
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
      toast.error(`Failed to process: ${(err as Error).message}`)
    } finally {
      setProcessingAi(false)
      resetProgress()
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

  // Handler for viewing diff (opens Session Changes tab)
  const handleViewDiff = () => {
    setActiveTab('changes')
  }

  // Handler to clear pending changes flag
  const handleClearPendingChanges = () => {
    setHasPendingChanges(false)
  }

  // Clear pending changes when switching to Changes tab
  useEffect(() => {
    if (activeTab === 'changes' && hasPendingChanges) {
      setHasPendingChanges(false)
    }
  }, [activeTab, hasPendingChanges])

  // Format file size helper
  const formatSize = (bytes: number): string => {
    if (bytes < 1024) return `${bytes} B`
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
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

  // Filter out meta messages if setting is disabled
  let filteredItems = timeline?.items || []
  if (!showMetaMessages && timeline) {
    filteredItems = filteredItems.filter(item => {
      if (isTimelineGroup(item)) {
        // Keep group if neither message is meta
        return item.messages.every(msg => msg.originalMessage.type !== 'meta')
      }
      // Keep single message if not meta
      return item.originalMessage.type !== 'meta'
    })
  }

  // Apply reverse order if requested
  const orderedItems = reverseOrder ? [...filteredItems].reverse() : filteredItems

  return (
    <div className="space-y-4">
      {/* Page Header */}
      <div className="flex items-center justify-between gap-4">
        <div className="flex items-center gap-3">
          <h1 className="text-2xl font-bold">Session Detail</h1>
          {sessionId &&
            session?.session_end_time &&
            isSessionActive(sessionId, new Date(session.session_end_time).toISOString()) && (
              <span className="badge badge-success gap-1.5 animate-pulse">
                <span className="relative flex h-2 w-2 items-center justify-center">
                  <span className="animate-ping absolute h-full w-full rounded-full bg-white opacity-75" />
                  <span className="relative rounded-full h-2 w-2 bg-white" />
                </span>
                <span>LIVE</span>
              </span>
            )}
        </div>
        <button onClick={() => navigate('/sessions')} className="btn btn-sm btn-ghost">
          <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d="M15 19l-7-7 7-7"
            />
          </svg>
          Back to Sessions
        </button>
      </div>

      {/* Session Detail Header */}
      {session && (
        <>
          <SessionDetailHeader
            session={{
              provider: session.provider,
              projectName: session.project_name,
              sessionStartTime: session.session_start_time
                ? new Date(session.session_start_time).toISOString()
                : null,
              durationMs: session.duration_ms,
              fileSize: session.file_size,
              cwd: session.cwd || undefined,
              project: project
                ? {
                    name: project.name,
                    gitRemoteUrl: project.github_repo || undefined,
                    cwd: undefined,
                  }
                : undefined,
              aiModelSummary: (session as any).ai_model_summary || undefined,
              gitBranch: session.git_branch || undefined,
              firstCommitHash: session.first_commit_hash || undefined,
              latestCommitHash: session.latest_commit_hash || undefined,
            }}
            messageCount={messageCount}
            rating={((session as any).assessment_rating as SessionRating) || null}
            onRate={handleQuickRate}
            onProcessSession={handleProcessWithAi}
            processingStatus={(session as any).ai_model_summary ? 'completed' : 'pending'}
            isProcessing={processingAi}
            processingProgress={
              progress.currentStep
                ? {
                    stepName: progress.currentStep.name,
                    percentage: progress.currentStep.percentage,
                  }
                : null
            }
            onCwdClick={session.cwd ? handleCwdClick : undefined}
            onViewDiff={session.first_commit_hash ? handleViewDiff : undefined}
            syncStatus={{
              synced: session.synced_to_server === 1,
              failed: !!session.sync_failed_reason,
              reason: session.sync_failed_reason || undefined,
              onSync: handleSyncSession,
              onShowError: error => toast.error(error, 10000),
            }}
            ProviderIcon={ProviderIcon}
          />

          {/* AI Processing Progress */}
          {progress.currentStep && (
            <div className="card bg-base-100 border border-primary">
              <div className="card-body p-4">
                <AiProcessingProgress step={progress.currentStep} />
              </div>
            </div>
          )}
        </>
      )}

      {/* Tabs Navigation with Controls */}
      <div className="card bg-base-200 border border-base-300 border-b-2 rounded-lg overflow-hidden">
        <div className="flex items-stretch">
          {/* Left: Tab Buttons */}
          <div className="tabs tabs-bordered flex-1">
            <button
              className={`tab tab-lg gap-2 rounded-tl-lg ${
                activeTab === 'transcript'
                  ? 'tab-active bg-base-100 text-primary font-semibold border-b-2 border-primary'
                  : 'hover:bg-base-300'
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
                    : 'hover:bg-base-300'
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
                  : 'hover:bg-base-300'
              }`}
              onClick={() => setActiveTab('metrics')}
              title="Metrics"
            >
              <ChartBarIcon className="w-5 h-5" />
              <span className="hidden md:inline">Metrics</span>
            </button>
            {session.cwd && (
              <button
                className={`tab tab-lg gap-2 ${
                  activeTab === 'context'
                    ? 'tab-active bg-base-100 text-primary font-semibold border-b-2 border-primary'
                    : 'hover:bg-base-300'
                }`}
                onClick={() => setActiveTab('context')}
                title="Context"
              >
                <DocumentTextIcon className="w-5 h-5" />
                <span className="hidden md:inline">Context</span>
                {activeTab !== 'context' && contextStats && contextStats.fileCount > 0 && (
                  <span className="badge badge-info badge-sm">
                    {formatSize(contextStats.totalSize)}
                  </span>
                )}
              </button>
            )}
            {session.cwd && session.first_commit_hash && (
              <button
                className={`tab tab-lg gap-2 ${
                  activeTab === 'changes'
                    ? 'tab-active bg-base-100 text-primary font-semibold border-b-2 border-primary'
                    : hasPendingChanges
                      ? 'hover:bg-base-300 animate-pulse'
                      : 'hover:bg-base-300'
                }`}
                onClick={() => setActiveTab('changes')}
                title={hasPendingChanges ? 'New changes detected' : 'Changes'}
              >
                <svg
                  className={`w-5 h-5 ${hasPendingChanges && activeTab !== 'changes' ? 'text-primary' : ''}`}
                  fill="none"
                  stroke="currentColor"
                  viewBox="0 0 24 24"
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    strokeWidth={2}
                    d="M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4"
                  />
                </svg>
                <span className={`hidden md:inline ${hasPendingChanges && activeTab !== 'changes' ? 'text-primary' : ''}`}>
                  Changes
                </span>
                {activeTab !== 'changes' &&
                  gitDiffStats &&
                  (gitDiffStats.additions > 0 || gitDiffStats.deletions > 0) && (
                    <span className="flex items-center gap-1">
                      {gitDiffStats.additions > 0 && (
                        <span className="badge badge-success badge-sm">
                          {gitDiffStats.additions}
                        </span>
                      )}
                      {gitDiffStats.deletions > 0 && (
                        <span className="badge badge-error badge-sm">{gitDiffStats.deletions}</span>
                      )}
                    </span>
                  )}
                {hasPendingChanges && activeTab !== 'changes' && (
                  <span className="relative flex h-2 w-2">
                    <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-primary opacity-75" />
                    <span className="relative inline-flex rounded-full h-2 w-2 bg-primary" />
                  </span>
                )}
              </button>
            )}
          </div>

          {/* Right: Tab-specific Controls */}
          {activeTab === 'transcript' && (
            <div className="flex items-center gap-2 px-3 bg-base-100 border-l border-base-300 rounded-tr-lg">
              <button
                onClick={() => setReverseOrder(!reverseOrder)}
                className={`btn btn-xs gap-1.5 ${reverseOrder ? 'btn-primary' : 'btn-ghost'}`}
              >
                {reverseOrder ? (
                  <ArrowUpIcon className="w-3.5 h-3.5" />
                ) : (
                  <ArrowDownIcon className="w-3.5 h-3.5" />
                )}
                <span className="text-xs hidden lg:inline">
                  {reverseOrder ? 'Newest First' : 'Oldest First'}
                </span>
              </button>
              <div className="relative">
                <button
                  onClick={() => setShowSettings(!showSettings)}
                  className={`btn btn-xs gap-1.5 ${showSettings ? 'btn-primary' : 'btn-ghost'}`}
                >
                  <Cog6ToothIcon className="w-3.5 h-3.5" />
                  <span className="text-xs hidden lg:inline">Settings</span>
                </button>

                {/* Settings Dropdown */}
                {showSettings && (
                  <>
                    <div className="fixed inset-0 z-10" onClick={() => setShowSettings(false)} />
                    <div className="absolute right-0 top-full mt-2 w-80 bg-base-100 border border-base-300 rounded-lg shadow-lg z-20 p-4">
                      <h3 className="text-sm font-semibold mb-3">Timeline Settings</h3>
                      <label className="flex items-center gap-2 cursor-pointer">
                        <input
                          type="checkbox"
                          checked={showMetaMessages}
                          onChange={e => setShowMetaMessages(e.target.checked)}
                          className="checkbox checkbox-sm checkbox-primary"
                        />
                        <span className="text-sm">Show meta messages</span>
                      </label>
                      <p className="text-xs text-base-content/60 mt-2">
                        Meta messages are internal system messages that provide context but are not
                        part of the main conversation.
                      </p>
                    </div>
                  </>
                )}
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Tab Content */}
      <div>
        {activeTab === 'transcript' && (
          <>
            {/* Timeline Messages */}
            {contentLoading ? (
              <div className="flex items-center justify-center h-64">
                <span className="loading loading-spinner loading-lg" />
              </div>
            ) : contentError ? (
              <div className="alert alert-error">
                <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    strokeWidth={2}
                    d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
                  />
                </svg>
                <span>Failed to load session content: {contentError}</span>
              </div>
            ) : (
              <VirtualizedMessageList items={orderedItems} />
            )}
          </>
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
        {activeTab === 'changes' && session.cwd && session.first_commit_hash && (
          <SessionChangesTab
            session={{
              sessionId: session.session_id,
              cwd: session.cwd,
              first_commit_hash: session.first_commit_hash,
              latest_commit_hash: session.latest_commit_hash || null,
              session_start_time: session.session_start_time,
              session_end_time: session.session_end_time,
            }}
            hasPendingChanges={hasPendingChanges}
            onRefresh={handleClearPendingChanges}
          />
        )}
        {activeTab === 'context' && session.cwd && (
          <SessionContextTab
            session={{
              sessionId: session.session_id,
              cwd: session.cwd,
            }}
            fileContent={fileContent}
          />
        )}
      </div>
    </div>
  )
}
