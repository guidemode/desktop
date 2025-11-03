# GuideAI Desktop

> **Your AI coding sessions, captured automatically.**

![GuideAI Desktop Screenshot](https://via.placeholder.com/800x500?text=Desktop+App+Screenshot) <!-- TODO: Replace with actual screenshot -->

A lightweight menubar app that watches your AI coding sessions and uploads them to GuideAI for analytics. Works with Claude Code, Gemini, GitHub Copilot, Codex, and OpenCode.

## Why Use This?

**🔄 Automatic Capture** - No manual logging. The app watches your AI tool sessions automatically.

**🔒 Privacy First** - Choose what to sync:
- **No Sync**: Keep everything local
- **Metrics Only**: Just usage stats, no code
- **Full Transcript**: Complete session history

**📊 Unified Analytics** - All your AI tools in one place. Compare effectiveness, track costs, identify patterns.

**⚡ Lightweight** - Runs quietly in your menubar. Uses minimal resources.

## Supported AI Tools

![Supported Providers](https://via.placeholder.com/600x150?text=Provider+Logos+Here) <!-- TODO: Add logos -->

- ✅ **Claude Code** - Anthropic
- ✅ **Gemini Code** - Google
- ✅ **GitHub Copilot** - GitHub
- ✅ **Codex** - AI assistant
- ✅ **OpenCode** - Open source

## Installation

### Download

- 🍎 [**macOS**](https://downloads.guideai.dev/desktop/latest/) (Universal - Intel & Apple Silicon)
- 🪟 [**Windows**](https://downloads.guideai.dev/desktop/latest/) (Windows 10+)
- 🐧 [**Linux**](https://downloads.guideai.dev/desktop/latest/) (.deb / .AppImage)

### First Launch

1. Install and open the app
2. Click the menubar icon
3. Sign in with GitHub
4. Configure which AI tools to watch
5. Start coding!

That's it. GuideAI handles the rest automatically.

## Features

- **Automatic Detection** - Watches AI tool directories for new sessions
- **Smart Conversion** - Converts all formats to a unified structure
- **Intelligent Sync** - Uploads only when needed, with retry logic
- **Local Dashboard** - View sessions even without sync
- **Secure Auth** - GitHub OAuth integration
- **Cross-Platform** - macOS, Windows, and Linux

## For Developers

### Build from Source

**Requirements:**
- Node.js >= 24.0.0
- pnpm >= 9.0.0
- Rust (via rustup)

**Setup:**
```bash
git clone https://github.com/guideai-dev/desktop.git
cd desktop
pnpm install
pnpm tauri:dev
```

**See [CLAUDE.md](CLAUDE.md) for:**
- Complete development setup
- Architecture documentation
- How to add new AI provider support
- Testing and debugging

### Key Technologies

- **Frontend**: React 18 + TypeScript + Tailwind CSS
- **Backend**: Rust + Tauri
- **Database**: SQLite (local storage)
- **Build**: Vite + Cargo

## Configuration

Config file: `~/.guideai/config.json`

```json
{
  "apiKey": "your-api-key",
  "serverUrl": "https://be.guideai.dev",
  "username": "your-username",
  "tenantId": "your-tenant-id"
}
```

## Platform Notes

**macOS:**
- Requires macOS 10.15+
- Universal binary (Intel + Apple Silicon)

**Windows:**
- Requires Windows 10+
- Note: Claude Code needs WSL

**Linux:**
- GTK dependencies required
- Works on most modern distributions

## Support

- 🐛 [**Report Issues**](https://github.com/guideai-dev/desktop/issues)
- 💬 [**Discussions**](https://github.com/guideai-dev/desktop/discussions)
- 📧 **Email**: support@guideai.dev
- 📚 **Docs**: https://docs.guideai.dev

## Related Packages

Part of the GuideAI ecosystem:

- [@guideai-dev/session-processing](https://github.com/guideai-dev/session-processing) - Analytics engine
- [@guideai-dev/types](https://github.com/guideai-dev/types) - Shared types
- [@guideai-dev/cli](https://github.com/guideai-dev/cli) - Command-line tool

## License

MIT License - see [LICENSE](LICENSE) file for details.

---

Built with ❤️ by the GuideAI team
