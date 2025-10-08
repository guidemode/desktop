import type { ReactNode } from 'react'
import { MemoryRouter } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'

const invokeMock = vi.fn()
const listenMock = vi.fn()
const toast = {
  success: vi.fn(),
  error: vi.fn(),
  info: vi.fn(),
}
const refreshMock = vi.fn()
const invalidateSessionsMock = vi.fn()
const quickRateMock = vi.fn().mockResolvedValue(undefined)
const navigateMock = vi.fn()

const sessions = [
  {
    id: '1',
    sessionId: 'session-1',
    provider: 'claude-code',
    filePath: '/tmp/session-1.jsonl',
    fileName: 'session-1.jsonl',
    assessmentStatus: 'pending',
    projectName: 'Project A',
    durationMs: 1000,
    sessionStartTime: new Date().toISOString(),
    sessionEndTime: new Date().toISOString(),
    createdAt: new Date().toISOString(),
    uploadedAt: new Date().toISOString(),
  },
]

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: unknown[]) => listenMock(...args),
}))

vi.mock('react-router-dom', async (importOriginal) => {
  const actual = await importOriginal<typeof import('react-router-dom')>()
  return {
    ...actual,
    useNavigate: () => navigateMock,
  }
})

vi.mock('../../hooks/useToast', () => ({
  __esModule: true,
  useToast: () => toast,
}))

vi.mock('../../hooks/useLocalSessions', () => ({
  __esModule: true,
  useLocalSessions: () => ({
    sessions,
    loading: false,
    error: null,
    refresh: refreshMock,
  }),
  useInvalidateSessions: () => invalidateSessionsMock,
}))

vi.mock('../hooks/useLocalProjects', () => ({
  __esModule: true,
  useLocalProjects: () => ({ projects: [], loading: false, error: null, refresh: vi.fn() }),
}))

vi.mock('../hooks/useAiProcessing', () => ({
  __esModule: true,
  useAiProcessing: () => ({
    processSessionWithAi: vi.fn(),
    hasApiKey: () => false,
  }),
}))

vi.mock('../hooks/useSessionProcessing', () => ({
  __esModule: true,
  useSessionProcessing: () => ({
    processSession: vi.fn(),
  }),
}))

vi.mock('../hooks/useQuickRating', () => ({
  __esModule: true,
  useQuickRating: () => ({
    mutateAsync: quickRateMock,
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
    SessionCard: ({ session, onSyncSession }: { session: { sessionId: string }; onSyncSession?: (id: string) => void }) => (
      <div data-testid={`session-card-${session.sessionId}`}>
        <button onClick={() => onSyncSession?.(session.sessionId)}>Sync {session.sessionId}</button>
      </div>
    ),
    DateFilter: () => null,
  }
})


let SessionsPageComponent: typeof import('../SessionsPage').default

beforeAll(async () => {
  SessionsPageComponent = (await import('../SessionsPage')).default
})

function renderPage(ui: ReactNode) {
  const client = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
    },
  })

  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter>{ui}</MemoryRouter>
    </QueryClientProvider>
  )
}

describe('SessionsPage integration', () => {
  beforeEach(() => {
    invokeMock.mockReset()
    listenMock.mockResolvedValue(() => {})
    toast.success.mockReset()
    toast.error.mockReset()
    toast.info.mockReset()
    refreshMock.mockReset()
    invalidateSessionsMock.mockReset()
    navigateMock.mockReset()
  })


  it('queues session upload when provider allows transcripts', async () => {
    invokeMock.mockImplementation(async (command: string, args: any) => {
      if (command === 'load_config_command') {
        return { apiKey: 'abc', username: 'tester', serverUrl: 'http://server' }
      }
      if (command === 'get_all_projects') {
        return []
      }
      if (command === 'execute_sql' && args.sql.includes('FROM agent_sessions')) {
        return [
          {
            id: '1',
            session_id: 'session-1',
            provider: 'claude-code',
            file_name: 'session-1.jsonl',
            file_path: '/tmp/session-1.jsonl',
            project_name: 'Project A',
            duration_ms: 1000,
            session_start_time: Date.now(),
            session_end_time: Date.now(),
            created_at: Date.now(),
            uploaded_at: Date.now(),
            processing_status: 'pending',
            assessment_status: 'not_started',
            synced_to_server: 0,
          },
        ]
      }
      if (command === 'execute_sql' && args.sql.includes('SELECT provider')) {
        return [{ provider: 'claude-code' }]
      }
      if (command === 'execute_sql' && args.sql.includes('UPDATE agent_sessions')) {
        return []
      }
      if (command === 'load_provider_config_command') {
        return { syncMode: 'Transcript and Metrics' }
      }
      if (command === 'execute_sql') {
        return []
      }
      throw new Error(`Unexpected command ${command}`)
    })

    renderPage(<SessionsPageComponent />)

    const syncButton = within(await screen.findByTestId('session-card-session-1')).getByRole('button')
    await userEvent.click(syncButton)

    expect(invokeMock).toHaveBeenCalledWith(
      'load_provider_config_command',
      expect.objectContaining({ providerId: 'claude-code' })
    )
    expect(invokeMock).toHaveBeenCalledWith(
      'execute_sql',
      expect.objectContaining({ sql: expect.stringContaining('UPDATE agent_sessions') })
    )
    expect(toast.success).toHaveBeenCalledWith(
      'Session queued for upload. Check the Upload Queue page for status.'
    )
    expect(navigateMock).not.toHaveBeenCalled()
  })

  it('navigates to provider config when sync mode disallows uploads', async () => {
    invokeMock.mockImplementation(async (command: string, args: any) => {
      if (command === 'load_config_command') {
        return { apiKey: 'abc', username: 'tester', serverUrl: 'http://server' }
      }
      if (command === 'get_all_projects') {
        return []
      }
      if (command === 'execute_sql' && args.sql.includes('FROM agent_sessions')) {
        return [
          {
            id: '1',
            session_id: 'session-1',
            provider: 'claude-code',
            file_name: 'session-1.jsonl',
            file_path: '/tmp/session-1.jsonl',
            project_name: 'Project A',
            duration_ms: 1000,
            session_start_time: Date.now(),
            session_end_time: Date.now(),
            created_at: Date.now(),
            uploaded_at: Date.now(),
            processing_status: 'pending',
            assessment_status: 'not_started',
            synced_to_server: 0,
          },
        ]
      }
      if (command === 'execute_sql' && args.sql.includes('SELECT provider')) {
        return [{ provider: 'claude-code' }]
      }
      if (command === 'load_provider_config_command') {
        return { syncMode: 'Metrics Only' }
      }
      if (command === 'execute_sql') {
        return []
      }
      throw new Error(`Unexpected command ${command}`)
    })

    renderPage(<SessionsPageComponent />)

    const syncButton = within(await screen.findByTestId('session-card-session-1')).getByRole('button')
    await userEvent.click(syncButton)

    expect(navigateMock).toHaveBeenCalledWith('/provider/claude-code#sync-mode')
    expect(toast.success).not.toHaveBeenCalled()
  })

  it('redirects to login when user not authenticated', async () => {
    invokeMock.mockImplementation(async (command: string, args: any) => {
      if (command === 'load_config_command') {
        return { apiKey: undefined, username: undefined }
      }
      if (command === 'get_all_projects') {
        return []
      }
      if (command === 'execute_sql' && args.sql.includes('FROM agent_sessions')) {
        return [
          {
            id: '1',
            session_id: 'session-1',
            provider: 'claude-code',
            file_name: 'session-1.jsonl',
            file_path: '/tmp/session-1.jsonl',
            project_name: 'Project A',
            duration_ms: 1000,
            session_start_time: Date.now(),
            session_end_time: Date.now(),
            created_at: Date.now(),
            uploaded_at: Date.now(),
            processing_status: 'pending',
            assessment_status: 'not_started',
            synced_to_server: 0,
          },
        ]
      }
      if (command === 'execute_sql' && args.sql.includes('SELECT provider')) {
        return [{ provider: 'claude-code' }]
      }
      if (command === 'load_provider_config_command') {
        return { syncMode: 'Transcript and Metrics' }
      }
      if (command === 'execute_sql') {
        return []
      }
      throw new Error(`Unexpected command ${command}`)
    })

    renderPage(<SessionsPageComponent />)

    const syncButton = within(await screen.findByTestId('session-card-session-1')).getByRole('button')
    await userEvent.click(syncButton)

    expect(navigateMock).toHaveBeenCalledWith('/')
    expect(toast.success).not.toHaveBeenCalled()
  })
})
