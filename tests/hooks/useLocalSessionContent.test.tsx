import { renderHook, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { ReactNode } from 'react'
import { useLocalSessionContent } from '../../src/hooks/useLocalSessionContent'

const invoke = vi.fn()
const getParser = vi.fn()
const parseSession = vi.fn()
const processTimeline = vi.fn()

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invoke(...args),
}))

vi.mock('@guidemode/session-processing/ui', () => ({
  parserRegistry: {
    getParser: (...args: unknown[]) => getParser(...args),
  },
  messageProcessorRegistry: {
    getProcessor: () => ({
      process: (...args: unknown[]) => processTimeline(...args),
    }),
  },
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

describe('useLocalSessionContent', () => {
  beforeEach(() => {
    invoke.mockReset()
    getParser.mockReset()
    parseSession.mockReset()
    processTimeline.mockReset()
  })

  it('fetches and processes session content', async () => {
    invoke.mockResolvedValue('session-content')
    const parsedMessages = [{ id: 'msg-1' }]
    const parsedSession = {
      sessionId: 'session-1',
      provider: 'claude-code',
      messages: parsedMessages,
      startTime: new Date(),
      endTime: new Date(),
      duration: 1000,
    }
    parseSession.mockReturnValue(parsedSession)
    getParser.mockReturnValue({
      parseSession: parseSession,
    })
    const processedTimeline = { events: [] }
    processTimeline.mockReturnValue(processedTimeline)

    const client = createQueryClient()

    const { result } = renderHook(
      () => useLocalSessionContent('session-1', 'claude-code', '/tmp/session.jsonl'),
      {
        wrapper: withProvider(client),
      }
    )

    await waitFor(() => expect(result.current.loading).toBe(false))

    expect(invoke).toHaveBeenCalledWith('get_session_content', {
      provider: 'claude-code',
      filePath: '/tmp/session.jsonl',
      sessionId: 'session-1',
    })
    expect(getParser).toHaveBeenCalledWith('claude-code')
    expect(parseSession).toHaveBeenCalledWith('session-content')
    expect(processTimeline).toHaveBeenCalledWith(parsedMessages)

    expect(result.current.messages).toEqual(parsedMessages)
    expect(result.current.timeline).toEqual(processedTimeline)
    expect(result.current.fileContent).toBe('session-content')
    expect(result.current.error).toBeNull()

    client.clear()
  })

  it('handles missing parser errors', async () => {
    invoke.mockResolvedValue('session-content')
    getParser.mockReturnValue(null)

    const client = createQueryClient()

    const { result } = renderHook(
      () => useLocalSessionContent('session-1', 'claude-code', '/tmp/session.jsonl'),
      {
        wrapper: withProvider(client),
      }
    )

    await waitFor(() => expect(result.current.loading).toBe(false))

    expect(result.current.error).toBe('No parser found for provider: claude-code')
    expect(result.current.messages).toEqual([])
    expect(result.current.timeline).toBeNull()

    client.clear()
  })
})

