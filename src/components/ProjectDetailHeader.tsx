import { CalendarIcon, CodeBracketIcon, FolderIcon } from '@heroicons/react/24/outline'
import type { LocalProject } from '../hooks/useLocalProjects'

interface ProjectDetailHeaderProps {
  project: LocalProject
  onOpenFolder?: () => void
  onViewGithub?: () => void
}

export function ProjectDetailHeader({
  project,
  onOpenFolder,
  onViewGithub,
}: ProjectDetailHeaderProps) {
  const formatDate = (timestamp: number) => {
    return new Date(timestamp).toLocaleDateString('en-US', {
      year: 'numeric',
      month: 'short',
      day: 'numeric',
    })
  }

  return (
    <div className="card bg-base-100 border border-base-300 shadow-sm">
      <div className="card-body p-6">
        <div className="flex items-start justify-between gap-4">
          {/* Left side: Project info */}
          <div className="flex-1 space-y-3">
            {/* Project name and type */}
            <div className="flex items-center gap-3">
              <h2 className="text-2xl font-bold text-base-content">{project.name}</h2>
              <div className="badge badge-primary badge-outline">{project.type}</div>
            </div>

            {/* CWD */}
            <div className="flex items-center gap-2 text-sm text-base-content/70">
              <FolderIcon className="w-4 h-4" />
              <code className="text-xs bg-base-200 px-2 py-1 rounded">{project.cwd}</code>
            </div>

            {/* GitHub repo */}
            {project.githubRepo && (
              <div className="flex items-center gap-2 text-sm text-base-content/70">
                <CodeBracketIcon className="w-4 h-4" />
                <a
                  href={project.githubRepo}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="link link-primary text-xs"
                >
                  {project.githubRepo}
                </a>
              </div>
            )}

            {/* Metadata */}
            <div className="flex items-center gap-4 text-sm text-base-content/60">
              <div className="flex items-center gap-1.5">
                <CalendarIcon className="w-4 h-4" />
                <span>Created {formatDate(project.createdAt)}</span>
              </div>
              {project.updatedAt !== project.createdAt && (
                <div className="flex items-center gap-1.5">
                  <span>•</span>
                  <span>Updated {formatDate(project.updatedAt)}</span>
                </div>
              )}
              <div className="flex items-center gap-1.5">
                <span>•</span>
                <span>{project.sessionCount} sessions</span>
              </div>
            </div>
          </div>

          {/* Right side: Actions */}
          <div className="flex flex-col gap-2">
            {onOpenFolder && (
              <button type="button" onClick={onOpenFolder} className="btn btn-sm btn-outline">
                <FolderIcon className="w-4 h-4" />
                Open Folder
              </button>
            )}
            {project.githubRepo && onViewGithub && (
              <button type="button" onClick={onViewGithub} className="btn btn-sm btn-outline">
                <CodeBracketIcon className="w-4 h-4" />
                View on GitHub
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  )
}
