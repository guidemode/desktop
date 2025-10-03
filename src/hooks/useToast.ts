import { useToastStore } from '../stores/toastStore'

export function useToast() {
  const addToast = useToastStore((state) => state.addToast)

  return {
    success: (message: string, duration?: number) => {
      addToast({ type: 'success', message, duration })
    },
    error: (message: string, duration?: number) => {
      addToast({ type: 'error', message, duration })
    },
    info: (message: string, duration?: number) => {
      addToast({ type: 'info', message, duration })
    },
    warning: (message: string, duration?: number) => {
      addToast({ type: 'warning', message, duration })
    }
  }
}
