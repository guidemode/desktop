import { open } from '@tauri-apps/plugin-shell'
import { Link } from 'react-router-dom'
import { useLocalProjects } from '../hooks/useLocalProjects'

const PROJECT_TYPE_LABELS: Record<string, string> = {
  nodejs: 'Node.js',
  rust: 'Rust',
  python: 'Python',
  go: 'Go',
  generic: 'Generic',
}

const PROJECT_TYPE_COLORS: Record<string, string> = {
  nodejs: 'badge-success',
  rust: 'badge-error',
  python: 'badge-info',
  go: 'badge-primary',
  generic: 'badge-neutral',
}

export default function ProjectsPage() {
  const { projects, loading, error, refresh } = useLocalProjects()

  if (loading) {
    return (
      <div className="flex items-center justify-center min-h-[400px]">
        <span className="loading loading-spinner loading-lg" />
      </div>
    )
  }

  if (error) {
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
        <span>{error}</span>
      </div>
    )
  }

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold">Projects</h1>
          <p className="text-sm text-base-content/70 mt-1">
            {projects.length} {projects.length === 1 ? 'project' : 'projects'} found
          </p>
        </div>
        <button onClick={refresh} className="btn btn-sm btn-ghost">
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
            />
          </svg>
          Refresh
        </button>
      </div>

      {/* Projects Grid */}
      {projects.length === 0 ? (
        <div className="text-center py-12">
          <svg
            className="w-16 h-16 mx-auto text-base-content/30 mb-4"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"
            />
          </svg>
          <h3 className="text-lg font-semibold mb-2">No projects found</h3>
          <p className="text-base-content/70">Projects will appear here as sessions are detected</p>
        </div>
      ) : (
        <div className="grid gap-4 grid-cols-1 md:grid-cols-2 lg:grid-cols-3">
          {projects.map(project => (
            <Link
              key={project.id}
              to={`/projects/${encodeURIComponent(project.id)}`}
              className="card bg-base-100 border border-base-300 hover:shadow-lg hover:border-primary/50 transition-all"
            >
              <div className="card-body">
                {/* Project Header */}
                <div className="flex items-start justify-between gap-2 mb-2">
                  <h3 className="card-title text-lg truncate flex-1">{project.name}</h3>
                  <span
                    className={`badge badge-sm ${PROJECT_TYPE_COLORS[project.type] || 'badge-neutral'}`}
                  >
                    {PROJECT_TYPE_LABELS[project.type] || project.type}
                  </span>
                </div>

                {/* GitHub Repo */}
                {project.githubRepo && (
                  <button
                    type="button"
                    onClick={e => {
                      e.preventDefault()
                      e.stopPropagation()
                      if (project.githubRepo) {
                        open(project.githubRepo.replace(/\.git$/, ''))
                      }
                    }}
                    className="flex items-center gap-2 text-xs text-base-content/60 hover:text-primary transition-colors w-fit mb-2 cursor-pointer"
                  >
                    <svg className="w-3 h-3" fill="currentColor" viewBox="0 0 24 24">
                      <path
                        fillRule="evenodd"
                        d="M12 0C5.374 0 0 5.373 0 12c0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23A11.509 11.509 0 0112 5.803c1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576C20.566 21.797 24 17.3 24 12c0-6.627-5.373-12-12-12z"
                        clipRule="evenodd"
                      />
                    </svg>
                    <span className="truncate">
                      {project.githubRepo
                        .replace(/^https?:\/\/github\.com\//, '')
                        .replace(/\.git$/, '')}
                    </span>
                  </button>
                )}

                {/* Session Count */}
                <div className="flex items-baseline gap-2 mt-auto pt-3 border-t border-base-300">
                  <span className="text-2xl font-bold text-primary">{project.sessionCount}</span>
                  <span className="text-sm text-base-content/70">
                    {project.sessionCount === 1 ? 'session' : 'sessions'}
                  </span>
                </div>

                {/* Last Updated */}
                <div className="text-xs text-base-content/60">
                  Last activity:{' '}
                  {new Date(project.updatedAt).toLocaleDateString('en-US', {
                    month: 'short',
                    day: 'numeric',
                    year: 'numeric',
                  })}
                </div>
              </div>
            </Link>
          ))}
        </div>
      )}
    </div>
  )
}
