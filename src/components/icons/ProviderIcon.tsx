import claudeCodeSvg from '../../assets/icons/claude-code.svg'
import opencodeSvg from '../../assets/icons/opencode.svg'
import openaiCodexSvg from '../../assets/icons/openai-codex.svg'

interface ProviderIconProps {
  providerId: string
  className?: string
  size?: number
}

function ProviderIcon({ providerId, className = '', size = 24 }: ProviderIconProps) {
  const iconMap: Record<string, string> = {
    'claude-code': claudeCodeSvg,
    'opencode': opencodeSvg,
    'codex': openaiCodexSvg,
  }

  const iconPath = iconMap[providerId]

  if (!iconPath) {
    return null
  }

  // Add light background for OpenAI Codex
  const needsBackground = providerId === 'codex'
  const wrapperClassName = needsBackground ? 'inline-flex items-center justify-center bg-white rounded p-1' : ''

  const icon = (
    <img
      src={iconPath}
      alt={`${providerId} icon`}
      className={needsBackground ? '' : className}
      style={{ width: size, height: size }}
    />
  )

  if (needsBackground) {
    return (
      <div className={`${wrapperClassName} ${className}`}>
        {icon}
      </div>
    )
  }

  return icon
}

export default ProviderIcon