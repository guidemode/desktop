import '@testing-library/jest-dom/vitest'
import type { ReactNode } from 'react'
import { MemoryRouter, Route, Routes } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'

const invokeMock = vi.fn()
const listenMock = vi.fn()
const openMock = vi.fn()
const toast = {
  success: vi.fn(),
  error: vi.fn(),
  info: vi.fn(),
}
const navigateMock = vi.fn()

const sessionRow = {
  id: 'row-1',
  provider: 'claude-code',
  project_name: 'Project A',
  session_id: 'session-1',
  file_name: 'session.jsonl',
  file_path: '/tmp/session.jsonl',
  file_size: 1024,
  session_start_time: Date.now(),
  session_end_time: Date.now() + 1000,
  duration_ms: 1000,
  processing_status: 'pending',
  synced_to_server: 0,
  synced_at: null,
  server_session_id: null,
  created_at: Date.now(),
  uploaded_at: null,
  cwd: '/project',
  sync_failed_reason: null,
  ai_model_phase_analysis: null,
}

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: unknown[]) => listenMock(...args),
}))

vi.mock('@tauri-apps/plugin-shell', () => ({
  open: (...args: unknown[]) => openMock(...args),
}))

vi.mock('react-router-dom', async (importOriginal) => {
  const actual = await importOriginal<typeof import('react-router-dom')>()
  return {
    ...actual,
    useNavigate: () => navigateMock,
  }
})

vi.mock('../../hooks/useToast', () => ({
  useToast: () => toast,
}))

vi.mock('../../hooks/useLocalSessionContent', async () => {
  const actual = await vi.importActual<typeof import('../../hooks/useLocalSessionContent')>(
    '../../hooks/useLocalSessionContent'
  )

  return {
    ...actual,
    useLocalSessionContent: () => ({
      messages: [],
      timeline: null,
      fileContent: '',
      loading: false,
      error: null,
    }),
  }
})

vi.mock('../../hooks/useLocalSessionMetrics', async () => {
  const actual = await vi.importActual<typeof import('../../hooks/useLocalSessionMetrics')>(
    '../../hooks/useLocalSessionMetrics'
  )

  return {
    ...actual,
    useLocalSessionMetrics: () => ({
      metrics: null,
      loading: false,
      error: null,
    }),
  }
})

vi.mock('../../hooks/useAiProcessing', () => ({
  __esModule: true,
  useAiProcessing: () => ({
    processSessionWithAi: vi.fn().mockResolvedValue(undefined),
    hasApiKey: () => false,
  }),
}))

vi.mock('../hooks/useSessionProcessing', () => ({
  __esModule: true,
  useSessionProcessing: () => ({
    processSession: vi.fn().mockResolvedValue(undefined),
  }),
}))

vi.mock('../hooks/useSessionActivity', () => ({
  __esModule: true,
  useSessionActivity: () => undefined,
}))

const sessionActivityStore = {
  isSessionActive: () => false,
  clearAllActiveSessions: vi.fn(),
  setTrackingEnabled: vi.fn(),
}

vi.mock('../../stores/sessionActivityStore', () => ({
  __esModule: true,
  useSessionActivityStore: (selector?: (state: typeof sessionActivityStore) => unknown) =>
    selector ? selector(sessionActivityStore) : sessionActivityStore,
}))

vi.mock('@guideai-dev/session-processing/ui', async () => {
  const actual = await vi.importActual<typeof import('@guideai-dev/session-processing/ui')>(
    '@guideai-dev/session-processing/ui'
  )

  return {
    ...actual,
    SessionDetailHeader: ({
      syncStatus,
      onRate,
    }: {
      syncStatus: { onSync: () => void }
      onRate: (rating: string) => Promise<void> | void
    }) => (
      <div>
        <button onClick={() => syncStatus.onSync()}>Queue Upload</button>
        <button onClick={() => onRate('great')}>Rate Great</button>
      </div>
    ),
    TimelineMessage: () => null,
    TimelineGroup: () => null,
    isTimelineGroup: () => false,
    MetricsOverview: () => <div data-testid="metrics-overview">Metrics</div>,
    RatingBadge: () => null,
    PhaseTimeline: () => <div data-testid="phase-timeline">Timeline</div>,
  }
})

let SessionDetailPageComponent: typeof import('../SessionDetailPage').default

beforeAll(async () => {
  SessionDetailPageComponent = (await import('../SessionDetailPage')).default
})

function renderPage(route = '/sessions/session-1', ui?: ReactNode) {
  const client = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
    },
  })

  const renderResult = render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={[route]}>
        <Routes>
          <Route path="/sessions/:sessionId" element={ui ?? <SessionDetailPageComponent />} />
          <Route path="*" element={<div>Fallback</div>} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>
  )

  return { ...renderResult, client }
}

describe('SessionDetailPage integration', () => {
  beforeEach(() => {
    invokeMock.mockReset()
    listenMock.mockResolvedValue(() => {})
    openMock.mockResolvedValue(undefined)
    toast.success.mockReset()
    toast.error.mockReset()
    toast.info.mockReset()
    navigateMock.mockReset()
  })


  function setupInvokeForMetadata(config: { apiKey?: string; username?: string } = { apiKey: 'abc', username: 'tester' }) {
    invokeMock.mockImplementation(async (command: string, args: any) => {
      if (command === 'load_config_command') {
        return config
      }
      if (command === 'get_all_projects') {
        return []
      }
      if (command === 'execute_sql' && args.sql.includes('FROM agent_sessions')) {
        return [sessionRow]
      }
      if (command === 'execute_sql' && args.sql.includes('FROM projects')) {
        return []
      }
      if (command === 'execute_sql' && args.sql.includes('UPDATE agent_sessions')) {
        return []
      }
      if (command === 'execute_sql' && args.sql.includes('FROM session_metrics')) {
        return []
      }
      if (command === 'get_session_content') {
        return '{"timestamp":"2025-01-01T10:00:00Z","type":"message","message":{"role":"user","content":[{"type":"text","text":"Hello"}]}}'
      }
      if (command === 'quick_rate_session') {
        return {}
      }
      if (command === 'execute_sql') {
        return []
      }
      throw new Error(`Unexpected command ${command}`)
    })
  }

  it('queues upload when sync button clicked', async () => {
    setupInvokeForMetadata()

    renderPage()

    const syncButton = await screen.findByRole('button', { name: /Queue Upload/i })
    await userEvent.click(syncButton)

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        'execute_sql',
        expect.objectContaining({
          sql: expect.stringContaining('UPDATE agent_sessions SET sync_failed_reason = NULL'),
          params: ['session-1'],
        })
      )
    )
    await waitFor(() =>
      expect(toast.success).toHaveBeenCalledWith(
        'Session queued for upload. Check the Upload Queue page for status.'
      )
    )
  })

  // TODO: Add test for redirect to login when user missing
  // This requires proper mocking of useAuth hook reactivity

  // TODO: Add test for quick rating flow
  // This requires proper setup of mutation mocks
})
