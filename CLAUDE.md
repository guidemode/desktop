# Desktop - GuideAI Menubar Application

Cross-platform desktop menubar application built with Tauri, React, TypeScript, and Tailwind CSS.

## Architecture

### Stack
- **Tauri**: Rust-based desktop framework
- **React 18**: Frontend UI with hooks
- **TypeScript**: Full type safety
- **Tailwind CSS + DaisyUI**: Styling and components
- **TanStack Query**: Server state management
- **Vite**: Build tool and dev server

### Structure

```
apps/desktop/
├── src/                     # React frontend
│   ├── components/
│   │   ├── Login.tsx        # OAuth login component
│   │   └── UserInfo.tsx     # User profile display
│   ├── hooks/
│   │   └── useAuth.tsx      # Authentication state management
│   ├── App.tsx              # Root component
│   ├── main.tsx             # App entry point
│   ├── index.html           # HTML template
│   └── index.css            # Tailwind imports
├── src-tauri/               # Rust backend
│   ├── src/
│   │   ├── main.rs          # Tauri app entry + system tray
│   │   ├── config.rs        # Config file operations
│   │   └── commands.rs      # Tauri commands for frontend
│   ├── Cargo.toml           # Rust dependencies
│   ├── tauri.conf.json      # Tauri configuration
│   └── icons/               # App icons
├── package.json
├── tailwind.config.js       # DaisyUI + custom theme
├── vite.config.ts
└── tsconfig.json
```

### CSS Synchronization

**CRITICAL**: The `src/index.css` file must be kept in sync with `apps/server/src/ui/index.css`.

**Required to match:**
- Theme definitions (guideai-light and guideai-dark)
- All CSS custom properties and color values
- Base styles and border compatibility rules
- Main gradient definitions

**Allowed differences:**
- Server has drizzle-cube imports: `@import 'drizzle-cube/client/styles.css'` and `@source`
- Server has modal z-index override layer for drizzle-cube compatibility

When updating theme colors or base styles, **always update both files** to maintain visual consistency across desktop and server apps.

## Configuration

### Shared Config with CLI
- **Location**: `~/.guideai/config.json`
- **Format**: JSON with camelCase fields
- **Permissions**: 600 (read/write for owner only)
- **Structure**:
  ```json
  {
    "apiKey": "string",
    "serverUrl": "string",
    "username": "string",
    "tenantId": "string",
    "tenantName": "string"
  }
  ```

### Tauri Configuration
- **System Tray**: Always visible in menubar
- **Window**: Hidden by default, shows on tray click
- **Positioning**: Centered below tray icon
- **Behavior**: Click outside to hide
- **Platform**: macOS private APIs enabled

## Commands

### Development

```bash
# Start frontend dev server only
pnpm dev

# Start full Tauri app with hot reload
pnpm tauri:dev

# Start from workspace root
pnpm desktop:dev
```

### Build

```bash
# Build frontend only
pnpm build

# Build full Tauri app (current platform)
pnpm tauri:build

# Build from workspace root
pnpm desktop:build

# Platform-specific builds
pnpm tauri:build:windows     # Windows x64
pnpm tauri:build:macos       # macOS ARM64
pnpm tauri:build:linux       # Linux x64
```

### Local macOS Release Build
To run a full local build, sign, notarize, and upload for macOS, use the `build-macos-local.sh` script. This script is designed to replicate the GitHub Actions release workflow on your local machine.

**Prerequisites:**
- Ensure you have all the required environment variables set in `apps/server/.dev.vars`. The script will validate them for you.
- You need to have `wrangler` and `openssl` installed and available in your `PATH`.

**Usage:**
```bash
# Run the script from the project root
./scripts/build-macos-local.sh [version]
```
- If `[version]` is not provided, it will be automatically detected from `apps/desktop/package.json`.

**What it does:**
1.  **Builds** the universal macOS binary.
2.  **Signs** the application using your Apple Developer certificate.
3.  **Notarizes** the `.dmg` with Apple.
4.  **Uploads** the final artifacts to R2.
5.  **Generates** and uploads the updater manifest.


### Cross-Platform Build Requirements

#### Windows
To build for Windows from macOS/Linux, you need:
1. Install Windows cross-compilation target:
   ```bash
   rustup target add x86_64-pc-windows-msvc
   ```
2. Install additional tools (varies by platform)
3. Run: `pnpm tauri:build:windows`

**Note**: Cross-compiling to Windows from non-Windows platforms can be complex. For best results, build on native Windows or use CI/CD (GitHub Actions).

#### macOS
To build for macOS from other platforms:
1. Install target: `rustup target add aarch64-apple-darwin`
2. Requires macOS SDK (complex on non-macOS)
3. Best approach: Use native macOS or GitHub Actions

#### Linux
To build for Linux from other platforms:
1. Install target: `rustup target add x86_64-unknown-linux-gnu`
2. May need cross-compilation tools
3. Run: `pnpm tauri:build:linux`

### Other Commands

```bash
# Type checking
pnpm typecheck

# Clean artifacts
pnpm clean
```

## Features

### Authentication
- **Login**: Opens browser for GitHub OAuth
- **Logout**: Clears stored credentials
- **State Management**: Shared with CLI via config file
- **Auto-refresh**: Queries config changes

### User Interface
- **Menubar Integration**: Native system tray icon
- **Responsive Design**: Compact 350x400 window
- **Theme**: Custom GuideAI theme with DaisyUI
- **Loading States**: Skeleton screens and spinners

### System Integration
- **Cross-platform**: Windows, macOS, Linux
- **Native Performance**: Rust backend
- **System Tray**: Click to show/hide
- **Focus Management**: Auto-hide on focus loss

## Tauri Commands

### Config Operations
- `load_config_command()`: Read config from disk
- `save_config_command(config)`: Write config to disk
- `clear_config_command()`: Reset config to defaults

### Authentication
- `login_command(serverUrl)`: Open browser OAuth flow
- `logout_command()`: Clear stored credentials

## Dependencies

### Frontend
- `@guideai/types`: Shared TypeScript definitions
- `@heroicons/react`: Icon components
- `@tanstack/react-query`: Data fetching and caching
- `@tauri-apps/api`: Tauri frontend bindings
- `react` + `react-dom`: UI framework

### Styling
- `tailwindcss`: Utility-first CSS
- `daisyui`: Component library
- `autoprefixer`: CSS vendor prefixes

### Backend (Rust)
- `tauri`: Desktop app framework
- `tauri-plugin-positioner`: Window positioning
- `serde` + `serde_json`: Serialization
- `dirs`: Cross-platform directories
- `tokio`: Async runtime

## Development Workflow

### Running Desktop App

```bash
# Install dependencies
pnpm install

# Start development
pnpm tauri:dev

# Build for production
pnpm tauri:build
```

### Debugging

- **Frontend**: Browser DevTools (F12 in dev mode)
- **Backend**: Rust logs in terminal
- **Config**: Check `~/.guideai/config.json`

## Key Architectural Decisions

1. **Shared Config**: Same config file as CLI for seamless experience
2. **Menubar Design**: Compact, always-accessible interface
3. **Native Performance**: Rust backend for file operations
4. **OAuth Integration**: Browser-based authentication flow
5. **Responsive UI**: Adapts to small menubar window
6. **Auto-hide**: Unobtrusive user experience

## Platform Considerations

### macOS
- **Private APIs**: Enabled for advanced window management
- **System Tray**: Native menubar integration
- **Permissions**: File system access for config
- **Paths**: Uses `~/.guideai/` for config, provider-specific paths for data
  - Claude Code: `~/.claude/projects/`
  - OpenCode: `~/.local/share/opencode/storage/`
  - Codex: `~/.codex/config.toml`

### Windows
- **System Tray**: Notification area integration
- **Window Management**: Focus and positioning
- **Path Resolution**: Cross-platform path handling via `shellexpand` and `dirs` crates
- **Paths**: Platform-specific defaults
  - Config: `C:\Users\<username>\.guideai\`
  - Claude Code: `C:\Users\<username>\.claude\projects\` (WSL compatible)
  - OpenCode: `C:\Users\<username>\AppData\Local\opencode\` (via `%LOCALAPPDATA%`)
  - Codex: `C:\Users\<username>\.codex\config.toml` (WSL compatible)
- **Important**: Claude Code requires WSL (Windows Subsystem for Linux) on Windows, so paths may be WSL Linux paths

### Linux
- **System Tray**: Desktop environment integration
- **File Permissions**: Unix-style config security
- **Paths**: XDG Base Directory specification compatible
  - Claude Code: `~/.claude/projects/`
  - OpenCode: `~/.local/share/opencode/storage/` or `$XDG_DATA_HOME/opencode/`
  - Codex: `~/.codex/config.toml`

## Integration with GuideAI Ecosystem

- **CLI Compatibility**: Shares authentication state
- **Server Communication**: Uses same OAuth endpoints
- **Type Safety**: Shared types from `@guideai/types`
- **Consistent UI**: Matches server app design language

## Future Enhancements

- **File Scanning**: Background file processing
- **Notifications**: Status updates and alerts
- **Quick Actions**: Upload and manage files
- **Settings Panel**: Advanced configuration options
- **Auto-updates**: Seamless app updates