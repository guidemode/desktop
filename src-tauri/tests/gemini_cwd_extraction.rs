use guidemode_desktop::providers::gemini::utils::{
    extract_candidate_paths_from_content, find_matching_path, infer_cwd_from_session, verify_hash,
};
use guidemode_desktop::providers::gemini::parser::{GeminiMessage, GeminiSession, Thought, ToolCall};
use serde_json::json;
use std::collections::HashMap;

#[test]
fn test_extract_candidate_paths_from_content() {
    let content = r#"
--- /Users/cliftonc/work/guidemode/CLAUDE.md ---
Some content here
--- /Users/cliftonc/work/guidemode/apps/desktop/CLAUDE.md ---
More content
"#;

    let paths = extract_candidate_paths_from_content(content);
    assert_eq!(paths.len(), 2);
    assert!(paths.contains(&"/Users/cliftonc/work/guidemode/CLAUDE.md".to_string()));
    assert!(paths.contains(&"/Users/cliftonc/work/guidemode/apps/desktop/CLAUDE.md".to_string()));
}

#[test]
fn test_extract_candidate_paths_no_delimiter() {
    let content = "Reading file /home/user/projects/myapp/src/main.rs";

    let paths = extract_candidate_paths_from_content(content);
    assert_eq!(paths.len(), 1);
    assert_eq!(paths[0], "/home/user/projects/myapp/src/main.rs");
}

#[test]
fn test_extract_candidate_paths_multiple_per_line() {
    let content = "Comparing /Users/test/work/app/file1.txt and /Users/test/work/app/file2.txt";

    let paths = extract_candidate_paths_from_content(content);
    assert_eq!(paths.len(), 2);
}

#[test]
fn test_find_matching_path_exact_match() {
    // Hash for "/Users/cliftonc/work/guidemode"
    let expected_hash = "277996b93ab2729878c409f6bbc1aa9fd3e741575b334969086150a208f5e277";
    let full_path = "/Users/cliftonc/work/guidemode/CLAUDE.md";

    let result = find_matching_path(full_path, expected_hash);
    assert_eq!(result, Some("/Users/cliftonc/work/guidemode".to_string()));
}

#[test]
fn test_find_matching_path_nested_file() {
    // Hash for "/Users/cliftonc/work/guidemode"
    let expected_hash = "277996b93ab2729878c409f6bbc1aa9fd3e741575b334969086150a208f5e277";
    let full_path = "/Users/cliftonc/work/guidemode/apps/desktop/src/main.rs";

    let result = find_matching_path(full_path, expected_hash);
    assert_eq!(result, Some("/Users/cliftonc/work/guidemode".to_string()));
}

#[test]
fn test_find_matching_path_no_match() {
    // Random hash that won't match
    let expected_hash = "0000000000000000000000000000000000000000000000000000000000000000";
    let full_path = "/Users/test/project/file.txt";

    let result = find_matching_path(full_path, expected_hash);
    assert_eq!(result, None);
}

#[test]
fn test_verify_hash() {
    // Known hash for "/Users/cliftonc/work/guidemode"
    let workdir = "/Users/cliftonc/work/guidemode";
    let expected_hash = "277996b93ab2729878c409f6bbc1aa9fd3e741575b334969086150a208f5e277";

    assert!(verify_hash(workdir, expected_hash));
}

#[test]
fn test_verify_hash_mismatch() {
    let workdir = "/Users/cliftonc/work/guidemode";
    let wrong_hash = "0000000000000000000000000000000000000000000000000000000000000000";

    assert!(!verify_hash(workdir, wrong_hash));
}

#[test]
fn test_infer_cwd_from_session_with_thoughts() {
    // This test validates CWD extraction from Extended Thinking (thoughts field)
    // Simulates a real Gemini Code session where file paths appear in thoughts
    let project_hash = "277996b93ab2729878c409f6bbc1aa9fd3e741575b334969086150a208f5e277";

    let session = GeminiSession {
        session_id: "test-session-123".to_string(),
        project_hash: project_hash.to_string(),
        start_time: "2025-10-19T03:18:28.380Z".to_string(),
        last_updated: "2025-10-19T03:21:52.867Z".to_string(),
        messages: vec![
            // User message with no paths
            GeminiMessage {
                id: "msg-1".to_string(),
                timestamp: "2025-10-19T03:18:28.380Z".to_string(),
                message_type: "user".to_string(),
                content: "Review the rust code in apps/desktop".to_string(),
                tool_calls: None,
                thoughts: None,
                tokens: None,
                model: None,
            },
            // Gemini message with Extended Thinking containing file paths
            GeminiMessage {
                id: "msg-2".to_string(),
                timestamp: "2025-10-19T03:21:52.867Z".to_string(),
                message_type: "gemini".to_string(),
                content: "I will review the code.".to_string(),
                tool_calls: None,
                thoughts: Some(vec![
                    Thought {
                        subject: "Locating the Code".to_string(),
                        description: "I've pinpointed the relevant Rust code within `apps/desktop/src-tauri`. My next step is to generate a file listing.".to_string(),
                        timestamp: "2025-10-19T03:18:31.077Z".to_string(),
                    },
                    Thought {
                        subject: "Analyzing Files".to_string(),
                        description: "I've located `file_watcher.rs` at /Users/cliftonc/work/guidemode/apps/desktop/src-tauri/src/file_watcher.rs, confirming my hypothesis.".to_string(),
                        timestamp: "2025-10-19T03:18:38.136Z".to_string(),
                    },
                ]),
                tokens: None,
                model: Some("gemini-2.5-pro".to_string()),
            },
        ],
    };

    let result = infer_cwd_from_session(&session, project_hash);
    assert_eq!(result, Some("/Users/cliftonc/work/guidemode".to_string()));
}

#[test]
fn test_infer_cwd_from_session_with_content() {
    // Test CWD extraction from message content (original behavior)
    let project_hash = "277996b93ab2729878c409f6bbc1aa9fd3e741575b334969086150a208f5e277";

    let session = GeminiSession {
        session_id: "test-session-456".to_string(),
        project_hash: project_hash.to_string(),
        start_time: "2025-10-19T00:00:00.000Z".to_string(),
        last_updated: "2025-10-19T00:05:00.000Z".to_string(),
        messages: vec![GeminiMessage {
            id: "msg-1".to_string(),
            timestamp: "2025-10-19T00:01:00.000Z".to_string(),
            message_type: "user".to_string(),
            content: "Reading file /Users/cliftonc/work/guidemode/CLAUDE.md".to_string(),
            tool_calls: None,
            thoughts: None,
            tokens: None,
            model: None,
        }],
    };

    let result = infer_cwd_from_session(&session, project_hash);
    assert_eq!(result, Some("/Users/cliftonc/work/guidemode".to_string()));
}

#[test]
fn test_infer_cwd_from_session_no_match() {
    // Test when no file paths match the hash
    let project_hash = "277996b93ab2729878c409f6bbc1aa9fd3e741575b334969086150a208f5e277";

    let session = GeminiSession {
        session_id: "test-session-789".to_string(),
        project_hash: project_hash.to_string(),
        start_time: "2025-10-19T00:00:00.000Z".to_string(),
        last_updated: "2025-10-19T00:05:00.000Z".to_string(),
        messages: vec![GeminiMessage {
            id: "msg-1".to_string(),
            timestamp: "2025-10-19T00:01:00.000Z".to_string(),
            message_type: "user".to_string(),
            content: "No file paths here!".to_string(),
            tool_calls: None,
            thoughts: None,
            tokens: None,
            model: None,
        }],
    };

    let result = infer_cwd_from_session(&session, project_hash);
    assert_eq!(result, None);
}

#[test]
fn test_infer_cwd_from_tool_calls() {
    // Test CWD extraction from tool call arguments (Priority 1 - most reliable)
    let project_hash = "277996b93ab2729878c409f6bbc1aa9fd3e741575b334969086150a208f5e277";

    let tool_call = ToolCall {
        id: "read_file-1234567890".to_string(),
        name: "read_file".to_string(),
        args: Some(json!({
            "absolute_path": "/Users/cliftonc/work/guidemode/apps/desktop/CLAUDE.md"
        })),
        result: None,
        status: Some("success".to_string()),
        extra: HashMap::new(),
    };

    let session = GeminiSession {
        session_id: "test-session-tool".to_string(),
        project_hash: project_hash.to_string(),
        start_time: "2025-10-19T04:15:00.000Z".to_string(),
        last_updated: "2025-10-19T04:16:00.000Z".to_string(),
        messages: vec![GeminiMessage {
            id: "msg-1".to_string(),
            timestamp: "2025-10-19T04:15:00.000Z".to_string(),
            message_type: "gemini".to_string(),
            content: "I'll read the file".to_string(),
            tool_calls: Some(vec![tool_call]),
            thoughts: None,
            tokens: None,
            model: Some("gemini-2.5-pro".to_string()),
        }],
    };

    let result = infer_cwd_from_session(&session, project_hash);
    assert_eq!(result, Some("/Users/cliftonc/work/guidemode".to_string()));
}

#[test]
fn test_infer_cwd_from_tool_calls_paths_array() {
    // Test CWD extraction from tool call with paths array (read_many_files)
    let project_hash = "277996b93ab2729878c409f6bbc1aa9fd3e741575b334969086150a208f5e277";

    let tool_call = ToolCall {
        id: "read_many-9876543210".to_string(),
        name: "read_many_files".to_string(),
        args: Some(json!({
            "paths": [
                "/Users/cliftonc/work/guidemode/package.json",
                "/Users/cliftonc/work/guidemode/apps/desktop/package.json"
            ]
        })),
        result: None,
        status: Some("success".to_string()),
        extra: HashMap::new(),
    };

    let session = GeminiSession {
        session_id: "test-session-many".to_string(),
        project_hash: project_hash.to_string(),
        start_time: "2025-10-19T04:20:00.000Z".to_string(),
        last_updated: "2025-10-19T04:21:00.000Z".to_string(),
        messages: vec![GeminiMessage {
            id: "msg-2".to_string(),
            timestamp: "2025-10-19T04:20:00.000Z".to_string(),
            message_type: "gemini".to_string(),
            content: "Reading multiple files".to_string(),
            tool_calls: Some(vec![tool_call]),
            thoughts: None,
            tokens: None,
            model: Some("gemini-2.5-pro".to_string()),
        }],
    };

    let result = infer_cwd_from_session(&session, project_hash);
    assert_eq!(result, Some("/Users/cliftonc/work/guidemode".to_string()));
}

#[test]
fn test_cwd_priority_tool_calls_over_thoughts() {
    // Test that tool calls have priority over thoughts when both exist
    let project_hash = "277996b93ab2729878c409f6bbc1aa9fd3e741575b334969086150a208f5e277";

    // Tool call with correct path
    let tool_call = ToolCall {
        id: "tool-123".to_string(),
        name: "read_file".to_string(),
        args: Some(json!({
            "absolute_path": "/Users/cliftonc/work/guidemode/CLAUDE.md"
        })),
        result: None,
        status: Some("success".to_string()),
        extra: HashMap::new(),
    };

    let session = GeminiSession {
        session_id: "test-priority".to_string(),
        project_hash: project_hash.to_string(),
        start_time: "2025-10-19T05:00:00.000Z".to_string(),
        last_updated: "2025-10-19T05:01:00.000Z".to_string(),
        messages: vec![GeminiMessage {
            id: "msg-priority".to_string(),
            timestamp: "2025-10-19T05:00:30.000Z".to_string(),
            message_type: "gemini".to_string(),
            content: "Some unrelated path /wrong/path/file.txt".to_string(),
            tool_calls: Some(vec![tool_call]),
            thoughts: Some(vec![Thought {
                subject: "Reading".to_string(),
                description: "Another wrong path /another/wrong/path.md".to_string(),
                timestamp: "2025-10-19T05:00:29.000Z".to_string(),
            }]),
            tokens: None,
            model: Some("gemini-2.5-pro".to_string()),
        }],
    };

    let result = infer_cwd_from_session(&session, project_hash);
    // Should find the correct path from tool call, not from thoughts or content
    assert_eq!(result, Some("/Users/cliftonc/work/guidemode".to_string()));
}
