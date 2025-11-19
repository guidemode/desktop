import { renderHook, act } from '@testing-library/react'
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'
import { useSessionProcessing } from '../../src/hooks/useSessionProcessing'

const mockInvoke = vi.fn()
const mockGetProcessor = vi.fn()
const mockProcessMetrics = vi.fn()

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}))

vi.mock('@guidemode/session-processing/processors', () => ({
  ProcessorRegistry: class {
    getProcessor(provider: string) {
      return mockGetProcessor(provider)
    }
  },
}))

const FIXED_TIME = new Date('2024-01-01T00:00:00.000Z')

describe('useSessionProcessing', () => {
  let randomUUIDSpy: ReturnType<typeof vi.spyOn>

  beforeAll(() => {
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true)
    vi.useFakeTimers()
    vi.setSystemTime(FIXED_TIME)

    if (globalThis.crypto && 'randomUUID' in globalThis.crypto) {
      randomUUIDSpy = vi.spyOn(globalThis.crypto, 'randomUUID').mockReturnValue('00000000-0000-0000-0000-000000000123')
    } else {
      // Provide minimal crypto shim if unavailable (e.g., older Node versions)
      randomUUIDSpy = vi
        .spyOn(Object.assign(globalThis, { crypto: { randomUUID: () => '00000000-0000-0000-0000-000000000123' } }).crypto, 'randomUUID')
        .mockReturnValue('00000000-0000-0000-0000-000000000123')
    }
  })

  afterAll(() => {
    randomUUIDSpy?.mockRestore()
    vi.useRealTimers()
  })

  beforeEach(() => {
    mockInvoke.mockReset()
    mockGetProcessor.mockReset()
    mockProcessMetrics.mockReset()
    mockInvoke.mockClear()
    mockGetProcessor.mockClear()
    mockProcessMetrics.mockClear()
  })

  it('processes a session, stores metrics, and updates status', async () => {
    const metricsResults = [
      {
        metricType: 'performance',
        metrics: {
          response_latency_ms: 120,
          task_completion_time_ms: 450,
          metadata: { total_responses: 3 },
        },
      },
      {
        metricType: 'usage',
        metrics: {
          read_write_ratio: 1.5,
          input_clarity_score: 0.7,
          metadata: {
            read_operations: 5,
            write_operations: 2,
            total_user_messages: 4,
          },
        },
      },
      {
        metricType: 'error',
        metrics: {
          error_count: 2,
          error_types: ['timeout', 'api'],
          last_error_message: 'request failed',
          recovery_attempts: 1,
          fatal_errors: 0,
        },
      },
      {
        metricType: 'engagement',
        metrics: {
          interruption_rate: 0.1,
          session_length_minutes: 15,
          total_interruptions: 2,
          metadata: {
            total_responses: 6,
          },
        },
      },
      {
        metricType: 'quality',
        metrics: {
          task_success_rate: 0.8,
          iteration_count: 3,
          process_quality_score: 0.9,
          used_plan_mode: true,
          used_todo_tracking: false,
          over_top_affirmations: 2,
          metadata: {
            successful_operations: 5,
            total_operations: 7,
            exit_plan_mode_count: 1,
            todo_write_count: 2,
            over_top_affirmations_phrases: ['phrase-a', 'phrase-b'],
            improvement_tips: ['tip1', 'tip2'],
          },
        },
      },
    ]

    // Mock session details query to return empty (no git diff will be fetched)
    mockInvoke.mockResolvedValueOnce([])

    mockProcessMetrics.mockResolvedValue(metricsResults)
    mockGetProcessor.mockReturnValue({ processMetrics: mockProcessMetrics })

    const { result } = renderHook(() => useSessionProcessing())

    let response: unknown
    await act(async () => {
      response = await result.current.processSession(
        'session-1',
        'claude-code',
        'session content',
        'user-9'
      )
    })

    expect(response).toEqual(metricsResults)

    expect(result.current.processing).toBe(false)
    expect(result.current.error).toBeNull()

    expect(mockGetProcessor).toHaveBeenCalledWith('claude-code')
    expect(mockProcessMetrics).toHaveBeenCalledWith('session content', {
      sessionId: 'session-1',
      tenantId: 'local',
      userId: 'user-9',
      provider: 'claude-code',
    })

    expect(mockInvoke).toHaveBeenCalledTimes(3)

    // First call is to fetch session details for git diff
    const sessionDetailsCall = mockInvoke.mock.calls[0]
    expect(sessionDetailsCall[0]).toBe('execute_sql')
    expect(sessionDetailsCall[1].sql).toContain('SELECT cwd, first_commit_hash')

    const insertCall = mockInvoke.mock.calls[1]
    expect(insertCall[0]).toBe('execute_sql')
    expect(insertCall[1]).toMatchObject({
      sql: expect.stringContaining('INSERT OR REPLACE INTO session_metrics'),
    })
    expect(insertCall[1].params).toEqual([
      '00000000-0000-0000-0000-000000000123',
      'session-1',
      'claude-code',
      FIXED_TIME.getTime(),
      120,
      450,
      3,
      1.5,
      0.7,
      5,
      2,
      4,
      2,
      'timeout,api',
      'request failed',
      1,
      0,
      0.1,
      15,
      2,
      6,
      0.8,
      3,
      0.9,
      1,
      0,
      2,
      5,
      7,
      1,
      2,
      'phrase-a,phrase-b',
      null,
      null,
      null,
      'tip1\ntip2',
      null,
      'tip1\ntip2',
      null,
      null,
      null,
      null,
      null,
      null,
      null,
      null,
      null,
      null,
      null,
      null, // git_diff_improvement_tips (11th git metric)
      // Context management metrics (added in migrations 017-020)
      null, // total_input_tokens
      null, // total_output_tokens
      null, // total_cache_created
      null, // total_cache_read
      null, // context_length
      null, // context_window_size
      null, // context_utilization_percent
      null, // compact_event_count
      null, // compact_event_steps
      null, // avg_tokens_per_message
      null, // messages_until_first_compact
      null, // context_improvement_tips
      FIXED_TIME.getTime(),
    ])

    const updateCall = mockInvoke.mock.calls[2]
    expect(updateCall[0]).toBe('execute_sql')
    expect(updateCall[1]).toEqual({
      sql: expect.stringContaining('UPDATE agent_sessions'),
      params: [FIXED_TIME.getTime(), 'session-1'],
    })
  })

  it('surface errors when no processor is registered for a provider', async () => {
    mockGetProcessor.mockReturnValue(undefined)

    const { result } = renderHook(() => useSessionProcessing())

    let thrown: unknown
    await act(async () => {
      try {
        await result.current.processSession('session-2', 'unknown', 'content')
      } catch (error) {
        thrown = error
      }
    })

    expect(thrown).toBeInstanceOf(Error)
    expect((thrown as Error).message).toContain('No processor found for provider: unknown')
    expect(result.current.error).toBe('No processor found for provider: unknown')
    expect(result.current.processing).toBe(false)
    expect(mockInvoke).not.toHaveBeenCalled()
  })
})
