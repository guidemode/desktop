# Windows Build Support

This document describes the Windows support implementation for GuideAI Desktop.

## Overview

GuideAI Desktop now supports Windows builds with proper cross-platform path handling for all three supported AI coding agents:
- **Claude Code** - Anthropic's AI coding assistant
- **OpenCode** - Open source coding assistant
- **Codex** - OpenAI Codex integration

## What Was Implemented

### 1. Platform-Specific Path Defaults (TypeScript)

**File**: `apps/desktop/src/types/providers.ts`

Added platform detection and Windows-specific default paths:

```typescript
const PLATFORM_DEFAULTS: Record<string, Record<string, string>> = {
  'claude-code': {
    win32: '~/.claude',
    darwin: '~/.claude',
    linux: '~/.claude'
  },
  'opencode': {
    win32: '%LOCALAPPDATA%/opencode',
    darwin: '~/.local/share/opencode',
    linux: '~/.local/share/opencode'
  },
  'codex': {
    win32: '~/.codex',
    darwin: '~/.codex',
    linux: '~/.codex'
  }
}
```

The platform is automatically detected using `navigator.platform` and the appropriate default is selected.

### 2. Enhanced Rust Path Resolution

**Files**:
- `apps/desktop/src-tauri/src/providers/claude.rs`
- `apps/desktop/src-tauri/src/providers/codex.rs`
- `apps/desktop/src-tauri/src/providers/opencode.rs` (already had good fallbacks)

Enhanced path resolution with fallback logic:

```rust
// Primary path from user config
let primary_base = PathBuf::from(expanded.into_owned());

// Fallback to standard home directory location
if let Some(home_dir) = dirs::home_dir() {
    base_candidates.push(home_dir.join(".claude"));
}

// Find first existing path
let base_path = base_candidates
    .into_iter()
    .find(|path| path.exists())
    .ok_or_else(|| format!("Home directory not found"))?;
```

### 3. Windows Build Scripts

**File**: `apps/desktop/package.json`

Added platform-specific build commands:

```json
{
  "tauri:build:windows": "tauri build --target x86_64-pc-windows-msvc",
  "tauri:build:windows:prod": "NODE_ENV=production tauri build --target x86_64-pc-windows-msvc",
  "tauri:build:macos": "tauri build --target aarch64-apple-darwin",
  "tauri:build:linux": "tauri build --target x86_64-unknown-linux-gnu"
}
```

### 4. Documentation Updates

**File**: `apps/desktop/CLAUDE.md`

Added comprehensive Windows build instructions and platform-specific path documentation.

## Windows Path Mappings

| Provider | UI Default | Windows Resolution |
|----------|-----------|-------------------|
| Claude Code | `~/.claude` | `C:\Users\<username>\.claude` |
| OpenCode | `%LOCALAPPDATA%/opencode` | `C:\Users\<username>\AppData\Local\opencode` |
| Codex | `~/.codex` | `C:\Users\<username>\.codex` |

## Cross-Platform Path Handling

The implementation uses two key Rust crates for cross-platform compatibility:

1. **`shellexpand`** - Expands `~` to the user's home directory
   - Works correctly on Windows (resolves to `C:\Users\<username>`)
   - Handles tilde expansion cross-platform

2. **`dirs`** - Provides platform-specific directory paths
   - `dirs::home_dir()` - Returns user home directory
   - `dirs::data_dir()` - Returns platform-specific data directory
     - Windows: `C:\Users\<username>\AppData\Roaming`
     - macOS: `~/Library/Application Support`
     - Linux: `~/.local/share`

## Building for Windows

### Prerequisites

1. **Install Rust Windows target:**
   ```bash
   rustup target add x86_64-pc-windows-msvc
   ```

2. **Cross-compilation setup** (if building from macOS/Linux):
   - Cross-compilation to Windows is complex
   - **Recommended**: Build on native Windows or use GitHub Actions
   - Requires Windows SDK and additional tooling

### Build Commands

```bash
# From workspace root
cd apps/desktop

# Development build for Windows
pnpm tauri:build:windows

# Production build for Windows
pnpm tauri:build:windows:prod
```

### Build Output

Windows builds generate:
- **MSI installer**: `apps/desktop/src-tauri/target/release/bundle/msi/*.msi`
- **NSIS installer** (fallback): `apps/desktop/src-tauri/target/release/bundle/nsis/*.exe`

**Note**: Our GitHub Actions workflow builds on native Windows using `windows-latest`, which produces MSI installers by default.

### Download Links

Once a release is published, Windows builds are available at:
- **Versioned**: `https://install.guideai.dev/desktop/v{VERSION}/GuideAI-Desktop-{VERSION}-windows.msi`
- **Latest**: `https://install.guideai.dev/desktop/latest/GuideAI-Desktop-windows.msi`
- **Web UI**: Available on the home page download card at `https://guideai.dev`

## Important Notes

### Native Windows Builds

**GitHub Actions Approach:**
- Builds on `windows-latest` runner (native Windows environment)
- Produces MSI installers using WiX toolset (pre-installed on Windows runners)
- More reliable than cross-compilation from Linux
- Full feature support including code signing (when configured)

**Why Native over Cross-Compilation:**
- MSI installers require Windows and WiX toolset
- Cross-compilation has limited support and requires experimental features
- Native builds are the recommended approach for production
- Avoids Linux library dependency issues (appindicator, etc.)

### Claude Code on Windows

Claude Code **requires WSL** (Windows Subsystem for Linux) on Windows:
- Most Windows users run Claude Code through WSL
- Paths will be WSL Linux paths (e.g., `/home/<username>/.claude`)
- The default `~/.claude` works correctly with WSL/Git Bash

### OpenCode on Windows

OpenCode uses Windows-native paths:
- Default: `%LOCALAPPDATA%\opencode` → `C:\Users\<username>\AppData\Local\opencode`
- Fallback to `dirs::data_dir()` ensures correct Windows path resolution

### Testing Path Resolution

To test path resolution on Windows:

1. **Check default paths** in the UI:
   - Settings should show Windows-appropriate defaults
   - OpenCode should show `%LOCALAPPDATA%/opencode`
   - Claude/Codex should show `~/.claude` and `~/.codex`

2. **Verify Rust resolution**:
   - Paths are resolved using `shellexpand::tilde()`
   - Fallbacks use `dirs::home_dir()` for Windows compatibility
   - Check logs for actual resolved paths

## CI/CD Recommendations

For automated Windows builds, use GitHub Actions:

```yaml
- name: Build Windows
  if: matrix.platform == 'windows-latest'
  run: |
    rustup target add x86_64-pc-windows-msvc
    pnpm tauri:build:windows:prod
```

See `.github/workflows/` for existing CI/CD examples.

## Troubleshooting

### Issue: Paths not resolving on Windows

**Solution**: Ensure the path is expanded correctly. The app now:
1. First tries the user-configured path with `shellexpand::tilde()`
2. Falls back to `dirs::home_dir()` + provider subdirectory
3. Uses first existing path

### Issue: Cross-compilation fails

**Solution**:
- Use native Windows for building
- OR use GitHub Actions with Windows runner
- Cross-compilation to Windows requires complex setup

### Issue: Claude Code not found on Windows

**Check**:
1. Is Claude Code installed via WSL?
2. Is the path pointing to WSL home directory?
3. Try WSL path: `/home/<username>/.claude` or Windows path: `C:\Users\<username>\.claude`

## Future Enhancements

Potential improvements for Windows support:

1. **Windows Environment Variable Expansion**
   - Add support for expanding `%APPDATA%`, `%LOCALAPPDATA%`, etc. in user input
   - Current implementation handles defaults but not manual user paths with env vars

2. **WSL Path Detection**
   - Auto-detect if Claude Code is installed in WSL
   - Offer WSL path as alternative when scanning

3. **Windows-Specific Icons**
   - Optimize system tray icon for Windows notification area
   - Already has `icon.ico` in bundle config

4. **Windows Auto-Start**
   - Add option to start GuideAI Desktop on Windows login
   - Register with Windows startup folder

## References

- [Tauri Windows Guide](https://v1.tauri.app/v1/guides/building/windows/)
- [shellexpand crate](https://crates.io/crates/shellexpand)
- [dirs crate](https://crates.io/crates/dirs)
- [Claude Code Windows Installation](https://docs.claude.com/en/docs/claude-code/setup)
