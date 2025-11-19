import {
  MetricsOverview,
  PhaseTimeline,
  ScrollToTopButton,
  SessionDetailHeader,
  type SessionPhaseAnalysis,
  SessionTodosTab,
  TokenUsageChart,
  VirtualizedMessageList,
  extractTodosAuto,
  isTimelineGroup,
} from '@guidemode/session-processing/ui'
import type { SessionRating } from '@guidemode/session-processing/ui'
import {
  ArrowDownIcon,
  ArrowUpIcon,
  BugAntIcon,
  ChartBarIcon,
  ChatBubbleLeftRightIcon,
  CheckCircleIcon,
  ClipboardIcon,
  ClockIcon,
  Cog6ToothIcon,
  DocumentTextIcon,
} from '@heroicons/react/24/outline'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { useCallback, useEffect, useMemo, useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { JsonBlock } from '../components/JsonBlock'
import { SessionChangesTab } from '../components/SessionChangesTab'
import { SessionContextTab } from '../components/SessionContextTab'
import { ValidationReport } from '../components/ValidationReport'
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
import { useValidationStatus } from '../hooks/useValidationStatus'
import { useSessionActivityStore } from '../stores/sessionActivityStore'
import {
  type AgentSessionRow,
  type LocalProject,
  type ProjectRow,
  mapAgentSessionRow,
  mapProjectRow,
} from '../utils/dbMappers'

// SQL result types
interface AgentSessionWithRating extends AgentSessionRow {
  assessment_rating: string | null
  synced_to_server: number
  synced_at: number | null
  server_session_id: string | null
  uploaded_at: number | null
  sync_failed_reason: string | null
  ai_model_phase_analysis: string | null
  git_branch: string | null
  latest_commit_hash: string | null
  ai_model_summary: string | null
  ai_model_quality_score: number | null
  ai_model_metadata: string | null
}

interface ProjectRowExtended extends ProjectRow {
  github_repo: string | null
  cwd: string
  type: string
}

// Desktop-specific session type (similar to AgentSession but with desktop schema)
interface DesktopSession {
  id: string
  provider: string
  projectName: string
  projectId: string | null
  sessionId: string
  fileName: string | null
  filePath: string | null
  fileSize: number | null
  sessionStartTime: string | null
  sessionEndTime: string | null
  durationMs: number | null
  processingStatus: 'pending' | 'completed' | 'failed'
  createdAt: string
  cwd?: string | null
  firstCommitHash: string | null
  gitBranch?: string | null
  // Desktop-specific fields
  updatedAt?: string
  lastCommitHash?: string | null
  syncedToServer?: boolean
  syncedAt?: string | null
  serverSessionId?: string | null
  uploadedAt?: string | null
  syncFailedReason?: string | null
  aiModelPhaseAnalysis?: string | null
  latestCommitHash?: string | null
  assessmentRating?: SessionRating | null
  aiModelSummary?: string | null
  aiModelQualityScore?: number | null
  aiModelMetadata?: string | null
  errorMessage?: string | null
  coreMetricsStatus?: string | null
}

// Fetch function for session metadata
async function fetchSessionMetadata(sessionId: string): Promise<DesktopSession | null> {
  const result = await invoke<AgentSessionWithRating[]>('execute_sql', {
    sql: `SELECT s.*, a.rating as assessment_rating
          FROM agent_sessions s
          LEFT JOIN session_assessments a ON s.session_id = a.session_id
          WHERE s.session_id = ? LIMIT 1`,
    params: [sessionId],
  })

  if (result.length === 0) {
    throw new Error('Session not found')
  }

  // Map snake_case SQL result to camelCase TypeScript type
  const row = result[0]
  const session = mapAgentSessionRow(row)

  // Helper to safely convert timestamp (milliseconds) to ISO string
  const toISOString = (timestamp: number | null): string | null => {
    if (!timestamp) return null
    try {
      return new Date(timestamp).toISOString()
    } catch {
      return null
    }
  }

  // Add additional fields not in base AgentSessionRow
  return {
    ...session,
    cwd: row.cwd ?? undefined,
    syncedToServer: row.synced_to_server === 1,
    syncedAt: toISOString(row.synced_at),
    serverSessionId: row.server_session_id,
    uploadedAt: toISOString(row.uploaded_at),
    syncFailedReason: row.sync_failed_reason,
    aiModelPhaseAnalysis: row.ai_model_phase_analysis,
    gitBranch: row.git_branch,
    latestCommitHash: row.latest_commit_hash,
    assessmentRating: row.assessment_rating as SessionRating | null,
    aiModelSummary: row.ai_model_summary,
    aiModelQualityScore: row.ai_model_quality_score,
    aiModelMetadata: row.ai_model_metadata,
  }
}

// Fetch project for session
async function fetchSessionProject(sessionId: string): Promise<LocalProject | null> {
  const result = await invoke<ProjectRowExtended[]>('execute_sql', {
    sql: `SELECT p.* FROM projects p
          JOIN agent_sessions s ON p.id = s.project_id
          WHERE s.session_id = ? LIMIT 1`,
    params: [sessionId],
  })

  if (result.length === 0) {
    return null
  }

  // Map snake_case SQL result to camelCase TypeScript type
  const row = result[0]
  const project = mapProjectRow(row)

  // Add additional fields (cast to any to allow extra desktop-specific fields)
  return {
    ...project,
    githubRepo: row.github_repo,
    cwd: row.cwd,
    type: row.type,
  } as LocalProject & { githubRepo: string | null }
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
  const [showMetaMessages, setShowMetaMessages] = useState(() => {
    const saved = localStorage.getItem('transcript-show-meta')
    return saved === 'true'
  })
  const [showThinkingBlocks, setShowThinkingBlocks] = useState(() => {
    const saved = localStorage.getItem('transcript-show-thinking')
    return saved !== 'false' // Default to true (shown)
  })
  const [processingAi, setProcessingAi] = useState(false)
  const { processSessionWithAi, hasApiKey } = useAiProcessing()
  const { processSession: processMetrics } = useSessionProcessing()
  const { progress, updateProgress, reset: resetProgress } = useAiProcessingProgress()
  const toast = useToast()
  const quickRatingMutation = useQuickRating()
  const [hasPendingChanges, setHasPendingChanges] = useState(false)

  // Scroll to message function for TokenUsageChart clickable bars
  const scrollToMessage = useCallback((messageId: string) => {
    const messageElement = document.querySelector(`[data-message-id="${messageId}"]`)
    if (messageElement) {
      messageElement.scrollIntoView({
        behavior: 'smooth',
        block: 'center',
      })
      // Add enhanced flash/highlight effect
      messageElement.classList.add('ring-4', 'ring-primary', 'ring-offset-2', 'bg-primary/10')

      // Flash effect: add and remove quickly for attention
      setTimeout(() => {
        messageElement.classList.remove('bg-primary/10')
        messageElement.classList.add('bg-primary/20')
      }, 150)
      setTimeout(() => {
        messageElement.classList.remove('bg-primary/20')
        messageElement.classList.add('bg-primary/10')
      }, 300)
      setTimeout(() => {
        messageElement.classList.remove('bg-primary/10')
        messageElement.classList.add('bg-primary/20')
      }, 450)

      // Remove all effects after animation
      setTimeout(() => {
        messageElement.classList.remove(
          'ring-4',
          'ring-primary',
          'ring-offset-2',
          'bg-primary/20',
          'bg-primary/10'
        )
      }, 2000)
    }
  }, [])

  // Track session activity from file watchers
  useSessionActivity()
  const isSessionActive = useSessionActivityStore(state => state.isSessionActive)

  // Tab state - default to transcript
  const [activeTab, setActiveTab] = useState<
    'phase-timeline' | 'transcript' | 'metrics' | 'changes' | 'context' | 'todos' | 'raw-jsonl'
  >('transcript')

  // Fetch session metadata with TanStack Query
  const {
    data: session,
    isLoading: loading,
    error,
  } = useQuery({
    queryKey: ['session-metadata', sessionId],
    queryFn: () => {
      if (!sessionId) {
        throw new Error('Session ID is required')
      }
      return fetchSessionMetadata(sessionId)
    },
    enabled: !!sessionId,
  })

  // Fetch project for session
  const { data: project } = useQuery({
    queryKey: ['session-project', sessionId],
    queryFn: () => {
      if (!sessionId) {
        throw new Error('Session ID is required')
      }
      return fetchSessionProject(sessionId)
    },
    enabled: !!sessionId,
  })

  // Validate session canonical JSONL file
  const { status: validationStatus } = useValidationStatus(
    session?.filePath,
    session?.provider,
    session?.sessionId
  )

  // Parse phase analysis if available
  const phaseAnalysis: SessionPhaseAnalysis | null = (session as any)?.aiModelPhaseAnalysis
    ? JSON.parse((session as any).aiModelPhaseAnalysis)
    : null

  // Helper to check if we should show unstaged changes
  // Shows unstaged if: no commits during session AND session ended within 48 hours
  const shouldShowUnstaged = (session: DesktopSession | null | undefined): boolean => {
    if (!session) return false

    const noCommitsDuringSession = session.firstCommitHash === session.latestCommitHash
    if (!noCommitsDuringSession) return false

    if (!session.sessionEndTime) return false

    const now = Date.now()
    const sessionEndMs = new Date(session.sessionEndTime).getTime()
    const timeSinceEnd = now - sessionEndMs
    const fortyEightHours = 48 * 60 * 60 * 1000

    return timeSinceEnd < fortyEightHours && timeSinceEnd >= 0
  }

  // Fetch git diff stats for tab badge (only when Changes tab is NOT active)
  const { data: gitDiffStats } = useQuery({
    queryKey: ['session-git-diff-stats', sessionId, shouldShowUnstaged(session)],
    queryFn: async () => {
      if (!session?.cwd || !session?.firstCommitHash) {
        return null
      }
      const diffs = await invoke<any[]>('get_session_git_diff', {
        cwd: session.cwd,
        firstCommitHash: session.firstCommitHash,
        latestCommitHash: (session as any).latestCommitHash || null,
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
    enabled: !!session?.cwd && !!session?.firstCommitHash && activeTab !== 'changes',
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

  // Save transcript settings to localStorage
  useEffect(() => {
    localStorage.setItem('transcript-show-meta', showMetaMessages.toString())
  }, [showMetaMessages])

  useEffect(() => {
    localStorage.setItem('transcript-show-thinking', showThinkingBlocks.toString())
  }, [showThinkingBlocks])

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
  } = useLocalSessionContent(
    session?.sessionId ?? undefined,
    session?.provider,
    session?.filePath ?? undefined
  )

  // Load local metrics if available
  const { metrics, loading: metricsLoading } = useLocalSessionMetrics(
    session?.sessionId ?? undefined
  )

  // Handle AI processing
  const handleProcessWithAi = async () => {
    if (!session || !timeline) return

    setProcessingAi(true)
    resetProgress()

    try {
      // Parse session content
      const { ProcessorRegistry } = await import('@guidemode/session-processing/processors')
      const registry = new ProcessorRegistry()
      const processor = registry.getProcessor(session.provider)

      if (!processor) {
        throw new Error(`No processor found for provider: ${session.provider}`)
      }

      const content = await invoke<string>('get_session_content', {
        provider: session.provider,
        filePath: session.filePath,
        sessionId: session.sessionId,
      })
      const parsedSession = processor.parseSession(content, session.provider)

      // Step 1: Calculate metrics (always)
      updateProgress({
        name: 'Calculating Metrics',
        description: 'Analyzing session performance and quality metrics',
        percentage: 0,
      })
      await processMetrics(session.sessionId, session.provider, content, 'local')

      // Step 2: Process with AI if API key available
      if (hasApiKey()) {
        await processSessionWithAi(session.sessionId, parsedSession, updateProgress)
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

  // Check if session transcript contains any todo tracking events
  // This checks the raw transcript directly, without waiting for metrics processing
  // Supports both Claude Code (TodoWrite) and Codex (update_plan)
  const hasTodos = useMemo(() => {
    if (!fileContent) return false
    const todos = extractTodosAuto(fileContent)
    return todos.length > 0
  }, [fileContent])

  // Filter out meta messages, empty assistant messages, and thinking blocks (based on settings)
  const filteredItems = useMemo(() => {
    let filtered = timeline?.items || []
    if (timeline) {
      filtered = filtered.filter(item => {
        if (isTimelineGroup(item)) {
          // Keep group if messages pass all filter criteria
          return item.messages.every(msg => {
            const isMetaMessage = msg.originalMessage.type === 'meta'
            const isEmptyAssistant =
              msg.originalMessage.type === 'assistant_response' &&
              typeof msg.originalMessage.content === 'string' &&
              msg.originalMessage.content.trim() === ''
            const isThinkingMessage = msg.originalMessage.metadata?.isThinking === true

            // Filter out meta (if disabled), empty assistant messages (always), and thinking (if disabled)
            return (
              (!showMetaMessages ? !isMetaMessage : true) &&
              !isEmptyAssistant &&
              (!showThinkingBlocks ? !isThinkingMessage : true)
            )
          })
        }
        // For single messages
        const isMetaMessage = item.originalMessage.type === 'meta'
        const isEmptyAssistant =
          item.originalMessage.type === 'assistant_response' &&
          typeof item.originalMessage.content === 'string' &&
          item.originalMessage.content.trim() === ''
        const isThinkingMessage = item.originalMessage.metadata?.isThinking === true

        // Filter out meta (if disabled), empty assistant messages (always), and thinking (if disabled)
        return (
          (!showMetaMessages ? !isMetaMessage : true) &&
          !isEmptyAssistant &&
          (!showThinkingBlocks ? !isThinkingMessage : true)
        )
      })
    }
    return filtered
  }, [timeline, showMetaMessages, showThinkingBlocks])

  // Apply reverse order if requested
  const orderedItems = useMemo(() => {
    return reverseOrder ? [...filteredItems].reverse() : filteredItems
  }, [filteredItems, reverseOrder])

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

  // Calculate filter statistics
  const totalParsedItems = timeline?.items?.length || 0
  const displayedItems = filteredItems.length
  const hiddenItems = totalParsedItems - displayedItems

  // Calculate breakdown of hidden items
  const metaMessageCount =
    timeline?.items?.filter(item => {
      if (isTimelineGroup(item)) {
        return item.messages.some(msg => msg.originalMessage.type === 'meta')
      }
      return item.originalMessage.type === 'meta'
    }).length || 0

  const thinkingBlockCount =
    timeline?.items?.filter(item => {
      if (isTimelineGroup(item)) {
        return item.messages.some(msg => msg.originalMessage.metadata?.isThinking === true)
      }
      return item.originalMessage.metadata?.isThinking === true
    }).length || 0

  const emptyAssistantCount =
    timeline?.items?.filter(item => {
      if (isTimelineGroup(item)) {
        return item.messages.some(
          msg =>
            msg.originalMessage.type === 'assistant_response' &&
            typeof msg.originalMessage.content === 'string' &&
            msg.originalMessage.content.trim() === ''
        )
      }
      return (
        item.originalMessage.type === 'assistant_response' &&
        typeof item.originalMessage.content === 'string' &&
        item.originalMessage.content.trim() === ''
      )
    }).length || 0

  return (
    <div className="space-y-4">
      {/* Page Header */}
      <div className="flex items-center justify-between gap-4">
        <div className="flex items-center gap-3">
          <h1 className="text-2xl font-bold">Session Detail</h1>
          {sessionId &&
            session?.sessionEndTime &&
            isSessionActive(sessionId, session.sessionEndTime) && (
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
        <SessionDetailHeader
          session={{
            provider: session.provider,
            projectName: session.projectName,
            sessionStartTime: session.sessionStartTime,
            durationMs: session.durationMs ?? null,
            fileSize: session.fileSize ?? undefined,
            cwd: session.cwd ?? undefined,
            project: project
              ? {
                  name: project.name,
                  gitRemoteUrl: (project as any).githubRepo ?? undefined,
                  cwd: undefined,
                }
              : undefined,
            aiModelSummary: session.aiModelSummary ?? undefined,
            gitBranch: session.gitBranch ?? undefined,
            firstCommitHash: session.firstCommitHash ?? undefined,
            latestCommitHash: session.latestCommitHash ?? undefined,
          }}
          messageCount={messageCount}
          rating={(session.assessmentRating as SessionRating) ?? null}
          onRate={handleQuickRate}
          onProcessSession={handleProcessWithAi}
          processingStatus={session.aiModelSummary ? 'completed' : 'pending'}
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
          onViewDiff={session.firstCommitHash ? handleViewDiff : undefined}
          onProjectClick={
            session.projectId ? () => navigate(`/projects/${session.projectId}`) : undefined
          }
          syncStatus={{
            synced: session.syncedToServer === true,
            failed: !!session.syncFailedReason,
            reason: session.syncFailedReason ?? undefined,
            onSync: handleSyncSession,
            onShowError: error => toast.error(error, 10000),
          }}
          ProviderIcon={ProviderIcon}
        />
      )}

      {/* Tabs Navigation with Controls */}
      <div className="card bg-base-200 border border-base-300 border-b-2 rounded-lg">
        <div className="flex items-stretch">
          {/* Left: Tab Buttons */}
          <div className="tabs tabs-bordered flex-1 flex justify-between">
            <div className="flex">
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
              {(metrics?.quality?.usedTodoTracking || hasTodos) && (
                <button
                  className={`tab tab-lg gap-2 ${
                    activeTab === 'todos'
                      ? 'tab-active bg-base-100 text-primary font-semibold border-b-2 border-primary'
                      : 'hover:bg-base-300'
                  }`}
                  onClick={() => setActiveTab('todos')}
                  title="Todos"
                >
                  <CheckCircleIcon className="w-5 h-5" />
                  <span className="hidden md:inline">Todos</span>
                </button>
              )}
              {session.cwd && session.firstCommitHash && (
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
                  <span
                    className={`hidden md:inline ${hasPendingChanges && activeTab !== 'changes' ? 'text-primary' : ''}`}
                  >
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
                          <span className="badge badge-error badge-sm">
                            {gitDiffStats.deletions}
                          </span>
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

            {/* Right-aligned Raw JSONL tab with validation status */}
            <button
              className={`tab tab-lg ${
                activeTab === 'raw-jsonl'
                  ? 'tab-active bg-base-100 text-primary font-semibold border-b-2 border-primary'
                  : 'hover:bg-base-300'
              }`}
              onClick={() => setActiveTab('raw-jsonl')}
              title={`Raw JSONL${validationStatus === 'valid' ? ' (Valid)' : validationStatus === 'errors' ? ' (Validation Errors)' : validationStatus === 'warnings' ? ' (Validation Warnings)' : ''}`}
            >
              <BugAntIcon
                className={`w-5 h-5 ${
                  validationStatus === 'valid'
                    ? 'text-success'
                    : validationStatus === 'errors'
                      ? 'text-error'
                      : validationStatus === 'warnings'
                        ? 'text-warning'
                        : ''
                }`}
              />
            </button>
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
                    <div className="absolute right-0 top-full mt-2 w-80 bg-base-100 border border-base-300 rounded-lg shadow-lg z-20 p-4 space-y-4">
                      <h3 className="text-sm font-semibold">Timeline Settings</h3>

                      <div>
                        <label className="flex items-center gap-2 cursor-pointer">
                          <input
                            type="checkbox"
                            checked={showMetaMessages}
                            onChange={e => setShowMetaMessages(e.target.checked)}
                            className="checkbox checkbox-sm checkbox-primary"
                          />
                          <span className="text-sm">Show meta messages</span>
                        </label>
                        <p className="text-xs text-base-content/60 mt-1.5 ml-6">
                          Internal system messages that provide context but are not part of the main
                          conversation.
                        </p>
                      </div>

                      <div>
                        <label className="flex items-center gap-2 cursor-pointer">
                          <input
                            type="checkbox"
                            checked={showThinkingBlocks}
                            onChange={e => setShowThinkingBlocks(e.target.checked)}
                            className="checkbox checkbox-sm checkbox-primary"
                          />
                          <span className="text-sm">Show thinking blocks</span>
                        </label>
                        <p className="text-xs text-base-content/60 mt-1.5 ml-6">
                          AI reasoning and thought process blocks that explain how responses were
                          formed.
                        </p>
                      </div>
                    </div>
                  </>
                )}
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Filter Stats Banner */}
      {activeTab === 'transcript' && hiddenItems > 0 && (
        <div className="alert bg-base-200 border border-base-300">
          <div className="flex-1">
            <div className="flex items-center gap-3 text-sm">
              <svg
                className="w-5 h-5 text-info"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
                />
              </svg>
              <div className="flex-1">
                <div className="font-semibold text-base-content">
                  Showing {displayedItems} of {totalParsedItems} items
                </div>
                <div className="text-xs text-base-content/70 mt-1">
                  {hiddenItems} item{hiddenItems !== 1 ? 's' : ''} hidden
                  {!showMetaMessages && metaMessageCount > 0 && (
                    <span className="ml-1">
                      ({metaMessageCount} meta message{metaMessageCount !== 1 ? 's' : ''})
                    </span>
                  )}
                  {!showThinkingBlocks && thinkingBlockCount > 0 && (
                    <span className="ml-1">
                      ({thinkingBlockCount} thinking block{thinkingBlockCount !== 1 ? 's' : ''})
                    </span>
                  )}
                  {emptyAssistantCount > 0 && (
                    <span className="ml-1">
                      ({emptyAssistantCount} empty assistant response
                      {emptyAssistantCount !== 1 ? 's' : ''})
                    </span>
                  )}
                </div>
              </div>
              <div className="flex gap-2">
                {!showMetaMessages && metaMessageCount > 0 && (
                  <button
                    onClick={() => setShowMetaMessages(true)}
                    className="btn btn-xs btn-ghost gap-1"
                  >
                    Show Meta
                  </button>
                )}
                {!showThinkingBlocks && thinkingBlockCount > 0 && (
                  <button
                    onClick={() => setShowThinkingBlocks(true)}
                    className="btn btn-xs btn-ghost gap-1"
                  >
                    Show Thinking
                  </button>
                )}
              </div>
            </div>
          </div>
        </div>
      )}

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
              <>
                {/* TokenUsageChart uses ALL messages for accurate totals */}
                <TokenUsageChart items={timeline?.items || []} onMessageClick={scrollToMessage} />
                {/* VirtualizedMessageList uses filtered messages */}
                <VirtualizedMessageList items={orderedItems} />
                {/* Floating scroll to top button - appears when scrolled down */}
                <ScrollToTopButton />
              </>
            )}
          </>
        )}
        {activeTab === 'phase-timeline' && phaseAnalysis && (
          <PhaseTimeline phaseAnalysis={phaseAnalysis} />
        )}
        {activeTab === 'metrics' && (
          <MetricsOverview
            sessionId={session.sessionId ?? ''}
            metrics={metrics}
            isLoading={metricsLoading}
            error={null}
            onProcessSession={
              session.aiModelSummary || session.aiModelQualityScore
                ? undefined
                : handleProcessWithAi
            }
            isProcessing={processingAi}
            aiModelSummary={session.aiModelSummary}
            aiModelQualityScore={session.aiModelQualityScore}
            aiModelMetadata={
              session.aiModelMetadata ? JSON.parse(session.aiModelMetadata) : undefined
            }
          />
        )}
        {activeTab === 'changes' && session.cwd && session.firstCommitHash && (
          <SessionChangesTab
            session={{
              sessionId: session.sessionId ?? '',
              cwd: session.cwd,
              first_commit_hash: session.firstCommitHash,
              latest_commit_hash: session.latestCommitHash ?? null,
              session_start_time: session.sessionStartTime
                ? new Date(session.sessionStartTime).getTime()
                : null,
              session_end_time: session.sessionEndTime
                ? new Date(session.sessionEndTime).getTime()
                : null,
            }}
            hasPendingChanges={hasPendingChanges}
            onRefresh={handleClearPendingChanges}
          />
        )}
        {activeTab === 'context' && session.cwd && (
          <SessionContextTab
            session={{
              sessionId: session.sessionId ?? '',
              cwd: session.cwd,
            }}
            fileContent={fileContent}
          />
        )}
        {activeTab === 'todos' && (
          <SessionTodosTab
            session={{
              sessionId: session.sessionId ?? '',
            }}
            fileContent={fileContent}
          />
        )}
        {activeTab === 'raw-jsonl' && (
          <div className="card bg-base-100 border border-base-300">
            <div className="card-body">
              <div className="flex items-center justify-between mb-4">
                <h3 className="text-lg font-semibold">Raw JSONL File Content</h3>
                {fileContent && (
                  <button
                    onClick={() => {
                      navigator.clipboard.writeText(fileContent).then(
                        () => toast.success('Copied to clipboard!'),
                        () => toast.error('Failed to copy to clipboard')
                      )
                    }}
                    className="btn btn-sm btn-ghost gap-2"
                  >
                    <ClipboardIcon className="w-4 h-4" />
                    Copy All
                  </button>
                )}
              </div>
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
              ) : fileContent ? (
                <>
                  {/* Validation Report */}
                  {session && (
                    <div className="mb-6">
                      <ValidationReport
                        sessionId={session.sessionId}
                        provider={session.provider}
                        project={project?.name ?? 'unknown'}
                        filePath={session.filePath ?? ''}
                      />
                    </div>
                  )}

                  {/* Raw JSONL Content */}
                  <JsonBlock content={fileContent} maxHeight="800px" />
                </>
              ) : (
                <div className="alert">
                  <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      strokeWidth={2}
                      d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
                    />
                  </svg>
                  <span>No file content available</span>
                </div>
              )}
              <div className="mt-4 p-3 bg-info/10 rounded-lg text-xs text-info">
                <p className="font-semibold mb-1">💡 About Raw JSONL Format:</p>
                <ul className="list-disc list-inside space-y-1 text-info/80">
                  <li>Each line is a separate JSON object representing a session event</li>
                  <li>
                    Messages are in canonical format after conversion from provider-specific formats
                  </li>
                  <li>Use this view to debug parsing issues or inspect raw session data</li>
                  <li>
                    Tip: Copy to clipboard and format with a JSON formatter for better readability
                  </li>
                </ul>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  )
}
