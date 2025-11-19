import { DateFilter, SessionCard } from '@guidemode/session-processing/ui'
import type { SessionRating } from '@guidemode/session-processing/ui'
import type { DateFilterValue } from '@guidemode/session-processing/ui'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { useCallback, useEffect, useRef, useState } from 'react'
import { flushSync } from 'react-dom'
import { useNavigate, useSearchParams } from 'react-router-dom'
import { ActiveSessionCard } from '../components/ActiveSessionCard'
import ConfirmDialog from '../components/ConfirmDialog'
import ProcessingModeDialog from '../components/ProcessingModeDialog'
import ProviderIcon from '../components/icons/ProviderIcon'
import { useAiProcessing } from '../hooks/useAiProcessing'
import { useAuth } from '../hooks/useAuth'
import { useLocalProjects } from '../hooks/useLocalProjects'
import { useInvalidateSessions, useLocalSessions } from '../hooks/useLocalSessions'
import { useQuickRating } from '../hooks/useQuickRating'
import { useSessionActivity } from '../hooks/useSessionActivity'
import { useSessionProcessing } from '../hooks/useSessionProcessing'
import { useToast } from '../hooks/useToast'
import { useSessionActivityStore } from '../stores/sessionActivityStore'

const SESSIONS_PER_PAGE = 50
const PROVIDER_FILTER_KEY = 'sessions.providerFilter'
const DATE_FILTER_KEY = 'sessions.dateFilter'
const ACTIVE_FILTER_KEY = 'sessions.activeFilter'

export default function SessionsPage() {
  const navigate = useNavigate()
  const { user } = useAuth()
  const [searchParams, setSearchParams] = useSearchParams()

  // Initialize filters from localStorage
  const [dateFilter, setDateFilter] = useState<DateFilterValue>(() => {
    const saved = localStorage.getItem(DATE_FILTER_KEY)
    return saved ? JSON.parse(saved) : { option: 'all' }
  })
  const [providerFilter, setProviderFilter] = useState<string>(() => {
    // Check URL parameter first, then fall back to localStorage
    const urlProvider = searchParams.get('provider')
    if (urlProvider) {
      return urlProvider
    }
    return localStorage.getItem(PROVIDER_FILTER_KEY) || 'all'
  })
  const [projectFilter, setProjectFilter] = useState<string>('all')
  const [showActiveOnly, setShowActiveOnly] = useState<boolean>(() => {
    const saved = localStorage.getItem(ACTIVE_FILTER_KEY)
    return saved === 'true'
  })
  const [processingSessionId, setProcessingSessionId] = useState<string | null>(null)
  const [selectionMode, setSelectionMode] = useState(false)
  const [selectedSessionIds, setSelectedSessionIds] = useState<string[]>([])
  const [bulkProcessing, setBulkProcessing] = useState(false)
  const [bulkProgress, setBulkProgress] = useState({ current: 0, total: 0 })
  const [syncErrorModal, setSyncErrorModal] = useState<{ sessionId: string; error: string } | null>(
    null
  )
  const [confirmDialog, setConfirmDialog] = useState<{ isOpen: boolean; count: number } | null>(
    null
  )
  const [displayCount, setDisplayCount] = useState(SESSIONS_PER_PAGE)
  const [processingMode, setProcessingMode] = useState<'core' | 'full'>('full')
  const [modeSelectionDialog, setModeSelectionDialog] = useState<{
    isOpen: boolean
    count: number
  } | null>(null)
  const { processSessionWithAi, hasApiKey } = useAiProcessing()
  const { processSession: processMetrics } = useSessionProcessing()
  const toast = useToast()
  const observerRef = useRef<IntersectionObserver | null>(null)
  const loadMoreRef = useRef<HTMLDivElement | null>(null)
  const processAllTimeoutRef = useRef<NodeJS.Timeout | null>(null)
  const cancelBulkProcessingRef = useRef(false)

  // Track session activity from file watchers
  useSessionActivity()
  const isSessionActive = useSessionActivityStore(state => state.isSessionActive)
  const clearAllActiveSessions = useSessionActivityStore(state => state.clearAllActiveSessions)

  const { sessions, loading, error, refresh } = useLocalSessions({
    provider: providerFilter === 'all' ? undefined : providerFilter,
    projectId: projectFilter === 'all' ? undefined : projectFilter,
    dateFilter: dateFilter,
  })
  const invalidateSessions = useInvalidateSessions()
  const { projects } = useLocalProjects()
  const quickRatingMutation = useQuickRating()

  // Count active sessions
  const activeSessions = sessions.filter(session =>
    isSessionActive(session.sessionId as string, session.sessionEndTime)
  )
  const hasActiveSessions = activeSessions.length > 0

  // Filter sessions by active status if enabled AND there are active sessions
  const filteredSessions = showActiveOnly && hasActiveSessions ? activeSessions : sessions

  // Initialize project filter from URL parameter
  useEffect(() => {
    const projectParam = searchParams.get('project')
    if (projectParam) {
      setProjectFilter(projectParam)
    }
  }, [searchParams])

  // Initialize provider filter from URL parameter
  useEffect(() => {
    const providerParam = searchParams.get('provider')
    if (providerParam) {
      setProviderFilter(providerParam)
    } else if (!searchParams.has('provider')) {
      // If no URL param, keep current filter (don't reset to 'all')
      return
    }
  }, [searchParams])

  // Persist provider filter to localStorage
  useEffect(() => {
    localStorage.setItem(PROVIDER_FILTER_KEY, providerFilter)
  }, [providerFilter])

  // Persist date filter to localStorage
  useEffect(() => {
    localStorage.setItem(DATE_FILTER_KEY, JSON.stringify(dateFilter))
  }, [dateFilter])

  // Persist active filter to localStorage
  useEffect(() => {
    localStorage.setItem(ACTIVE_FILTER_KEY, String(showActiveOnly))
  }, [showActiveOnly])

  // Reset display count when filters change
  // biome-ignore lint/correctness/useExhaustiveDependencies: intentionally resetting on filter changes
  useEffect(() => {
    setDisplayCount(SESSIONS_PER_PAGE)
  }, [providerFilter, dateFilter, projectFilter, showActiveOnly])

  // Infinite scroll observer
  const loadMore = useCallback(() => {
    if (displayCount < filteredSessions.length) {
      setDisplayCount(prev => Math.min(prev + SESSIONS_PER_PAGE, filteredSessions.length))
    }
  }, [displayCount, filteredSessions.length])

  useEffect(() => {
    if (!loadMoreRef.current) return

    observerRef.current = new IntersectionObserver(
      entries => {
        if (entries[0].isIntersecting) {
          loadMore()
        }
      },
      { threshold: 0.1 }
    )

    observerRef.current.observe(loadMoreRef.current)

    return () => {
      if (observerRef.current) {
        observerRef.current.disconnect()
      }
    }
  }, [loadMore])

  // Get visible sessions
  const visibleSessions = filteredSessions.slice(0, displayCount)

  // Listen for sync/update events from backend and invalidate cache
  useEffect(() => {
    let unlistenSynced: (() => void) | undefined
    let unlistenFailed: (() => void) | undefined
    let unlistenUpdated: (() => void) | undefined

    listen('session-synced', () => {
      invalidateSessions()
    }).then(fn => {
      unlistenSynced = fn
    })

    listen('session-sync-failed', () => {
      invalidateSessions()
    }).then(fn => {
      unlistenFailed = fn
    })

    listen('session-updated', () => {
      invalidateSessions()
    }).then(fn => {
      unlistenUpdated = fn
    })

    return () => {
      unlistenSynced?.()
      unlistenFailed?.()
      unlistenUpdated?.()
    }
  }, [invalidateSessions])

  const handleViewSession = (sessionId: string) => {
    navigate(`/sessions/${sessionId}`)
  }

  const handleProcessSession = async (
    sessionId: string,
    provider: string,
    filePath: string,
    silent = false,
    mode: 'core' | 'full' = 'full'
  ) => {
    setProcessingSessionId(sessionId)
    try {
      // Load session content
      const content = await invoke<string>('get_session_content', {
        provider,
        filePath,
        sessionId,
      })

      // Parse session using the content hook's parser
      const { ProcessorRegistry } = await import('@guidemode/session-processing/processors')
      const registry = new ProcessorRegistry()
      const processor = registry.getProcessor(provider)

      if (!processor) {
        throw new Error(`No processor found for provider: ${provider}`)
      }

      const parsedSession = processor.parseSession(content, provider)

      // Step 1: Calculate metrics (always)
      await processMetrics(sessionId, provider, content, 'local')

      // Step 2: Process with AI if mode is 'full' and API key is available
      if (mode === 'full' && hasApiKey()) {
        await processSessionWithAi(sessionId, parsedSession)
      }

      // Refresh sessions to show updated results
      await refresh()

      if (!silent) {
        if (mode === 'core') {
          toast.success('Core metrics calculated successfully!')
        } else if (hasApiKey()) {
          toast.success('Processing complete! Metrics calculated and AI summary generated.')
        } else {
          toast.success('Metrics calculated! AI processing skipped (no API key configured).')
        }
      }
    } catch (err) {
      console.error('Failed to process session:', err)
      if (!silent) {
        toast.error(`Failed to process session: ${(err as Error).message}`)
      }
    } finally {
      setProcessingSessionId(null)
    }
  }

  const handleBulkProcess = async (skipConfirmation = false, mode?: 'core' | 'full') => {
    // Use the passed mode or fall back to state
    const effectiveMode = mode ?? processingMode

    // Use ALL sessions from database, not just visible ones
    // Filter based on processing mode
    const sessionsToProcess =
      effectiveMode === 'core'
        ? sessions.filter(s => selectedSessionIds.includes(s.sessionId as string))
        : sessions.filter(
            s =>
              selectedSessionIds.includes(s.sessionId as string) &&
              s.assessmentStatus !== 'completed'
          )

    if (sessionsToProcess.length === 0) {
      if (effectiveMode === 'core') {
        toast.info('No sessions selected.')
      } else {
        toast.info('No sessions selected or all selected sessions already have AI processing.')
      }
      return
    }

    // Show confirmation dialog unless skipping
    if (skipConfirmation) {
      handleConfirmBulkProcess(effectiveMode)
    } else {
      setConfirmDialog({ isOpen: true, count: sessionsToProcess.length })
    }
  }

  const handleConfirmBulkProcess = async (mode?: 'core' | 'full') => {
    setConfirmDialog(null)
    cancelBulkProcessingRef.current = false

    // Use the passed mode or fall back to state
    const effectiveMode = mode ?? processingMode

    // Use ALL sessions from database, not just visible ones
    // Filter based on processing mode
    const sessionsToProcess =
      effectiveMode === 'core'
        ? sessions.filter(s => selectedSessionIds.includes(s.sessionId as string))
        : sessions.filter(
            s =>
              selectedSessionIds.includes(s.sessionId as string) &&
              s.assessmentStatus !== 'completed'
          )

    setBulkProcessing(true)
    setBulkProgress({ current: 0, total: sessionsToProcess.length })

    let successCount = 0
    let errorCount = 0

    for (let i = 0; i < sessionsToProcess.length; i++) {
      // Check if cancelled using ref (not state closure)
      if (cancelBulkProcessingRef.current) {
        break
      }

      const session = sessionsToProcess[i]
      setBulkProgress({ current: i + 1, total: sessionsToProcess.length })

      try {
        await handleProcessSession(
          session.sessionId as string,
          session.provider,
          session.filePath as string,
          true,
          effectiveMode
        )
        successCount++

        // Add 2-second delay between requests to avoid rate limiting (only if using AI in full mode)
        if (effectiveMode === 'full' && hasApiKey() && i < sessionsToProcess.length - 1) {
          await new Promise(resolve => setTimeout(resolve, 2000))
        }
      } catch (err) {
        console.error(`Failed to process session ${session.sessionId}:`, err)
        errorCount++
      }
    }

    setBulkProcessing(false)
    setSelectionMode(false)
    setSelectedSessionIds([])
    refresh()

    const modeLabel = effectiveMode === 'core' ? 'core metrics' : 'full processing'
    if (errorCount > 0) {
      toast.warning(
        `Bulk ${modeLabel} complete!\n✓ ${successCount} successful\n✗ ${errorCount} failed`
      )
    } else {
      toast.success(`Bulk ${modeLabel} complete! ${successCount} sessions processed successfully.`)
    }
  }

  const handleProcessAll = async () => {
    // Clear any pending timeout from previous calls
    if (processAllTimeoutRef.current) {
      clearTimeout(processAllTimeoutRef.current)
      processAllTimeoutRef.current = null
    }

    // Count all sessions (for mode selection dialog)
    const allSessionsCount = sessions.length

    if (allSessionsCount === 0) {
      toast.info('No sessions found.')
      return
    }

    // Show mode selection dialog
    setModeSelectionDialog({ isOpen: true, count: allSessionsCount })
  }

  const handleModeSelected = async (mode: 'core' | 'full') => {
    setModeSelectionDialog(null)

    // Filter sessions based on mode
    const sessionsToProcess =
      mode === 'core'
        ? sessions // Process ALL sessions in core mode
        : sessions.filter(s => s.assessmentStatus !== 'completed') // Only unprocessed in full mode

    if (sessionsToProcess.length === 0) {
      toast.info('All sessions already processed.')
      return
    }

    const sessionIds = sessionsToProcess.map(s => s.sessionId as string)

    // Set mode and selection using flushSync to ensure immediate state updates
    flushSync(() => {
      setProcessingMode(mode)
      setSelectedSessionIds(sessionIds)
      setSelectionMode(true)
    })

    // Start processing immediately - call directly instead of through setTimeout
    // to avoid any closure or timing issues
    cancelBulkProcessingRef.current = false
    setBulkProcessing(true)
    setBulkProgress({ current: 0, total: sessionsToProcess.length })

    let successCount = 0
    let errorCount = 0

    for (let i = 0; i < sessionsToProcess.length; i++) {
      // Check if cancelled
      if (cancelBulkProcessingRef.current) {
        break
      }

      const session = sessionsToProcess[i]
      setBulkProgress({ current: i + 1, total: sessionsToProcess.length })

      try {
        await handleProcessSession(
          session.sessionId as string,
          session.provider,
          session.filePath as string,
          true,
          mode
        )
        successCount++

        // Add 2-second delay between requests to avoid rate limiting (only if using AI in full mode)
        if (mode === 'full' && hasApiKey() && i < sessionsToProcess.length - 1) {
          await new Promise(resolve => setTimeout(resolve, 2000))
        }
      } catch (err) {
        console.error(`Failed to process session ${session.sessionId}:`, err)
        errorCount++
      }
    }

    setBulkProcessing(false)
    setSelectionMode(false)
    setSelectedSessionIds([])
    refresh()

    const modeLabel = mode === 'core' ? 'core metrics' : 'full processing'
    if (errorCount > 0) {
      toast.warning(
        `Bulk ${modeLabel} complete!\n✓ ${successCount} successful\n✗ ${errorCount} failed`
      )
    } else {
      toast.success(`Bulk ${modeLabel} complete! ${successCount} sessions processed successfully.`)
    }
  }

  const handleToggleSelection = (sessionId: string, checked: boolean) => {
    flushSync(() => {
      setSelectedSessionIds(prev => {
        if (checked) {
          // Add if not already present
          if (prev.includes(sessionId)) {
            return prev
          }
          return [...prev, sessionId]
        }
        // Remove if present
        return prev.filter(id => id !== sessionId)
      })
    })
  }

  const handleSelectAll = (checked: boolean) => {
    if (checked) {
      // Select all visible sessions not yet completed
      const unprocessedSessionIds = visibleSessions
        .filter(s => s.assessmentStatus !== 'completed')
        .map(s => s.sessionId as string)
      setSelectedSessionIds(unprocessedSessionIds)
    } else {
      setSelectedSessionIds([])
    }
  }

  const handleSyncSession = async (sessionId: string) => {
    // Check if user is logged in
    if (!user) {
      // Redirect to login
      navigate('/')
      return
    }

    // Trigger manual upload for this session
    try {
      // Get session details to trigger upload
      const sessionResult: any[] = await invoke('execute_sql', {
        sql: 'SELECT provider FROM agent_sessions WHERE session_id = ?',
        params: [sessionId],
      })

      if (sessionResult.length === 0) {
        toast.error('Session not found')
        return
      }

      const session = sessionResult[0]
      const providerId = session.provider

      // Check if provider's sync mode allows uploads
      const providerConfig = await invoke('load_provider_config_command', { providerId })
      if ((providerConfig as any).syncMode === 'Nothing') {
        // Navigate to provider config page with hash to highlight sync mode setting
        navigate(`/provider/${providerId}#sync-mode`)
        return
      }

      // Clear the sync_failed_reason to allow retry
      await invoke('execute_sql', {
        sql: 'UPDATE agent_sessions SET sync_failed_reason = NULL WHERE session_id = ?',
        params: [sessionId],
      })

      // The upload queue will pick it up automatically on next poll
      toast.success('Session queued for upload. Check the Upload Queue page for status.')
      refresh()
    } catch (err) {
      console.error('Failed to queue session for upload:', err)
      toast.error(`Failed to queue session: ${(err as Error).message}`)
    }
  }

  const handleShowSyncError = (sessionId: string, error: string) => {
    setSyncErrorModal({ sessionId, error })
  }

  const handleQuickRate = async (sessionId: string, rating: SessionRating) => {
    try {
      await quickRatingMutation.mutateAsync({ sessionId, rating })
      toast.success('Rating saved!')
    } catch (err) {
      console.error('Failed to rate session:', err)
      toast.error(`Failed to save rating: ${(err as Error).message}`)
    }
  }

  if (loading) {
    return (
      <div className="flex items-center justify-center min-h-[400px]">
        <span className="loading loading-spinner loading-lg" />
      </div>
    )
  }

  if (error) {
    return (
      <div className="alert alert-error">
        <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
          />
        </svg>
        <span>{error}</span>
      </div>
    )
  }

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold">Sessions</h1>
          <p className="text-sm text-base-content/70 mt-1">
            {sessions.length} {sessions.length === 1 ? 'session' : 'sessions'} found
          </p>
        </div>
        <button
          onClick={() => {
            clearAllActiveSessions()
            refresh()
          }}
          className="btn btn-sm btn-ghost"
        >
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
            />
          </svg>
          Refresh
        </button>
      </div>

      {/* Filters - Always visible */}
      <div className="flex items-center gap-3 mb-4">
        {/* Spacer to push filters to the right */}
        <div className="flex-1" />

        {/* Right side: Filters */}
        <label
          className={`label gap-2 ${!hasActiveSessions ? 'opacity-50 cursor-not-allowed' : 'cursor-pointer'}`}
        >
          <span className="label-text text-sm">Active Only</span>
          <input
            type="checkbox"
            className="toggle toggle-primary toggle-sm"
            checked={showActiveOnly}
            disabled={!hasActiveSessions}
            onChange={e => setShowActiveOnly(e.target.checked)}
          />
        </label>
        <DateFilter value={dateFilter} onChange={setDateFilter} />
        <select
          className="select select-bordered select-sm"
          value={providerFilter}
          onChange={e => {
            const value = e.target.value
            setProviderFilter(value)
            // Update URL parameter
            if (value === 'all') {
              searchParams.delete('provider')
            } else {
              searchParams.set('provider', value)
            }
            setSearchParams(searchParams)
          }}
        >
          <option value="all">All Providers</option>
          <option value="claude-code">Claude Code</option>
          <option value="cursor">Cursor CLI</option>
          <option value="github-copilot">GitHub Copilot</option>
          <option value="opencode">OpenCode</option>
          <option value="codex">Codex</option>
          <option value="gemini-code">Gemini Code</option>
        </select>
        <select
          className="select select-bordered select-sm"
          value={projectFilter}
          onChange={e => {
            const value = e.target.value
            setProjectFilter(value)
            // Update URL parameter
            if (value === 'all') {
              searchParams.delete('project')
            } else {
              searchParams.set('project', value)
            }
            setSearchParams(searchParams)
          }}
        >
          <option value="all">All Projects</option>
          {projects.map(project => (
            <option key={project.id} value={project.id}>
              {project.name}
            </option>
          ))}
        </select>
      </div>

      {/* Action Bar - Only visible when sessions exist */}
      {sessions.length > 0 && (
        <div className="flex items-center gap-3 mb-4">
          {/* Left side: Selection actions */}
          {selectionMode ? (
            <>
              {/* Select All Checkbox */}
              <input
                type="checkbox"
                className="checkbox checkbox-primary"
                checked={
                  selectedSessionIds.length ===
                    visibleSessions.filter(s => s.assessmentStatus !== 'completed').length &&
                  selectedSessionIds.length > 0
                }
                onChange={e => handleSelectAll(e.target.checked)}
              />
              <span className="text-sm">
                {selectedSessionIds.length > 0
                  ? `${selectedSessionIds.length} selected`
                  : 'Select All Visible'}
              </span>
            </>
          ) : (
            <>
              <button onClick={() => setSelectionMode(true)} className="btn btn-sm btn-outline">
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    strokeWidth={2}
                    d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-6 9l2 2 4-4"
                  />
                </svg>
                Select
              </button>
              <button onClick={handleProcessAll} className="btn btn-sm btn-primary">
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    strokeWidth={2}
                    d="M13 10V3L4 14h7v7l9-11h-7z"
                  />
                </svg>
                Process All
              </button>
            </>
          )}

          {/* Spacer */}
          <div className="flex-1" />

          {/* Action Buttons (in selection mode) */}
          {selectionMode && (
            <>
              <button
                onClick={() => handleBulkProcess(false)}
                className="btn btn-sm btn-primary"
                disabled={selectedSessionIds.length === 0 || bulkProcessing}
              >
                {bulkProcessing ? (
                  <>
                    <span className="loading loading-spinner loading-xs" />
                    Processing ({bulkProgress.current}/{bulkProgress.total})
                  </>
                ) : (
                  <>Process Selected ({selectedSessionIds.length})</>
                )}
              </button>
              <button
                onClick={() => {
                  cancelBulkProcessingRef.current = true
                  setBulkProcessing(false)
                  setProcessingSessionId(null)
                  setSelectionMode(false)
                  setSelectedSessionIds([])
                  invalidateSessions()
                }}
                className="btn btn-sm btn-ghost"
              >
                Cancel
              </button>
            </>
          )}
        </div>
      )}

      {/* Sessions List */}
      {filteredSessions.length === 0 ? (
        <div className="text-center py-12">
          <svg
            className="w-16 h-16 mx-auto text-base-content/30 mb-4"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
            />
          </svg>
          <h3 className="text-lg font-semibold mb-2">No sessions found</h3>
          <p className="text-base-content/70">
            {showActiveOnly && sessions.length > 0
              ? 'No active sessions match the current filters'
              : "Sessions will appear here as they're detected by the file watchers"}
          </p>
        </div>
      ) : (
        <>
          <div className="grid gap-4">
            {visibleSessions.map(session => {
              const isCompleted = session.assessmentStatus === 'completed'
              const isSelected = selectedSessionIds.includes(session.sessionId as string)
              const isActive = isSessionActive(session.sessionId as string, session.sessionEndTime)

              // Use ActiveSessionCard when showActiveOnly filter is enabled AND there are active sessions
              if (showActiveOnly && hasActiveSessions) {
                return (
                  <ActiveSessionCard
                    key={session.sessionId as string}
                    session={{
                      sessionId: session.sessionId as string,
                      provider: session.provider,
                      projectName: session.projectName,
                      sessionStartTime: session.sessionStartTime,
                      sessionEndTime: session.sessionEndTime,
                      durationMs: session.durationMs,
                      filePath: session.filePath as string,
                      fileSize: session.fileSize,
                      cwd: session.cwd || null,
                      gitBranch: session.gitBranch || null,
                      firstCommitHash: session.firstCommitHash || null,
                      latestCommitHash: session.latestCommitHash || null,
                      aiModelSummary: session.aiModelSummary,
                      aiModelQualityScore: session.aiModelQualityScore,
                      processingStatus: session.processingStatus,
                      assessmentStatus: session.assessmentStatus,
                      assessmentRating: session.assessmentRating,
                      syncedToServer: session.syncedToServer,
                      syncFailedReason: session.syncFailedReason,
                    }}
                    isActive={isActive}
                    isProcessing={
                      processingSessionId === session.sessionId || (bulkProcessing && isSelected)
                    }
                    onViewSession={() => handleViewSession(session.sessionId as string)}
                    onProcessSession={
                      isCompleted
                        ? undefined
                        : () =>
                            handleProcessSession(
                              session.sessionId as string,
                              session.provider,
                              session.filePath as string
                            )
                    }
                    onRateSession={handleQuickRate}
                    onSyncSession={handleSyncSession}
                    onShowSyncError={handleShowSyncError}
                    ProviderIcon={ProviderIcon}
                  />
                )
              }

              // Default SessionCard for non-active filter view
              return (
                <SessionCard
                  key={session.sessionId as string}
                  session={{
                    ...session,
                    aiModelQualityScore: session.aiModelQualityScore,
                  }}
                  isSelected={isSelected}
                  isActive={isActive}
                  onSelect={
                    selectionMode
                      ? checked => handleToggleSelection(session.sessionId as string, checked)
                      : undefined
                  }
                  onViewSession={() => handleViewSession(session.sessionId as string)}
                  onProcessSession={
                    isCompleted || selectionMode
                      ? undefined
                      : () =>
                          handleProcessSession(
                            session.sessionId as string,
                            session.provider,
                            session.filePath as string
                          )
                  }
                  onRateSession={handleQuickRate}
                  onSyncSession={handleSyncSession}
                  onShowSyncError={handleShowSyncError}
                  isProcessing={
                    processingSessionId === session.sessionId || (bulkProcessing && isSelected)
                  }
                  ProviderIcon={ProviderIcon}
                />
              )
            })}
          </div>

          {/* Load more trigger */}
          {displayCount < filteredSessions.length && (
            <div ref={loadMoreRef} className="flex items-center justify-center py-8">
              <span className="loading loading-spinner loading-md" />
              <span className="ml-3 text-sm text-base-content/70">
                Loading more... ({displayCount} of {filteredSessions.length})
              </span>
            </div>
          )}

          {/* Show completion message */}
          {displayCount >= filteredSessions.length &&
            filteredSessions.length > SESSIONS_PER_PAGE && (
              <div className="text-center py-4 text-sm text-base-content/70">
                All {filteredSessions.length} sessions loaded
              </div>
            )}
        </>
      )}

      {/* Processing Mode Selection Dialog */}
      <ProcessingModeDialog
        isOpen={modeSelectionDialog?.isOpen ?? false}
        sessionCount={modeSelectionDialog?.count ?? 0}
        onSelectMode={handleModeSelected}
        onCancel={() => {
          setModeSelectionDialog(null)
        }}
      />

      {/* Confirm Bulk Process Dialog */}
      <ConfirmDialog
        isOpen={confirmDialog?.isOpen ?? false}
        title={`Process Sessions (${processingMode === 'core' ? 'Core Metrics' : 'Full Processing'})`}
        message={`Process ${confirmDialog?.count ?? 0} session(s) with ${processingMode === 'core' ? 'core metrics only' : `full processing${hasApiKey() ? ' (including AI)' : ''}`}? This may take a few minutes.`}
        confirmText="Process"
        cancelText="Cancel"
        variant="info"
        onConfirm={handleConfirmBulkProcess}
        onCancel={() => {
          // Clear any pending timeout
          if (processAllTimeoutRef.current) {
            clearTimeout(processAllTimeoutRef.current)
            processAllTimeoutRef.current = null
          }
          setConfirmDialog(null)
          setBulkProcessing(false)
          setProcessingSessionId(null)
          setBulkProgress({ current: 0, total: 0 })
          invalidateSessions()
        }}
      />

      {/* Sync Error Modal */}
      {syncErrorModal && (
        <div className="modal modal-open">
          <div className="modal-box">
            <h3 className="font-bold text-lg mb-4">Sync Failed</h3>
            <p className="text-sm text-base-content/70 mb-2">
              Session ID: {syncErrorModal.sessionId}
            </p>
            <div className="bg-error/10 border border-error/20 rounded p-4 mb-4">
              <p className="text-sm font-mono text-error whitespace-pre-wrap break-words">
                {syncErrorModal.error}
              </p>
            </div>
            <div className="modal-action">
              <button
                className="btn btn-primary btn-sm"
                onClick={() => {
                  setSyncErrorModal(null)
                  handleSyncSession(syncErrorModal.sessionId)
                }}
              >
                Retry Upload
              </button>
              <button className="btn btn-ghost btn-sm" onClick={() => setSyncErrorModal(null)}>
                Close
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
