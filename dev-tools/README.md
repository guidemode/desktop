# GuideAI Desktop Dev Tools

Development and debugging utilities for the GuideAI desktop app, focused on Cursor provider analysis.

## Tools

- **cursor_analysis** - Analyze Cursor session files
- **cursor_protobuf_decoder** - Decode Protocol Buffer data from Cursor logs
- **inspect_cursor** - Inspect Cursor database and file structures
- **cursor_hex_inspector** - Hex dump and analyze Cursor binary data
- **cursor_blob_analyzer** - Analyze Cursor blob storage

## Building

These tools are separate from the main Tauri app to avoid build conflicts during desktop builds.

```bash
# Build all tools
cd apps/desktop/dev-tools
cargo build --release

# Build a specific tool
cargo build --release --bin cursor_analysis

# Run a tool directly
cargo run --release --bin cursor_analysis -- [args]
```

## Output

Compiled binaries will be in `target/release/`:
- `cursor_analysis`
- `cursor_protobuf_decoder`
- `inspect_cursor`
- `cursor_hex_inspector`
- `cursor_blob_analyzer`

## Why Separate?

These tools were originally `[[bin]]` targets in the main Cargo.toml, but Tauri v2 attempts to bundle ALL binary targets into the app bundle, even when using `required-features`. Moving them to a separate workspace prevents this issue while keeping the tools available for development.
