import {
  CheckCircleIcon,
  ExclamationTriangleIcon,
  QuestionMarkCircleIcon,
  XCircleIcon,
} from '@heroicons/react/24/outline'
import type React from 'react'

export type ValidationStatus = 'valid' | 'warnings' | 'errors' | 'unknown'

interface ValidationBadgeProps {
  status: ValidationStatus
  errorCount?: number
  warningCount?: number
  className?: string
}

export function ValidationBadge({
  status,
  errorCount = 0,
  warningCount = 0,
  className = '',
}: ValidationBadgeProps): React.ReactElement {
  const icons = {
    valid: <CheckCircleIcon className="w-4 h-4 text-success" />,
    warnings: <ExclamationTriangleIcon className="w-4 h-4 text-warning" />,
    errors: <XCircleIcon className="w-4 h-4 text-error" />,
    unknown: <QuestionMarkCircleIcon className="w-4 h-4 text-base-content/50" />,
  }

  const labels = {
    valid: 'Valid',
    warnings: `${warningCount} Warning${warningCount !== 1 ? 's' : ''}`,
    errors: `${errorCount} Error${errorCount !== 1 ? 's' : ''}`,
    unknown: 'Not Validated',
  }

  const badgeColors = {
    valid: 'badge-success',
    warnings: 'badge-warning',
    errors: 'badge-error',
    unknown: 'badge-ghost',
  }

  return (
    <div
      className={`badge badge-sm gap-1 ${badgeColors[status]} ${className}`}
      data-status={status}
    >
      {icons[status]}
      <span>{labels[status]}</span>
    </div>
  )
}
