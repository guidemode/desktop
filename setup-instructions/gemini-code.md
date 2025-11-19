# Gemini Code Setup

## Installation

  1. Visit: [https://github.com/google-gemini/gemini-cli](https://github.com/google-gemini/gemini-cli)
  2. After installation run `gemini` and authenticate using your Google account or key.

## Critical Configuration Required

For GuideMode to properly track Gemini sessions, you **must** configure the following settings in Gemini Code, you can do this via the `/settings` command in a session:

1. **Session Retention**: true

2. **Output Format**: json

### Configuration File

These settings should result in your `~/.gemini/settings.json` containing entries:

```json
{
  "general": {
    "sessionRetention": {
      "enabled": true
    }
  },
  "output": {
    "format": "json"
  }
}
```

⚠️ **Without these settings, GuideMode will not be able to track your Gemini sessions correctly.**
