import { useState, useEffect, useRef, useCallback } from 'react'
import { flushSync } from 'react-dom'
import { useNavigate, useSearchParams } from 'react-router-dom'
import { SessionCard, DateFilter } from '@guideai-dev/session-processing/ui'
import type { SessionRating } from '@guideai-dev/session-processing/ui'
import ProviderIcon from '../components/icons/ProviderIcon'
import { useLocalSessions, useInvalidateSessions } from '../hooks/useLocalSessions'
import { useAiProcessing } from '../hooks/useAiProcessing'
import { useSessionProcessing } from '../hooks/useSessionProcessing'
import { useAuth } from '../hooks/useAuth'
import { useToast } from '../hooks/useToast'
import type { DateFilterValue } from '@guideai-dev/session-processing/ui'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { useSessionActivity } from '../hooks/useSessionActivity'
import { useSessionActivityStore } from '../stores/sessionActivityStore'
import ConfirmDialog from '../components/ConfirmDialog'
import { useLocalProjects } from '../hooks/useLocalProjects'
import { useQuickRating } from '../hooks/useQuickRating'

const SESSIONS_PER_PAGE = 50

export default function SessionsPage() {
  const navigate = useNavigate()
  const { user } = useAuth()
  const [searchParams, setSearchParams] = useSearchParams()
  const [dateFilter, setDateFilter] = useState<DateFilterValue>({ option: 'all' })
  const [providerFilter, setProviderFilter] = useState<string>('all')
  const [projectFilter, setProjectFilter] = useState<string>('all')
  const [clearing, setClearing] = useState(false)
  const [rescanning, setRescanning] = useState(false)
  const [processingSessionId, setProcessingSessionId] = useState<string | null>(null)
  const [selectionMode, setSelectionMode] = useState(false)
  const [selectedSessionIds, setSelectedSessionIds] = useState<string[]>([])
  const [bulkProcessing, setBulkProcessing] = useState(false)
  const [bulkProgress, setBulkProgress] = useState({ current: 0, total: 0 })
  const [syncErrorModal, setSyncErrorModal] = useState<{ sessionId: string; error: string } | null>(null)
  const [confirmDialog, setConfirmDialog] = useState<{ isOpen: boolean; count: number } | null>(null)
  const [confirmClearDialog, setConfirmClearDialog] = useState(false)
  const [displayCount, setDisplayCount] = useState(SESSIONS_PER_PAGE)
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
  const setTrackingEnabled = useSessionActivityStore(state => state.setTrackingEnabled)

  const { sessions, loading, error, refresh } = useLocalSessions({
    provider: providerFilter === 'all' ? undefined : providerFilter,
    projectId: projectFilter === 'all' ? undefined : projectFilter,
    dateFilter: dateFilter,
  })
  const invalidateSessions = useInvalidateSessions()
  const { projects } = useLocalProjects()
  const quickRatingMutation = useQuickRating()

  // Initialize project filter from URL parameter
  useEffect(() => {
    const projectParam = searchParams.get('project')
    if (projectParam) {
      setProjectFilter(projectParam)
    }
  }, [searchParams])

  // Reset display count when filters change
  useEffect(() => {
    setDisplayCount(SESSIONS_PER_PAGE)
  }, [providerFilter, dateFilter, projectFilter])

  // Infinite scroll observer
  const loadMore = useCallback(() => {
    if (displayCount < sessions.length) {
      setDisplayCount(prev => Math.min(prev + SESSIONS_PER_PAGE, sessions.length))
    }
  }, [displayCount, sessions.length])

  useEffect(() => {
    if (!loadMoreRef.current) return

    observerRef.current = new IntersectionObserver(
      (entries) => {
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
  const visibleSessions = sessions.slice(0, displayCount)

  // Listen for sync/update events from backend and invalidate cache
  useEffect(() => {
    const unlistenSynced = listen('session-synced', () => {
      invalidateSessions()
    })

    const unlistenFailed = listen('session-sync-failed', () => {
      invalidateSessions()
    })

    const unlistenUpdated = listen('session-updated', () => {
      invalidateSessions()
    })

    return () => {
      unlistenSynced.then(fn => fn())
      unlistenFailed.then(fn => fn())
      unlistenUpdated.then(fn => fn())
    }
  }, [invalidateSessions])

  const handleViewSession = (sessionId: string) => {
    navigate(`/sessions/${sessionId}`)
  }

  const handleProcessSession = async (sessionId: string, provider: string, filePath: string, silent = false) => {
    setProcessingSessionId(sessionId)
    try {
      // Load session content
      const content = await invoke<string>('get_session_content', {
        provider,
        filePath,
        sessionId,
      })

      // Parse session using the content hook's parser
      const { ProcessorRegistry } = await import('@guideai-dev/session-processing/processors')
      const registry = new ProcessorRegistry()
      const processor = registry.getProcessor(provider)

      if (!processor) {
        throw new Error(`No processor found for provider: ${provider}`)
      }

      const parsedSession = processor.parseSession(content)

      // Step 1: Calculate metrics (always)
      await processMetrics(sessionId, provider, content, 'local')

      // Step 2: Process with AI if API key is available
      if (hasApiKey()) {
        await processSessionWithAi(sessionId, parsedSession)
      }

      // Refresh sessions to show updated results
      await refresh()

      if (!silent) {
        if (hasApiKey()) {
          toast.success('Processing complete! Metrics calculated and AI summary generated.')
        } else {
          toast.success('Metrics calculated! AI processing skipped (no API key configured).')
        }
      }
    } catch (err) {
      console.error('Failed to process session:', err)
      if (!silent) {
        toast.error('Failed to process session: ' + (err as Error).message)
      }
    } finally {
      setProcessingSessionId(null)
    }
  }

  const handleBulkProcess = async () => {
    // Use ALL sessions from database, not just visible ones
    const sessionsToProcess = sessions.filter(
      (s) => selectedSessionIds.includes(s.sessionId as string) && s.assessmentStatus !== 'completed'
    )

    if (sessionsToProcess.length === 0) {
      toast.info('No sessions selected or all selected sessions already have AI processing.')
      return
    }

    // Show confirmation dialog
    setConfirmDialog({ isOpen: true, count: sessionsToProcess.length })
  }

  const handleConfirmBulkProcess = async () => {
    setConfirmDialog(null)
    cancelBulkProcessingRef.current = false

    // Use ALL sessions from database, not just visible ones
    const sessionsToProcess = sessions.filter(
      (s) => selectedSessionIds.includes(s.sessionId as string) && s.assessmentStatus !== 'completed'
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
        await handleProcessSession(session.sessionId as string, session.provider, session.filePath as string, true)
        successCount++

        // Add 2-second delay between requests to avoid rate limiting (only if using AI)
        if (hasApiKey() && i < sessionsToProcess.length - 1) {
          await new Promise((resolve) => setTimeout(resolve, 2000))
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

    if (errorCount > 0) {
      toast.warning(`Bulk processing complete!\n✓ ${successCount} successful\n✗ ${errorCount} failed`)
    } else {
      toast.success(`Bulk processing complete! ${successCount} sessions processed successfully.`)
    }
  }

  const handleProcessAll = async () => {
    // Clear any pending timeout from previous calls
    if (processAllTimeoutRef.current) {
      clearTimeout(processAllTimeoutRef.current)
      processAllTimeoutRef.current = null
    }

    // Use ALL sessions from database, not just visible ones
    const sessionsToProcess = sessions.filter((s) => s.assessmentStatus !== 'completed')

    if (sessionsToProcess.length === 0) {
      toast.info('All sessions already processed.')
      return
    }

    // Auto-select all sessions not yet processed (including those not yet loaded in view)
    const sessionIds = sessionsToProcess.map((s) => s.sessionId as string)
    setSelectedSessionIds(sessionIds)
    setSelectionMode(true)

    // Trigger bulk process
    processAllTimeoutRef.current = setTimeout(() => {
      processAllTimeoutRef.current = null
      handleBulkProcess()
    }, 100)
  }

  const handleToggleSelection = (sessionId: string, checked: boolean) => {
    flushSync(() => {
      setSelectedSessionIds((prev) => {
        if (checked) {
          // Add if not already present
          if (prev.includes(sessionId)) {
            return prev
          }
          return [...prev, sessionId]
        } else {
          // Remove if present
          return prev.filter(id => id !== sessionId)
        }
      })
    })
  }

  const handleSelectAll = (checked: boolean) => {
    if (checked) {
      // Select all visible sessions not yet completed
      const unprocessedSessionIds = visibleSessions
        .filter((s) => s.assessmentStatus !== 'completed')
        .map((s) => s.sessionId as string)
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
      toast.error('Failed to queue session: ' + (err as Error).message)
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
      toast.error('Failed to save rating: ' + (err as Error).message)
    }
  }

  const handleRescan = async () => {
    // Disable activity tracking and clear existing active sessions
    setTrackingEnabled(false)
    clearAllActiveSessions()

    setRescanning(true)
    try {
      // Rescan all enabled providers
      const providers = ['claude-code', 'github-copilot', 'opencode', 'codex']
      let totalFound = 0

      for (const provider of providers) {
        try {
          const sessions = await invoke<any[]>('scan_historical_sessions', {
            providerId: provider,
          })
          totalFound += sessions.length
        } catch (err) {
          // Provider might not be enabled, that's ok
          console.error(`Error scanning ${provider}:`, err)
        }
      }

      refresh()
      setRescanning(false)

      // Re-enable activity tracking after a 10 second delay
      setTimeout(() => {
        setTrackingEnabled(true)
      }, 10000)

      toast.success(`Rescanned and found ${totalFound} sessions. Sessions reloaded!`)
    } catch (err) {
      console.error('Error during rescan:', err)
      toast.error('Failed to rescan: ' + String(err))
      setRescanning(false)
      setTrackingEnabled(true) // Re-enable on error
    }
  }

  const handleClearAndReload = async () => {
    // Disable activity tracking and clear existing active sessions
    setTrackingEnabled(false)
    clearAllActiveSessions()

    setClearing(true)
    try {
      const result = await invoke<string>('clear_all_sessions')

      // Rescan all enabled providers
      const providers = ['claude-code', 'github-copilot', 'opencode', 'codex']
      let totalFound = 0

      for (const provider of providers) {
        try {
          const sessions = await invoke<any[]>('scan_historical_sessions', {
            providerId: provider,
          })
          totalFound += sessions.length
        } catch (err) {
          // Provider might not be enabled, that's ok
          console.error(`Error scanning ${provider}:`, err)
        }
      }

      refresh()
      setClearing(false)

      // Re-enable activity tracking after a 10 second delay
      setTimeout(() => {
        setTrackingEnabled(true)
      }, 10000)

      toast.success(`${result}\n\nRescanned and found ${totalFound} sessions.\n\nSessions reloaded!`, 8000)
    } catch (err) {
      console.error('Error during clear and rescan:', err)
      toast.error('Failed to clear and rescan: ' + String(err))
      setClearing(false)
      setTrackingEnabled(true) // Re-enable on error
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
        <div className="flex gap-2">
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
          <button
            onClick={handleRescan}
            className="btn btn-sm btn-primary btn-outline"
            disabled={rescanning}
          >
            {rescanning ? (
              <>
                <span className="loading loading-spinner loading-xs"></span>
                Rescanning...
              </>
            ) : (
              <>
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    strokeWidth={2}
                    d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
                  />
                </svg>
                Rescan
              </>
            )}
          </button>
          <button
            onClick={() => setConfirmClearDialog(true)}
            className="btn btn-sm btn-error btn-outline"
            disabled={clearing}
          >
            {clearing ? (
              <>
                <span className="loading loading-spinner loading-xs"></span>
                Clearing...
              </>
            ) : (
              <>
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    strokeWidth={2}
                    d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
                  />
                </svg>
                Clear & Rescan
              </>
            )}
          </button>
        </div>
      </div>

      {/* Filters - Always visible */}
      <div className="flex items-center gap-3 mb-4">
        {/* Spacer to push filters to the right */}
        <div className="flex-1"></div>

        {/* Right side: Filters */}
        <DateFilter value={dateFilter} onChange={setDateFilter} />
        <select
          className="select select-bordered select-sm"
          value={providerFilter}
          onChange={(e) => setProviderFilter(e.target.value)}
        >
          <option value="all">All Providers</option>
          <option value="claude-code">Claude Code</option>
          <option value="github-copilot">GitHub Copilot</option>
          <option value="opencode">OpenCode</option>
          <option value="codex">Codex</option>
        </select>
        <select
          className="select select-bordered select-sm"
          value={projectFilter}
          onChange={(e) => {
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
          {projects.map((project) => (
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
                checked={selectedSessionIds.length === visibleSessions.filter((s) => s.assessmentStatus !== 'completed').length && selectedSessionIds.length > 0}
                onChange={(e) => handleSelectAll(e.target.checked)}
              />
              <span className="text-sm">
                {selectedSessionIds.length > 0
                  ? `${selectedSessionIds.length} selected`
                  : 'Select All Visible'
                }
              </span>
            </>
          ) : (
            <>
              <button
                onClick={() => setSelectionMode(true)}
                className="btn btn-sm btn-outline"
              >
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-6 9l2 2 4-4" />
                </svg>
                Select
              </button>
              <button onClick={handleProcessAll} className="btn btn-sm btn-primary">
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 10V3L4 14h7v7l9-11h-7z" />
                </svg>
                Process All
              </button>
            </>
          )}

          {/* Spacer */}
          <div className="flex-1"></div>

          {/* Action Buttons (in selection mode) */}
          {selectionMode && (
            <>
              <button
                onClick={handleBulkProcess}
                className="btn btn-sm btn-primary"
                disabled={selectedSessionIds.length === 0 || bulkProcessing}
              >
                {bulkProcessing ? (
                  <>
                    <span className="loading loading-spinner loading-xs"></span>
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
      {sessions.length === 0 ? (
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
            Sessions will appear here as they're detected by the file watchers
          </p>
        </div>
      ) : (
        <>
          <div className="grid gap-4">
            {visibleSessions.map((session) => {
              const isCompleted = session.assessmentStatus === 'completed'
              const isSelected = selectedSessionIds.includes(session.sessionId as string)
              return (
                <SessionCard
                  key={session.sessionId as string}
                  session={{
                    ...session,
                    aiModelQualityScore: session.aiModelQualityScore,
                  }}
                  isSelected={isSelected}
                  isActive={isSessionActive(session.sessionId as string)}
                  onSelect={selectionMode ? (checked) => handleToggleSelection(session.sessionId as string, checked) : undefined}
                  onViewSession={() => handleViewSession(session.sessionId as string)}
                  onProcessSession={
                    isCompleted || selectionMode
                      ? undefined
                      : () => handleProcessSession(session.sessionId as string, session.provider, session.filePath as string)
                  }
                  onRateSession={handleQuickRate}
                  onSyncSession={handleSyncSession}
                  onShowSyncError={handleShowSyncError}
                  isProcessing={processingSessionId === session.sessionId || (bulkProcessing && isSelected)}
                  ProviderIcon={ProviderIcon}
                />
              )
            })}
          </div>

          {/* Load more trigger */}
          {displayCount < sessions.length && (
            <div ref={loadMoreRef} className="flex items-center justify-center py-8">
              <span className="loading loading-spinner loading-md" />
              <span className="ml-3 text-sm text-base-content/70">
                Loading more... ({displayCount} of {sessions.length})
              </span>
            </div>
          )}

          {/* Show completion message */}
          {displayCount >= sessions.length && sessions.length > SESSIONS_PER_PAGE && (
            <div className="text-center py-4 text-sm text-base-content/70">
              All {sessions.length} sessions loaded
            </div>
          )}
        </>
      )}

      {/* Confirm Bulk Process Dialog */}
      <ConfirmDialog
        isOpen={confirmDialog?.isOpen ?? false}
        title="Process Sessions"
        message={`Process ${confirmDialog?.count ?? 0} session(s)${hasApiKey() ? ' with AI' : ''}? This may take a few minutes.`}
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
            <p className="text-sm text-base-content/70 mb-2">Session ID: {syncErrorModal.sessionId}</p>
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
              <button
                className="btn btn-ghost btn-sm"
                onClick={() => setSyncErrorModal(null)}
              >
                Close
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Clear and Rescan Confirmation Dialog */}
      <ConfirmDialog
        isOpen={confirmClearDialog}
        title="Clear and Rescan All Sessions?"
        message={`⚠️ This will delete ALL sessions and metrics from the local database, including AI-generated summaries and quality assessments.

This action cannot be undone and may require re-processing sessions with AI (which may incur costs).

After clearing, all sessions will be rescanned from source files.`}
        confirmText="Clear & Rescan"
        cancelText="Cancel"
        variant="error"
        onConfirm={() => {
          setConfirmClearDialog(false)
          handleClearAndReload()
        }}
        onCancel={() => setConfirmClearDialog(false)}
      />
    </div>
  )
}
