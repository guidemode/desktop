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
│   │   ├── main.rs          # Tauri app entry + system tray + event initialization
│   │   ├── config.rs        # Config file operations
│   │   ├── commands.rs      # Tauri commands for frontend
│   │   ├── database.rs      # Database operations with transactions
│   │   ├── events/          # Event-driven architecture (NEW)
│   │   │   ├── mod.rs       # Event exports
│   │   │   ├── types.rs     # Event type definitions
│   │   │   ├── bus.rs       # EventBus implementation
│   │   │   └── handlers.rs  # Database & frontend event handlers
│   │   ├── providers/       # File watchers for AI providers
│   │   │   ├── claude_watcher.rs
│   │   │   ├── copilot_watcher.rs
│   │   │   ├── opencode_watcher.rs
│   │   │   ├── codex_watcher.rs
│   │   │   ├── gemini_watcher.rs
│   │   │   └── db_helpers.rs
│   │   ├── upload_queue/    # Async upload processing
│   │   ├── types.rs         # Type safety wrappers (NEW)
│   │   ├── shutdown.rs      # Graceful shutdown coordinator (NEW)
│   │   └── ...
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

## Rust Backend Architecture

The desktop application uses an **event-driven architecture** to decouple components and ensure reliable data flow. This was implemented as part of a comprehensive Rust improvements initiative (see `RUST_IMPROVEMENTS_PLAN.md`).

### Event-Driven Architecture

#### Core Components

1. **EventBus** (`src/events/bus.rs`)
   - Publish-subscribe pattern using `tokio::sync::broadcast`
   - Distributes session events to multiple handlers
   - Sequenced events for ordering guarantees
   - 1000-event buffer capacity

2. **Event Types** (`src/events/types.rs`)
   - `SessionEvent`: Main event wrapper with sequence, timestamp, provider
   - `SessionEventPayload`: Enum of event types
     - `SessionChanged`: New/updated session detected
     - `Completed`: Session finished (has end time)
     - `Failed`: Session processing failed

3. **Event Handlers** (`src/events/handlers.rs`)
   - `DatabaseEventHandler`: Writes events to SQLite database
   - `FrontendEventHandler`: Emits Tauri events to React UI
   - Both support graceful shutdown via `ShutdownCoordinator`

#### Data Flow

```
Provider Watcher                EventBus                 Handlers
    (Claude,                                           (Database,
     Copilot,          ┌──────────────────┐            Frontend)
     etc.)             │                  │
       │               │  SessionEvent    │
       │  publish()    │  ┌────────────┐  │
       ├──────────────►│  │ Sequence   │  │
       │               │  │ Timestamp  │  │──subscribe()──┐
       │               │  │ Provider   │  │               │
       │               │  │ Payload    │  │               ▼
       │               │  └────────────┘  │         DatabaseEventHandler
       │               │                  │               │
       │               └──────────────────┘               │
       │                                                  ├─► insert_session()
       │                                                  │
       │                                                  ▼
       │                                            FrontendEventHandler
       │                                                  │
       │                                                  └─► emit("session-updated")
       │
     File System
```

**Benefits:**
- Watchers don't directly touch database (loose coupling)
- Multiple handlers process same event (extensibility)
- Async processing without blocking watchers
- Easy to add metrics, logging, or analytics handlers

### Database Layer

#### Transaction Safety (`src/database.rs`)

All database operations use **transactions** to prevent race conditions:

```rust
// Example: update_session() uses transaction for atomicity
with_connection_mut(|conn| {
    let tx = conn.transaction()?;

    // Read existing data
    let (existing_start, existing_cwd, ...) = tx.query_row(...)?;

    // Modify and write
    tx.execute("UPDATE ...", ...)?;

    // Commit atomically
    tx.commit()?;
})
```

**Key Improvements:**
- Atomic read-modify-write operations
- No lost updates from concurrent threads
- Consistent state across operations

#### Connection Management

- **Retry Logic**: 3 attempts with 100ms delay between retries
- Handles temporary SQLite lock contention
- Global static connection with `Mutex` protection

```rust
// Retry logic for database connection
fn get_db_connection() -> Result<MutexGuard<'static, Option<Connection>>> {
    const MAX_RETRIES: u32 = 3;
    const RETRY_DELAY: Duration = Duration::from_millis(100);

    for attempt in 0..MAX_RETRIES {
        match DB_CONNECTION.lock() {
            Ok(guard) => return Ok(guard),
            Err(_) if attempt < MAX_RETRIES - 1 => {
                std::thread::sleep(RETRY_DELAY);
                continue;
            }
            Err(_) => return Err(rusqlite::Error::InvalidQuery),
        }
    }
    unreachable!()
}
```

### Type Safety (`src/types.rs`)

Newtype wrappers provide compile-time safety for domain types:

```rust
pub struct SessionId(String);  // Can't mix with ProjectId
pub struct ProjectId(String);  // Type-safe identifiers
```

**Benefits:**
- Prevent accidentally passing wrong ID type
- Better IDE autocomplete and type checking
- Self-documenting function signatures
- Available for gradual adoption across codebase

### Graceful Shutdown (`src/shutdown.rs`)

The `ShutdownCoordinator` ensures clean application exit:

```rust
// Initialized at startup
let shutdown = ShutdownCoordinator::new();

// Event handlers listen for shutdown
tokio::select! {
    event = event_rx.recv() => { /* process */ }
    _ = shutdown_rx.recv() => {
        // Graceful shutdown - flush events, close connections
        break;
    }
}
```

**Features:**
- Broadcast-based coordination
- Handlers can finish in-flight work
- No lost events on exit
- Supports multiple subscribers

### Canonical Format Architecture

**All providers convert to a single, unified JSONL format for downstream processing.**

#### Core Concept

**Location:** `src/providers/canonical/mod.rs`

The canonical format is GuideAI's universal message format that all AI providers (Claude Code, Gemini, Copilot, Codex, OpenCode) convert to. This simplifies downstream processing—instead of 5+ provider-specific parsers, we have one canonical parser.

**Key Benefits:**
- Single TypeScript parser handles all providers
- Consistent processing and metrics across providers
- Easy to add new providers (only Rust converter needed)
- Provider-specific features preserved in `providerMetadata`

#### Message Structure

```rust
pub struct CanonicalMessage {
    uuid: String,              // Unique message ID
    timestamp: String,         // ISO 8601
    message_type: MessageType, // user, assistant, meta
    session_id: String,
    provider: String,          // "claude-code", "gemini-code", etc.

    // Context
    cwd: Option<String>,
    git_branch: Option<String>,
    version: Option<String>,

    // Core content
    message: MessageContent,

    // Preserve provider-specific data
    provider_metadata: Option<Value>,
}
```

**Content Blocks:**
- `Text` - Simple text messages
- `ToolUse` - Function calls, commands
- `ToolResult` - Function outputs
- `Thinking` - Extended reasoning (Gemini, Claude)

#### Provider Converters

Each provider implements the `ToCanonical` trait:

```rust
pub trait ToCanonical {
    fn to_canonical(&self) -> Result<Option<CanonicalMessage>>;
    fn provider_name(&self) -> &str;
}
```

**Implementations:** `src/providers/{provider}/converter.rs`

**Canonical Output:** `~/.guideai/sessions/{provider}/{project}/{session}.jsonl`

#### Adding New Providers

**Example: Adding "cursor-ai"**

1. **Create converter** - `src/providers/cursor/converter.rs`:
   ```rust
   impl ToCanonical for CursorMessage {
       fn to_canonical(&self) -> Result<Option<CanonicalMessage>> {
           // Map native format to canonical structure
           // Preserve cursor-specific fields in provider_metadata
       }
   }
   ```

2. **Create watcher** - `src/providers/cursor_watcher.rs`:
   ```rust
   // Watch Cursor session files
   // Parse to CursorMessage
   // Convert using to_canonical()
   // Write to canonical path
   // Publish SessionEvent
   ```

3. **Register provider** - `src/providers/mod.rs`:
   ```rust
   pub mod cursor;
   mod cursor_watcher;
   ```

4. **Update TypeScript** - `packages/session-processing/src/parsers/registry.ts`:
   ```typescript
   const providerAliases = [..., 'cursor-ai']
   ```

**That's it!** The canonical parser automatically handles the new provider.

See `src/providers/canonical/converter.rs` for the trait definition and helper functions.

### Provider Watchers

Each AI provider (Claude, Copilot, OpenCode, Codex, Gemini) has a dedicated file watcher:

**Architecture:**
- Monitor provider-specific file paths
- Detect session file changes (size, timestamp)
- **Convert native format to canonical JSONL** using `ToCanonical` trait
- Write to canonical path (`~/.guideai/sessions/{provider}/{project}/`)
- Extract metadata (CWD, git info, timing)
- Publish `SessionEvent` to EventBus
- Track session state in memory

**Example: Claude Watcher**
```rust
// Parse native format
let entry = parse_claude_entry(&line)?;

// Convert to canonical
let canonical = entry.to_canonical()?;

// Write to canonical path
write_canonical_message(&canonical_path, &canonical)?;

// Publish event
if is_new_session {
    event_bus.publish("claude", SessionEventPayload::SessionChanged {
        session_id,
        project_name,
        file_path: canonical_path, // Points to canonical JSONL
        file_size,
    })?;
}
```

### Upload Queue (`src/upload_queue/`)

Asynchronous upload processing with modular architecture (see `upload_queue/CLAUDE.md`):

- **Retry Logic**: Exponential backoff for server/network errors
- **Deduplication**: SHA256 hashing prevents duplicate uploads
- **Concurrency**: Max 3 parallel uploads
- **Validation**: JSONL timestamp checking
- **Compression**: Gzip for efficient transfer

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

### Environment Variables

The desktop app uses Vite's environment variable system with the following files:

- **`.env`** - Shared defaults (committed to git)
  - `VITE_SERVER_URL=https://be.guideai.dev`
  - Loaded in all modes (dev and production)

- **`.env.local`** - Local development overrides (NOT committed)
  - Your personal server URL for development
  - Example: `VITE_SERVER_URL=https://clifton.guideai.dev`
  - Takes priority over `.env`
  - Copy from `.env.example` to get started

- **`.env.production`** - Production-specific settings (committed)
  - `VITE_SERVER_URL=https://be.guideai.dev`
  - Only loaded during `vite build` (production builds)

**Loading Priority (highest to lowest):**
1. Existing shell environment variables (highest)
2. `.env.[mode].local` (e.g., `.env.development.local`)
3. `.env.local`
4. `.env.[mode]` (e.g., `.env.development` or `.env.production`)
5. `.env` (lowest)

**For local development:**
1. Copy `.env.example` to `.env.local`
2. Edit `.env.local` with your preferred server URL
3. Restart the dev server (`pnpm tauri:dev`)

**Note:** Environment variable changes require a dev server restart to take effect.

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

### Quality Checks

```bash
# Type checking (TypeScript + Rust)
pnpm typecheck

# Linting (TypeScript + Rust)
pnpm lint

# Testing (TypeScript + Rust)
pnpm test

# Clean artifacts
pnpm clean
```

## Development Workflow

**IMPORTANT: Always run quality checks locally before committing changes.**

### Pre-Commit Checklist

Run these commands in the **desktop app directory** (`apps/desktop/`) before committing:

```bash
# 1. Type checking (REQUIRED - zero errors)
# Checks both TypeScript and Rust code
pnpm typecheck

# 2. Linting (REQUIRED - zero errors)
# Lints both TypeScript and Rust code
pnpm lint

# 3. Building (REQUIRED - must succeed)
pnpm build

# 4. Testing (REQUIRED when tests exist)
# Runs both TypeScript (Vitest) and Rust (Cargo) tests
pnpm test
```

### Quick Quality Check

Run all checks in sequence:

```bash
# Full quality check (TypeScript + Rust)
pnpm typecheck && pnpm lint && pnpm build && pnpm test
```

**If any check fails, your code MUST NOT be committed. Fix all errors before proceeding.**

### TypeScript-Specific Checks

```bash
# Type check only TypeScript
pnpm typecheck:ts

# Lint only TypeScript
pnpm lint:ts

# Test only TypeScript (frontend React code)
pnpm test:ts

# Watch mode for TypeScript tests
pnpm test:watch
```

### Rust-Specific Checks

```bash
# Type check only Rust (cargo check + clippy)
pnpm typecheck:rust

# Lint only Rust (clippy)
pnpm lint:rust

# Test only Rust (backend Tauri code)
pnpm test:rust

# Format Rust code
pnpm format:rust
```

### Testing Guidelines

- **Test new features**: Add tests for all new functionality
  - **TypeScript**: React component tests with Testing Library
  - **Rust**: Unit tests and integration tests with `#[cfg(test)]`
- **Keep it pragmatic**: Focus on core functionality and edge cases
- **Use existing patterns**:
  - **TypeScript**: Leverage existing test setup in `vitest.config.ts`
  - **Rust**: Follow existing test patterns in `src-tauri/src/*/tests.rs`
- **Run locally first**: Always run tests before pushing

### Code Quality Standards

- **Zero tolerance**: No lint errors, type errors, or test failures allowed in commits
- **Type safety**:
  - **TypeScript**: Proper types throughout (no `any` without justification)
  - **Rust**: Proper error handling with `Result<T, E>`, avoid `unwrap()` in production code
- **Test coverage**: Core functionality must be tested
- **Consistent style**:
  - **TypeScript**: Biome enforces consistent formatting
  - **Rust**: `cargo fmt` enforces Rust conventions

### From Workspace Root

To check the desktop app from the workspace root:

```bash
pnpm --filter @guideai-dev/desktop typecheck
pnpm --filter @guideai-dev/desktop lint
pnpm --filter @guideai-dev/desktop test
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

1. **Event-Driven Architecture**: Decoupled components communicate via EventBus
   - Watchers publish events, handlers consume them
   - Enables extensibility without modifying existing code
   - Clean separation of concerns (file watching vs database vs UI)

2. **Transaction Safety**: All database operations use SQLite transactions
   - Prevents race conditions in concurrent access
   - Ensures data consistency across operations
   - Atomic read-modify-write patterns

3. **Graceful Shutdown**: Coordinated shutdown across async components
   - No lost events or incomplete operations
   - Handlers finish in-flight work before exit
   - Clean resource cleanup

4. **Shared Config**: Same config file as CLI for seamless experience

5. **Menubar Design**: Compact, always-accessible interface

6. **Native Performance**: Rust backend for file operations and concurrent processing

7. **OAuth Integration**: Browser-based authentication flow

8. **Responsive UI**: Adapts to small menubar window

9. **Auto-hide**: Unobtrusive user experience

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

## Completed Improvements

See `RUST_IMPROVEMENTS_PLAN.md` for full details of the 4-phase improvement initiative.

✅ **Phase 1: Critical Fixes** - Transaction safety for database operations
  - Atomic read-modify-write operations prevent race conditions
  - No lost updates from concurrent threads
  - Consistent state across all database operations

✅ **Phase 2: Code Deduplication** - Extracted common provider watcher code
  - Created `providers/common/` module with shared utilities
  - `SessionState` module for unified session tracking logic
  - `file_utils.rs` for common file filtering and validation
  - `watcher_status.rs` for generic status types
  - `constants.rs` for timing/size thresholds
  - Eliminated ~900 lines of duplicated code across 5 providers

✅ **Phase 3: Event-Driven Architecture** - Decoupled components via EventBus
  - Watchers publish events, handlers consume them
  - Enables extensibility without modifying existing code
  - Clean separation: file watching → database → UI
  - Fixed session state tracking bugs

✅ **Phase 4: Type Safety & Polish** - Production-ready reliability
  - SessionId and ProjectId newtypes for compile-time safety
  - Database connection retry logic (3 attempts, 100ms delay)
  - Graceful shutdown coordination for clean exits
  - Event handlers flush in-flight work before shutdown

✅ **Upload Queue Refactoring** - Modular, maintainable architecture
  - Concurrent uploads with retry and deduplication
  - SHA256 hashing prevents duplicate uploads
  - Exponential backoff for server/network errors
  - 69 tests, all passing

## Future Enhancements

### Observability & Monitoring
- **Metrics & Telemetry**: Add observability for upload success rates and performance
- **Event Persistence**: Optional EventStore for debugging and event replay
- **Persistent Queue**: Crash recovery for upload queue

### User Experience
- **DB Trigger/Notification**: Replace polling with reactive updates
- **Desktop Notifications**: Status updates for session completion and upload status
- **Settings Panel**: Advanced configuration UI
- **Auto-updates**: Seamless app updates (partially implemented)