# Cursor Provider

Cursor session detection, protobuf decoding, and conversion to canonical JSONL format.

## Architecture

### Overview

Cursor stores AI sessions in SQLite databases with Protocol Buffer-encoded messages:
- **Location**: `~/.cursor/chats/{hash}/{uuid}/store.db`
- **Format**: SQLite with WAL mode for concurrent access
- **Storage**: Content-addressable blobs (SHA-256 IDs)
- **Messages**: Hybrid protobuf/JSON format

### Module Structure

```
cursor/
├── mod.rs              # Session discovery, CWD mapping
├── db.rs               # SQLite operations (read-only, WAL-safe)
├── protobuf.rs         # Protocol Buffer schema
├── converter.rs        # Protobuf → Canonical JSONL conversion
├── scanner.rs          # Batch session processing
├── debug.rs            # Inspection utilities
└── types.rs            # Session metadata types
```

### Tools (Binary Utilities)

```
src/bin/
├── inspect_cursor.rs         # High-level session inspector
├── cursor_hex_inspector.rs   # Low-level wire format analyzer
└── cursor_blob_analyzer.rs   # Corruption pattern detector
```

---

## Protocol Buffer Schema

### Field 1: Polymorphic Content

**Critical**: Field 1 has different types depending on message role:

- **Assistant messages**: Nested `ContentWrapper` message
  ```protobuf
  message ContentWrapper {
    optional string text = 1;
  }
  ```

- **User messages**: Direct string content

- **Tree/reference blobs**: Nested structure (not messages)

This polymorphism required creating two schema variants:

1. `CursorBlob` - Decodes Field 1 as `ContentWrapper` (for assistant)
2. `CursorBlobDirectContent` - Decodes Field 1 as string (for user)

### Complete Schema

```protobuf
message CursorBlob {
  optional ContentWrapper content_wrapper = 1;  // Nested for assistant
  optional string uuid = 2;                      // Message ID (user only)
  optional string metadata = 3;                  // Usually empty
  optional string complex_data = 4;              // JSON: tool calls, etc.
  optional string additional_content = 5;        // Tool outputs
  optional bytes blob_references = 8;            // SHA-256 hashes
}

message CursorBlobDirectContent {
  optional string content = 1;                   // Direct for user
  // Fields 2-8 same as CursorBlob
}
```

### Key Insights

- **No timestamps**: Ordering relies on database rowid
- **No CWD**: Must derive from `~/.cursor/projects` hash
- **Hybrid format**: Some messages are JSON (Anthropic API format)
- **Role inference**:
  - User messages: Have UUID in Field 2
  - Assistant messages: No UUID, use content_wrapper

---

## Conversion Flow

### 1. Database Reading

```rust
// db.rs
pub fn get_decoded_messages(
    conn: &Connection,
) -> Result<Vec<(String, Vec<u8>, CursorMessage)>, Error> {
    // Returns: (blob_id, raw_data, decoded_message)
    // raw_data needed for fallback decoding of user messages
}
```

**Why raw_data?** User messages need to be decoded as `CursorBlobDirectContent` to extract Field 1 string.

### 2. Message Decoding

```rust
// protobuf.rs
impl CursorMessage {
    pub fn decode_from_bytes(data: &[u8]) -> Result<Self, Error> {
        // Try protobuf first
        match CursorBlob::decode(data) {
            Ok(blob) => Ok(CursorMessage::Protobuf(blob)),
            Err(_) => {
                // Fallback to JSON (Anthropic API format)
                let json_msg = serde_json::from_slice::<JsonMessage>(data)?;
                Ok(CursorMessage::Json(json_msg))
            }
        }
    }
}
```

### 3. Canonical Conversion

```rust
// converter.rs
pub struct CursorMessageWithRaw<'a> {
    pub message: &'a CursorMessage,
    pub raw_data: &'a [u8],  // For fallback decoding
}

impl<'a> ToCanonical for CursorMessageWithRaw<'a> {
    fn to_canonical(&self) -> Result<Option<CanonicalMessage>> {
        // Extract content with raw fallback
        let content = get_content_with_raw_fallback(blob, raw_data);

        // Build canonical message
        CanonicalMessage { ... }
    }
}

fn get_content_with_raw_fallback(blob: &CursorBlob, raw_data: &[u8]) -> String {
    // Try content_wrapper first (assistant)
    if let Some(wrapper) = &blob.content_wrapper {
        if let Some(text) = &wrapper.text {
            return text.clone();
        }
    }

    // Fall back to direct content (user)
    if let Ok(direct_blob) = CursorBlobDirectContent::decode(raw_data) {
        if let Some(content) = direct_blob.content {
            return content;
        }
    }

    String::new()
}
```

### 4. Output

**Canonical path**: `~/.guidemode/sessions/cursor/{project}/{session_id}.jsonl`

Each line is a `CanonicalMessage` JSON object with:
- Unified structure across all providers
- Provider-specific data in `provider_metadata`
- Tool calls, thinking blocks, etc. as `ContentBlock` variants

---

## CWD Mapping

Cursor doesn't store CWD in message blobs. We derive it from the projects directory:

```
~/.cursor/projects/Users-cliftonc-work-guidemode/
                   ^^^^^^^^^^^^^^^^^^^^^^^^^^^
                   CWD with / → - substitution
```

**Algorithm**:
1. Hash directory name: `0d265392dfc786bc1af0df28bb21fea3`
2. Reverse project name to CWD: `Users-cliftonc-work-guidemode` → `/Users/cliftonc/work/guidemode`
3. Verify MD5 match: `md5("/Users/cliftonc/work/guidemode") == hash`

---

## Analysis Tools

### Tool 1: inspect_cursor (High-level)

**Purpose**: Human-friendly session inspection

**Commands**:
```bash
# List all sessions
cargo run --bin inspect_cursor -- list

# View session messages
cargo run --bin inspect_cursor -- session ~/.cursor/chats/.../store.db

# Find tool use examples
cargo run --bin inspect_cursor -- tools ~/.cursor/chats/.../store.db

# Export to JSON
cargo run --bin inspect_cursor -- export ~/.cursor/chats/.../store.db output.json
```

**Use when**: You want to browse messages and understand session structure

**Output Example**:
```
=== Message 2 ===
ID: 72d5bc2a...
Type: Protobuf
  Is Message Blob: true
  Content: "I'll run the workspace type check now."
  UUID:
  Role: assistant
```

### Tool 2: cursor_hex_inspector (Wire format)

**Purpose**: Low-level protobuf wire format analysis

**Command**:
```bash
cargo run --bin cursor_hex_inspector -- ~/.cursor/chats/.../store.db
```

**Features**:
- Hex dump with field markers highlighted
- Decodes field types (varint, length-delimited, etc.)
- Shows wire-level structure

**Use when**: You need to see raw field types and validate schema

**Output Example**:
```
Blob 2: 72d5bc2a...

Hex dump:
0000: 0a 28 0a 26 49 27 6c 6c 20 72 75 6e 20 74 68 65   .(& I'll run the
      ^^^^
      Field 1 (length-delimited, 40 bytes) - ContentWrapper

0010: 20 77 6f 72 6b 73 70 61 63 65 20 74 79 70 65 20    workspace type
0020: 63 68 65 63 6b 20 6e 6f 77 2e 1a 00               check now...

Decoded structure:
  Field 1 (LEN): 40 bytes - Nested message (ContentWrapper)
    └─ Field 1 (LEN): 38 bytes - "I'll run the workspace type check now."
  Field 3 (LEN): 0 bytes - Empty metadata
```

### Tool 3: cursor_blob_analyzer (Corruption detector)

**Purpose**: Detect and analyze corruption patterns

**Command**:
```bash
cargo run --bin cursor_blob_analyzer -- ~/.cursor/chats/.../store.db
```

**Features**:
- Detects `\n&` corruption pattern (protobuf wire markers misinterpreted as content)
- Shows field type and content preview for each blob
- Identifies which messages have issues

**Use when**: You suspect content corruption or want to validate fixes

**Output Example**:
```
=== Cursor Blob Analyzer ===

Blob 2/32: 72d5bc2a...
  Field 1: NESTED MESSAGE (40 bytes)
    └─ Contains: "I'll run the workspace type check now."
  ✓ Content is clean (no corruption detected)

Blob 3/32: 232bb144...
  Field 1: CORRUPTED? Starts with \x0a\x26
    └─ Raw bytes: [0a 26 49 27 6c 6c ...]
  ⚠️  POSSIBLE CORRUPTION: Looks like protobuf markers
  💡 FIX: Decode as nested ContentWrapper, not direct string
```

---

## Debugging Workflow

### Problem: Corrupted message content

**Symptoms**:
```json
{
  "content": "\n&I'll run the workspace type check now."
           ^^^^
           Corruption: protobuf field markers
}
```

**Investigation Steps**:

1. **Inspect with cursor_blob_analyzer**:
   ```bash
   cargo run --bin cursor_blob_analyzer -- ~/.cursor/chats/.../store.db
   ```
   This reveals which blobs have `\n&` patterns and whether they're truly corrupted or just nested messages.

2. **Examine wire format with cursor_hex_inspector**:
   ```bash
   cargo run --bin cursor_hex_inspector -- ~/.cursor/chats/.../store.db
   ```
   This shows the actual protobuf field structure. Look for:
   - `0a XX` (Field 1, length-delimited) at start
   - Another `0a XX` nested inside = ContentWrapper!

3. **Verify with inspect_cursor**:
   ```bash
   cargo run --bin inspect_cursor -- session ~/.cursor/chats/.../store.db
   ```
   Check if content now displays correctly with the raw fallback fix.

4. **Test canonical output**:
   ```bash
   cat ~/.guidemode/sessions/cursor/project/session.jsonl | jq '.message.content'
   ```
   Verify no corruption in final canonical format.

### Root Cause Resolution

**The Fix** (implemented in `protobuf.rs` + `converter.rs`):

1. Created `ContentWrapper` schema for nested Field 1
2. Added `CursorBlobDirectContent` for direct string Field 1
3. Converter uses raw data fallback to try both schemas
4. Assistant messages → extract from `content_wrapper.text`
5. User messages → decode as `CursorBlobDirectContent` to get direct string

**Why it works**:
- Protobuf Field 1 is polymorphic (nested vs direct)
- Can't know which without trying both
- Raw data allows re-decoding with alternative schema
- No corruption - just wrong schema interpretation!

---

## Testing

### Unit Tests

```bash
cargo test --lib cursor
```

Tests cover:
- Session discovery
- Metadata parsing
- CWD hash mapping
- Message role inference
- Tree blob detection

### Integration Tests

```bash
# Scan all Cursor sessions (dry run)
cargo run --bin inspect_cursor -- list

# Process a specific session
cargo run --bin inspect_cursor -- session ~/.cursor/chats/.../store.db

# Verify canonical output
cat ~/.guidemode/sessions/cursor/project/*.jsonl | jq .
```

---

## Common Issues

### Issue 1: "Corrupted" content with `\n&` prefix

**Cause**: Field 1 decoded as string instead of nested ContentWrapper

**Solution**: Use `CursorMessageWithRaw` wrapper to enable fallback decoding

### Issue 2: Empty content for user messages

**Cause**: User messages have direct string Field 1, but schema expects ContentWrapper

**Solution**: `get_content_with_raw_fallback()` re-decodes as `CursorBlobDirectContent`

### Issue 3: Session not found after discovery

**Cause**: CWD hash doesn't match any project folder

**Solution**: Session will use "unknown" project name, CWD will be None

### Issue 4: "Invalid protobuf" errors

**Cause**: Tree/reference blobs are not messages, they fail decode

**Solution**: Expected behavior - scanner skips non-message blobs

---

## Future Enhancements

### Potential Improvements

1. **Timestamp extraction**: Reverse-engineer Cursor's timestamp storage
2. **Streaming parser**: Process large sessions incrementally
3. **Change detection**: Watch database file_size + data_version for updates
4. **Model tracking**: Extract model name from complex_data if available
5. **Format validator**: Scan entire database and report corruption stats

### Analysis Tools Wishlist

1. **cursor_format_validator**: Database-wide validation and statistics
2. **cursor_timeline**: Visualize session timeline and conversation flow
3. **cursor_diff**: Compare two sessions or track changes over time
4. **cursor_export**: Export to various formats (Markdown, HTML, etc.)

---

## References

### Key Files

- `protobuf.rs:19-31` - ContentWrapper schema definition
- `protobuf.rs:232-253` - get_field1_content() with fallback logic
- `converter.rs:13-27` - CursorMessageWithRaw wrapper
- `converter.rs:112-136` - get_content_with_raw_fallback() implementation
- `db.rs:78-121` - get_decoded_messages() with raw data return
- `scanner.rs:113-136` - Canonical conversion loop

### External Resources

- Protocol Buffers encoding: https://protobuf.dev/programming-guides/encoding/
- SQLite WAL mode: https://www.sqlite.org/wal.html
- Prost (Rust protobuf): https://docs.rs/prost/latest/prost/

---

**Last Updated**: 2025-11-05
**Status**: ✅ Production ready - all corruption issues resolved
