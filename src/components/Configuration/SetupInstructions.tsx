import { ChevronDownIcon, ChevronUpIcon, InformationCircleIcon } from '@heroicons/react/24/outline'
import { useState } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'

interface SetupInstructionsProps {
  content: string
  isLoading?: boolean
  isProviderInstalled: boolean
  providerName: string
}

function SetupInstructions({
  content,
  isLoading = false,
  isProviderInstalled,
  providerName,
}: SetupInstructionsProps) {
  const [isExpanded, setIsExpanded] = useState(!isProviderInstalled)

  if (isLoading) {
    return (
      <div className="bg-base-200 border border-base-300 rounded-lg p-4">
        <div className="flex items-center gap-2">
          <span className="loading loading-spinner loading-sm" />
          <span className="text-sm text-base-content/70">Loading setup instructions...</span>
        </div>
      </div>
    )
  }

  if (!content) {
    return null
  }

  return (
    <div className="bg-info/10 border border-info/30 rounded-lg overflow-hidden">
      {/* Header - Always visible */}
      <button
        type="button"
        className="w-full px-4 py-3 flex items-center justify-between hover:bg-info/20 transition-colors"
        onClick={() => setIsExpanded(!isExpanded)}
      >
        <div className="flex items-center gap-2">
          <InformationCircleIcon className="w-5 h-5 text-info" />
          <span className="font-medium text-info">{providerName} Setup Instructions</span>
        </div>
        {isExpanded ? (
          <ChevronUpIcon className="w-4 h-4 text-info" />
        ) : (
          <ChevronDownIcon className="w-4 h-4 text-info" />
        )}
      </button>

      {/* Content - Collapsible */}
      {isExpanded && (
        <div className="px-4 pb-4 prose prose-sm max-w-none markdown-content">
          <ReactMarkdown
            remarkPlugins={[remarkGfm]}
            components={{
              // Customize markdown rendering for better styling
              h1: ({ children, ...props }) => (
                <h1 className="text-xl font-bold mt-4 mb-2 text-base-content" {...props}>
                  {children}
                </h1>
              ),
              h2: ({ children, ...props }) => (
                <h2 className="text-lg font-semibold mt-3 mb-2 text-base-content" {...props}>
                  {children}
                </h2>
              ),
              h3: ({ children, ...props }) => (
                <h3 className="text-base font-semibold mt-2 mb-1 text-base-content" {...props}>
                  {children}
                </h3>
              ),
              p: ({ children, ...props }) => (
                <p className="text-sm text-base-content/80 my-1 leading-relaxed" {...props}>
                  {children}
                </p>
              ),
              code: ({ inline, children, ...props }: any) =>
                inline ? (
                  <code
                    className="bg-base-300 px-1.5 py-0.5 rounded text-xs font-mono text-base-content"
                    {...props}
                  >
                    {children}
                  </code>
                ) : (
                  <code
                    className="block bg-base-300 p-3 rounded text-xs font-mono overflow-x-auto text-base-content"
                    {...props}
                  >
                    {children}
                  </code>
                ),
              pre: ({ children, ...props }) => (
                <pre className="bg-base-300 p-3 rounded overflow-x-auto my-2" {...props}>
                  {children}
                </pre>
              ),
              ul: ({ children, ...props }) => (
                <ul className="list-disc list-outside ml-6 space-y-1 my-2 text-sm" {...props}>
                  {children}
                </ul>
              ),
              ol: ({ children, ...props }) => (
                <ol className="list-decimal list-outside ml-6 space-y-1 my-2 text-sm" {...props}>
                  {children}
                </ol>
              ),
              li: ({ children, ...props }) => (
                <li
                  className="text-base-content/80 leading-relaxed [&>p]:inline [&>p]:m-0"
                  {...props}
                >
                  {children}
                </li>
              ),
              a: ({ children, ...props }) => (
                <a
                  className="text-info hover:text-info/80 underline"
                  target="_blank"
                  rel="noopener noreferrer"
                  {...props}
                >
                  {children}
                </a>
              ),
              blockquote: ({ children, ...props }) => (
                <blockquote
                  className="border-l-4 border-warning pl-4 py-2 my-2 bg-warning/10 italic"
                  {...props}
                >
                  {children}
                </blockquote>
              ),
              hr: props => <hr className="my-4 border-base-300" {...props} />,
              strong: ({ children, ...props }) => (
                <strong className="font-semibold text-base-content" {...props}>
                  {children}
                </strong>
              ),
            }}
          >
            {content}
          </ReactMarkdown>
        </div>
      )}
    </div>
  )
}

export default SetupInstructions
