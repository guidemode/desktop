# GuideAI Desktop

Cross-platform desktop menubar application for monitoring and analyzing AI coding agent sessions.

## Features

- **Session Monitoring**: Track Claude Code, OpenCode, Codex, and other AI coding agents
- **Real-time Processing**: Analyze sessions with AI-powered insights
- **Metrics Dashboard**: View detailed performance and usage statistics
- **Secure Authentication**: OAuth integration with GuideAI server
- **Cross-platform**: Works on macOS, Windows, and Linux

## Installation

### Download Pre-built Binaries

Download the latest release for your platform:

- **macOS**: [GuideAI-Desktop-macOS.dmg](https://downloads.guideai.dev/desktop/latest/GuideAI-Desktop-macOS.dmg)
- **Windows**: [GuideAI-Desktop-windows.msi](https://downloads.guideai.dev/desktop/latest/GuideAI-Desktop-windows.msi)
- **Linux**:
  - [GuideAI-Desktop-linux.deb](https://downloads.guideai.dev/desktop/latest/GuideAI-Desktop-linux.deb) (Debian/Ubuntu)
  - [GuideAI-Desktop-linux.AppImage](https://downloads.guideai.dev/desktop/latest/GuideAI-Desktop-linux.AppImage) (Universal)

### Build from Source

Requirements:
- Node.js >= 24.0.0
- pnpm >= 9.0.0
- Rust and Cargo

```bash
# Clone the repository
git clone https://github.com/guideai-dev/desktop.git
cd desktop

# Install dependencies
pnpm install

# Build dependencies
pnpm --filter @guideai-dev/types build
pnpm --filter @guideai-dev/session-processing build

# Run in development
pnpm tauri:dev

# Build for production
pnpm tauri:build
```

## Usage

1. **Launch the app**: The app appears in your system tray/menubar
2. **Login**: Click the tray icon and login with GitHub OAuth
3. **Configure watchers**: Set up file watchers for your AI agent directories
4. **Monitor sessions**: Sessions are automatically detected and uploaded for processing

## Architecture

- **Frontend**: React 18 + TypeScript + Tailwind CSS + DaisyUI
- **Backend**: Tauri (Rust) for native system integration
- **Database**: SQLite for local session storage
- **State**: Zustand for React state management

## Configuration

Config file location: `~/.guideai/config.json`

Example:
```json
{
  "apiKey": "your-api-key",
  "serverUrl": "https://guideai.dev",
  "username": "your-username",
  "tenantId": "your-tenant-id",
  "tenantName": "Your Team"
}
```

## Development

This repository is automatically synced from the [GuideAI private monorepo](https://github.com/guideai-dev/guideai).

### Contributing

We welcome contributions! Please follow these steps:

1. Fork this repository
2. Create a feature branch
3. Make your changes
4. Submit a pull request

**Note**: All pull requests are reviewed and manually backported to the private monorepo. This ensures security and compatibility with the broader GuideAI ecosystem.

### Local Development

```bash
# Start development server (frontend only)
pnpm dev

# Start full Tauri app with hot reload
pnpm tauri:dev

# Type checking
pnpm typecheck

# Clean build artifacts
pnpm clean
```

## Platform-Specific Notes

### macOS
- Requires macOS 10.15+
- Apple Silicon and Intel supported (universal binary)
- Config: `~/.guideai/config.json`

### Windows
- Requires Windows 10+
- Config: `C:\Users\<username>\.guideai\config.json`
- **Note**: Claude Code requires WSL on Windows

### Linux
- Requires modern Linux distribution
- GTK dependencies required (WebKit2GTK)
- Config: `~/.guideai/config.json`

## License

MIT License - see [LICENSE](LICENSE) file for details.

## Links

- [GuideAI Website](https://guideai.dev)
- [Documentation](https://docs.guideai.dev)
- [GitHub Organization](https://github.com/guideai-dev)
- [Related Packages](#related-packages)

## Related Packages

This desktop app depends on other GuideAI open source packages:

- [@guideai-dev/types](https://github.com/guideai-dev/types) - Shared TypeScript types
- [@guideai-dev/session-processing](https://github.com/guideai-dev/session-processing) - Session processing and AI models
- [@guideai-dev/cli](https://github.com/guideai-dev/cli) - Command-line interface

## Support

- **Issues**: [GitHub Issues](https://github.com/guideai-dev/desktop/issues)
- **Discussions**: [GitHub Discussions](https://github.com/guideai-dev/desktop/discussions)
- **Email**: support@guideai.dev
