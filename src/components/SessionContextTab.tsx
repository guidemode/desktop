import {
  ChevronDownIcon,
  ChevronRightIcon,
  CodeBracketIcon,
  DocumentTextIcon,
} from '@heroicons/react/24/outline'
import { useQuery } from '@tanstack/react-query'
import { invoke } from '@tauri-apps/api/core'
import { useEffect, useState } from 'react'
import { type FileUsageStats, useContextFileUsage } from '../hooks/useContextFileUsage'

// Dynamic import types for syntax highlighter
interface SyntaxHighlighterDeps {
  Prism: any
  oneDark: any
  oneLight: any
}

interface ContextFile {
  fileName: string
  filePath: string
  relativePath: string
  content: string
  size: number
}

interface SessionContextTabProps {
  session: {
    sessionId: string
    cwd: string
  }
  fileContent: string | null
}

async function fetchContextFiles(cwd: string): Promise<ContextFile[]> {
  const result = await invoke<any[]>('scan_context_files', { cwd })

  // Convert snake_case from Rust to camelCase for TypeScript
  return result.map((item: any) => ({
    fileName: item.file_name,
    filePath: item.file_path,
    relativePath: item.relative_path,
    content: item.content,
    size: item.size,
  }))
}

export function SessionContextTab({ session, fileContent }: SessionContextTabProps) {
  const [expandedFiles, setExpandedFiles] = useState<Set<string>>(new Set())

  // Fetch context files with React Query (1 minute cache)
  const {
    data: contextFiles = [],
    isLoading: loading,
    error,
  } = useQuery<ContextFile[], Error>({
    queryKey: ['session-context', session.sessionId, session.cwd],
    queryFn: () => fetchContextFiles(session.cwd),
    staleTime: 60000, // 1 minute cache
  })

  // Track which context files are mentioned in the transcript
  const usageCounts = useContextFileUsage(fileContent, contextFiles)

  const toggleFile = (filePath: string) => {
    setExpandedFiles(prev => {
      const next = new Set(prev)
      if (next.has(filePath)) {
        next.delete(filePath)
      } else {
        next.add(filePath)
      }
      return next
    })
  }

  const expandAll = () => {
    setExpandedFiles(new Set(contextFiles.map(f => f.filePath)))
  }

  const collapseAll = () => {
    setExpandedFiles(new Set())
  }

  if (loading) {
    return (
      <div className="flex items-center justify-center min-h-[400px]">
        <span className="loading loading-spinner loading-lg" />
      </div>
    )
  }

  if (error) {
    const errorMessage = error instanceof Error ? error.message : String(error)
    return (
      <div className="alert alert-error">
        <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
          />
        </svg>
        <div>
          <div className="font-semibold">Failed to load context files</div>
          <div className="text-sm opacity-80">{errorMessage}</div>
        </div>
      </div>
    )
  }

  if (contextFiles.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center min-h-[400px] text-base-content/60">
        <DocumentTextIcon className="w-16 h-16 mb-4" />
        <p className="text-lg font-medium">No context files found</p>
        <p className="text-sm">
          Looking for CLAUDE.md, AGENTS.md, or GEMINI.md files in this project
        </p>
      </div>
    )
  }

  // Calculate total size of all context files
  const totalSize = contextFiles.reduce((sum, file) => sum + file.size, 0)
  const formatSize = (bytes: number): string => {
    if (bytes < 1024) return `${bytes} B`
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  }

  return (
    <div className="space-y-4">
      {/* Info Alert */}
      <div className="alert bg-info/10 border border-info/20">
        <svg
          xmlns="http://www.w3.org/2000/svg"
          fill="none"
          viewBox="0 0 24 24"
          className="stroke-info shrink-0 w-6 h-6 opacity-60"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth="2"
            d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
          />
        </svg>
        <div>
          <h3 className="font-semibold text-info">Context Files Detected</h3>
          <div className="text-sm opacity-70">
            These files provide instructions and context for AI coding agents. Files in .gitignore
            are automatically excluded.
            <br />
            We attempt to show when a file has been read in a session, this is currently not fully
            reliable (but we are working on it!).
          </div>
        </div>
      </div>

      {/* Header with stats and controls */}
      <div className="card bg-base-200 border border-base-300">
        <div className="card-body p-4">
          <div className="flex items-center justify-between flex-wrap gap-4">
            {/* Stats */}
            <div className="flex items-center gap-2">
              <div className="badge badge-lg gap-2">
                <DocumentTextIcon className="w-4 h-4" />
                {contextFiles.length} {contextFiles.length === 1 ? 'file' : 'files'} found
              </div>
              <div className="badge badge-lg badge-ghost gap-2">
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    strokeWidth={2}
                    d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
                  />
                </svg>
                {formatSize(totalSize)}
              </div>
            </div>

            {/* Controls */}
            <div className="flex items-center gap-2">
              <button className="btn btn-xs btn-ghost" onClick={expandAll}>
                Expand All
              </button>
              <button className="btn btn-xs btn-ghost" onClick={collapseAll}>
                Collapse All
              </button>
            </div>
          </div>
        </div>
      </div>

      {/* File list */}
      <div className="space-y-2">
        {contextFiles.map(file => (
          <ContextFileCard
            key={file.filePath}
            file={file}
            expanded={expandedFiles.has(file.filePath)}
            onToggle={() => toggleFile(file.filePath)}
            usageStats={usageCounts.get(file.relativePath)}
          />
        ))}
      </div>
    </div>
  )
}

interface ContextFileCardProps {
  file: ContextFile
  expanded: boolean
  onToggle: () => void
  usageStats?: FileUsageStats
}

/**
 * Load syntax highlighter dependencies dynamically (optional peer deps)
 * Uses Prism with one-dark and one-light themes
 */
async function loadSyntaxHighlighterDeps(): Promise<SyntaxHighlighterDeps | null> {
  try {
    const [prismModule, oneDarkStyle, oneLightStyle] = await Promise.all([
      import('react-syntax-highlighter/dist/esm/prism.js'),
      import('react-syntax-highlighter/dist/esm/styles/prism/one-dark.js'),
      import('react-syntax-highlighter/dist/esm/styles/prism/one-light.js'),
    ])

    return {
      Prism: prismModule.default,
      oneDark: oneDarkStyle.default,
      oneLight: oneLightStyle.default,
    }
  } catch (err) {
    // Syntax highlighter not installed - will fall back to plain text
    console.error('Failed to load syntax highlighter:', err)
    return null
  }
}

function ContextFileCard({ file, expanded, onToggle, usageStats }: ContextFileCardProps) {
  const [showRaw, setShowRaw] = useState(true) // Default to raw view
  const [syntaxHighlighterDeps, setSyntaxHighlighterDeps] = useState<SyntaxHighlighterDeps | null>(
    null
  )
  const [depsChecked, setDepsChecked] = useState(false)

  // Get theme for code highlighting
  const theme = document.documentElement.dataset.theme || 'guideai-dark'
  const isDark = theme.includes('dark')

  // Load syntax highlighter dependencies when file is expanded
  useEffect(() => {
    if (expanded && !depsChecked) {
      loadSyntaxHighlighterDeps()
        .then(deps => {
          if (deps) {
            console.log('Syntax highlighter loaded successfully')
          } else {
            console.log('Failed to load syntax highlighter')
          }
          setSyntaxHighlighterDeps(deps)
          setDepsChecked(true)
        })
        .catch(err => {
          console.error('Error loading syntax highlighter:', err)
          setDepsChecked(true)
        })
    }
  }, [expanded, depsChecked])

  // Format file size
  const formatSize = (bytes: number): string => {
    if (bytes < 1024) return `${bytes} B`
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  }

  // Check if file has been used
  const isUsed = usageStats && (usageStats.toolCalls > 0 || usageStats.messages > 0)

  return (
    <div
      className={`card border border-base-300 ${isUsed ? 'bg-secondary/10 border-secondary/30' : 'bg-base-100'}`}
    >
      {/* File header */}
      <div
        className={`card-body p-3 cursor-pointer transition-colors ${isUsed ? 'hover:bg-secondary/15' : 'hover:bg-base-200'}`}
        onClick={onToggle}
      >
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            {expanded ? (
              <ChevronDownIcon className="w-5 h-5 flex-shrink-0" />
            ) : (
              <ChevronRightIcon className="w-5 h-5 flex-shrink-0" />
            )}
            <code className="font-mono text-sm font-semibold">{file.relativePath}</code>
          </div>
          <div className="flex items-center gap-2 text-sm text-base-content/60">
            {usageStats && usageStats.toolCalls > 0 && (
              <span className="badge badge-secondary badge-sm font-semibold">
                {usageStats.toolCalls} Tool Call{usageStats.toolCalls !== 1 ? 's' : ''}
              </span>
            )}
            {usageStats && usageStats.messages > 0 && (
              <span className="badge badge-secondary badge-sm font-semibold">
                {usageStats.messages} Message{usageStats.messages !== 1 ? 's' : ''}
              </span>
            )}
            <span>{formatSize(file.size)}</span>
          </div>
        </div>
      </div>

      {/* File content */}
      {expanded && (
        <div className="border-t border-base-300">
          <div className="p-4">
            {/* Toggle buttons */}
            <div className="flex items-center gap-2 mb-3">
              <button
                className={`btn btn-xs ${showRaw ? 'btn-primary' : 'btn-ghost'}`}
                onClick={e => {
                  e.stopPropagation()
                  setShowRaw(true)
                }}
              >
                <CodeBracketIcon className="w-3.5 h-3.5" />
                Raw
              </button>
              <button
                className={`btn btn-xs ${!showRaw ? 'btn-primary' : 'btn-ghost'}`}
                onClick={e => {
                  e.stopPropagation()
                  setShowRaw(false)
                }}
              >
                <DocumentTextIcon className="w-3.5 h-3.5" />
                Preview
              </button>
            </div>

            {/* Content display */}
            {showRaw ? (
              syntaxHighlighterDeps && depsChecked ? (
                <div className="overflow-auto max-h-[600px]">
                  <syntaxHighlighterDeps.Prism
                    language="markdown"
                    style={isDark ? syntaxHighlighterDeps.oneDark : syntaxHighlighterDeps.oneLight}
                    customStyle={{
                      margin: 0,
                      borderRadius: '0.375rem',
                      fontSize: '0.75rem',
                      lineHeight: '1.25rem',
                    }}
                    showLineNumbers={true}
                  >
                    {file.content}
                  </syntaxHighlighterDeps.Prism>
                </div>
              ) : (
                <pre
                  className={`text-xs whitespace-pre-wrap overflow-auto max-h-[600px] p-4 rounded ${
                    isDark ? 'bg-base-300' : 'bg-base-200'
                  }`}
                >
                  <code className="text-base-content/90">{file.content}</code>
                </pre>
              )
            ) : (
              <div
                className={`prose prose-sm max-w-none overflow-auto max-h-[600px] p-4 rounded ${
                  isDark ? 'bg-base-300' : 'bg-base-200'
                }`}
              >
                <MarkdownRenderer content={file.content} />
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  )
}

interface MarkdownRendererProps {
  content: string
}

function MarkdownRenderer({ content }: MarkdownRendererProps) {
  // For now, use a simple renderer
  // This could be enhanced with react-markdown like the TextBlock component
  // but for simplicity, we'll just render as pre-formatted text with basic markdown styling

  // Split by lines and render with basic markdown patterns
  const lines = content.split('\n')

  return (
    <div className="space-y-2">
      {lines.map((line, index) => {
        // Headers
        if (line.startsWith('# ')) {
          return (
            <h1 key={index} className="text-lg font-bold mt-4 mb-2">
              {line.substring(2)}
            </h1>
          )
        }
        if (line.startsWith('## ')) {
          return (
            <h2 key={index} className="text-base font-bold mt-3 mb-2">
              {line.substring(3)}
            </h2>
          )
        }
        if (line.startsWith('### ')) {
          return (
            <h3 key={index} className="text-sm font-semibold mt-2 mb-1">
              {line.substring(4)}
            </h3>
          )
        }

        // Code blocks (simple detection)
        if (line.startsWith('```')) {
          return null // Skip code block markers for now
        }

        // Bullet points
        if (line.startsWith('- ') || line.startsWith('* ')) {
          return (
            <li key={index} className="text-sm ml-4">
              {line.substring(2)}
            </li>
          )
        }

        // Empty lines
        if (line.trim() === '') {
          return <div key={index} className="h-2" />
        }

        // Regular text
        return (
          <p key={index} className="text-sm leading-relaxed">
            {line}
          </p>
        )
      })}
    </div>
  )
}
