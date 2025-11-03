interface JsonBlockProps {
  content: string
  maxHeight?: string
  className?: string
}

/**
 * JSON display component for raw JSONL content
 * Uses plain text rendering for performance with large files
 */
export function JsonBlock({ content, maxHeight = '800px', className = '' }: JsonBlockProps) {
  return (
    <div className={`bg-base-200 rounded-lg p-4 overflow-auto ${className}`} style={{ maxHeight }}>
      <pre className="text-xs font-mono whitespace-pre overflow-x-auto">{content}</pre>
    </div>
  )
}
