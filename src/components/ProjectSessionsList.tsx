import { DateFilter, SessionCard } from '@guideai-dev/session-processing/ui'
import type { DateFilterValue, SessionRating } from '@guideai-dev/session-processing/ui'
import { invoke } from '@tauri-apps/api/core'
import { useCallback, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useLocalSessions } from '../hooks/useLocalSessions'
import { useQuickRating } from '../hooks/useQuickRating'
import { useToast } from '../hooks/useToast'
import { useSessionActivityStore } from '../stores/sessionActivityStore'
import ProviderIcon from './icons/ProviderIcon'

interface ProjectSessionsListProps {
  projectId: string
}

const SESSIONS_LIMIT = 50

export function ProjectSessionsList({ projectId }: ProjectSessionsListProps) {
  const navigate = useNavigate()
  const toast = useToast()
  const { activeSessions } = useSessionActivityStore()

  const [dateFilter, setDateFilter] = useState<DateFilterValue>({ option: 'all' })
  const [showActiveOnly, setShowActiveOnly] = useState(false)

  // Fetch sessions for this project
  const { sessions, loading } = useLocalSessions({
    projectId,
    dateFilter,
  })

  // Filter for active sessions if toggle is on, then limit to last 50
  const filteredSessions = (
    showActiveOnly ? sessions.filter(session => activeSessions.has(session.sessionId)) : sessions
  ).slice(0, SESSIONS_LIMIT)

  const handleViewSession = (sessionId: string) => {
    navigate(`/sessions/${sessionId}`)
  }

  const quickRateMutation = useQuickRating()

  const handleQuickRate = useCallback(
    (sessionId: string, rating: SessionRating) => {
      quickRateMutation.mutate(
        { sessionId, rating },
        {
          onSuccess: () => {
            toast.success('Rating saved')
          },
          onError: (error: Error) => {
            toast.error(`Failed to save rating: ${error.message}`)
          },
        }
      )
    },
    [quickRateMutation, toast]
  )

  const handleSyncSession = useCallback(
    async (sessionId: string) => {
      try {
        toast.info('Syncing session...')
        await invoke('upload_session_to_queue', { sessionId })
        toast.success('Session queued for upload')
      } catch (error) {
        toast.error(`Failed to sync: ${error}`)
      }
    },
    [toast]
  )

  const handleShowSyncError = useCallback(
    (error: string) => {
      toast.error(error, 10000)
    },
    [toast]
  )

  if (loading) {
    return (
      <div className="flex items-center justify-center py-12">
        <span className="loading loading-spinner loading-lg" />
      </div>
    )
  }

  if (sessions.length === 0) {
    return (
      <div className="card bg-base-100 border border-base-300">
        <div className="card-body items-center text-center py-12">
          <p className="text-base-content/60">No sessions found for this project</p>
        </div>
      </div>
    )
  }

  return (
    <div className="space-y-4">
      {/* Filters */}
      <div className="flex items-center gap-3 flex-wrap">
        {/* Active Only Toggle */}
        <label className="flex items-center gap-2 cursor-pointer">
          <input
            type="checkbox"
            className="toggle toggle-primary"
            checked={showActiveOnly}
            onChange={e => setShowActiveOnly(e.target.checked)}
          />
          <span className="text-sm font-medium">Active Only</span>
          {showActiveOnly && activeSessions.size > 0 && (
            <span className="badge badge-primary badge-sm">{activeSessions.size}</span>
          )}
        </label>

        {/* Date Filter */}
        <DateFilter value={dateFilter} onChange={setDateFilter} />

        {/* Results Count */}
        <div className="text-sm text-base-content/60 ml-auto">
          Showing {filteredSessions.length} of {sessions.length}{' '}
          {sessions.length === 1 ? 'session' : 'sessions'}
          {sessions.length > SESSIONS_LIMIT && ' (most recent)'}
        </div>
      </div>

      {/* Sessions List */}
      {filteredSessions.length === 0 ? (
        <div className="card bg-base-100 border border-base-300">
          <div className="card-body items-center text-center py-12">
            <p className="text-base-content/60">
              No {showActiveOnly ? 'active ' : ''}sessions found
            </p>
          </div>
        </div>
      ) : (
        <div className="grid gap-4">
          {filteredSessions.map(session => {
            const isActive = activeSessions.has(session.sessionId)

            return (
              <SessionCard
                key={session.sessionId}
                session={{
                  ...session,
                  aiModelQualityScore: session.aiModelQualityScore,
                }}
                isActive={isActive}
                onViewSession={() => handleViewSession(session.sessionId)}
                onRateSession={handleQuickRate}
                onSyncSession={handleSyncSession}
                onShowSyncError={handleShowSyncError}
                isProcessing={false}
                ProviderIcon={ProviderIcon}
              />
            )
          })}
        </div>
      )}
    </div>
  )
}
