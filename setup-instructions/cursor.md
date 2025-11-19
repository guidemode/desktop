# Cursor CLI Setup

## Installation

**Important**: GuideMode currently supports **Cursor CLI** (the command-line tool), not the Cursor IDE.

### Install Cursor CLI

1. **Visit the official CLI page**: [cursor.com/cli](https://cursor.com/cli)
2. **Follow the installation instructions** for your operating system
3. **Authenticate** with your Cursor account

### Verify Installation

After installation, verify Cursor CLI is working:

```bash
cursor --version
```

### Default Location

Cursor CLI stores sessions in:
- **macOS/Linux**: `~/.cursor/chats/`
- **Windows**: `~/.cursor/chats/`

GuideMode will automatically detect sessions in this directory once you've started using Cursor CLI.

## Note

This integration is for **Cursor CLI** only. Support for the Cursor IDE may be added in a future release.
