# Native Streaming

spotatui includes **native Spotify Connect** support, allowing it to play audio directly on your computer without needing an external player like spotifyd.

## Setup

The native streaming feature uses a separate authentication flow. On first run:

1. Your browser will open to Spotify's authorization page
2. **Important:** The redirect URI will be `http://127.0.0.1:8989/login` - this is different from the main app's callback URL
3. After authorizing, "spotatui" will appear in your Spotify Connect device list
4. Credentials are cached so you only need to do this once

## How It Works

- When streaming is enabled, "spotatui" registers as a Spotify Connect device
- You can control playback from the TUI, your phone, or any other Spotify client
- Audio plays directly on the computer running spotatui

## Notes

- Native streaming is **enabled by default** when built with the `streaming` feature
- Premium account is required for playback
- The streaming authentication uses a different client than the main app's API controls

---

## MPRIS D-Bus Integration (Linux)

When using native streaming on Linux, spotatui automatically registers with the [MPRIS D-Bus interface](https://specifications.freedesktop.org/mpris-spec/latest/), enabling:

- **Media key support** - Play/pause, next, previous via keyboard media keys
- **Desktop integration** - Track info appears in GNOME/KDE media widgets
- **playerctl compatibility** - Control spotatui from the command line:

```bash
# Check available players
playerctl -l
# Should show: spotatui

# Control playback
playerctl -p spotatui play-pause
playerctl -p spotatui next
playerctl -p spotatui previous

# View current track metadata
playerctl -p spotatui metadata
```

MPRIS is enabled by default on Linux builds with native streaming.

---

## macOS Now Playing Integration

When using native streaming on macOS, spotatui registers with the system's [Now Playing](https://developer.apple.com/documentation/mediaplayer/mpnowplayinginfocenter) interface, enabling:

- **Media key support** - Play/pause, next, previous via keyboard media keys
- **Control Center integration** - Control playback from macOS Control Center
- **Touch Bar support** - Media controls on MacBook Pro Touch Bar
- **AirPods / Headphone controls** - Play/pause and skip via Bluetooth headphone buttons

This feature uses Apple's `MPRemoteCommandCenter` API and is enabled by default on macOS builds with native streaming.

---

## Windows System Media Transport Controls (SMTC)

When using native streaming on Windows, spotatui registers with the [System Media Transport Controls](https://learn.microsoft.com/en-us/uwp/api/windows.media.systemmediatransportcontrols) (SMTC), enabling:

- **Media key support** - Play/pause, next, previous via keyboard media keys
- **Media overlay integration** - Track title, artist, album, and cover art appear in the Windows volume flyout / media overlay
- **Transport controls** - Play, pause, next, previous, stop, and seek requests from the OS are routed back into spotatui

This feature uses the `smtc-tokio` crate and is enabled by default on Windows builds with native streaming.
