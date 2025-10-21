import { renderHook, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useSessionIngest } from '../../src/hooks/useSessionIngest'

const sessionExists = vi.fn()
const insertSession = vi.fn()
const listen = vi.fn()

vi.mock('../../src/services/sessionIngestion', () => ({
  sessionExists: (...args: unknown[]) => sessionExists(...args),
  insertSession: (...args: unknown[]) => insertSession(...args),
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: unknown[]) => listen(...args),
}))

describe('useSessionIngest', () => {
  let listener: ((event: { payload: any }) => Promise<void> | void) | null = null
  let unlistenSpy: ReturnType<typeof vi.fn>

  beforeEach(() => {
    listener = null
    unlistenSpy = vi.fn()

    listen.mockImplementation(async (_event: string, callback: typeof listener) => {
      listener = callback
      return unlistenSpy
    })

    sessionExists.mockReset()
    insertSession.mockReset()
  })

  afterEach(() => {
    vi.clearAllMocks()
  })

  const fireSessionDetected = async (payloadOverrides: Partial<Record<string, unknown>> = {}) => {
    const payload = {
      provider: 'claude-code',
      project_name: 'example-project',
      session_id: 'session-123',
      file_name: 'session-123.jsonl',
      file_path: '/tmp/session-123.jsonl',
      file_size: 1024,
      session_start_time: 111,
      session_end_time: 222,
      duration_ms: 111,
      ...payloadOverrides,
    }

    await listener?.({ payload })

    return payload
  }

  it('subscribes to session detected events and stores new sessions', async () => {
    sessionExists.mockResolvedValue(false)
    insertSession.mockResolvedValue('new-id')

    const { unmount } = renderHook(() => useSessionIngest())

    await waitFor(() => expect(listen).toHaveBeenCalledTimes(1))
    expect(listener).toBeTruthy()

    const payload = await fireSessionDetected()

    expect(sessionExists).toHaveBeenCalledWith(payload.session_id, payload.file_name)
    expect(insertSession).toHaveBeenCalledWith({
      provider: payload.provider,
      projectName: payload.project_name,
      sessionId: payload.session_id,
      fileName: payload.file_name,
      filePath: payload.file_path,
      fileSize: payload.file_size,
      sessionStartTime: payload.session_start_time,
      sessionEndTime: payload.session_end_time,
      durationMs: payload.duration_ms,
    })

    unmount()
    await waitFor(() => expect(unlistenSpy).toHaveBeenCalled())
  })

  it('skips ingestion for duplicate sessions', async () => {
    sessionExists.mockResolvedValue(true)

    renderHook(() => useSessionIngest())
    await waitFor(() => expect(listen).toHaveBeenCalled())

    await fireSessionDetected()

    expect(sessionExists).toHaveBeenCalled()
    expect(insertSession).not.toHaveBeenCalled()
  })
})

