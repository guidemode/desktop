import { useMutation, useQueryClient } from '@tanstack/react-query'
import { invoke } from '@tauri-apps/api/core'
import type { SessionRating } from '@guideai-dev/session-processing/ui'

interface QuickRatingParams {
  sessionId: string
  rating: SessionRating
}

/**
 * Hook for quick rating a session with thumbs up/meh/thumbs down
 */
export function useQuickRating() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: async ({ sessionId, rating }: QuickRatingParams) => {
      try {
        const result = await invoke('quick_rate_session', {
          sessionId,  // Tauri converts camelCase to snake_case automatically
          rating,
        })
        return result
      } catch (error) {
        console.error('Error saving rating:', error)
        throw error
      }
    },
    onSuccess: () => {
      // Invalidate session queries to refresh the UI
      queryClient.invalidateQueries({ queryKey: ['sessions'] })
      queryClient.invalidateQueries({ queryKey: ['session'] })
      queryClient.invalidateQueries({ queryKey: ['local-sessions'] })
      queryClient.invalidateQueries({ queryKey: ['session-metadata'] })
    },
    onError: (error) => {
      console.error('Rating mutation error:', error)
    },
  })
}
