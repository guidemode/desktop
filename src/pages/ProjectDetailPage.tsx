import { ArrowLeftIcon, ClockIcon, DocumentTextIcon } from '@heroicons/react/24/outline'
import { useQuery } from '@tanstack/react-query'
import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-shell'
import { useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { ProjectClaudeTab } from '../components/ProjectClaudeTab'
import { ProjectDetailHeader } from '../components/ProjectDetailHeader'
import { ProjectSessionsList } from '../components/ProjectSessionsList'
import { SessionChangesTab } from '../components/SessionChangesTab'
import { SessionContextTab } from '../components/SessionContextTab'
import ProviderIcon from '../components/icons/ProviderIcon'
import { useClaudeFiles } from '../hooks/useClaudeFiles'
import type { LocalProject } from '../hooks/useLocalProjects'

type TabType = 'sessions' | 'context' | 'changes' | 'claude'

export default function ProjectDetailPage() {
  const { projectId } = useParams<{ projectId: string }>()
  const navigate = useNavigate()
  const [activeTab, setActiveTab] = useState<TabType>('sessions')

  // Fetch project data
  const { data: project, isLoading } = useQuery<LocalProject | null>({
    queryKey: ['project', projectId],
    queryFn: async () => {
      if (!projectId) throw new Error('Project ID is required')
      const result = await invoke<any>('get_project_by_id', { projectId })

      if (!result) return null

      // Result already has camelCase fields from Rust command
      return {
        id: result.id,
        name: result.name,
        githubRepo: result.githubRepo,
        cwd: result.cwd,
        type: result.type,
        createdAt: result.createdAt,
        updatedAt: result.updatedAt,
        sessionCount: result.sessionCount || 0,
      }
    },
    enabled: !!projectId,
  })

  // Check if project has .claude folder
  const { data: claudeFiles = [] } = useClaudeFiles(project?.cwd, !!project)
  const hasClaude = claudeFiles.length > 0

  // For the changes tab, we'll use a simple approach:
  // Just show current uncommitted changes (HEAD to working directory)
  // We can use "HEAD" as the first commit hash

  const handleOpenFolder = async () => {
    if (project?.cwd) {
      try {
        await invoke('open_folder_in_os', { path: project.cwd })
      } catch (error) {
        console.error('Failed to open folder:', error)
      }
    }
  }

  const handleViewGithub = async () => {
    if (project?.githubRepo) {
      try {
        await open(project.githubRepo)
      } catch (error) {
        console.error('Failed to open GitHub:', error)
      }
    }
  }

  if (isLoading) {
    return (
      <div className="flex items-center justify-center min-h-screen">
        <span className="loading loading-spinner loading-lg" />
      </div>
    )
  }

  if (!project) {
    return (
      <div className="p-6">
        <div className="alert alert-error">
          <span>Project not found</span>
        </div>
      </div>
    )
  }

  return (
    <div className="p-6 space-y-6">
      {/* Page Header */}
      <div className="flex items-center justify-between">
        <h1 className="text-3xl font-bold">Project Detail</h1>
        <button
          type="button"
          onClick={() => navigate('/projects')}
          className="btn btn-ghost btn-sm gap-2"
        >
          <ArrowLeftIcon className="w-4 h-4" />
          Back to Projects
        </button>
      </div>

      {/* Project Detail Header */}
      <ProjectDetailHeader
        project={project}
        onOpenFolder={handleOpenFolder}
        onViewGithub={project.githubRepo ? handleViewGithub : undefined}
      />

      {/* Tabs */}
      <div className="card bg-base-200 border border-base-300 border-b-2 rounded-lg overflow-hidden">
        <div className="flex items-stretch">
          {/* Tab Buttons */}
          <div className="tabs tabs-bordered flex-1">
            <button
              type="button"
              className={`tab tab-lg gap-2 rounded-tl-lg ${
                activeTab === 'sessions'
                  ? 'tab-active bg-base-100 text-primary font-semibold border-b-2 border-primary'
                  : 'hover:bg-base-300'
              }`}
              onClick={() => setActiveTab('sessions')}
              title="Recent Sessions"
            >
              <ClockIcon className="w-5 h-5" />
              <span className="hidden md:inline">Recent Sessions</span>
              <div className="badge badge-sm ml-2">{project.sessionCount}</div>
            </button>

            <button
              type="button"
              className={`tab tab-lg gap-2 ${
                activeTab === 'context'
                  ? 'tab-active bg-base-100 text-primary font-semibold border-b-2 border-primary'
                  : 'hover:bg-base-300'
              }`}
              onClick={() => setActiveTab('context')}
              title="Context Files"
            >
              <DocumentTextIcon className="w-5 h-5" />
              <span className="hidden md:inline">Context</span>
            </button>

            {hasClaude && (
              <button
                type="button"
                className={`tab tab-lg gap-2 ${
                  activeTab === 'claude'
                    ? 'tab-active bg-base-100 text-primary font-semibold border-b-2 border-primary'
                    : 'hover:bg-base-300'
                }`}
                onClick={() => setActiveTab('claude')}
                title="Claude Configuration"
              >
                <ProviderIcon providerId="claude-code" size={20} />
                <span className="hidden md:inline">Claude</span>
                <div className="badge badge-sm ml-2">{claudeFiles.length}</div>
              </button>
            )}

            <button
              type="button"
              className={`tab tab-lg gap-2 ${
                activeTab === 'changes'
                  ? 'tab-active bg-base-100 text-primary font-semibold border-b-2 border-primary'
                  : 'hover:bg-base-300'
              }`}
              onClick={() => setActiveTab('changes')}
              title="Git Changes"
            >
              <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4"
                />
              </svg>
              <span className="hidden md:inline">Changes</span>
            </button>
          </div>
        </div>
      </div>

      {/* Tab Content */}
      <div className="min-h-[400px]">
        {activeTab === 'sessions' && <ProjectSessionsList projectId={project.id} />}

        {activeTab === 'context' && (
          <SessionContextTab
            session={{
              sessionId: `project-${project.id}`,
              cwd: project.cwd,
            }}
            fileContent={null}
            hideInfoBanner={true}
          />
        )}

        {activeTab === 'claude' && (
          <ProjectClaudeTab
            project={{
              id: project.id,
              cwd: project.cwd,
            }}
          />
        )}

        {activeTab === 'changes' && (
          <SessionChangesTab
            session={{
              sessionId: `project-${project.id}`,
              cwd: project.cwd,
              first_commit_hash: 'HEAD',
              latest_commit_hash: 'HEAD', // Same hash means show uncommitted changes
              session_start_time: null,
              session_end_time: null, // null end time = active session = show uncommitted
            }}
            hideSessionInfo={true}
          />
        )}
      </div>
    </div>
  )
}
