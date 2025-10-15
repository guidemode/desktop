import {
  XCircleIcon,
  ComputerDesktopIcon,
  ChartBarIcon,
  CloudArrowUpIcon,
  ExclamationCircleIcon,
} from '@heroicons/react/24/solid'
import type { ProviderStatus } from '../types/providers'

interface ProviderStatusIndicatorProps {
  /**
   * Current status of the provider
   */
  status: ProviderStatus

  /**
   * Icon size in pixels
   * @default 20
   */
  size?: number

  /**
   * Whether to show tooltip on hover
   * @default true
   */
  showTooltip?: boolean

  /**
   * Additional CSS classes to apply to the icon
   */
  className?: string

  /**
   * Accessibility label override
   * If not provided, generates from status
   */
  ariaLabel?: string
}

// Map status to icon component
const iconMap: Record<ProviderStatus, typeof XCircleIcon> = {
  'not-installed': ExclamationCircleIcon,
  disabled: XCircleIcon,
  'local-only': ComputerDesktopIcon,
  'metrics-only': ChartBarIcon,
  'full-sync': CloudArrowUpIcon,
}

// Map status to color class
const colorMap: Record<ProviderStatus, string> = {
  'not-installed': 'text-base-content/30',
  disabled: 'text-base-content/30',
  'local-only': 'text-info',
  'metrics-only': 'text-warning',
  'full-sync': 'text-success',
}

// Generate aria-label from status
const generateAriaLabel = (status: ProviderStatus): string => {
  switch (status) {
    case 'not-installed':
      return 'Provider is not installed'
    case 'disabled':
      return 'Provider is disabled'
    case 'local-only':
      return 'Provider is processing locally only'
    case 'metrics-only':
      return 'Provider is collecting metrics only'
    case 'full-sync':
      return 'Provider has full synchronization enabled'
    default:
      return 'Provider status'
  }
}

// Generate tooltip text from status
const generateTooltipText = (status: ProviderStatus): string => {
  switch (status) {
    case 'not-installed':
      return 'Not Installed - Provider directory not found'
    case 'disabled':
      return 'Disabled - Provider is turned off'
    case 'local-only':
      return 'Local Only - Sessions stored locally, no cloud sync'
    case 'metrics-only':
      return 'Metrics Only - Only metrics synced, transcripts stay local'
    case 'full-sync':
      return 'Full Sync - Complete session data synced to cloud'
    default:
      return 'Unknown status'
  }
}

function ProviderStatusIndicator({
  status,
  size = 20,
  showTooltip = true,
  className = '',
  ariaLabel,
}: ProviderStatusIndicatorProps) {
  // Clamp size to reasonable bounds (12-48px)
  const clampedSize = Math.max(12, Math.min(48, size))

  // Get icon component for this status
  const IconComponent = iconMap[status] || XCircleIcon

  // Get color class
  const colorClass = colorMap[status] || 'text-base-content/30'

  // Get aria label
  const label = ariaLabel || generateAriaLabel(status)

  // Render icon
  const icon = (
    <IconComponent
      className={colorClass}
      style={{ width: clampedSize, height: clampedSize }}
      aria-label={label}
      role="img"
      tabIndex={0}
    />
  )

  // Wrap in tooltip if enabled
  if (showTooltip) {
    const tooltipText = generateTooltipText(status)
    return (
      <div className={`tooltip ${className}`} data-tip={tooltipText}>
        {icon}
      </div>
    )
  }

  return <div className={className}>{icon}</div>
}

export default ProviderStatusIndicator
