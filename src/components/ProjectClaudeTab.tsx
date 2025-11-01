import { ChevronDownIcon, ChevronRightIcon, CodeBracketIcon } from '@heroicons/react/24/outline'
import { useEffect, useState } from 'react'
import { type ClaudeFile, type ClaudeFileType, useClaudeFiles } from '../hooks/useClaudeFiles'
import ProviderIcon from './icons/ProviderIcon'

// Dynamic import types for syntax highlighter
interface SyntaxHighlighterDeps {
  Prism: any
  oneDark: any
  oneLight: any
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

interface ProjectClaudeTabProps {
  project: {
    id: string
    cwd: string
  }
}

export function ProjectClaudeTab({ project }: ProjectClaudeTabProps) {
  const [expandedFiles, setExpandedFiles] = useState<Set<string>>(new Set())

  // Fetch Claude configuration files
  const { data: claudeFiles = [], isLoading: loading, error } = useClaudeFiles(project.cwd)

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
    setExpandedFiles(new Set(claudeFiles.map(f => f.file_path)))
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
          <div className="font-semibold">Failed to load Claude files</div>
          <div className="text-sm opacity-80">{errorMessage}</div>
        </div>
      </div>
    )
  }

  if (claudeFiles.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center min-h-[400px] text-base-content/60">
        <ProviderIcon providerId="claude-code" size={64} className="mb-4 opacity-60" />
        <p className="text-lg font-medium">No Claude configuration found</p>
        <p className="text-sm text-center max-w-md">
          This project doesn't have a <code className="badge badge-sm">.claude</code> folder yet.
          Add commands, skills, or configuration to customize Claude Code for this project.
        </p>
      </div>
    )
  }

  // Calculate stats
  const totalSize = claudeFiles.reduce((sum, file) => sum + file.size, 0)
  const commandCount = claudeFiles.filter(f => f.file_type === 'command').length
  const skillCount = claudeFiles.filter(f => f.file_type === 'skill').length
  const configCount = claudeFiles.filter(f => f.file_type === 'config').length

  const formatSize = (bytes: number): string => {
    if (bytes < 1024) return `${bytes} B`
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  }

  return (
    <div className="space-y-4">
      {/* Header with stats and controls */}
      <div className="card bg-base-200 border border-base-300">
        <div className="card-body p-4">
          <div className="flex items-center justify-between flex-wrap gap-4">
            {/* Stats */}
            <div className="flex items-center gap-2 flex-wrap">
              <div className="badge badge-lg gap-2">
                <ProviderIcon providerId="claude-code" size={16} />
                {claudeFiles.length} {claudeFiles.length === 1 ? 'file' : 'files'}
              </div>
              {commandCount > 0 && (
                <div className="badge badge-lg badge-primary gap-2">
                  {commandCount} {commandCount === 1 ? 'command' : 'commands'}
                </div>
              )}
              {skillCount > 0 && (
                <div className="badge badge-lg badge-secondary gap-2">
                  {skillCount} {skillCount === 1 ? 'skill' : 'skills'}
                </div>
              )}
              {configCount > 0 && (
                <div className="badge badge-lg badge-ghost gap-2">
                  {configCount} config {configCount === 1 ? 'file' : 'files'}
                </div>
              )}
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
        {claudeFiles.map(file => (
          <ClaudeFileCard
            key={file.file_path}
            file={file}
            expanded={expandedFiles.has(file.file_path)}
            onToggle={() => toggleFile(file.file_path)}
          />
        ))}
      </div>
    </div>
  )
}

interface ClaudeFileCardProps {
  file: ClaudeFile
  expanded: boolean
  onToggle: () => void
}

function ClaudeFileCard({ file, expanded, onToggle }: ClaudeFileCardProps) {
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
          setSyntaxHighlighterDeps(deps)
          setDepsChecked(true)
        })
        .catch(err => {
          console.error('Error loading syntax highlighter:', err)
          setDepsChecked(true)
        })
    }
  }, [expanded, depsChecked])

  const formatSize = (bytes: number): string => {
    if (bytes < 1024) return `${bytes} B`
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  }

  const getBadgeForType = (type: ClaudeFileType) => {
    switch (type) {
      case 'command':
        return <div className="badge badge-primary badge-sm">Command</div>
      case 'skill':
        return <div className="badge badge-secondary badge-sm">Skill</div>
      case 'config':
        return <div className="badge badge-ghost badge-sm">Config</div>
      default:
        return null
    }
  }

  // Determine language for syntax highlighting
  const getLanguage = (fileType: ClaudeFileType): string => {
    return fileType === 'config' ? 'json' : 'markdown'
  }

  return (
    <div className="card bg-base-200 border border-base-300 hover:border-base-content/20 transition-colors">
      {/* Card header - always visible */}
      <div className="card-body p-4 cursor-pointer select-none" onClick={onToggle}>
        <div className="flex items-center justify-between gap-4">
          <div className="flex items-center gap-3 flex-1 min-w-0">
            {/* Expand/collapse icon */}
            {expanded ? (
              <ChevronDownIcon className="w-5 h-5 flex-shrink-0 text-base-content/60" />
            ) : (
              <ChevronRightIcon className="w-5 h-5 flex-shrink-0 text-base-content/60" />
            )}

            {/* File icon */}
            {file.file_type === 'config' ? (
              <CodeBracketIcon className="w-5 h-5 flex-shrink-0 text-base-content/60" />
            ) : (
              <svg
                className="w-5 h-5 flex-shrink-0 text-base-content/60"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
                />
              </svg>
            )}

            {/* File info */}
            <div className="flex-1 min-w-0">
              <div className="flex items-center gap-2 flex-wrap">
                <span className="font-mono text-sm font-medium truncate">{file.relative_path}</span>
                {getBadgeForType(file.file_type)}
              </div>

              {/* Show description from metadata if available */}
              {file.metadata?.description && (
                <div className="text-xs text-base-content/60 mt-1 line-clamp-1">
                  {file.metadata.description}
                </div>
              )}
            </div>
          </div>

          {/* File size */}
          <div className="text-xs text-base-content/60 flex-shrink-0">{formatSize(file.size)}</div>
        </div>
      </div>

      {/* Expanded content */}
      {expanded && (
        <div className="px-4 pb-4">
          <div className="border-t border-base-300 pt-4">
            {/* Metadata section */}
            {file.metadata && (file.metadata.name || file.metadata.description) && (
              <div className="mb-4 space-y-2">
                {file.metadata.name && (
                  <div>
                    <span className="text-xs font-semibold text-base-content/60 uppercase">
                      Name
                    </span>
                    <div className="text-sm font-mono">{file.metadata.name}</div>
                  </div>
                )}
                {file.metadata.description && (
                  <div>
                    <span className="text-xs font-semibold text-base-content/60 uppercase">
                      Description
                    </span>
                    <div className="text-sm">{file.metadata.description}</div>
                  </div>
                )}
                <div className="divider my-2" />
              </div>
            )}

            {/* Content rendering with syntax highlighting */}
            {syntaxHighlighterDeps && depsChecked ? (
              <div className="overflow-auto max-h-[600px]">
                <syntaxHighlighterDeps.Prism
                  language={getLanguage(file.file_type)}
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
              // Fallback to plain text if syntax highlighter not loaded
              <pre
                className={`text-xs whitespace-pre-wrap overflow-auto max-h-[600px] p-4 rounded ${
                  isDark ? 'bg-base-300' : 'bg-base-200'
                }`}
              >
                <code className="text-base-content/90">{file.content}</code>
              </pre>
            )}
          </div>
        </div>
      )}
    </div>
  )
}
