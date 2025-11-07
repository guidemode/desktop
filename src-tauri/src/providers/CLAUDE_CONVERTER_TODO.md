# Claude Code Converter - Implementation TODO

## Overview

Claude Code logs are the **basis** for the canonical format, but they still need normalization before they're valid canonical JSONL. Currently, native Claude Code files fail validation because they contain system events and lack the `provider` field.

## Current Status

**Validation Results** (as of 2025-11-06):
- ✅ **Codex**: 9 sessions, 1186 messages - **100% valid**
- ✅ **Gemini**: (assumed valid - uses converters)
- ✅ **OpenCode**: (assumed valid - uses converters)
- ✅ **GitHub Copilot**: (assumed valid - uses converters)
- ❌ **Claude Code**: **0% valid** (native logs, not converted)
- ❌ **Cursor**: **0% valid** (in progress)

## Why Claude Needs a Converter

### Issue 1: System Events

Native Claude logs contain non-conversational events:

```jsonl
{"type":"file-history-snapshot","messageId":"...","snapshot":{...},"isSnapshotUpdate":false}
{"type":"summary","uuid":"...","timestamp":"...","message":{...}}
{"type":"system","subtype":"compact_boundary","content":"Conversation compacted",...}
```

**These should be filtered out** - canonical format only includes:
- `type: "user"`
- `type: "assistant"`
- `type: "meta"`

### Issue 2: Missing `provider` Field

Native Claude logs don't have a `provider` field because they ARE the source format. But canonical JSONL requires:

```json
{
  "uuid": "...",
  "timestamp": "...",
  "type": "user",
  "sessionId": "...",
  "provider": "claude-code",  // ← REQUIRED, must be added
  "message": {...}
}
```

### Issue 3: Nullable Fields

Native Claude logs have `parentUuid: null` in some messages. These should be:
- Either removed entirely (if optional)
- Or converted to empty string (if needed for compatibility)

## Implementation Plan

### 1. Create Claude Converter Module

**Location**: `apps/desktop/src-tauri/src/providers/claude/`

```
claude/
├── mod.rs           # Module exports
├── scanner.rs       # File watcher for ~/.claude/projects/
├── converter.rs     # ToCanonical implementation
└── types.rs         # Claude-specific types
```

### 2. Converter Logic

```rust
// src/providers/claude/converter.rs
use crate::providers::canonical::{CanonicalMessage, ToCanonical};

pub struct ClaudeEntry {
    // Parse native Claude JSONL format
}

impl ToCanonical for ClaudeEntry {
    fn to_canonical(&self) -> Result<Option<CanonicalMessage>> {
        // 1. Filter: Skip system events
        match self.entry_type.as_str() {
            "file-history-snapshot" => return Ok(None),
            "summary" => return Ok(None),
            "system" if self.subtype == Some("compact_boundary") => return Ok(None),
            "system" if self.subtype == Some("informational") => return Ok(None),
            _ => {}
        }

        // 2. Only allow conversational types
        let message_type = match self.entry_type.as_str() {
            "user" => MessageType::User,
            "assistant" => MessageType::Assistant,
            "meta" | "system" => MessageType::Meta,
            _ => return Ok(None), // Skip unknown types
        };

        // 3. Build canonical message
        Ok(Some(CanonicalMessage {
            uuid: self.uuid.clone(),
            timestamp: self.timestamp.clone(),
            type: message_type,
            session_id: self.session_id.clone(),
            provider: "claude-code".to_string(), // ← ADD THIS
            cwd: self.cwd.clone(),
            git_branch: self.git_branch.clone(),
            version: self.version.clone(),
            parent_uuid: self.parent_uuid.clone().filter(|s| !s.is_empty()),
            is_sidechain: self.is_sidechain,
            user_type: self.user_type.clone(),
            message: self.message.clone(),
            provider_metadata: None, // Claude IS canonical, no metadata needed
            is_meta: self.is_meta,
            request_id: self.request_id.clone(),
            tool_use_result: None,
        }))
    }

    fn provider_name(&self) -> &str {
        "claude-code"
    }
}
```

### 3. Scanner Implementation

**Watch**: `~/.claude/projects/**/session.jsonl`

```rust
// src/providers/claude/scanner.rs
pub fn scan_claude_sessions() -> Result<Vec<CanonicalMessage>> {
    let claude_dir = shellexpand::tilde("~/.claude/projects");

    // Find all session.jsonl files
    let sessions = find_session_files(&claude_dir)?;

    for session_path in sessions {
        // Read JSONL line by line
        let file = File::open(&session_path)?;
        let reader = BufReader::new(file);

        let mut canonical_messages = Vec::new();
        for line in reader.lines() {
            let line = line?;

            // Parse as Claude entry
            let entry: ClaudeEntry = serde_json::from_str(&line)?;

            // Convert to canonical (filters out system events)
            if let Some(canonical) = entry.to_canonical()? {
                canonical_messages.push(canonical);
            }
        }

        // Write to canonical path
        let canonical_path = get_canonical_path(&session_path)?;
        write_canonical_jsonl(&canonical_path, &canonical_messages)?;
    }

    Ok(())
}
```

### 4. Integration with Desktop App

**Modify**: `src/main.rs` to include Claude watcher

```rust
// Start all provider watchers
tokio::spawn(watch_claude_sessions(event_bus.clone(), shutdown.clone()));
tokio::spawn(watch_codex_sessions(event_bus.clone(), shutdown.clone()));
tokio::spawn(watch_gemini_sessions(event_bus.clone(), shutdown.clone()));
// ... other watchers
```

## Validation Testing

After implementation, validate with CLI:

```bash
# Test converted Claude sessions
pnpm cli validate ~/.guideai/sessions/claude-code/ --provider claude-code

# Should show 100% valid (like Codex)
# ✓ All validations passed!
```

## Success Criteria

- ✅ Claude sessions filter out system events
- ✅ All messages have `provider: "claude-code"`
- ✅ 100% validation pass rate (like Codex)
- ✅ No nullable `parentUuid` values
- ✅ Only `user`, `assistant`, `meta` message types

## Notes

- **Claude IS Canonical**: The structure stays the same (camelCase fields)
- **Minimal Changes**: Only filtering and adding `provider` field
- **No Metadata**: Claude doesn't need `providerMetadata` since it's the source format
- **System Events**: Useful for debugging but not part of canonical conversation flow

## Related Files

- Validation schema: `packages/types/src/canonical-validation.ts`
- Other converters: `apps/desktop/src-tauri/src/providers/{codex,gemini,opencode}/converter.rs`
- Validation guide: `apps/desktop/src-tauri/src/providers/VALIDATION_GUIDE.md`

---

**Created**: 2025-11-06
**Priority**: High (blocks Claude session validation)
