import { useState, Component, type ReactNode, type ErrorInfo } from "react";
import { useQuery } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { DiffView, DiffModeEnum } from "@git-diff-view/react";
import "./git-diff-scoped.css";
import {
  ChevronRightIcon,
  ChevronDownIcon,
  DocumentTextIcon,
  PlusIcon,
  MinusIcon,
  ExclamationTriangleIcon,
} from "@heroicons/react/24/outline";

interface FileDiff {
  oldPath: string;
  newPath: string;
  changeType: "added" | "deleted" | "modified" | "renamed";
  language: string | null;
  hunks: string[];
  stats: {
    additions: number;
    deletions: number;
  };
  isBinary: boolean;
  oldContent?: string | null;
  newContent?: string | null;
}

interface SessionChangesTabProps {
  session: {
    sessionId: string;
    cwd: string;
    first_commit_hash: string;
    latest_commit_hash: string;
  };
  isActive?: boolean;
}

async function fetchGitDiff(
  cwd: string,
  firstCommitHash: string,
  latestCommitHash: string,
  isActive: boolean
): Promise<FileDiff[]> {
  // Tauri returns snake_case from Rust, so we need to handle it
  const result = await invoke<any[]>("get_session_git_diff", {
    cwd,
    firstCommitHash,
    latestCommitHash,
    isActive,
  });

  // Convert snake_case to camelCase for TypeScript
  return result.map((item: any) => ({
    oldPath: item.old_path || "",
    newPath: item.new_path || "",
    changeType: item.change_type as any,
    language: item.language || null,
    hunks: item.hunks || [],
    stats: {
      additions: item.stats?.additions || 0,
      deletions: item.stats?.deletions || 0,
    },
    isBinary: item.is_binary || false,
    oldContent: item.old_content || null,
    newContent: item.new_content || null,
  }));
}

export function SessionChangesTab({
  session,
  isActive = false,
}: SessionChangesTabProps) {
  const [expandedFiles, setExpandedFiles] = useState<Set<string>>(new Set());
  const [viewMode, setViewMode] = useState<"split" | "unified">("split");

  // Check if we're showing unstaged changes (commits are the same)
  const isShowingUnstagedChanges = session.first_commit_hash === session.latest_commit_hash && isActive;

  // Fetch git diff with React Query
  const {
    data: diffs = [],
    isLoading: loading,
    error,
  } = useQuery({
    queryKey: ["session-git-diff", session.sessionId, isActive],
    queryFn: () =>
      fetchGitDiff(
        session.cwd,
        session.first_commit_hash,
        session.latest_commit_hash,
        isActive
      ),
    // Auto-expand first 5 files on initial load
    onSuccess: (data) => {
      if (expandedFiles.size === 0) {
        const firstFive = data.slice(0, 5).map((f) => f.newPath);
        setExpandedFiles(new Set(firstFive));
      }
    },
  });

  const toggleFile = (filePath: string) => {
    setExpandedFiles((prev) => {
      const next = new Set(prev);
      if (next.has(filePath)) {
        next.delete(filePath);
      } else {
        next.add(filePath);
      }
      return next;
    });
  };

  const expandAll = () => {
    setExpandedFiles(new Set(diffs.map((f) => f.newPath)));
  };

  const collapseAll = () => {
    setExpandedFiles(new Set());
  };

  const totalStats = diffs.reduce(
    (acc, file) => ({
      additions: acc.additions + file.stats.additions,
      deletions: acc.deletions + file.stats.deletions,
    }),
    { additions: 0, deletions: 0 }
  );

  if (loading) {
    return (
      <div className="flex items-center justify-center min-h-[400px]">
        <span className="loading loading-spinner loading-lg" />
      </div>
    );
  }

  if (error) {
    const errorMessage = error instanceof Error ? error.message : String(error);
    return (
      <div className="alert alert-error">
        <svg
          className="w-6 h-6"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
          />
        </svg>
        <div>
          <div className="font-semibold">Failed to load git diff</div>
          <div className="text-sm opacity-80">{errorMessage}</div>
        </div>
      </div>
    );
  }

  if (diffs.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center min-h-[400px] text-base-content/60">
        <DocumentTextIcon className="w-16 h-16 mb-4" />
        <p className="text-lg font-medium">No changes to display</p>
        <p className="text-sm">
          {session.first_commit_hash === session.latest_commit_hash && !isActive
            ? "Session is inactive with no commits during the session"
            : "The first and latest commits have identical content"}
        </p>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {/* Info Alert for Unstaged Changes */}
      {isShowingUnstagedChanges && (
        <div className="alert bg-info/10 border border-info/20">
          <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" className="stroke-info shrink-0 w-6 h-6 opacity-60">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"></path>
          </svg>
          <div>
            <h3 className="font-semibold text-info">Showing Uncommitted Changes</h3>
            <div className="text-sm opacity-70">No commits were made during this active session. Displaying all uncommitted changes (staged, unstaged, and untracked files) since the session started. This will only be shown while the session is active.</div>
          </div>
        </div>
      )}

      {/* Header with stats and controls */}
      <div className="card bg-base-200 border border-base-300">
        <div className="card-body p-4">
          <div className="flex items-center justify-between flex-wrap gap-4">
            {/* Stats */}
            <div className="flex items-center gap-4">
              <div className="badge badge-lg gap-2">
                <DocumentTextIcon className="w-4 h-4" />
                {diffs.length} {diffs.length === 1 ? "file" : "files"} changed
              </div>
              <div className="badge badge-lg badge-success gap-2">
                <PlusIcon className="w-4 h-4" />
                {totalStats.additions} additions
              </div>
              <div className="badge badge-lg badge-error gap-2">
                <MinusIcon className="w-4 h-4" />
                {totalStats.deletions} deletions
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
              <div className="divider divider-horizontal mx-0"></div>
              <div className="btn-group">
                <button
                  className={`btn btn-xs ${
                    viewMode === "split" ? "btn-active" : ""
                  }`}
                  onClick={() => setViewMode("split")}
                >
                  Split
                </button>
                <button
                  className={`btn btn-xs ${
                    viewMode === "unified" ? "btn-active" : ""
                  }`}
                  onClick={() => setViewMode("unified")}
                >
                  Unified
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* File diffs */}
      <div className="space-y-2">
        {diffs.map((file, index) => (
          <FileDiffCard
            key={`${file.newPath}-${index}`}
            file={file}
            expanded={expandedFiles.has(file.newPath)}
            onToggle={() => toggleFile(file.newPath)}
            viewMode={viewMode}
          />
        ))}
      </div>
    </div>
  );
}

interface FileDiffCardProps {
  file: FileDiff;
  expanded: boolean;
  onToggle: () => void;
  viewMode: "split" | "unified";
}

function FileDiffCard({
  file,
  expanded,
  onToggle,
  viewMode,
}: FileDiffCardProps) {
  const changeTypeColors = {
    added: "badge-success",
    deleted: "badge-error",
    modified: "badge-info",
    renamed: "badge-warning",
  };

  // Get current theme from document
  const theme = document.documentElement.dataset.theme || "guideai-dark";
  const diffTheme = theme.includes("light") ? "light" : "dark";

  // Validate and clean hunks
  const validHunks = file.hunks.filter((hunk) => {
    if (!hunk || !hunk.trim()) {
      return false;
    }
    // Check if hunk has proper unified diff format
    // Must have: Index, ---, +++, and @@ lines
    const hasHeader = hunk.includes("---") && hunk.includes("+++");
    const hasHunkMarker = hunk.includes("@@");
    const hasContent = /^[\+\- ]/.test(hunk.split('\n').slice(-2).join('\n'));

    if (!hasHeader || !hasHunkMarker) {
      console.warn(`Invalid hunk format for ${file.newPath}:`, hunk.substring(0, 200));
      return false;
    }

    // Ensure hunk ends with a newline
    return true;
  }).map(hunk => {
    // Ensure hunk ends properly
    return hunk.endsWith('\n') ? hunk : hunk + '\n';
  });

  return (
    <div className="card bg-base-100 border border-base-300">
      {/* File header */}
      <div
        className="card-body p-3 cursor-pointer hover:bg-base-200 transition-colors"
        onClick={onToggle}
      >
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            {expanded ? (
              <ChevronDownIcon className="w-5 h-5 flex-shrink-0" />
            ) : (
              <ChevronRightIcon className="w-5 h-5 flex-shrink-0" />
            )}
            <code className="font-mono text-sm font-semibold">
              {file.newPath}
            </code>
            <span
              className={`badge badge-sm ${changeTypeColors[file.changeType]}`}
            >
              {file.changeType}
            </span>
            {file.isBinary && <span className="badge badge-sm">binary</span>}
          </div>
          <div className="flex items-center gap-2 text-sm">
            <span className="text-success">+{file.stats.additions}</span>
            <span className="text-error">-{file.stats.deletions}</span>
          </div>
        </div>
      </div>

      {/* Diff content */}
      {expanded && (
        <div className="border-t border-base-300">
          {file.isBinary ? (
            <div className="p-8 text-center text-base-content/60">
              <DocumentTextIcon className="w-12 h-12 mx-auto mb-2 opacity-50" />
              <p>Binary file changed</p>
            </div>
          ) : validHunks.length === 0 ? (
            <div className="p-8 text-center text-base-content/60">
              <DocumentTextIcon className="w-12 h-12 mx-auto mb-2 opacity-50" />
              <p>No valid diff content available</p>
              {file.hunks.length > 0 && (
                <p className="text-xs mt-2">
                  Invalid hunks detected - check console for details
                </p>
              )}
            </div>
          ) : (
            <div className="diff-view-wrapper">
              <DiffView
                data={{
                  oldFile: {
                    fileName: file.oldPath,
                    fileLang: file.language || undefined,
                    content: file.oldContent || undefined,
                  },
                  newFile: {
                    fileName: file.newPath,
                    fileLang: file.language || undefined,
                    content: file.newContent || undefined,
                  },
                  hunks: validHunks,
                }}
                diffViewMode={
                  viewMode === "split"
                    ? DiffModeEnum.Split
                    : DiffModeEnum.Unified
                }
                diffViewTheme={diffTheme}
                diffViewHighlight={true}
                diffViewWrap={false}
                diffViewFontSize={12}
              />
            </div>
          )}
        </div>
      )}
    </div>
  );
}
