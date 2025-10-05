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
      console.log('[useQuickRating] Starting quick rating:', { sessionId, rating })
      try {
        const result = await invoke('quick_rate_session', {
          sessionId,  // Tauri converts camelCase to snake_case automatically
          rating,
        })
        console.log('[useQuickRating] Rating saved successfully:', result)
        return result
      } catch (error) {
        console.error('[useQuickRating] Error saving rating:', error)
        throw error
      }
    },
    onSuccess: () => {
      console.log('[useQuickRating] Invalidating queries')
      // Invalidate session queries to refresh the UI
      queryClient.invalidateQueries({ queryKey: ['sessions'] })
      queryClient.invalidateQueries({ queryKey: ['session'] })
      queryClient.invalidateQueries({ queryKey: ['local-sessions'] })
      queryClient.invalidateQueries({ queryKey: ['session-metadata'] })
    },
    onError: (error) => {
      console.error('[useQuickRating] Mutation error:', error)
    },
  })
}
