import '@testing-library/jest-dom/vitest'
import type { ReactNode } from 'react'
import { MemoryRouter } from 'react-router-dom'
import { render, screen, within, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import UploadQueuePage from '../UploadQueuePage'

const writeText = vi.fn().mockResolvedValue(undefined)

const queueItems = {
  pending: [
    {
      id: 'pending-1',
      provider: 'claude-code',
      project_name: 'Project A',
      file_path: '/tmp/pending.jsonl',
      file_name: 'pending.jsonl',
      queued_at: new Date().toISOString(),
      retry_count: 0,
      file_size: 1500,
    },
  ],
  failed: [
    {
      id: 'failed-1',
      provider: 'copilot',
      project_name: 'Project B',
      file_path: '/tmp/failed.jsonl',
      file_name: 'failed.jsonl',
      queued_at: new Date().toISOString(),
      retry_count: 2,
      last_error: 'Network error',
      next_retry_at: new Date().toISOString(),
      file_size: 4096,
    },
  ],
}

const status = { pending: 1, processing: 0, failed: 1, recent_uploads: [] }

const retryUpload = { mutate: vi.fn(), isPending: false }
const removeItem = { mutate: vi.fn(), isPending: false }
const retryAllFailed = { mutate: vi.fn(), isPending: false }
const clearAllFailed = { mutate: vi.fn(), isPending: false }

vi.mock('../../hooks/useAuth', () => ({
  useAuth: () => ({ user: { id: 'user-1', email: 'test@example.com' } }),
}))

vi.mock('../../components/LogViewer', () => ({
  default: () => <div data-testid="log-viewer">Log viewer mock</div>,
}))

vi.mock('../../hooks/useUploadQueue', () => ({
  useUploadQueueItems: () => ({ data: queueItems, isLoading: false }),
  useUploadQueueStatus: () => ({ data: status }),
  useRetryUpload: () => retryUpload,
  useRemoveQueueItem: () => removeItem,
  useRetryAllFailed: () => retryAllFailed,
  useClearAllFailed: () => clearAllFailed,
}))

function renderPage(ui: ReactNode) {
  return render(<MemoryRouter>{ui}</MemoryRouter>)
}

describe('UploadQueuePage integration', () => {
  const originalClipboard = navigator.clipboard

  beforeEach(() => {
    writeText.mockReset()
    retryUpload.mutate.mockReset()
    removeItem.mutate.mockReset()
    retryAllFailed.mutate.mockReset()
    clearAllFailed.mutate.mockReset()
    Object.defineProperty(window.navigator, 'clipboard', {
      value: { writeText },
      configurable: true,
    })
  })

  afterEach(() => {
    if (originalClipboard) {
      Object.defineProperty(window.navigator, 'clipboard', {
        value: originalClipboard,
        configurable: true,
      })
    } else {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      delete (window.navigator as any).clipboard
    }
    vi.restoreAllMocks()
  })

  it('renders queue summary and items', () => {
    renderPage(<UploadQueuePage />)

    expect(screen.getByText('Upload Queue')).toBeInTheDocument()

    const pendingCardLabel = screen
      .getAllByText('Pending')
      .find(element => element.closest('.card')?.querySelector('.text-2xl'))!
    const pendingCard = pendingCardLabel.closest('.card') as HTMLElement
    expect(within(pendingCard).getByText('1')).toBeInTheDocument()

    const failedCardLabel = screen
      .getAllByText('Failed')
      .find(element => element.closest('.card')?.querySelector('.text-2xl'))!
    const failedCard = failedCardLabel.closest('.card') as HTMLElement
    expect(within(failedCard).getByText('1')).toBeInTheDocument()

    expect(screen.getByText('Project A')).toBeInTheDocument()

    const failedItemRow = screen.getByText('Project B').closest('.card') as HTMLElement
    expect(within(failedItemRow).getByText('Network error')).toBeInTheDocument()
  })

  it('triggers retry and clear actions', async () => {
    const user = userEvent.setup()
    renderPage(<UploadQueuePage />)

    // Retry all failed
    await user.click(screen.getAllByRole('button', { name: /Retry All Failed/i })[0])
    expect(retryAllFailed.mutate).toHaveBeenCalled()

    // Clear all failed
    await user.click(screen.getAllByRole('button', { name: /Clear All Failed/i })[0])
    expect(clearAllFailed.mutate).toHaveBeenCalled()

    // Retry single failed item
    const retryButtons = screen.getAllByTitle('Retry upload')
    await user.click(retryButtons[0])
    expect(retryUpload.mutate).toHaveBeenCalledWith('failed-1')

    // Remove item
    const removeButtons = screen.getAllByTitle('Remove from queue')
    await user.click(removeButtons[0])
    expect(removeItem.mutate).toHaveBeenCalledWith('failed-1')
  })

  it('allows copying file path and toggling logs', async () => {
    const user = userEvent.setup()
    renderPage(<UploadQueuePage />)

    await user.click(screen.getAllByTitle('Copy file path')[0])
    await waitFor(() => expect(screen.getByText('✓')).toBeInTheDocument())

    await user.click(screen.getAllByRole('button', { name: /Logs/i })[0])
    expect(screen.getByTestId('log-viewer')).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: /Back/i }))
    expect(screen.queryByTestId('log-viewer')).not.toBeInTheDocument()
  })
})
