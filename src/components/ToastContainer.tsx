import { useToastStore } from '../stores/toastStore'
import type { ToastType } from '../stores/toastStore'

export function ToastContainer() {
  const { toasts, removeToast } = useToastStore()

  const getAlertClass = (type: ToastType): string => {
    switch (type) {
      case 'success':
        return 'alert-success'
      case 'error':
        return 'alert-error'
      case 'warning':
        return 'alert-warning'
      case 'info':
        return 'alert-info'
      default:
        return ''
    }
  }

  const getIcon = (type: ToastType) => {
    switch (type) {
      case 'success':
        return (
          <svg xmlns="http://www.w3.org/2000/svg" className="stroke-current shrink-0 h-6 w-6" fill="none" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
        )
      case 'error':
        return (
          <svg xmlns="http://www.w3.org/2000/svg" className="stroke-current shrink-0 h-6 w-6" fill="none" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M10 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2m7-2a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
        )
      case 'warning':
        return (
          <svg xmlns="http://www.w3.org/2000/svg" className="stroke-current shrink-0 h-6 w-6" fill="none" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
          </svg>
        )
      case 'info':
        return (
          <svg xmlns="http://www.w3.org/2000/svg" className="stroke-current shrink-0 h-6 w-6" fill="none" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
        )
    }
  }

  if (toasts.length === 0) {
    return null
  }

  return (
    <div className="fixed left-0 right-0 z-50 flex flex-col gap-0" style={{ bottom: '-5px' }}>
      {toasts.map((toast) => (
        <div key={toast.id} className={`alert ${getAlertClass(toast.type)} rounded-none shadow-lg w-full flex items-center justify-between px-6 py-4`}>
          <div className="flex items-center gap-3 flex-1 min-w-0">
            {getIcon(toast.type)}
            <span className="text-sm whitespace-pre-wrap break-words">{toast.message}</span>
          </div>
          <button
            className="btn btn-ghost btn-sm btn-circle flex-shrink-0"
            onClick={() => removeToast(toast.id)}
            aria-label="Close"
          >
            ✕
          </button>
        </div>
      ))}
    </div>
  )
}
