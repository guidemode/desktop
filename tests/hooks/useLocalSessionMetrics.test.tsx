import type { ReactNode } from 'react'
import { renderHook, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useLocalSessionMetrics } from '../../src/hooks/useLocalSessionMetrics'

const invoke = vi.fn()

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invoke(...args),
}))

const createQueryClient = () =>
  new QueryClient({
    defaultOptions: {
      queries: { retry: false },
    },
  })

const withProvider = (client: QueryClient) =>
  ({ children }: { children: ReactNode }) =>
    <QueryClientProvider client={client}>{children}</QueryClientProvider>

describe('useLocalSessionMetrics', () => {
  beforeEach(() => {
    invoke.mockReset()
  })

  it('returns null when no metrics exist', async () => {
    invoke.mockResolvedValue([])

    const client = createQueryClient()

    const { result } = renderHook(() => useLocalSessionMetrics('session-1'), {
      wrapper: withProvider(client),
    })

    await waitFor(() => expect(result.current.loading).toBe(false))

    expect(invoke).toHaveBeenCalledWith('execute_sql', expect.any(Object))
    expect(result.current.metrics).toBeNull()

    client.clear()
  })

  it('maps database row to UI metrics format', async () => {
    invoke.mockResolvedValue([
      {
        created_at: 1700000000000,
        read_write_ratio: 1.5,
        input_clarity_score: 0.7,
        usage_improvement_tips: 'tip1\ntip2',
        error_count: 2,
        fatal_errors: 1,
        recovery_attempts: 3,
        error_types: 'timeout,api',
        last_error_message: 'boom',
        interruption_rate: 0.1,
        session_length_minutes: 12,
        task_success_rate: 0.9,
        iteration_count: 4,
        process_quality_score: 0.95,
        used_plan_mode: 1,
        used_todo_tracking: 0,
        over_top_affirmations: 2,
        exit_plan_mode_count: 1,
        todo_write_count: 3,
        over_top_affirmations_phrases: 'phrase1,phrase2',
        quality_improvement_tips: 'tip1\ntip2',
        response_latency_ms: 120,
        task_completion_time_ms: 480,
      },
    ])

    const client = createQueryClient()

    const { result } = renderHook(() => useLocalSessionMetrics('session-2'), {
      wrapper: withProvider(client),
    })

    await waitFor(() => expect(result.current.loading).toBe(false))

    expect(result.current.metrics).toMatchObject({
      createdAt: new Date(1700000000000).toISOString(),
      usage: {
        readWriteRatio: '1.5',
        inputClarityScore: '0.7',
        improvementTips: ['tip1', 'tip2'],
      },
      error: {
        errorCount: 2,
        fatalErrors: 1,
        recoveryAttempts: 3,
        errorTypes: ['timeout', 'api'],
        lastErrorMessage: 'boom',
      },
      engagement: {
        interruptionRate: '0.1',
        sessionLengthMinutes: '12',
      },
      quality: {
        taskSuccessRate: '0.9',
        iterationCount: 4,
        processQualityScore: '0.95',
        usedPlanMode: true,
        usedTodoTracking: false,
        overTopAffirmations: 2,
        overTopAffirmationsPhrases: ['phrase1', 'phrase2'],
        improvementTips: ['tip1', 'tip2'],
      },
      performance: {
        responseLatencyMs: '120',
        taskCompletionTimeMs: '480',
      },
    })

    client.clear()
  })
})

