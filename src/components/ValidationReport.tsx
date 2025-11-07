import {
  type JSONLValidationResult,
  validateJSONL,
} from '@guideai-dev/session-processing/validation'
import { XCircleIcon } from '@heroicons/react/24/outline'
import { invoke } from '@tauri-apps/api/core'
import type React from 'react'
import { useEffect, useState } from 'react'
import { ValidationBadge } from './ValidationBadge'

interface ValidationReportProps {
  sessionId: string
  provider: string
  project: string
  filePath: string
}

export function ValidationReport({
  sessionId,
  provider,
  filePath,
}: ValidationReportProps): React.ReactElement {
  const [result, setResult] = useState<JSONLValidationResult | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const validateSession = async () => {
    setLoading(true)
    setError(null)
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
    } catch (err) {
      console.error('Validation failed:', err)
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    validateSession()
  }, [sessionId, filePath])

  if (loading) {
    return (
      <div className="flex items-center justify-center p-4">
        <span className="loading loading-spinner loading-md" />
      </div>
    )
  }

  if (error) {
    return (
      <div className="alert alert-error">
        <XCircleIcon className="w-6 h-6" />
        <div>
          <div className="font-bold">Validation Error</div>
          <div className="text-sm">{error}</div>
        </div>
      </div>
    )
  }

  if (!result) return <div />

  const warnings = result.sessionResult?.warnings || []
  const status = !result.valid ? 'errors' : warnings.length > 0 ? 'warnings' : 'valid'

  return (
    <div className="validation-report">
      {/* Compact Header */}
      <div className="flex items-center justify-between p-3 bg-base-200 rounded-lg">
        <div className="flex items-center gap-3">
          <span className="text-sm font-semibold">Validation</span>
          <ValidationBadge
            status={status}
            errorCount={result.errors.length}
            warningCount={warnings.length}
          />
        </div>
        <button
          type="button"
          className="btn btn-xs btn-ghost"
          onClick={validateSession}
          disabled={loading}
        >
          {loading ? <span className="loading loading-spinner loading-xs" /> : 'Re-validate'}
        </button>
      </div>

      {/* Only show details if there are errors or warnings */}
      {(result.errors.length > 0 || warnings.length > 0) && (
        <div className="mt-3 space-y-2">
          {/* Errors */}
          {result.errors.length > 0 && (
            <div className="space-y-1">
              <h4 className="text-xs font-semibold text-error">Errors ({result.errors.length})</h4>
              <ul className="space-y-1 max-h-40 overflow-y-auto">
                {result.errors.map((error, i) => (
                  <li key={i} className="text-xs p-2 bg-error/10 rounded">
                    <span className="font-mono text-error">Line {error.line}</span>
                    <span className="mx-1">•</span>
                    <span className="text-base-content/80">{error.message}</span>
                  </li>
                ))}
              </ul>
            </div>
          )}

          {/* Warnings */}
          {warnings.length > 0 && (
            <div className="space-y-1">
              <h4 className="text-xs font-semibold text-warning">Warnings ({warnings.length})</h4>
              <ul className="space-y-1 max-h-40 overflow-y-auto">
                {warnings.map((warning, i) => (
                  <li key={i} className="text-xs p-2 bg-warning/10 rounded">
                    <span className="font-mono text-warning">Line {warning.line}</span>
                    <span className="mx-1">•</span>
                    <span className="font-medium text-base-content/70">[{warning.code}]</span>
                    <span className="mx-1">•</span>
                    <span className="text-base-content/80">{warning.message}</span>
                  </li>
                ))}
              </ul>
            </div>
          )}
        </div>
      )}
    </div>
  )
}
