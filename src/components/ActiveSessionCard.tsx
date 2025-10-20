import {
  TimelineGroup as TimelineGroupComponent,
  TimelineMessage as TimelineMessageComponent,
  isTimelineGroup,
} from '@guideai-dev/session-processing/ui'
import { RatingBadge } from '@guideai-dev/session-processing/ui'
import type { SessionRating } from '@guideai-dev/session-processing/ui'
import {
  ArrowDownIcon,
  ArrowUpIcon,
  ChartBarIcon,
  CloudArrowUpIcon,
  ExclamationTriangleIcon,
  SparklesIcon,
  XCircleIcon,
} from '@heroicons/react/24/outline'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { useEffect } from 'react'
import { useLocalSessionContent } from '../hooks/useLocalSessionContent'

interface ActiveSessionCardProps {
  session: {
    sessionId: string
    provider: string
    projectName: string
    sessionStartTime: string | null
    sessionEndTime: string | null
    durationMs: number | null
    filePath: string
    fileSize: number | null
    cwd: string | null
    gitBranch: string | null
    firstCommitHash: string | null
    latestCommitHash: string | null
    aiModelSummary?: string | null
    aiModelQualityScore?: number | null
    processingStatus: string
    assessmentStatus: string
    assessmentRating?: string | null
    syncedToServer?: boolean
    syncFailedReason?: string | null
  }
  isActive: boolean
  isProcessing?: boolean
  onViewSession: () => void
  onProcessSession?: () => void
  onRateSession?: (sessionId: string, rating: SessionRating) => void | Promise<void>
  onSyncSession?: (sessionId: string) => void
  onShowSyncError?: (sessionId: string, error: string) => void
  ProviderIcon: React.ComponentType<{ providerId: string; className?: string; size?: number }>
}

interface GitDiffStats {
  additions: number
  deletions: number
}

export function ActiveSessionCard({
  session,
  isActive,
  isProcessing = false,
  onViewSession,
  onProcessSession,
  onRateSession,
  onSyncSession,
  onShowSyncError,
  ProviderIcon,
}: ActiveSessionCardProps) {
  const queryClient = useQueryClient()

  // Debug logging for session data
  useEffect(() => {
    console.log('[ActiveSessionCard] Rendering with session:', {
      sessionId: session.sessionId,
      cwd: session.cwd,
      firstCommitHash: session.firstCommitHash,
      latestCommitHash: session.latestCommitHash,
      isActive,
      gitBranch: session.gitBranch,
    })
  }, [
    session.sessionId,
    session.cwd,
    session.firstCommitHash,
    session.latestCommitHash,
    isActive,
    session.gitBranch,
  ])

  // Fetch session content for transcript
  const { timeline, loading: contentLoading } = useLocalSessionContent(
    session.sessionId,
    session.provider,
    session.filePath
  )

  // Listen for session-updated events and invalidate content cache
  useEffect(() => {
    const unlistenUpdated = listen<string>('session-updated', event => {
      // Only invalidate if this is the session that was updated
      if (event.payload === session.sessionId) {
        queryClient.invalidateQueries({
          queryKey: ['session-content', session.sessionId, session.provider, session.filePath],
        })
        queryClient.invalidateQueries({
          queryKey: ['active-session-git-diff-stats', session.sessionId],
        })
      }
    })

    return () => {
      unlistenUpdated.then(fn => fn())
    }
  }, [session.sessionId, session.provider, session.filePath, queryClient])

  // Fetch git diff stats
  const { data: gitDiffStats } = useQuery({
    queryKey: ['active-session-git-diff-stats', session.sessionId],
    queryFn: async () => {
      console.log('[ActiveSessionCard] Git diff query running', {
        sessionId: session.sessionId,
        cwd: session.cwd,
        firstCommitHash: session.firstCommitHash,
        latestCommitHash: session.latestCommitHash,
        isActive,
      })

      if (!session.cwd || !session.firstCommitHash) {
        console.log('[ActiveSessionCard] Git diff query skipped - missing cwd or firstCommitHash')
        return null
      }

      const diffs = await invoke<any[]>('get_session_git_diff', {
        cwd: session.cwd,
        firstCommitHash: session.firstCommitHash,
        latestCommitHash: session.latestCommitHash || null,
        isActive: true, // Always show unstaged for active sessions
      })

      console.log('[ActiveSessionCard] Git diff response', { diffCount: diffs.length, diffs })

      const stats = diffs.reduce(
        (acc: GitDiffStats, file: any) => ({
          additions: acc.additions + (file.stats?.additions || 0),
          deletions: acc.deletions + (file.stats?.deletions || 0),
        }),
        { additions: 0, deletions: 0 }
      )

      console.log('[ActiveSessionCard] Git diff stats', stats)
      return stats
    },
    enabled: !!session.cwd && !!session.firstCommitHash && isActive,
    refetchInterval: 5000, // Refetch every 5 seconds for real-time updates
  })

  // Get the latest message from timeline (last 1 item)
  const getLatestItems = () => {
    if (!timeline?.items || timeline.items.length === 0) return []

    // Get the last item
    return timeline.items.slice(-1)
  }

  const latestItems = getLatestItems()

  // Format helpers
  const formatDuration = (ms: number | null): string => {
    if (!ms) return '0m'
    const minutes = Math.floor(ms / 60000)
    const hours = Math.floor(minutes / 60)
    if (hours > 0) {
      return `${hours}h ${minutes % 60}m`
    }
    return `${minutes}m`
  }

  const formatTime = (dateString: string | null) => {
    if (!dateString) return 'N/A'
    return new Date(dateString).toLocaleTimeString()
  }

  const formatShortDate = (dateString: string | null) => {
    if (!dateString) return 'N/A'
    return new Date(dateString).toLocaleDateString('en-US', {
      month: 'short',
      day: 'numeric',
    })
  }

  const formatFileSize = (bytes: number | null) => {
    if (bytes === null || bytes === undefined) return null
    if (bytes === 0) return '0 B'
    const k = 1024
    const sizes = ['B', 'KB', 'MB', 'GB']
    const i = Math.floor(Math.log(bytes) / Math.log(k))
    return `${Number.parseFloat((bytes / k ** i).toFixed(1))} ${sizes[i]}`
  }

  const formatCommitRange = () => {
    const firstCommit = session.firstCommitHash?.substring(0, 7)
    const latestCommit = session.latestCommitHash?.substring(0, 7)

    if (!firstCommit) return null

    if (!latestCommit || firstCommit === latestCommit) {
      return (
        <span
          className="font-mono text-xs"
          title={`Unstaged changes from ${session.firstCommitHash}`}
        >
          {firstCommit} → <span className="text-warning">UNSTAGED</span>
        </span>
      )
    }

    return (
      <span
        className="font-mono text-xs"
        title={`Changes from ${session.firstCommitHash} to ${session.latestCommitHash}`}
      >
        {firstCommit} → {latestCommit}
      </span>
    )
  }

  // Status helpers
  const getProcessingStatusInfo = (status: string) => {
    const isMetricsOnly = !session.filePath

    if (isMetricsOnly) {
      return {
        icon: <ChartBarIcon className="w-4 h-4" strokeWidth={2} />,
        color: 'text-success',
        bgColor: 'bg-success/20',
        label: 'Metrics Only',
        clickable: false,
      }
    }

    switch (status) {
      case 'pending':
        return {
          icon: <SparklesIcon className="w-4 h-4" strokeWidth={2} />,
          color: 'text-base-content/30',
          bgColor: 'bg-base-200',
          label: 'Pending AI Processing',
          clickable: true,
        }
      case 'processing':
        return {
          icon: null,
          color: 'text-info',
          bgColor: 'bg-info/20',
          label: 'AI Processing...',
          clickable: false,
        }
      case 'completed':
        return {
          icon: <SparklesIcon className="w-4 h-4" strokeWidth={2} />,
          color: 'text-success',
          bgColor: 'bg-success/20',
          label: 'AI Processed',
          clickable: false,
        }
      case 'failed':
        return {
          icon: <XCircleIcon className="w-4 h-4" strokeWidth={2} />,
          color: 'text-error',
          bgColor: 'bg-error/20',
          label: 'AI Processing Failed',
          clickable: true,
        }
      default:
        return {
          icon: <SparklesIcon className="w-4 h-4" strokeWidth={2} />,
          color: 'text-base-content/30',
          bgColor: 'bg-base-200',
          label: 'Pending AI Processing',
          clickable: true,
        }
    }
  }

  const getSyncStatusInfo = () => {
    if (session.syncFailedReason) {
      return {
        icon: <ExclamationTriangleIcon className="w-4 h-4" strokeWidth={2} />,
        color: 'text-error',
        bgColor: 'bg-error/20',
        label: 'Sync Failed',
        clickable: true,
      }
    }
    if (session.syncedToServer) {
      return {
        icon: <CloudArrowUpIcon className="w-4 h-4" strokeWidth={2} />,
        color: 'text-success',
        bgColor: 'bg-success/20',
        label: 'Synced',
        clickable: false,
      }
    }
    return {
      icon: <CloudArrowUpIcon className="w-4 h-4" strokeWidth={2} />,
      color: 'text-base-content/30',
      bgColor: 'bg-base-200',
      label: 'Not synced',
      clickable: true,
    }
  }

  const handleProcessClick = (e: React.MouseEvent) => {
    e.preventDefault()
    e.stopPropagation()
    if (onProcessSession && !isProcessing) {
      onProcessSession()
    }
  }

  const handleSyncClick = (e: React.MouseEvent) => {
    e.preventDefault()
    e.stopPropagation()

    if (session.syncFailedReason && onShowSyncError) {
      onShowSyncError(session.sessionId, session.syncFailedReason)
    } else if (!session.syncedToServer && onSyncSession) {
      onSyncSession(session.sessionId)
    }
  }

  const handleRate = (rating: SessionRating) => {
    if (onRateSession) {
      onRateSession(session.sessionId, rating)
    }
  }

  const processingInfo = getProcessingStatusInfo(session.processingStatus)
  const syncInfo = getSyncStatusInfo()
  const actuallyProcessing = isProcessing || session.processingStatus === 'processing'

  return (
    <div
      className="card bg-base-100 border-2 border-success shadow-lg hover:shadow-xl transition-all cursor-pointer"
      onClick={onViewSession}
    >
      <div className="card-body p-5 space-y-4">
        {/* Top Row: Provider + Project Info + Status */}
        <div className="flex items-start justify-between gap-3">
          <div className="flex items-start gap-3 flex-1 min-w-0">
            <div className="flex-shrink-0 mt-0.5">
              <ProviderIcon providerId={session.provider} className="w-7 h-7" />
            </div>

            <div className="flex-1 min-w-0">
              <div className="flex items-center gap-2 flex-wrap">
                <h3 className="text-lg font-semibold">{session.projectName}</h3>
                {session.gitBranch && (
                  <span className="badge badge-ghost badge-sm font-mono">{session.gitBranch}</span>
                )}
                {formatCommitRange()}
              </div>

              <div className="flex flex-wrap items-center gap-3 text-sm text-base-content/70 mt-1">
                {session.aiModelQualityScore !== null &&
                  session.aiModelQualityScore !== undefined && (
                    <div className="flex items-center gap-1">
                      <span>Quality:</span>
                      <span
                        className={`font-medium ${
                          session.aiModelQualityScore >= 80
                            ? 'text-success'
                            : session.aiModelQualityScore >= 60
                              ? 'text-warning'
                              : 'text-error'
                        }`}
                      >
                        {session.aiModelQualityScore}%
                      </span>
                    </div>
                  )}
                {session.filePath && formatFileSize(session.fileSize) && (
                  <div>Size: {formatFileSize(session.fileSize)}</div>
                )}
                <div>Duration: {formatDuration(session.durationMs)}</div>
                <div className="hidden sm:block">
                  {formatTime(session.sessionStartTime)} → {formatTime(session.sessionEndTime)}
                </div>
                <div className="font-medium">{formatShortDate(session.sessionStartTime)}</div>
                {/* Git diff stats badges */}
                {gitDiffStats && (gitDiffStats.additions > 0 || gitDiffStats.deletions > 0) && (
                  <div className="flex items-center gap-2">
                    <div className="flex items-center gap-1">
                      <ArrowUpIcon className="w-3.5 h-3.5 text-success" />
                      <span className="text-success font-mono text-xs font-medium">
                        {gitDiffStats.additions}
                      </span>
                    </div>
                    <div className="flex items-center gap-1">
                      <ArrowDownIcon className="w-3.5 h-3.5 text-error" />
                      <span className="text-error font-mono text-xs font-medium">
                        {gitDiffStats.deletions}
                      </span>
                    </div>
                  </div>
                )}
              </div>
            </div>
          </div>

          {/* Status badges */}
          <div className="flex items-center gap-2 flex-shrink-0">
            {isActive && (
              <span className="badge badge-success gap-1.5 animate-pulse">
                <span className="relative flex h-2 w-2 items-center justify-center">
                  <span className="animate-ping absolute h-full w-full rounded-full bg-white opacity-75" />
                  <span className="relative rounded-full h-2 w-2 bg-white" />
                </span>
                <span>LIVE</span>
              </span>
            )}

            {/* Processing Status */}
            <div
              className={`tooltip tooltip-left flex items-center justify-center w-8 h-8 rounded-md ${processingInfo.bgColor} ${
                processingInfo.clickable && !actuallyProcessing
                  ? 'cursor-pointer hover:scale-110'
                  : ''
              } transition-all`}
              data-tip={actuallyProcessing ? 'Processing...' : processingInfo.label}
              onClick={
                processingInfo.clickable && !actuallyProcessing ? handleProcessClick : undefined
              }
            >
              {actuallyProcessing ? (
                <span className="loading loading-spinner loading-xs text-info" />
              ) : (
                <span className={processingInfo.color}>{processingInfo.icon}</span>
              )}
            </div>

            {/* Rating */}
            {onRateSession && (
              <div onClick={e => e.stopPropagation()}>
                <RatingBadge
                  rating={(session.assessmentRating as SessionRating) || null}
                  onRate={handleRate}
                  disabled={actuallyProcessing}
                  size="md"
                  compact={true}
                />
              </div>
            )}

            {/* Sync Status */}
            {(onSyncSession || onShowSyncError) && (
              <div
                className={`tooltip tooltip-left flex items-center justify-center w-8 h-8 rounded-md ${syncInfo.bgColor} ${
                  syncInfo.clickable ? 'cursor-pointer hover:scale-110' : ''
                } transition-all`}
                data-tip={syncInfo.label}
                onClick={syncInfo.clickable ? handleSyncClick : undefined}
              >
                <span className={syncInfo.color}>{syncInfo.icon}</span>
              </div>
            )}
          </div>
        </div>

        {/* AI Summary */}
        {session.aiModelSummary && (
          <div className="text-sm text-base-content/70 bg-base-200 rounded p-3">
            {session.aiModelSummary}
          </div>
        )}

        {/* Latest Messages */}
        <div className="border-t border-base-300 pt-4">
          <div className="flex items-center gap-2 mb-3">
            <span className="text-sm font-semibold text-base-content/70 uppercase">
              Latest Activity
            </span>
            {contentLoading && <span className="loading loading-spinner loading-xs" />}
          </div>

          {latestItems.length > 0 ? (
            <div className="max-h-96 overflow-y-auto space-y-2">
              {latestItems.map(item => {
                if (isTimelineGroup(item)) {
                  return <TimelineGroupComponent key={item.id} group={item} />
                }
                return <TimelineMessageComponent key={item.id} message={item} />
              })}
            </div>
          ) : (
            <p className="text-sm text-base-content/50 italic bg-base-200 rounded p-3">
              No messages yet
            </p>
          )}
        </div>
      </div>
    </div>
  )
}
