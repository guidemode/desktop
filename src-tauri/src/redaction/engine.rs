//! Redaction engine: pattern matching and JSONL traversal.

use lazy_static::lazy_static;
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
struct PatternDef {
    name: String,
    pattern: String,
    #[serde(rename = "caseInsensitive")]
    case_insensitive: bool,
    multiline: bool,
    #[serde(rename = "captureGroup")]
    capture_group: Option<usize>,
    replacement: Option<String>,
}

struct CompiledPattern {
    regex: Regex,
    name: String,
    capture_group: Option<usize>,
    replacement: Option<String>,
}

lazy_static! {
    static ref PATTERNS: Vec<CompiledPattern> = {
        let json = include_str!(
            "../../../../../packages/session-processing/src/redaction/patterns.json"
        );
        let defs: Vec<PatternDef> = serde_json::from_str(json).expect("valid patterns.json");
        defs.into_iter()
            .filter_map(|d| {
                let mut pat = String::new();
                if d.case_insensitive {
                    pat.push_str("(?i)");
                }
                if d.multiline {
                    pat.push_str("(?m)");
                }
                pat.push_str(&d.pattern);
                match Regex::new(&pat) {
                    Ok(regex) => Some(CompiledPattern {
                        regex,
                        name: d.name,
                        capture_group: d.capture_group,
                        replacement: d.replacement,
                    }),
                    Err(e) => {
                        eprintln!("Warning: failed to compile redaction pattern '{}': {}", d.name, e);
                        None
                    }
                }
            })
            .collect()
    };
}

/// Statistics about redactions applied.
pub struct RedactionStats {
    pub total_redactions: u32,
    pub by_category: HashMap<String, u32>,
    pub lines_processed: u32,
    pub lines_with_redactions: u32,
    pub malformed_lines: u32,
}

/// Result of redacting JSONL content.
pub struct RedactedContent {
    pub content: String,
    pub stats: RedactionStats,
}

#[derive(Clone)]
struct RedactionMatch {
    start: usize,
    end: usize,
    category: String,
}

fn detect_patterns(text: &str) -> Vec<RedactionMatch> {
    let mut matches = Vec::new();

    for pattern in PATTERNS.iter() {
        for cap in pattern.regex.captures_iter(text) {
            if let Some(group_idx) = pattern.capture_group {
                if let Some(group_match) = cap.get(group_idx) {
                    matches.push(RedactionMatch {
                        start: group_match.start(),
                        end: group_match.end(),
                        category: pattern.name.clone(),
                    });
                }
            } else if let Some(full_match) = cap.get(0) {
                matches.push(RedactionMatch {
                    start: full_match.start(),
                    end: full_match.end(),
                    category: pattern.name.clone(),
                });
            }
        }
    }

    matches
}

fn merge_and_dedup(matches: &mut Vec<RedactionMatch>) -> Vec<RedactionMatch> {
    if matches.is_empty() {
        return Vec::new();
    }
    matches.sort_by_key(|m| m.start);

    let mut result: Vec<RedactionMatch> = vec![matches[0].clone()];
    for m in matches.iter().skip(1) {
        let last = result.last_mut().unwrap();
        if m.start < last.end {
            if m.end > last.end {
                last.end = m.end;
            }
        } else {
            result.push(m.clone());
        }
    }
    result
}

fn apply_replacements(text: &str, matches: &[RedactionMatch]) -> (String, HashMap<String, u32>) {
    if matches.is_empty() {
        return (text.to_string(), HashMap::new());
    }

    let mut counts: HashMap<String, u32> = HashMap::new();
    // Sort descending by start position
    let mut sorted: Vec<&RedactionMatch> = matches.iter().collect();
    sorted.sort_by(|a, b| b.start.cmp(&a.start));

    let mut result = text.to_string();
    for m in &sorted {
        let replacement = if m.category == "HOME_DIR" {
            "~".to_string()
        } else {
            format!("[REDACTED:{}]", m.category)
        };
        result.replace_range(m.start..m.end, &replacement);
        *counts.entry(m.category.clone()).or_insert(0) += 1;
    }
    (result, counts)
}

fn redact_text(text: &str) -> (String, HashMap<String, u32>) {
    if text.is_empty() {
        return (text.to_string(), HashMap::new());
    }
    let mut matches = detect_patterns(text);
    let deduped = merge_and_dedup(&mut matches);
    apply_replacements(text, &deduped)
}

fn merge_counts(target: &mut HashMap<String, u32>, source: &HashMap<String, u32>) {
    for (k, v) in source {
        *target.entry(k.clone()).or_insert(0) += v;
    }
}

fn redact_json_value(value: &mut serde_json::Value) -> HashMap<String, u32> {
    let mut counts = HashMap::new();

    match value {
        serde_json::Value::String(s) => {
            let (redacted, c) = redact_text(s);
            *s = redacted;
            merge_counts(&mut counts, &c);
        }
        serde_json::Value::Array(arr) => {
            for item in arr.iter_mut() {
                let c = redact_json_value(item);
                merge_counts(&mut counts, &c);
            }
        }
        serde_json::Value::Object(map) => {
            for (_key, val) in map.iter_mut() {
                let c = redact_json_value(val);
                merge_counts(&mut counts, &c);
            }
        }
        _ => {}
    }

    counts
}

fn redact_content_block(block: &mut serde_json::Value) -> HashMap<String, u32> {
    let mut all_counts = HashMap::new();

    // Redact text and thinking fields
    for field in &["text", "thinking"] {
        if let Some(serde_json::Value::String(s)) = block.get(*field) {
            let (redacted, c) = redact_text(s);
            block[*field] = serde_json::Value::String(redacted);
            merge_counts(&mut all_counts, &c);
        }
    }

    // Redact tool_result content
    if block.get("type").and_then(|t| t.as_str()) == Some("tool_result") {
        if let Some(content) = block.get("content") {
            if content.is_string() {
                let s = content.as_str().unwrap();
                let (redacted, c) = redact_text(s);
                block["content"] = serde_json::Value::String(redacted);
                merge_counts(&mut all_counts, &c);
            } else if content.is_array() {
                let mut content_clone = content.clone();
                if let serde_json::Value::Array(ref mut arr) = content_clone {
                    for sub in arr.iter_mut() {
                        let c = redact_content_block(sub);
                        merge_counts(&mut all_counts, &c);
                    }
                }
                block["content"] = content_clone;
            }
        }
    }

    // Redact tool_use input values
    if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
        if let Some(input) = block.get("input") {
            if input.is_object() {
                let mut input_clone = input.clone();
                let c = redact_json_value(&mut input_clone);
                merge_counts(&mut all_counts, &c);
                block["input"] = input_clone;
            }
        }
    }

    all_counts
}

fn redact_canonical_message(
    json_line: &str,
    home_dir: &str,
) -> Result<(String, HashMap<String, u32>), String> {
    let mut all_counts: HashMap<String, u32> = HashMap::new();

    let mut parsed: serde_json::Value =
        serde_json::from_str(json_line).map_err(|e| format!("JSON parse error: {}", e))?;

    // Redact cwd field
    if !home_dir.is_empty() {
        if let Some(serde_json::Value::String(cwd)) = parsed.get("cwd") {
            if cwd.contains(home_dir) {
                let redacted = cwd.replace(home_dir, "~");
                parsed["cwd"] = serde_json::Value::String(redacted);
                *all_counts.entry("HOME_DIR".to_string()).or_insert(0) += 1;
            }
        }
    }

    // Redact message.content
    if let Some(message) = parsed.get_mut("message") {
        if let Some(content) = message.get("content") {
            if content.is_string() {
                let s = content.as_str().unwrap();
                let (redacted, c) = redact_text(s);
                message["content"] = serde_json::Value::String(redacted);
                merge_counts(&mut all_counts, &c);
            } else if content.is_array() {
                let mut content_clone = content.clone();
                if let serde_json::Value::Array(ref mut arr) = content_clone {
                    for block in arr.iter_mut() {
                        let c = redact_content_block(block);
                        merge_counts(&mut all_counts, &c);
                    }
                }
                message["content"] = content_clone;
            }
        }
    }

    let result =
        serde_json::to_string(&parsed).map_err(|e| format!("JSON serialize error: {}", e))?;
    Ok((result, all_counts))
}

/// Redact all secrets and PII from JSONL content.
pub fn redact_jsonl_content(content: &str, home_dir: &str) -> Result<RedactedContent, String> {
    let lines: Vec<&str> = content.split('\n').collect();

    let mut stats = RedactionStats {
        total_redactions: 0,
        by_category: HashMap::new(),
        lines_processed: 0,
        lines_with_redactions: 0,
        malformed_lines: 0,
    };

    let mut result_lines: Vec<String> = Vec::with_capacity(lines.len());

    for line in &lines {
        if line.trim().is_empty() {
            result_lines.push(line.to_string());
            continue;
        }

        stats.lines_processed += 1;

        // Quick JSON validation
        if serde_json::from_str::<serde_json::Value>(line).is_err() {
            stats.malformed_lines += 1;
            result_lines.push(line.to_string());
            continue;
        }

        match redact_canonical_message(line, home_dir) {
            Ok((redacted_line, counts)) => {
                result_lines.push(redacted_line);
                let line_total: u32 = counts.values().sum();
                if line_total > 0 {
                    stats.lines_with_redactions += 1;
                    stats.total_redactions += line_total;
                    merge_counts(&mut stats.by_category, &counts);
                }
            }
            Err(_) => {
                stats.malformed_lines += 1;
                result_lines.push(line.to_string());
            }
        }
    }

    Ok(RedactedContent {
        content: result_lines.join("\n"),
        stats,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redact_empty_text() {
        let (text, counts) = redact_text("");
        assert_eq!(text, "");
        assert!(counts.is_empty());
    }

    #[test]
    fn test_redact_no_secrets() {
        let (text, counts) = redact_text("Hello world, this is a normal string");
        assert_eq!(text, "Hello world, this is a normal string");
        assert!(counts.is_empty());
    }

    #[test]
    fn test_redact_github_token() {
        let token = "ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef1234";
        let input = format!("token: {}", token);
        let (text, counts) = redact_text(&input);
        assert!(!text.contains(token));
        assert!(text.contains("[REDACTED:"));
        assert!(counts.values().sum::<u32>() > 0);
    }

    #[test]
    fn test_redact_anthropic_key() {
        let body = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefgh";
        let key = format!("sk-ant-api03-{}AA", body);
        let input = format!("ANTHROPIC_API_KEY={}", key);
        let (text, counts) = redact_text(&input);
        assert!(!text.contains(&key));
        assert!(text.contains("[REDACTED:"));
        assert!(counts.values().sum::<u32>() > 0);
    }

    #[test]
    fn test_redact_email() {
        let (text, counts) = redact_text("Contact me at john.doe@company.com for details");
        assert!(!text.contains("john.doe@company.com"));
        assert!(counts.values().sum::<u32>() > 0);
    }

    #[test]
    fn test_redact_home_dir() {
        let (text, counts) = redact_text("Working in /Users/johndoe/projects/myapp");
        assert!(!text.contains("/Users/johndoe"));
        assert!(text.contains("~"));
        assert!(counts.get("HOME_DIR").copied().unwrap_or(0) > 0);
    }

    #[test]
    fn test_redact_home_dir_linux() {
        let (text, counts) = redact_text("Path: /home/developer/workspace");
        assert!(!text.contains("/home/developer"));
        assert!(text.contains("~"));
        assert!(counts.get("HOME_DIR").copied().unwrap_or(0) > 0);
    }

    #[test]
    fn test_redact_guidemode_api_key() {
        let key = "gai_07c7aa28646e6d832c28ec611c1062e2b6731904eb30de4db73135";
        let input = format!("apiKey: {}", key);
        let (text, counts) = redact_text(&input);
        assert!(!text.contains(key));
        assert!(text.contains("[REDACTED:API_KEY]"));
        assert!(counts.get("API_KEY").copied().unwrap_or(0) > 0);
    }

    #[test]
    fn test_redact_postgresql_url() {
        let (text, counts) =
            redact_text("DATABASE_URL=postgresql://user:password123@db.example.com:5432/mydb");
        assert!(text.contains("[REDACTED:"));
        assert!(counts.values().sum::<u32>() > 0);
    }

    #[test]
    fn test_no_false_positives() {
        let snippets = vec![
            "import { loadConfig } from \"./config.js\"",
            "npm install -g guidemode",
            "const x = 42; function hello() { return \"world\"; }",
            "Command line interface for GuideMode app",
            "git commit -m \"fix: update redaction logic\"",
        ];
        for snippet in snippets {
            let (text, _) = redact_text(snippet);
            assert_eq!(text, snippet, "False positive for: {}", snippet);
        }
    }

    #[test]
    fn test_redact_canonical_message_string_content() {
        let token = "ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef1234";
        let input = serde_json::json!({
            "type": "message",
            "message": {
                "role": "user",
                "content": format!("My token is {}", token)
            }
        })
        .to_string();

        let (line, counts) = redact_canonical_message(&input, "").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&line).unwrap();
        let content = parsed["message"]["content"].as_str().unwrap();
        assert!(!content.contains(token));
        assert!(content.contains("[REDACTED:"));
        assert!(counts.values().sum::<u32>() > 0);
    }

    #[test]
    fn test_redact_canonical_message_cwd() {
        let input = serde_json::json!({
            "type": "message",
            "cwd": "/Users/clifton/projects/myapp",
            "message": { "role": "user", "content": "hello" }
        })
        .to_string();

        let (line, counts) = redact_canonical_message(&input, "/Users/clifton").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(parsed["cwd"].as_str().unwrap(), "~/projects/myapp");
        assert!(counts.get("HOME_DIR").copied().unwrap_or(0) > 0);
    }

    #[test]
    fn test_redact_jsonl_content() {
        let token = "ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef1234";
        let lines = vec![
            serde_json::json!({"type":"message","message":{"role":"user","content":format!("My token {}", token)}}).to_string(),
            serde_json::json!({"type":"message","message":{"role":"assistant","content":"I see you shared a token"}}).to_string(),
            serde_json::json!({"type":"message","message":{"role":"user","content":"email: test@secret.com"}}).to_string(),
        ];
        let content = lines.join("\n");

        let result = redact_jsonl_content(&content, "").unwrap();
        assert_eq!(result.stats.lines_processed, 3);
        assert!(result.stats.total_redactions > 0);
        assert!(result.stats.lines_with_redactions > 0);

        // Each line should still be valid JSON
        for line in result.content.split('\n') {
            if !line.trim().is_empty() {
                assert!(serde_json::from_str::<serde_json::Value>(line).is_ok());
            }
        }
    }

    #[test]
    fn test_redact_jsonl_empty_lines() {
        let lines = vec![
            serde_json::json!({"type":"system"}).to_string(),
            String::new(),
            serde_json::json!({"type":"message","message":{"role":"user","content":"hello"}}).to_string(),
        ];
        let content = lines.join("\n");

        let result = redact_jsonl_content(&content, "").unwrap();
        let result_lines: Vec<&str> = result.content.split('\n').collect();
        assert_eq!(result_lines.len(), 3);
        assert_eq!(result_lines[1], "");
        assert_eq!(result.stats.lines_processed, 2);
    }

    #[test]
    fn test_redact_jsonl_malformed_lines() {
        let lines = vec![
            serde_json::json!({"type":"message","message":{"role":"user","content":"hello"}}).to_string(),
            "this is not json {{{".to_string(),
            serde_json::json!({"type":"message","message":{"role":"assistant","content":"hi"}}).to_string(),
        ];
        let content = lines.join("\n");

        let result = redact_jsonl_content(&content, "").unwrap();
        let result_lines: Vec<&str> = result.content.split('\n').collect();
        assert_eq!(result_lines[1], "this is not json {{{");
        assert_eq!(result.stats.malformed_lines, 1);
        assert_eq!(result.stats.lines_processed, 3);
    }

    #[test]
    fn test_redact_tool_use_input() {
        let token = "ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef1234";
        let input = serde_json::json!({
            "type": "message",
            "message": {
                "role": "assistant",
                "content": [{
                    "type": "tool_use",
                    "id": "call-456",
                    "name": "write_file",
                    "input": {
                        "path": "/tmp/config.env",
                        "content": format!("API_KEY={}", token)
                    }
                }]
            }
        })
        .to_string();

        let (line, counts) = redact_canonical_message(&input, "").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&line).unwrap();
        let tool_content = parsed["message"]["content"][0]["input"]["content"]
            .as_str()
            .unwrap();
        assert!(tool_content.contains("[REDACTED:"));
        assert_eq!(
            parsed["message"]["content"][0]["id"].as_str().unwrap(),
            "call-456"
        );
        assert!(counts.values().sum::<u32>() > 0);
    }

    #[test]
    fn test_redact_tool_result_content() {
        let input = serde_json::json!({
            "type": "message",
            "message": {
                "role": "tool",
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": "call-123",
                    "content": "DATABASE_URL=postgresql://admin:secret@db.example.com:5432/myapp"
                }]
            }
        })
        .to_string();

        let (line, counts) = redact_canonical_message(&input, "").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&line).unwrap();
        let tool_content = parsed["message"]["content"][0]["content"]
            .as_str()
            .unwrap();
        assert!(tool_content.contains("[REDACTED:"));
        assert_eq!(
            parsed["message"]["content"][0]["tool_use_id"]
                .as_str()
                .unwrap(),
            "call-123"
        );
        assert!(counts.values().sum::<u32>() > 0);
    }

    #[test]
    fn test_redact_private_key() {
        let key_content = "-----BEGIN RSA PRIVATE KEY-----\nMIIEpAIBAAKCAQEA0Z3VS5JJcds3xfn\n-----END RSA PRIVATE KEY-----";
        let (text, counts) = redact_text(key_content);
        assert!(text.contains("[REDACTED:"));
        assert!(counts.values().sum::<u32>() > 0);
    }

    #[test]
    fn test_redact_secret_token() {
        let token = "abcdef0123456789abcdef0123456789abcdef01";
        let input = format!("api_key={}", token);
        let (text, counts) = redact_text(&input);
        assert!(!text.contains(token));
        assert!(text.contains("[REDACTED:"));
        assert!(counts.values().sum::<u32>() > 0);
    }

    #[test]
    fn test_redact_multiple_secrets() {
        let gh_token = "ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef1234";
        let input = format!("GitHub: {}, email: admin@secret-corp.com", gh_token);
        let (text, counts) = redact_text(&input);
        assert!(!text.contains(gh_token));
        assert!(!text.contains("admin@secret-corp.com"));
        assert!(counts.values().sum::<u32>() >= 2);
    }

    #[test]
    fn test_redact_aws_secret_key() {
        let (text, counts) =
            redact_text("AWS_SECRET_ACCESS_KEY=\"abcdef1234567890abcdef1234567890abcDEFGH\"");
        assert!(text.contains("[REDACTED:"));
        assert!(counts.values().sum::<u32>() > 0);
    }

    #[test]
    fn test_redact_thinking_block() {
        let token = "ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef1234";
        let input = serde_json::json!({
            "type": "message",
            "message": {
                "role": "assistant",
                "content": [{
                    "type": "thinking",
                    "thinking": format!("I see the token {} in the config", token)
                }]
            }
        })
        .to_string();

        let (line, counts) = redact_canonical_message(&input, "").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&line).unwrap();
        let thinking = parsed["message"]["content"][0]["thinking"]
            .as_str()
            .unwrap();
        assert!(!thinking.contains(token));
        assert!(counts.values().sum::<u32>() > 0);
    }
}
