# Canonical JSONL Validation Guide

Comprehensive guide for validating canonical JSONL format during provider development and testing.

## Table of Contents

1. [Overview](#overview)
2. [Quick Start](#quick-start)
3. [Canonical Format Requirements](#canonical-format-requirements)
4. [Validation System Architecture](#validation-system-architecture)
5. [TDD Workflow for New Providers](#tdd-workflow-for-new-providers)
6. [Common Validation Errors](#common-validation-errors)
7. [CLI Validation Tool](#cli-validation-tool)
8. [Integration Testing](#integration-testing)

---

## Overview

The validation system ensures that all provider converters produce correct, consistent canonical JSONL output. This is critical for:

- **Metrics Processing**: Accurate analytics across all providers
- **Transcript Display**: Proper UI rendering of sessions
- **Data Integrity**: No data loss during provider-specific → canonical conversion
- **Developer Experience**: Fast feedback during provider development

### Key Benefits

✅ **Tiered Validation**: Critical errors vs warnings
✅ **Semantic Checks**: Tool chain matching, timestamp ordering
✅ **CLI Integration**: Easy testing during development
✅ **Type-Safe**: Zod schemas with TypeScript types
✅ **Developer-Friendly**: Clear error messages with line numbers

---

## Quick Start

### Install Dependencies

The validation system is built into the workspace. Make sure you have built the packages:

```bash
# Build types and session-processing packages
pnpm --filter @guideai-dev/types build
pnpm --filter @guideai-dev/session-processing build
pnpm --filter @guideai-dev/cli build
```

### Validate a Single File

```bash
# Validate a canonical JSONL file
pnpm cli validate ~/.guideai/sessions/cursor/my-project/session-123.jsonl
```

### Validate All Sessions for a Provider

```bash
# Validate all Cursor sessions
pnpm cli validate ~/.guideai/sessions/cursor/ --provider cursor
```

### Watch Mode (for TDD)

```bash
# Watch and re-validate on file changes (coming soon)
pnpm cli validate ~/.guideai/sessions/cursor/ --watch
```

---

## Canonical Format Requirements

### Required Fields

Every canonical message MUST have:

| Field | Type | Description |
|-------|------|-------------|
| `uuid` | string | Unique message identifier (non-empty) |
| `timestamp` | string | RFC3339/ISO 8601 timestamp |
| `type` | enum | `"user"`, `"assistant"`, or `"meta"` |
| `sessionId` | string | Session identifier (non-empty) |
| `provider` | string | Provider name (non-empty) |
| `message` | object | Message content object |

### Message Content Structure

```typescript
{
  role: string,              // "user" or "assistant"
  content: string | ContentBlock[],  // Text or structured blocks
  model?: string,            // Optional model name
  usage?: {                  // Optional token usage
    input_tokens?: number,
    output_tokens?: number,
    cache_creation_input_tokens?: number,
    cache_read_input_tokens?: number
  }
}
```

### Content Block Types

Four canonical content block types:

#### 1. Text Block

```json
{
  "type": "text",
  "text": "The actual text content"
}
```

#### 2. Thinking Block

```json
{
  "type": "thinking",
  "thinking": "Internal reasoning process",
  "signature": "optional_encrypted_signature"
}
```

#### 3. Tool Use Block

```json
{
  "type": "tool_use",
  "id": "call-123",           // MUST be non-empty
  "name": "tool_name",        // MUST be non-empty
  "input": {                   // Any JSON value
    "param1": "value1"
  }
}
```

#### 4. Tool Result Block

```json
{
  "type": "tool_result",
  "tool_use_id": "call-123",  // MUST be non-empty
  "content": "result data",    // MUST be non-empty
  "is_error": false            // Optional boolean
}
```

### Critical Rules

#### Rule 1: Tool Result Message Types

**CRITICAL**: `tool_result` blocks MUST be in user messages:

```typescript
// ❌ WRONG - tool_result in assistant message
{
  "type": "assistant",
  "message": {
    "role": "assistant",
    "content": [
      { "type": "tool_result", ...}  // ERROR!
    ]
  }
}

// ✅ CORRECT - tool_result in user message
{
  "type": "user",
  "message": {
    "role": "user",
    "content": [
      { "type": "tool_result", "tool_use_id": "call-123", "content": "output" }
    ]
  }
}
```

#### Rule 2: Tool Chain Matching

**IMPORTANT**: Tool use/result relationship is asymmetric:

✅ **Valid**: `tool_use` without `tool_result` (waiting for result)
❌ **Invalid**: `tool_result` without corresponding `tool_use` (orphan)

```typescript
// ✅ VALID: Tool use waiting for result
messages: [
  {
    type: "assistant",
    message: {
      content: [
        { type: "tool_use", id: "call-1", name: "Read", input: {...} }
      ]
    }
  }
  // No tool_result yet - this is fine!
]

// ❌ INVALID: Orphan tool result
messages: [
  {
    type: "user",
    message: {
      content: [
        { type: "tool_result", tool_use_id: "call-999", content: "..." }
        // ERROR: call-999 never had a tool_use!
      ]
    }
  }
]
```

#### Rule 3: Timestamps

- **Format**: Must be valid RFC3339/ISO 8601
- **Ordering**: Should be monotonically increasing (warning if not)
- **Reasonable Range**: Not > 1 day in future, not > 5 years in past (warning)

#### Rule 4: Content Integrity

- `tool_use.id` must be non-empty
- `tool_use.name` must be non-empty
- `tool_result.tool_use_id` must be non-empty
- `tool_result.content` must be non-empty
- `tool_use.input` must be valid JSON

---

## Validation System Architecture

### Component Stack

```
┌─────────────────────────────────────────────────┐
│  CLI Validate Command (packages/cli)           │
│  • File I/O                                     │
│  • Directory scanning                           │
│  • Human-readable reports                       │
└──────────────────┬──────────────────────────────┘
                   │
┌──────────────────▼──────────────────────────────┐
│  Validation Library (session-processing)        │
│  • validateJSONL()                              │
│  • generateValidationReport()                   │
│  • Session-wide validation                      │
└──────────────────┬──────────────────────────────┘
                   │
┌──────────────────▼──────────────────────────────┐
│  Zod Schemas & Validators (types)               │
│  • CanonicalMessageSchema                       │
│  • validateCanonicalMessage()                   │
│  • validateToolChain()                          │
│  • validateTimestampOrdering()                  │
└─────────────────────────────────────────────────┘
```

### Validation Levels

#### Level 1: Schema Validation (Critical)

Uses Zod to validate:
- Field presence and types
- Enum values
- Required vs optional fields

**Errors**: Schema violations, missing required fields

#### Level 2: Semantic Validation (Critical)

Validates business logic:
- Message type/role consistency
- Tool result message types
- Timestamp format
- Content block correctness

**Errors**: Orphan tool results, wrong message types, empty required fields

#### Level 3: Session Validation (Warning/Error)

Cross-message checks:
- Tool use/result matching
- Timestamp ordering
- Message threading (parentUuid)

**Errors**: Orphan tool results
**Warnings**: Out-of-order timestamps, large time gaps

#### Level 4: Content Integrity (Warning)

Data quality checks:
- Duplicate content in metadata
- Large metadata payloads
- Unusual patterns

**Warnings**: Potential data duplication, inefficient storage

---

## TDD Workflow for New Providers

### Step 1: Setup Test Environment

```bash
# Create test fixtures directory
mkdir -p ~/.guideai/sessions/new-provider/test-project/

# Your converter will write here during development
```

### Step 2: Write Converter with Validation

```rust
// src/providers/new_provider/converter.rs
use crate::providers::canonical::{CanonicalMessage, ToCanonical};

impl ToCanonical for NewProviderMessage {
    fn to_canonical(&self) -> Result<Option<CanonicalMessage>> {
        // Build canonical message
        let canonical_msg = CanonicalMessage {
            uuid: self.id.clone(),
            timestamp: self.timestamp_rfc3339(),
            type: MessageType::User,
            session_id: self.session_id.clone(),
            provider: "new-provider".to_string(),
            message: MessageContent {
                role: "user".to_string(),
                content: ContentValue::Text(self.content.clone()),
                model: None,
                usage: None,
            },
            // ... other fields
        };

        // Validation happens externally via CLI
        Ok(Some(canonical_msg))
    }
}
```

### Step 3: Run Validation in Watch Mode

```bash
# Terminal 1: Run your converter/scanner
cargo run --bin scan_new_provider

# Terminal 2: Validate output continuously
pnpm cli validate ~/.guideai/sessions/new-provider/ --provider new-provider

# Or for specific session:
pnpm cli validate ~/.guideai/sessions/new-provider/test-project/session-123.jsonl --verbose
```

### Step 4: Fix Errors Iteratively

The CLI will show errors with line numbers:

```
=== Canonical JSONL Validation Report ===

Status: ✗ INVALID
Total Lines: 15
Parsed Lines: 15
Valid Messages: 12/15

Errors (3):
  ✗ Line 7: [EMPTY_TOOL_RESULT_CONTENT] tool_result block has empty content
  ✗ Line 9: [ORPHAN_TOOL_RESULT] tool_result references tool_use_id "call-999" which doesn't exist
  ✗ Line 12: [INVALID_TOOL_RESULT_MESSAGE_TYPE] tool_result blocks must be in user messages, found in assistant message

✗ Validation failed!
```

### Step 5: Iterate Until Clean

Goal: **Zero errors**, minimal warnings

```bash
# Success output:
=== Canonical JSONL Validation Report ===

Status: ✓ VALID
Total Lines: 15
Parsed Lines: 15
Valid Messages: 15/15

Session Info:
  Session ID: abc-123
  Provider: new-provider
  Messages: 15
  Duration: 42 minutes

✓ Validation passed!
```

### Step 6: Add to CI/CD

```yaml
# .github/workflows/validate-providers.yml
- name: Validate Canonical Output
  run: |
    pnpm cli validate ~/.guideai/sessions/ --strict --json > validation-report.json
    # Exit code 1 if any errors
```

---

## Common Validation Errors

### Error 1: Empty Tool Result Content

**Error Code**: `EMPTY_TOOL_RESULT_CONTENT`

**Cause**: Converter created `tool_result` block with empty content string

**Fix**:
```rust
// ❌ BEFORE
let content = tool_output.unwrap_or_default();  // Could be empty!
ContentBlock::ToolResult {
    tool_use_id: id,
    content,  // Empty!
    is_error: Some(false),
}

// ✅ AFTER
let content = tool_output.unwrap_or_default();
if content.is_empty() {
    return Err(anyhow::anyhow!(
        "Tool result has empty content for tool_use_id: {}", id
    ));
}
ContentBlock::ToolResult {
    tool_use_id: id,
    content,
    is_error: Some(false),
}
```

### Error 2: Wrong Message Type for Tool Result

**Error Code**: `INVALID_TOOL_RESULT_MESSAGE_TYPE`

**Cause**: `tool_result` block in assistant message instead of user message

**Fix**:
```rust
// ❌ BEFORE
CanonicalMessage {
    message_type: MessageType::Assistant,  // Wrong!
    message: MessageContent {
        role: "assistant".to_string(),
        content: ContentValue::Structured(vec![
            ContentBlock::ToolResult { ... }
        ])
    }
}

// ✅ AFTER
CanonicalMessage {
    message_type: MessageType::User,  // Correct
    message: MessageContent {
        role: "user".to_string(),
        content: ContentValue::Structured(vec![
            ContentBlock::ToolResult { ... }
        ])
    }
}
```

### Error 3: Orphan Tool Result

**Error Code**: `ORPHAN_TOOL_RESULT`

**Cause**: `tool_result` references a `tool_use_id` that doesn't exist

**Fix**:
```rust
// Check message ordering in your converter
// Ensure tool_use blocks are created BEFORE tool_result blocks

// Common issue: Wrong event ordering from provider logs
// Solution: Buffer events and sort by timestamp before converting
```

### Error 4: Invalid Timestamp Format

**Error Code**: `INVALID_TIMESTAMP_FORMAT`

**Cause**: Timestamp not in RFC3339 format

**Fix**:
```rust
// ❌ BEFORE
timestamp: self.timestamp.to_string()  // Unix timestamp!

// ✅ AFTER
use chrono::{DateTime, Utc};

timestamp: DateTime::<Utc>::from_timestamp(self.timestamp, 0)
    .ok_or_else(|| anyhow::anyhow!("Invalid timestamp"))?
    .to_rfc3339()
```

### Error 5: Duplicate Content in Metadata

**Warning Code**: `DUPLICATE_CONTENT_IN_METADATA`

**Cause**: Provider-specific content duplicated in `providerMetadata`

**Fix**:
```rust
// ❌ BEFORE - Duplicating content in metadata
provider_metadata: Some(serde_json::json!({
    "provider_type": "new-provider",
    "original_content": self.content.clone(),  // Duplicate!
    "thoughts": thinking_text.clone(),         // Duplicate!
}))

// ✅ AFTER - Only store flags/IDs in metadata
provider_metadata: Some(serde_json::json!({
    "provider_type": "new-provider",
    "has_thoughts": !thinking_text.is_empty(),
    "original_format_version": "1.0",
}))
```

---

## CLI Validation Tool

### Command Reference

```bash
# Basic usage
pnpm cli validate <path>

# Options
--strict          # Treat warnings as errors (exit code 1)
--json            # Output JSON format for CI/CD
--verbose         # Show detailed error information
--watch           # Watch for changes and re-validate (coming soon)
--provider <name> # Filter by provider (e.g., "cursor", "claude")
```

### Examples

```bash
# Validate single file
pnpm cli validate ~/.guideai/sessions/cursor/project/session.jsonl

# Validate all sessions for a provider
pnpm cli validate ~/.guideai/sessions/cursor/ --provider cursor

# Strict mode (warnings fail build)
pnpm cli validate ~/.guideai/sessions/ --strict

# JSON output for CI
pnpm cli validate ~/.guideai/sessions/ --json > report.json

# Verbose errors with details
pnpm cli validate ~/.guideai/sessions/cursor/session.jsonl --verbose
```

### Output Formats

#### Human-Readable (default)

```
=== Canonical JSONL Validation Report ===

Status: ✓ VALID
Total Lines: 15
Parsed Lines: 15
Valid Messages: 15/15

Session Info:
  Session ID: abc-123
  Provider: cursor
  Messages: 15
  Duration: 42 minutes

✓ Validation passed!
```

#### JSON Format (`--json`)

```json
[
  {
    "file": "/path/to/session.jsonl",
    "valid": true,
    "totalLines": 15,
    "validMessages": 15,
    "errors": [],
    "warnings": [],
    "sessionId": "abc-123",
    "provider": "cursor"
  }
]
```

---

## Integration Testing

### Testing Checklist for New Providers

Use this checklist when adding a new provider:

#### ✅ Basic Structure
- [ ] All messages have required fields (uuid, timestamp, type, sessionId, provider, message)
- [ ] Timestamps are valid RFC3339 format
- [ ] Message types are only `user`, `assistant`, or `meta`

#### ✅ Tool Handling
- [ ] `tool_use` blocks have non-empty `id` and `name`
- [ ] `tool_result` blocks have non-empty `tool_use_id` and `content`
- [ ] `tool_result` blocks are in user messages (not assistant)
- [ ] Every `tool_result` has a corresponding `tool_use`

#### ✅ Content Integrity
- [ ] No duplicate content in `providerMetadata`
- [ ] Tool inputs are valid JSON
- [ ] No empty required fields

#### ✅ Session-Wide
- [ ] Timestamps are generally increasing (no major out-of-order)
- [ ] Tool chains match correctly
- [ ] No orphan tool results

### Automated Testing

```rust
// In your provider's tests
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canonical_conversion_produces_valid_output() {
        // 1. Create test message
        let message = NewProviderMessage { /* ... */ };

        // 2. Convert to canonical
        let canonical = message.to_canonical()
            .expect("Conversion failed")
            .expect("No canonical message returned");

        // 3. Serialize to JSON
        let json = serde_json::to_string(&canonical)
            .expect("Failed to serialize");

        // 4. Validate with CLI (or use library directly)
        // For now, we validate structure manually:
        assert!(!canonical.uuid.is_empty());
        assert!(!canonical.session_id.is_empty());
        assert!(!canonical.provider.is_empty());

        // Validate timestamp format
        chrono::DateTime::parse_from_rfc3339(&canonical.timestamp)
            .expect("Invalid timestamp format");
    }
}
```

---

## Summary

### Key Takeaways

1. **Validation is Critical**: Ensures correct metrics, UI display, and data integrity
2. **TDD Workflow**: Use `pnpm cli validate` during development for fast feedback
3. **Tool Chain Rules**: `tool_result` must have `tool_use`, but not vice versa
4. **Message Types Matter**: `tool_result` blocks MUST be in user messages
5. **Timestamps**: Must be RFC3339 format
6. **Empty Fields**: Critical fields like `tool_use_id` and content cannot be empty

### Resources

- Validation schemas: `packages/types/src/canonical-validation.ts`
- Validation library: `packages/session-processing/src/validation/`
- CLI command: `packages/cli/src/validate.ts`
- Type definitions: `packages/types/src/sessions/messages.ts`

### Getting Help

- Check validation error codes in this guide
- Run with `--verbose` for detailed error information
- Review existing provider converters for patterns
- See canonical format docs: `provider-docs/claude/claude-jsonl.md`

---

**Last Updated**: 2025-11-06
**Status**: ✅ Production Ready
