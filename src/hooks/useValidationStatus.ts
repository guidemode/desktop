import { type JSONLValidationResult, validateJSONL } from '@guidemode/session-processing/validation'
import { invoke } from '@tauri-apps/api/core'
import { useCallback, useEffect, useState } from 'react'

export type ValidationStatus = 'valid' | 'errors' | 'warnings' | 'unknown' | 'loading'

/**
 * Hook to check validation status of a session's canonical JSONL file
 * Calls the validation library directly (no subprocess needed)
 * Returns 'valid', 'errors', 'warnings', 'unknown', or 'loading'
 */
export function useValidationStatus(
  filePath: string | null | undefined,
  provider?: string,
  sessionId?: string
): {
  status: ValidationStatus
  result: JSONLValidationResult | null
  validate: () => Promise<void>
  isValidating: boolean
} {
  const [status, setStatus] = useState<ValidationStatus>('unknown')
  const [result, setResult] = useState<JSONLValidationResult | null>(null)
  const [isValidating, setIsValidating] = useState(false)

  const validate = useCallback(async () => {
    if (!filePath || !provider || !sessionId) {
      setStatus('unknown')
      return
    }

    setIsValidating(true)
    setStatus('loading')

    try {
      // Read file content using existing Tauri command
      const content = await invoke<string>('get_session_content', {
        provider,
        filePath,
        sessionId,
      })

      // Validate using the session-processing library directly
      const validationResult = validateJSONL(content, {
        skipInvalidJSON: false,
        includeWarnings: true,
      })

      setResult(validationResult)

      if (!validationResult.valid) {
        setStatus('errors')
      } else if (
        validationResult.sessionResult &&
        validationResult.sessionResult.warnings.length > 0
      ) {
        setStatus('warnings')
      } else {
        setStatus('valid')
      }
    } catch (error) {
      console.error('Validation failed:', error)
      setStatus('unknown')
    } finally {
      setIsValidating(false)
    }
  }, [filePath, provider, sessionId])

  // Auto-validate on mount and when dependencies change
  useEffect(() => {
    if (filePath && provider && sessionId) {
      validate()
    }
  }, [validate, filePath, provider, sessionId])

  return { status, result, validate, isValidating }
}
