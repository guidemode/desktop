> [!WARNING]
> **This project is retired and no longer maintained.**
>
> Session capture now happens through the **GuideMode CLI**, which installs hooks into your
> AI coding agent — there is no app to keep running:
>
> ```bash
> npm install -g guidemode
> guidemode setup
> ```
>
> See [github.com/guidemode/cli](https://github.com/guidemode/cli) and
> [docs.guidemode.dev](https://docs.guidemode.dev/guides/cli/overview/).
>
> This repository is kept read-only for history. The desktop app will continue to work for
> as long as its server endpoints remain, but it will receive no further releases or fixes.

# GuideMode Desktop

> **Your AI coding sessions, captured automatically.**

![GuideMode Desktop Session Detail](https://www.guidemode.dev/session_detail.png)

A lightweight menubar app that watches your AI coding sessions and uploads them to GuideMode for analytics. Works with Claude Code, Gemini, GitHub Copilot, Codex, and OpenCode.

## Why Use This?

**🔄 Automatic Capture** - No manual logging. The app watches your AI tool sessions automatically.

**🔒 Privacy First** - Choose what to sync:
- **No Sync**: Keep everything local
- **Metrics Only**: Just usage stats, no code
- **Full Transcript**: Complete session history

**📊 Unified Analytics** - All your AI tools in one place. Compare effectiveness, track costs, identify patterns.

**⚡ Lightweight** - Built in Rust. Uses minimal resources.

## Supported AI Tools

- ✅ **Claude Code** - Anthropic
- ✅ **Gemini Code** - Google
- ✅ **GitHub Copilot** - GitHub
- ✅ **Codex** - AI assistant
- ✅ **OpenCode** - Open source

## Installation

### Download

- 🍎 [**macOS**](https://install.guidemode.dev/desktop/latest/GuideMode-Desktop-macOS.dmg) (Universal - Intel & Apple Silicon)
- 🪟 [**Windows**](https://install.guidemode.dev/desktop/latest/GuideMode-Desktop-windows.msi) (Windows 10+)
- 🐧 [**Linux**](https://install.guidemode.dev/desktop/latest/GuideMode-Desktop-linux.deb) (.deb)

### First Launch

1. Install and open the app
2. Click the menubar icon
3. Sign in with GitHub
4. Configure which AI tools to watch
5. Start coding!

That's it. GuideMode handles the rest automatically.

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
git clone https://github.com/guidemode/desktop.git
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

Config file: `~/.guidemode/config.json`

```json
{
  "apiKey": "your-api-key",
  "serverUrl": "https://app.guidemode.dev",
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

- 🐛 [**Report Issues**](https://github.com/guidemode/desktop/issues)
- 💬 [**Discussions**](https://github.com/guidemode/desktop/discussions)
- 📧 **Email**: support@guidemode.dev
- 📚 **Docs**: https://docs.guidemode.dev

## Related Packages

Part of the GuideMode ecosystem:

- [@guidemode/session-processing](https://github.com/guidemode/session-processing) - Analytics engine
- [@guidemode/types](https://github.com/guidemode/types) - Shared types
- [@guidemode/cli](https://github.com/guidemode/cli) - Command-line tool

## License

MIT License - see [LICENSE](LICENSE) file for details.
