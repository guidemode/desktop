import { act, renderHook } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useBackgroundProcessing } from './useBackgroundProcessing'

const invoke = vi.fn()
const processSession = vi.fn()

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invoke(...args),
}))

vi.mock('./useSessionProcessing', () => ({
  useSessionProcessing: () => ({
    processSession: (...args: unknown[]) => processSession(...args),
  }),
}))

describe('useBackgroundProcessing', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    invoke.mockReset()
    processSession.mockReset()
  })

  afterEach(() => {
    vi.runOnlyPendingTimers()
    vi.useRealTimers()
    vi.clearAllMocks()
  })

  it('processes unprocessed sessions when processNow is invoked', async () => {
    invoke.mockImplementation(async (command, args) => {
      if (command === 'execute_sql') {
        expect(args).toEqual({
          sql: expect.stringContaining('LEFT JOIN session_metrics'),
          params: [],
        })
        return [
          {
            session_id: 'session-1',
            provider: 'claude',
            file_path: '/tmp/a.jsonl',
          },
          {
            session_id: 'session-2',
            provider: 'copilot',
            file_path: '/tmp/b.jsonl',
          },
        ]
      }

      if (command === 'get_session_content') {
        return `content-for-${(args as { sessionId: string }).sessionId}`
      }

      throw new Error(`Unexpected command ${command}`)
    })

    processSession.mockResolvedValue(undefined)

    const { result } = renderHook(() => useBackgroundProcessing())

    await act(async () => {
      await result.current.processNow()
    })

    expect(processSession).toHaveBeenCalledTimes(2)
    expect(processSession).toHaveBeenNthCalledWith(
      1,
      'session-1',
      'claude',
      'content-for-session-1',
      'local'
    )
    expect(processSession).toHaveBeenNthCalledWith(
      2,
      'session-2',
      'copilot',
      'content-for-session-2',
      'local'
    )
    expect(result.current.isProcessing).toBe(false)
  })

  it('toggles enabled state via helpers', () => {
    invoke.mockResolvedValue([])

    const { result } = renderHook(() => useBackgroundProcessing())

    expect(result.current.isEnabled).toBe(false)

    act(() => {
      result.current.enable()
    })
    expect(result.current.isEnabled).toBe(true)

    act(() => {
      result.current.disable()
    })
    expect(result.current.isEnabled).toBe(false)
  })

  it('guards against concurrent processing', async () => {
    invoke.mockImplementation(async (command) => {
      if (command === 'execute_sql') {
        return [
          {
            session_id: 'session-3',
            provider: 'claude',
            file_path: '/tmp/c.jsonl',
          },
        ]
      }
      if (command === 'get_session_content') {
        return 'content'
      }
      throw new Error(`Unexpected command ${command}`)
    })

    const deferred = (() => {
      let resolve: () => void
      const promise = new Promise<void>(res => {
        resolve = res
      })
      return { promise, resolve: resolve! }
    })()

    processSession.mockImplementation(() => deferred.promise)

    const { result } = renderHook(() => useBackgroundProcessing())

    const firstCall = result.current.processNow()
    await act(async () => {
      await Promise.resolve()
    })
    expect(processSession).toHaveBeenCalledTimes(1)

    const secondCall = result.current.processNow()
    await act(async () => {
      await Promise.resolve()
    })
    expect(processSession).toHaveBeenCalledTimes(1)

    await act(async () => {
      deferred.resolve()
      await Promise.resolve()
    })

    await firstCall
    await secondCall
    await act(async () => {
      await Promise.resolve()
    })
    expect(result.current.isProcessing).toBe(false)
  })
})
