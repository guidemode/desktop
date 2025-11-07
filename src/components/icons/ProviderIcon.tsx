import claudeCodeSvg from '../../assets/icons/claude-code.svg'
import cursorSvg from '../../assets/icons/cursor.svg'
import geminiCodeSvg from '../../assets/icons/gemini-code.svg'
import githubCopilotSvg from '../../assets/icons/github-copilot.svg'
import openaiCodexSvg from '../../assets/icons/openai-codex.svg'
import opencodeSvg from '../../assets/icons/opencode.svg'

interface ProviderIconProps {
  providerId: string
  className?: string
  size?: number
}

function ProviderIcon({ providerId, className = '', size = 20 }: ProviderIconProps) {
  const iconMap: Record<string, string> = {
    'claude-code': claudeCodeSvg,
    'github-copilot': githubCopilotSvg,
    opencode: opencodeSvg,
    codex: openaiCodexSvg,
    'gemini-code': geminiCodeSvg,
    cursor: cursorSvg,
  }

  const iconPath = iconMap[providerId]

  if (!iconPath) {
    return null
  }

  // Add light background for OpenAI Codex, GitHub Copilot, and Cursor (dark icons)
  const needsBackground =
    providerId === 'codex' || providerId === 'github-copilot' || providerId === 'cursor'
  const wrapperClassName = needsBackground
    ? 'inline-flex items-center justify-center bg-white rounded'
    : ''

  const icon = (
    <img
      src={iconPath}
      alt={`${providerId} icon`}
      className={needsBackground ? '' : className}
      style={{
        width: needsBackground ? size * 0.7 : size,
        height: needsBackground ? size * 0.7 : size,
      }}
    />
  )

  if (needsBackground) {
    return (
      <div className={`${wrapperClassName} ${className}`} style={{ width: size, height: size }}>
        {icon}
      </div>
    )
  }

  return icon
}

export default ProviderIcon
