# Desktop App Validation Integration - Implementation TODO

## Overview

Integrate the validation system into the desktop app to validate canonical JSONL output from provider converters. This provides real-time feedback during session conversion and enables UI-based validation reporting.

## Architecture

```
Provider Converter    →    Validation    →    Desktop Database    →    UI
(Rust)                     (TypeScript)       (SQLite)                (React)
                           via Tauri
```

## Phase 1: Tauri Validation Command

### 1.1 Add Validation Command

**File**: `apps/desktop/src-tauri/src/commands/validation.rs` (NEW)

```rust
use serde::{Deserialize, Serialize};
use std::process::Command;
use tauri::command;

#[derive(Serialize, Deserialize)]
pub struct ValidationResult {
    pub valid: bool,
    pub total_lines: usize,
    pub valid_messages: usize,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationWarning>,
    pub session_id: Option<String>,
    pub provider: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct ValidationError {
    pub line: usize,
    pub code: String,
    pub message: String,
    pub details: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize)]
pub struct ValidationWarning {
    pub line: usize,
    pub code: String,
    pub message: String,
}

#[command]
pub async fn validate_canonical_file(file_path: String) -> Result<ValidationResult, String> {
    // Call CLI validator via subprocess
    let output = Command::new("node")
        .arg("/path/to/cli/dist/esm/cli.js")
        .arg("validate")
        .arg(&file_path)
        .arg("--json")
        .output()
        .map_err(|e| format!("Failed to run validator: {}", e))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    // Parse JSON output
    let json_output = String::from_utf8_lossy(&output.stdout);
    let result: Vec<ValidationResult> = serde_json::from_str(&json_output)
        .map_err(|e| format!("Failed to parse validator output: {}", e))?;

    result.into_iter().next().ok_or_else(|| "No validation result".to_string())
}

#[command]
pub async fn validate_session_directory(
    directory: String,
    provider: Option<String>,
) -> Result<Vec<ValidationResult>, String> {
    let mut args = vec![
        "/path/to/cli/dist/esm/cli.js".to_string(),
        "validate".to_string(),
        directory,
        "--json".to_string(),
    ];

    if let Some(p) = provider {
        args.push("--provider".to_string());
        args.push(p);
    }

    let output = Command::new("node")
        .args(&args)
        .output()
        .map_err(|e| format!("Failed to run validator: {}", e))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    let json_output = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&json_output)
        .map_err(|e| format!("Failed to parse validator output: {}", e))
}
```

### 1.2 Register Commands

**File**: `apps/desktop/src-tauri/src/main.rs`

```rust
mod commands {
    pub mod validation;
    // ... other commands
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::validation::validate_canonical_file,
            commands::validation::validate_session_directory,
            // ... other commands
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

## Phase 2: Database Schema Updates

### 2.1 Add Validation Metadata to Sessions

**File**: `apps/desktop/src-tauri/src/database.rs`

```sql
ALTER TABLE agent_sessions ADD COLUMN validation_status TEXT DEFAULT 'unknown';
-- Values: 'valid', 'warnings', 'errors', 'unknown'

ALTER TABLE agent_sessions ADD COLUMN validation_error_count INTEGER DEFAULT 0;
ALTER TABLE agent_sessions ADD COLUMN validation_warning_count INTEGER DEFAULT 0;
ALTER TABLE agent_sessions ADD COLUMN last_validated_at TEXT;
```

### 2.2 Update Session Insert/Update

```rust
pub fn update_session_validation(
    conn: &Connection,
    session_id: &str,
    validation_result: &ValidationResult,
) -> Result<()> {
    let status = if !validation_result.valid {
        "errors"
    } else if !validation_result.warnings.is_empty() {
        "warnings"
    } else {
        "valid"
    };

    conn.execute(
        "UPDATE agent_sessions
         SET validation_status = ?,
             validation_error_count = ?,
             validation_warning_count = ?,
             last_validated_at = datetime('now')
         WHERE session_id = ?",
        params![
            status,
            validation_result.errors.len(),
            validation_result.warnings.len(),
            session_id
        ],
    )?;

    Ok(())
}
```

## Phase 3: Converter Integration

### 3.1 Validate After Conversion

**Modify**: Each provider's scanner (e.g., `codex_watcher.rs`)

```rust
// After writing canonical JSONL
let canonical_path = write_canonical_jsonl(&session_id, &canonical_messages)?;

// Validate the output
let validation_result = validate_canonical_file(&canonical_path).await?;

if !validation_result.valid {
    error!(
        "Validation failed for session {}: {} errors",
        session_id,
        validation_result.errors.len()
    );

    // Log errors for debugging
    for error in &validation_result.errors {
        error!("  Line {}: {}", error.line, error.message);
    }
}

// Update database with validation status
update_session_validation(&conn, &session_id, &validation_result)?;

// Publish event with validation status
event_bus.publish("provider-name", SessionEventPayload::SessionChanged {
    session_id,
    validation_status: Some(validation_result.valid),
})?;
```

## Phase 4: UI Components

### 4.1 Validation Badge Component

**File**: `apps/desktop/src/components/ValidationBadge.tsx`

```typescript
interface ValidationBadgeProps {
  status: 'valid' | 'warnings' | 'errors' | 'unknown'
  errorCount?: number
  warningCount?: number
}

export function ValidationBadge({ status, errorCount = 0, warningCount = 0 }: ValidationBadgeProps) {
  const icons = {
    valid: <CheckCircleIcon className="w-4 h-4 text-success" />,
    warnings: <ExclamationTriangleIcon className="w-4 h-4 text-warning" />,
    errors: <XCircleIcon className="w-4 h-4 text-error" />,
    unknown: <QuestionMarkCircleIcon className="w-4 h-4 text-base-content/50" />
  }

  const labels = {
    valid: 'Valid',
    warnings: `${warningCount} Warning${warningCount !== 1 ? 's' : ''}`,
    errors: `${errorCount} Error${errorCount !== 1 ? 's' : ''}`,
    unknown: 'Not Validated'
  }

  return (
    <div className="badge badge-sm gap-1" data-status={status}>
      {icons[status]}
      <span>{labels[status]}</span>
    </div>
  )
}
```

### 4.2 Validation Report Viewer

**File**: `apps/desktop/src/components/ValidationReport.tsx`

```typescript
interface ValidationReportProps {
  sessionId: string
}

export function ValidationReport({ sessionId }: ValidationReportProps) {
  const [result, setResult] = useState<ValidationResult | null>(null)
  const [loading, setLoading] = useState(false)

  const validateSession = async () => {
    setLoading(true)
    try {
      const filePath = `~/.guidemode/sessions/${provider}/${project}/${sessionId}.jsonl`
      const validationResult = await invoke('validate_canonical_file', { filePath })
      setResult(validationResult)
    } catch (error) {
      console.error('Validation failed:', error)
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    validateSession()
  }, [sessionId])

  if (loading) return <div className="loading loading-spinner" />
  if (!result) return null

  return (
    <div className="validation-report">
      <div className="flex items-center justify-between mb-4">
        <h3 className="text-lg font-semibold">Validation Report</h3>
        <ValidationBadge
          status={result.valid ? 'valid' : 'errors'}
          errorCount={result.errors.length}
          warningCount={result.warnings.length}
        />
      </div>

      {result.errors.length > 0 && (
        <div className="mb-4">
          <h4 className="font-medium text-error mb-2">Errors ({result.errors.length})</h4>
          <ul className="space-y-2">
            {result.errors.map((error, i) => (
              <li key={i} className="text-sm">
                <span className="font-mono text-error">Line {error.line}</span>
                <span className="mx-2">•</span>
                <span className="font-medium">[{error.code}]</span>
                <span className="mx-2">•</span>
                <span>{error.message}</span>
              </li>
            ))}
          </ul>
        </div>
      )}

      {result.warnings.length > 0 && (
        <div>
          <h4 className="font-medium text-warning mb-2">Warnings ({result.warnings.length})</h4>
          <ul className="space-y-2">
            {result.warnings.map((warning, i) => (
              <li key={i} className="text-sm">
                <span className="font-mono text-warning">Line {warning.line}</span>
                <span className="mx-2">•</span>
                <span className="font-medium">[{warning.code}]</span>
                <span className="mx-2">•</span>
                <span>{warning.message}</span>
              </li>
            ))}
          </ul>
        </div>
      )}

      {result.valid && result.errors.length === 0 && (
        <div className="alert alert-success">
          <CheckCircleIcon className="w-6 h-6" />
          <span>All {result.valid_messages} messages validated successfully!</span>
        </div>
      )}

      <button
        className="btn btn-sm btn-outline mt-4"
        onClick={validateSession}
      >
        Re-validate
      </button>
    </div>
  )
}
```

### 4.3 Session List Integration

**Modify**: `apps/desktop/src/components/SessionList.tsx`

```typescript
<div className="session-item">
  <div className="session-header">
    <span className="session-id">{session.id}</span>
    <ValidationBadge
      status={session.validationStatus}
      errorCount={session.validationErrorCount}
      warningCount={session.validationWarningCount}
    />
  </div>
  {/* ... rest of session item */}
</div>
```

## Phase 5: Settings Panel

### 5.1 Validation Settings

**File**: `apps/desktop/src/components/Settings.tsx`

```typescript
<div className="form-control">
  <label className="label cursor-pointer">
    <span className="label-text">Validate sessions after conversion</span>
    <input
      type="checkbox"
      className="toggle"
      checked={settings.autoValidate}
      onChange={(e) => updateSetting('autoValidate', e.target.checked)}
    />
  </label>
</div>

<div className="form-control">
  <label className="label cursor-pointer">
    <span className="label-text">Fail on validation warnings</span>
    <input
      type="checkbox"
      className="toggle"
      checked={settings.strictValidation}
      onChange={(e) => updateSetting('strictValidation', e.target.checked)}
    />
  </label>
</div>
```

## Implementation Checklist

### Backend (Rust)
- [ ] Create `commands/validation.rs`
- [ ] Add Tauri commands for file and directory validation
- [ ] Update database schema with validation fields
- [ ] Integrate validation into converter pipeline
- [ ] Add validation logging

### Frontend (TypeScript/React)
- [ ] Create `ValidationBadge` component
- [ ] Create `ValidationReport` component
- [ ] Update session list to show validation status
- [ ] Add validation settings panel
- [ ] Add "Re-validate" action to session details

### Testing
- [ ] Test validation with Codex sessions (should pass)
- [ ] Test validation with malformed sessions (should fail)
- [ ] Test UI updates when validation status changes
- [ ] Test bulk validation of directories

## Success Criteria

- ✅ Sessions show validation status in UI
- ✅ Users can view detailed validation reports
- ✅ Invalid sessions are flagged visually
- ✅ Validation runs automatically after conversion
- ✅ Settings allow disabling auto-validation

## Related Files

- CLI validator: `packages/cli/src/validate.ts`
- Validation library: `packages/session-processing/src/validation/`
- Validation schemas: `packages/types/src/canonical-validation.ts`
- Guide: `apps/desktop/src-tauri/src/providers/VALIDATION_GUIDE.md`

---

**Created**: 2025-11-06
**Priority**: Medium (enhances developer experience)
**Dependencies**: Validation system must be working (✅ Complete)
