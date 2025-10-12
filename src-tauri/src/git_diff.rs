use git2::{Diff, DiffFormat, DiffOptions, Repository};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDiff {
    pub old_path: String,
    pub new_path: String,
    pub change_type: String, // "added", "deleted", "modified", "renamed"
    pub language: Option<String>,
    pub hunks: Vec<String>,
    pub stats: DiffStats,
    pub is_binary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffStats {
    pub additions: u32,
    pub deletions: u32,
}

/// Get diff between two commits in a repository, or uncommitted changes if commits are the same AND session is active
pub fn get_commit_diff(
    cwd: &str,
    first_commit_hash: &str,
    latest_commit_hash: &str,
    is_active: bool,
) -> Result<Vec<FileDiff>, String> {
    // Open repository
    let repo = Repository::open(cwd)
        .map_err(|e| format!("Failed to open git repository at {}: {}", cwd, e))?;

    // Create diff options
    let mut diff_opts = DiffOptions::new();
    diff_opts.context_lines(3); // Standard 3 lines of context
    diff_opts.include_untracked(true); // Include untracked files
    diff_opts.recurse_untracked_dirs(true); // Recurse into untracked directories

    // Check if commits are the same - if so, show uncommitted changes ONLY if session is active
    if first_commit_hash == latest_commit_hash {
        // Only show uncommitted changes for active sessions
        // For inactive sessions with identical commits, return empty diff to avoid huge diffs
        if !is_active {
            return Ok(Vec::new());
        }

        // Get the commit and its tree
        let commit_oid = repo
            .revparse_single(first_commit_hash)
            .map_err(|e| format!("Failed to find commit {}: {}", first_commit_hash, e))?;
        let commit = commit_oid
            .peel_to_commit()
            .map_err(|e| format!("Failed to peel commit: {}", e))?;
        let tree = commit
            .tree()
            .map_err(|e| format!("Failed to get commit tree: {}", e))?;

        // Diff between commit tree and working directory (includes staged, unstaged, and untracked files)
        let diff = repo
            .diff_tree_to_workdir_with_index(Some(&tree), Some(&mut diff_opts))
            .map_err(|e| format!("Failed to create diff to working directory: {}", e))?;

        return parse_diff(diff);
    }

    // Get commits for diff between two different commits
    let first_oid = repo
        .revparse_single(first_commit_hash)
        .map_err(|e| format!("Failed to find first commit {}: {}", first_commit_hash, e))?;
    let first_commit = first_oid
        .peel_to_commit()
        .map_err(|e| format!("Failed to peel first commit: {}", e))?;

    let latest_oid = repo
        .revparse_single(latest_commit_hash)
        .map_err(|e| format!("Failed to find latest commit {}: {}", latest_commit_hash, e))?;
    let latest_commit = latest_oid
        .peel_to_commit()
        .map_err(|e| format!("Failed to peel latest commit: {}", e))?;

    // Get trees
    let first_tree = first_commit
        .tree()
        .map_err(|e| format!("Failed to get first commit tree: {}", e))?;
    let latest_tree = latest_commit
        .tree()
        .map_err(|e| format!("Failed to get latest commit tree: {}", e))?;

    // Diff between two trees
    let diff = repo
        .diff_tree_to_tree(Some(&first_tree), Some(&latest_tree), Some(&mut diff_opts))
        .map_err(|e| format!("Failed to create diff: {}", e))?;

    // Parse diff into FileDiff structures
    parse_diff(diff)
}

/// Parse git2 Diff into structured FileDiff objects
fn parse_diff(diff: Diff) -> Result<Vec<FileDiff>, String> {
    let mut file_diffs: Vec<FileDiff> = Vec::new();
    let mut current_file: Option<FileDiff> = None;
    let mut current_file_content = String::new();
    let mut current_hunk_header: Option<String> = None;
    let mut file_headers_added = false;

    // Print diff and collect output
    diff.print(DiffFormat::Patch, |delta, hunk, line| {
        let old_path = delta.old_file().path().unwrap_or(Path::new("")).to_string_lossy().to_string();
        let new_path = delta.new_file().path().unwrap_or(Path::new("")).to_string_lossy().to_string();

        // Detect file change
        if current_file.is_none() ||
           current_file.as_ref().unwrap().new_path != new_path {
            // Save previous file if exists
            if let Some(mut file) = current_file.take() {
                if !current_file_content.is_empty() {
                    // Store the entire file diff as a single string
                    file.hunks.push(current_file_content.clone());
                    current_file_content.clear();
                }
                file_diffs.push(file);
            }

            // Start new file
            let change_type = match delta.status() {
                git2::Delta::Added => "added",
                git2::Delta::Deleted => "deleted",
                git2::Delta::Modified => "modified",
                git2::Delta::Renamed => "renamed",
                _ => "modified",
            };

            current_file = Some(FileDiff {
                old_path: old_path.clone(),
                new_path: new_path.clone(),
                change_type: change_type.to_string(),
                language: detect_language(&new_path),
                hunks: Vec::new(),
                stats: DiffStats { additions: 0, deletions: 0 },
                is_binary: delta.new_file().is_binary(),
            });
            current_hunk_header = None;
            file_headers_added = false;
        }

        // Add file headers once at the beginning (matching @git-diff-view format)
        if !file_headers_added {
            // Add Index header
            current_file_content.push_str(&format!("Index: {}\n", new_path));
            current_file_content.push_str("===================================================================\n");
            // Add file headers with tabs (like the library expects)
            current_file_content.push_str(&format!("--- {}\t\n", old_path));
            current_file_content.push_str(&format!("+++ {}\t\n", new_path));
            file_headers_added = true;
        }

        // Handle hunk header - detect new hunk by comparing header
        if let Some(hunk_data) = hunk {
            let header = format!(
                "@@ -{},{} +{},{} @@",
                hunk_data.old_start(),
                hunk_data.old_lines(),
                hunk_data.new_start(),
                hunk_data.new_lines()
            );

            // Check if this is a NEW hunk (different header than current)
            if current_hunk_header.as_ref() != Some(&header) {
                // Start new hunk
                if !current_file_content.is_empty() && !current_file_content.ends_with('\n') {
                    current_file_content.push('\n');
                }
                current_file_content.push_str(&header);
                current_hunk_header = Some(header);
            }
        }

        // Add line to current file content
        let origin = line.origin();
        let content = String::from_utf8_lossy(line.content());

        match origin {
            '+' | '-' | ' ' => {
                current_file_content.push('\n');
                current_file_content.push(origin);
                current_file_content.push_str(&content.trim_end_matches('\n'));

                // Update stats
                if let Some(ref mut file) = current_file {
                    if origin == '+' {
                        file.stats.additions += 1;
                    } else if origin == '-' {
                        file.stats.deletions += 1;
                    }
                }
            }
            _ => {}
        }

        true // Continue iteration
    })
    .map_err(|e| format!("Failed to print diff: {}", e))?;

    // Save last file
    if let Some(mut file) = current_file.take() {
        if !current_file_content.is_empty() {
            file.hunks.push(current_file_content);
        }
        file_diffs.push(file);
    }

    Ok(file_diffs)
}

/// Detect programming language from file extension
fn detect_language(file_path: &str) -> Option<String> {
    let path = Path::new(file_path);
    let ext = path.extension()?.to_str()?;

    let lang = match ext {
        "rs" => "rust",
        "ts" | "tsx" => "typescript",
        "js" | "jsx" => "javascript",
        "py" => "python",
        "go" => "go",
        "java" => "java",
        "cpp" | "cc" | "cxx" => "cpp",
        "c" => "c",
        "h" | "hpp" => "cpp",
        "rb" => "ruby",
        "php" => "php",
        "swift" => "swift",
        "kt" => "kotlin",
        "cs" => "csharp",
        "sh" | "bash" => "bash",
        "sql" => "sql",
        "html" => "html",
        "css" => "css",
        "scss" | "sass" => "scss",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "xml" => "xml",
        "md" => "markdown",
        _ => return None,
    };

    Some(lang.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_language() {
        assert_eq!(detect_language("main.rs"), Some("rust".to_string()));
        assert_eq!(detect_language("App.tsx"), Some("typescript".to_string()));
        assert_eq!(detect_language("script.py"), Some("python".to_string()));
        assert_eq!(detect_language("unknown.xyz"), None);
    }
}
