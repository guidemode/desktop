import type { ReactNode } from 'react'
import { renderHook, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useInvalidateSessions, useLocalSession, useLocalSessions } from '../../src/hooks/useLocalSessions'

const invoke = vi.fn()

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invoke(...args),
}))

const createQueryClient = () =>
  new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  })

const withProvider = (client: QueryClient) =>
  ({ children }: { children: ReactNode }) =>
    <QueryClientProvider client={client}>{children}</QueryClientProvider>

describe('useLocalSessions', () => {
  beforeEach(() => {
    invoke.mockReset()
  })

  it('loads sessions and transforms metrics', async () => {
    const now = new Date('2024-01-15T10:00:00.000Z').getTime()
    const row = {
      id: 'row-1',
      session_id: 'session-1',
      provider: 'claude-code',
      file_name: 'session.jsonl',
      file_path: '/tmp/session.jsonl',
      project_name: 'Project Alpha',
      project_id: 'proj-1',
      session_start_time: now - 60000,
      session_end_time: now,
      file_size: 2048,
      duration_ms: 60000,
      processing_status: 'completed',
      processed_at: now,
      assessment_status: 'completed',
      assessment_completed_at: now,
      assessment_rating: 4,
      ai_model_summary: 'summary',
      ai_model_quality_score: 0.9,
      ai_model_metadata: JSON.stringify({ notes: 'metadata' }),
      created_at: now - 1000,
      uploaded_at: now - 500,
      synced_to_server: 1,
      sync_failed_reason: null,
      response_latency_ms: 120,
      task_completion_time_ms: 450,
      read_write_ratio: 1.5,
      input_clarity_score: 0.7,
      task_success_rate: 0.8,
      iteration_count: 3,
      process_quality_score: 0.9,
      used_plan_mode: 1,
      used_todo_tracking: 0,
      interruption_rate: 0.1,
      session_length_minutes: 15,
      error_count: 2,
      fatal_errors: 0,
      duration_minutes: 1,
    }

    invoke.mockResolvedValue([row])

    const client = createQueryClient()

    try {
      const { result } = renderHook(() => useLocalSessions(), {
        wrapper: withProvider(client),
      })

      await waitFor(() => expect(result.current.loading).toBe(false))

      expect(invoke).toHaveBeenCalledWith(
        'execute_sql',
        expect.objectContaining({
          params: [],
        })
      )

      expect(result.current.sessions).toHaveLength(1)
      const session = result.current.sessions[0]
      expect(session).toMatchObject({
        id: 'row-1',
        sessionId: 'session-1',
        provider: 'claude-code',
        fileName: 'session.jsonl',
        filePath: '/tmp/session.jsonl',
        projectName: 'Project Alpha',
        projectId: 'proj-1',
        processingStatus: 'completed',
        assessmentStatus: 'completed',
        assessmentRating: 4,
        syncedToServer: true,
        metrics: {
          response_latency_ms: 120,
          task_completion_time_ms: 450,
          read_write_ratio: 1.5,
          input_clarity_score: 0.7,
          task_success_rate: 0.8,
          iteration_count: 3,
          process_quality_score: 0.9,
          used_plan_mode: true,
          used_todo_tracking: false,
          interruption_rate: 0.1,
          session_length_minutes: 15,
          error_count: 2,
          fatal_errors: 0,
        },
      })
    } finally {
      client.clear()
    }
  })

  it('applies filters to query parameters', async () => {
    invoke.mockResolvedValue([])

    const client = createQueryClient()

    const from = '2024-01-01'
    const to = '2024-01-02'

    try {
      const { result } = renderHook(
        () =>
          useLocalSessions({
            provider: 'copilot',
            projectId: 'proj-42',
            dateFilter: {
              option: 'range',
              range: { from, to },
            },
          }),
        {
          wrapper: withProvider(client),
        }
      )

      await waitFor(() => expect(result.current.loading).toBe(false))

      expect(invoke).toHaveBeenCalledTimes(1)
      const params = invoke.mock.calls[0][1].params as Array<string | number>
      const expectedFrom = new Date(from)
      expectedFrom.setHours(0, 0, 0, 0)
      const expectedTo = new Date(to)
      expectedTo.setHours(23, 59, 59, 999)

      expect(params).toEqual([
        'copilot',
        'proj-42',
        expectedFrom.getTime(),
        expectedTo.getTime(),
        expectedFrom.getTime(),
        expectedTo.getTime(),
      ])
    } finally {
      client.clear()
    }
  })
})

describe('useInvalidateSessions', () => {
  it('invalidates local sessions query', async () => {
    const client = createQueryClient()
    const invalidateSpy = vi.spyOn(client, 'invalidateQueries')

    try {
      const { result } = renderHook(() => useInvalidateSessions(), {
        wrapper: withProvider(client),
      })

      result.current()

      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['local-sessions'] })
    } finally {
      invalidateSpy.mockRestore()
      client.clear()
    }
  })
})

describe('useLocalSession', () => {
  beforeEach(() => {
    invoke.mockReset()
  })

  it('loads a single session and its content', async () => {
    const now = Date.now()

    invoke.mockImplementation(async (command, args) => {
      if (command === 'execute_sql') {
        return [
          {
            id: 'row-9',
            session_id: 'session-9',
            provider: 'claude-code',
            file_name: 'session.jsonl',
            project_name: 'Project Z',
            session_start_time: now - 1000,
            session_end_time: now,
            file_size: 1024,
            duration_ms: 1000,
            processing_status: 'completed',
            processed_at: now,
            assessment_status: 'done',
            assessment_completed_at: now,
            ai_model_summary: null,
            ai_model_quality_score: null,
            ai_model_metadata: null,
            created_at: now,
            uploaded_at: now,
            file_path: '/tmp/session.jsonl',
            response_latency_ms: 100,
            task_completion_time_ms: 200,
            read_write_ratio: 1.5,
            input_clarity_score: 0.6,
            task_success_rate: 0.9,
            iteration_count: 2,
            process_quality_score: 0.8,
            used_plan_mode: 1,
            used_todo_tracking: 0,
            interruption_rate: 0.2,
            session_length_minutes: 20,
            error_count: 1,
            fatal_errors: 0,
            improvement_tips: 'tip1\ntip2',
          },
        ]
      }

      if (command === 'read_session_file') {
        expect(args).toEqual({ filePath: '/tmp/session.jsonl' })
        return 'file-content'
      }

      throw new Error(`Unexpected command ${command}`)
    })

    const { result } = renderHook(() => useLocalSession('session-9'))

    await waitFor(() => expect(result.current.loading).toBe(false))

    expect(result.current.session).toMatchObject({
      sessionId: 'session-9',
      provider: 'claude-code',
      fileName: 'session.jsonl',
      metrics: {
        response_latency_ms: 100,
        improvement_tips: ['tip1', 'tip2'],
      },
    })
    expect(result.current.content).toBe('file-content')
    expect(result.current.error).toBeNull()
  })

  it('reports error when session is missing', async () => {
    invoke.mockImplementation(async (command) => {
      if (command === 'execute_sql') {
        return []
      }
      throw new Error(`Unexpected command ${command}`)
    })

    const { result } = renderHook(() => useLocalSession('missing'))

    await waitFor(() => expect(result.current.loading).toBe(false))

    expect(result.current.error).toBe('Session not found')
    expect(result.current.session).toBeNull()
  })
})
