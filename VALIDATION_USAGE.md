# Validation Integration - Usage Guide

This document shows how to use the newly integrated validation system in the desktop app.

## Components

### 1. ValidationBadge

A compact badge showing validation status inline.

```tsx
import { ValidationBadge } from './components/ValidationBadge'

// Simple usage
<ValidationBadge status="valid" />

// With error/warning counts
<ValidationBadge
  status="errors"
  errorCount={5}
  warningCount={2}
/>
```

**Props:**
- `status`: 'valid' | 'warnings' | 'errors' | 'unknown'
- `errorCount?`: number (for errors status)
- `warningCount?`: number (for warnings status)
- `className?`: string (additional CSS classes)

### 2. ValidationReport

A detailed validation report with expandable errors/warnings.

```tsx
import { ValidationReport } from './components/ValidationReport'

<ValidationReport
  sessionId="abc-123"
  provider="cursor"
  project="my-project"
  filePath="/path/to/session.jsonl"
/>
```

**Props:**
- `sessionId`: string - Session identifier
- `provider`: string - Provider name (cursor, codex, etc.)
- `project`: string - Project name
- `filePath`: string - Full path to canonical JSONL file

**Features:**
- Auto-validates on mount
- Shows stats (total lines, valid messages, errors, warnings)
- Expandable error/warning lists with line numbers
- Re-validate button

## Integration Examples

### Example 1: Add Validation Tab to Session Detail View

```tsx
// In SessionDetailView.tsx or similar
import { ValidationReport } from './components/ValidationReport'

function SessionDetailView({ session }) {
  return (
    <div className="tabs tabs-boxed">
      <a className="tab">Overview</a>
      <a className="tab">Messages</a>
      <a className="tab tab-active">Validation</a>
    </div>

    <div className="tab-content">
      <ValidationReport
        sessionId={session.id}
        provider={session.provider}
        project={session.project}
        filePath={session.file_path}
      />
    </div>
  )
}
```

### Example 2: Add Badge to Session List

```tsx
// In SessionList.tsx or similar
import { ValidationBadge } from './components/ValidationBadge'

function SessionListItem({ session }) {
  // You can call validate_canonical_file to get status
  const [validationStatus, setValidationStatus] = useState('unknown')

  return (
    <div className="session-item">
      <div className="session-header">
        <span>{session.id}</span>
        <ValidationBadge status={validationStatus} />
      </div>
      {/* ... rest of session item */}
    </div>
  )
}
```

### Example 3: Validate on Demand (Button Click)

```tsx
import { invoke } from '@tauri-apps/api/core'
import { useState } from 'react'

function ValidateButton({ filePath }) {
  const [result, setResult] = useState(null)
  const [loading, setLoading] = useState(false)

  const handleValidate = async () => {
    setLoading(true)
    try {
      const validationResult = await invoke('validate_canonical_file', {
        filePath,
      })
      setResult(validationResult)

      if (validationResult.valid) {
        // Show success toast
        console.log('✓ Validation passed!')
      } else {
        // Show error toast
        console.log('✗ Validation failed:', validationResult.errors)
      }
    } catch (error) {
      console.error('Validation error:', error)
    } finally {
      setLoading(false)
    }
  }

  return (
    <button
      onClick={handleValidate}
      className="btn btn-sm"
      disabled={loading}
    >
      {loading ? 'Validating...' : 'Validate'}
    </button>
  )
}
```

### Example 4: Bulk Validation (Directory)

```tsx
import { invoke } from '@tauri-apps/api/core'

async function validateAllSessions(provider: string) {
  const directory = `~/.guidemode/sessions/${provider}/`

  const results = await invoke('validate_session_directory', {
    directory,
    provider,
  })

  const totalFiles = results.length
  const validFiles = results.filter(r => r.valid).length
  const totalErrors = results.reduce((sum, r) => sum + r.errors.length, 0)

  console.log(`Validated ${totalFiles} sessions`)
  console.log(`Valid: ${validFiles}, Invalid: ${totalFiles - validFiles}`)
  console.log(`Total errors: ${totalErrors}`)

  return results
}
```

## Tauri Commands

### validate_canonical_file

Validate a single canonical JSONL file.

```rust
#[tauri::command]
pub async fn validate_canonical_file(file_path: String) -> Result<ValidationResult, String>
```

**Usage:**
```tsx
const result = await invoke('validate_canonical_file', {
  filePath: '/path/to/session.jsonl'
})
```

**Returns:**
```typescript
{
  valid: boolean
  total_lines: number
  valid_messages: number
  errors: Array<{
    line: number
    code: string
    message: string
    details?: any
  }>
  warnings: Array<{
    line: number
    code: string
    message: string
  }>
  session_id?: string
  provider?: string
}
```

### validate_session_directory

Validate all canonical JSONL files in a directory.

```rust
#[tauri::command]
pub async fn validate_session_directory(
    directory: String,
    provider: Option<String>,
) -> Result<Vec<ValidationResult>, String>
```

**Usage:**
```tsx
const results = await invoke('validate_session_directory', {
  directory: '~/.guidemode/sessions/cursor/',
  provider: 'cursor' // optional filter
})
```

**Returns:**
Array of `ValidationResult` objects (same structure as single file validation).

## Best Practices

1. **On-Demand Validation**: Don't validate every session on load - it's expensive. Instead:
   - Add a "Validate" button in session details
   - Show validation tab only when user clicks it
   - Cache validation results temporarily

2. **Developer Mode**: Consider making validation a dev-only feature:
   ```tsx
   const isDev = import.meta.env.DEV

   {isDev && (
     <ValidationReport {...props} />
   )}
   ```

3. **Error Handling**: Always handle validation errors gracefully:
   ```tsx
   try {
     await invoke('validate_canonical_file', { filePath })
   } catch (error) {
     console.error('Validation failed:', error)
     // Show user-friendly error message
   }
   ```

4. **Performance**: For large directories, consider:
   - Pagination (validate in chunks)
   - Background processing
   - Progress indicators

## Testing

### Test with Valid Session
```bash
# Via CLI
pnpm cli validate ~/.guidemode/sessions/codex/project/session.jsonl

# Should show all valid
```

### Test with Invalid Session
```bash
# Create test file with invalid content
echo '{"invalid": "json"}' > /tmp/test-invalid.jsonl

# Via CLI
pnpm cli validate /tmp/test-invalid.jsonl --verbose

# Should show errors
```

## Troubleshooting

### Issue: "CLI validator not found"

**Cause**: CLI package not built.

**Solution**:
```bash
cd packages/cli
pnpm build
```

### Issue: "Failed to parse validator output"

**Cause**: CLI output format mismatch.

**Solution**: Check that CLI returns JSON array with `--json` flag:
```bash
pnpm cli validate session.jsonl --json
```

### Issue: ValidationReport doesn't load

**Cause**: Missing file path or invalid path.

**Solution**: Ensure `filePath` prop points to an existing canonical JSONL file:
```tsx
<ValidationReport
  filePath={session.file_path} // Must be absolute path
  {...otherProps}
/>
```

## Future Enhancements

Potential improvements to consider:

1. **Auto-validation on conversion**: Validate sessions immediately after conversion
2. **Database storage**: Store validation results in SQLite for quick access
3. **Validation badges in session lists**: Show inline status without clicking
4. **Bulk fix tool**: Automatically fix common validation issues
5. **Watch mode**: Re-validate when files change
6. **Custom rules**: Provider-specific validation rules

---

**Status**: ✅ Ready to use
**Last Updated**: 2025-11-06
