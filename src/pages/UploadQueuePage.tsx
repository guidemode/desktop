import {
  ArrowPathIcon,
  ChevronLeftIcon,
  ClipboardDocumentIcon,
  DocumentTextIcon,
  XMarkIcon,
} from '@heroicons/react/24/outline'
import { useState } from 'react'
import { Link } from 'react-router-dom'
import LogViewer from '../components/LogViewer'
import { useAuth } from '../hooks/useAuth'
import {
  type UploadItem,
  useClearAllFailed,
  useRemoveQueueItem,
  useRetryAllFailed,
  useRetryUpload,
  useUploadQueueItems,
  useUploadQueueStatus,
} from '../hooks/useUploadQueue'

function UploadQueuePage() {
  const { user } = useAuth()
  const { data: queueItems, isLoading: itemsLoading } = useUploadQueueItems()
  const { data: status } = useUploadQueueStatus()
  const retryUpload = useRetryUpload()
  const removeItem = useRemoveQueueItem()
  const retryAllFailed = useRetryAllFailed()
  const clearAllFailed = useClearAllFailed()
  const [copiedId, setCopiedId] = useState<string | null>(null)
  const [showLogs, setShowLogs] = useState(false)

  const copyToClipboard = async (text: string, itemId: string) => {
    try {
      await navigator.clipboard.writeText(text)
      setCopiedId(itemId)
      setTimeout(() => setCopiedId(null), 2000)
    } catch (err) {
      console.error('Failed to copy:', err)
    }
  }

  const formatFileSize = (bytes: number) => {
    if (bytes < 1024) return `${bytes} B`
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  }

  const getStatusBadge = (item: UploadItem, isPending: boolean) => {
    if (item.last_error) {
      return <div className="badge badge-error badge-sm">Failed ({item.retry_count}/3)</div>
    }
    if (isPending && item.next_retry_at) {
      return <div className="badge badge-warning badge-sm">Retrying...</div>
    }
    if (isPending) {
      return <div className="badge badge-info badge-sm">Pending</div>
    }
    return null
  }

  const QueueItemCard = ({ item, isPending }: { item: UploadItem; isPending: boolean }) => {
    // Extract session ID from filename (remove .jsonl extension and truncate GUID)
    const displayId = item.file_name.replace('.jsonl', '').slice(0, 8)

    return (
      <div className="card bg-base-100 border border-base-300 hover:border-base-content/20 transition-colors">
        <div className="card-body p-3">
          <div className="flex items-center gap-3">
            {/* Status badge */}
            <div className="shrink-0">{getStatusBadge(item, isPending)}</div>

            {/* File info */}
            <div className="flex-1 min-w-0">
              <div className="flex items-center gap-2">
                <span className="font-medium text-sm font-mono" title={item.file_name}>
                  {displayId}
                </span>
                <span className="badge badge-ghost badge-sm shrink-0">{item.provider}</span>
                <span className="text-xs text-base-content/60 shrink-0">
                  {formatFileSize(item.file_size)}
                </span>
              </div>
              {item.last_error && (
                <div className="text-xs text-error truncate mt-1" title={item.last_error}>
                  {item.last_error}
                </div>
              )}
            </div>

            {/* Project name */}
            <div
              className="text-xs text-base-content/60 shrink-0 max-w-[200px] truncate"
              title={item.project_name}
            >
              {item.project_name}
            </div>

            {/* Actions */}
            <div className="flex gap-1 shrink-0">
              <button
                className="btn btn-ghost btn-xs"
                onClick={() => copyToClipboard(item.file_path, item.id)}
                title="Copy file path"
              >
                {copiedId === item.id ? (
                  <span className="text-success text-xs">✓</span>
                ) : (
                  <ClipboardDocumentIcon className="w-4 h-4" />
                )}
              </button>
              {item.last_error && (
                <button
                  className="btn btn-ghost btn-xs text-warning"
                  onClick={() => retryUpload.mutate(item.id)}
                  disabled={retryUpload.isPending}
                  title="Retry upload"
                >
                  <ArrowPathIcon className="w-4 h-4" />
                </button>
              )}
              <button
                className="btn btn-ghost btn-xs text-error"
                onClick={() => removeItem.mutate(item.id)}
                disabled={removeItem.isPending}
                title="Remove from queue"
              >
                <XMarkIcon className="w-4 h-4" />
              </button>
            </div>
          </div>
        </div>
      </div>
    )
  }

  if (!user) {
    return (
      <div className="p-6">
        <div className="mb-6">
          <h1 className="text-2xl font-bold text-base-content">Upload Queue</h1>
          <p className="text-sm text-base-content/70">Manage pending and failed session uploads</p>
        </div>

        <div className="card bg-base-100 shadow-sm border border-base-300">
          <div className="card-body">
            <p className="text-base-content/70">
              Uploading to the GuideAI server is only possible if you are logged in.{' '}
              <Link to="/settings" className="link link-primary">
                Go to settings to log in
              </Link>
              .
            </p>
          </div>
        </div>
      </div>
    )
  }

  if (showLogs) {
    return (
      <div className="h-full flex flex-col">
        {/* Header */}
        <div className="p-4 border-b border-base-300 bg-base-100">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-3">
              <button onClick={() => setShowLogs(false)} className="btn btn-sm btn-ghost">
                <ChevronLeftIcon className="w-5 h-5" />
                Back
              </button>
              <div>
                <h1 className="text-2xl font-bold text-base-content">Upload Queue Logs</h1>
                <p className="text-sm text-base-content/70">
                  View upload events and system activity
                </p>
              </div>
            </div>
          </div>
        </div>

        {/* Logs Display - Full height with no padding */}
        <div className="flex-1 overflow-hidden">
          <LogViewer provider="upload-queue" fullHeight />
        </div>
      </div>
    )
  }

  return (
    <div className="p-4 space-y-4">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold text-base-content">Upload Queue</h1>
          <p className="text-sm text-base-content/70">Manage pending and failed session uploads</p>
        </div>
        <div className="flex gap-2">
          <button onClick={() => setShowLogs(true)} className="btn btn-outline btn-sm gap-2">
            <DocumentTextIcon className="w-4 h-4" />
            Logs
          </button>
          <button
            className="btn btn-warning btn-sm"
            onClick={() => retryAllFailed.mutate()}
            disabled={retryAllFailed.isPending || !queueItems?.failed.length}
          >
            <ArrowPathIcon className="w-4 h-4" />
            Retry All Failed
          </button>
          <button
            className="btn btn-error btn-sm"
            onClick={() => clearAllFailed.mutate()}
            disabled={clearAllFailed.isPending || !queueItems?.failed.length}
          >
            <XMarkIcon className="w-4 h-4" />
            Clear All Failed
          </button>
        </div>
      </div>

      {/* Status Cards */}
      <div className="grid grid-cols-3 gap-4">
        <div className="card bg-base-100 shadow-sm border border-base-300">
          <div className="card-body p-4">
            <div className="text-xs text-base-content/70 uppercase">Pending</div>
            <div className="text-2xl font-bold">{status?.pending || 0}</div>
          </div>
        </div>
        <div className="card bg-base-100 shadow-sm border border-base-300">
          <div className="card-body p-4">
            <div className="text-xs text-base-content/70 uppercase">Processing</div>
            <div className="text-2xl font-bold">{status?.processing || 0}</div>
          </div>
        </div>
        <div className="card bg-base-100 shadow-sm border border-base-300">
          <div className="card-body p-4">
            <div className="text-xs text-base-content/70 uppercase">Failed</div>
            <div className="text-2xl font-bold text-error">{status?.failed || 0}</div>
          </div>
        </div>
      </div>

      {/* Queue Items Table */}
      <div className="card bg-base-100 shadow-sm border border-base-300">
        <div className="card-body">
          <div className="flex items-center gap-2 mb-4">
            <h2 className="text-lg font-semibold">Queue Items</h2>
            {itemsLoading && <span className="loading loading-spinner loading-sm" />}
          </div>

          {!queueItems || (queueItems.pending.length === 0 && queueItems.failed.length === 0) ? (
            <div className="text-center text-base-content/70 py-12">
              <div className="text-4xl mb-4">📤</div>
              <div className="text-lg font-medium mb-2">No items in queue</div>
              <div className="text-sm">All uploads are complete</div>
            </div>
          ) : (
            <div className="space-y-3">
              {/* Failed items first */}
              {queueItems.failed.map(item => (
                <QueueItemCard key={item.id} item={item} isPending={false} />
              ))}
              {/* Then pending items */}
              {queueItems.pending.map(item => (
                <QueueItemCard key={item.id} item={item} isPending={true} />
              ))}
            </div>
          )}

          {queueItems && (queueItems.pending.length > 0 || queueItems.failed.length > 0) && (
            <div className="text-xs text-base-content/70 mt-4">
              Showing {queueItems.pending.length + queueItems.failed.length} items (
              {queueItems.failed.length} failed, {queueItems.pending.length} pending)
            </div>
          )}
        </div>
      </div>
    </div>
  )
}

export default UploadQueuePage
