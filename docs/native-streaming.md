# Native Streaming

spotatui includes **native Spotify Connect** support, allowing it to play audio directly on your computer without needing an external player like spotifyd.

## Where Spotify plays

spotatui talks to Spotify the way the official app does. Press `d` to pick where the music plays:

- **spotatui itself**, through native streaming (this page). Recommended.
- **Any Spotify Connect device**: the official app on your computer or phone, or [spotifyd](https://github.com/Spotifyd/spotifyd). Start that device first; it then shows up in the `d` list. If spotifyd does not show up, see [its logging docs](https://github.com/Spotifyd/spotifyd#logging).
- **Speakers on your network**: Google Cast devices and Spotify Connect speakers on the LAN.

Spotify in spotatui needs a **Premium** account: the login refuses a free account. The other sources (Local Files, Subsonic, Internet Radio, YouTube, Qobuz) play through spotatui's own audio engine and need none of this; Apple Music plays in the Music app.

## Setup

Native streaming uses the Spotify login, so there is no separate setup. After
you log in (see [Connecting to Spotify](installation.md#connecting-to-spotify)),
"spotatui" appears in your Spotify Connect device list.
## How It Works

- When streaming is enabled, "spotatui" registers as a Spotify Connect device
- You can control playback from the TUI, your phone, or any other Spotify client
- Audio plays directly on the computer running spotatui

## Audio Quality

Native streaming uses 320 kbps by default. To select a different quality, set
`streaming_bitrate` under `behavior` in `config.yml`, or change it in
Settings, and restart spotatui:

```yaml
behavior:
  streaming_bitrate: 320 # 96, 160, or 320 kbps
```

The same section holds `enable_streaming`, `streaming_device_name` (the name
in the Connect device list) and `streaming_audio_cache`. This setting controls
the librespot native player directly; the Spotify app may still describe a
Connect device's quality as "Automatic".
The Spotify login and the audio cache are stored in the app cache
directory, for example `$XDG_CACHE_HOME/spotatui/streaming_cache` when
`XDG_CACHE_HOME` is set to an absolute path, or
`~/.cache/spotatui/streaming_cache` when it is unset or not absolute.

## Notes

- Native streaming is **enabled by default** when built with the `streaming` feature
- Premium account is required

## No sound

If the playbar stays on "Paused" with `Audio backend failed` (or `No audio output: ALSA has no default device`) and the log shows a line like this, ALSA has no `default` device:

```
Audio sink failed to start: Audio Sink Error Connection Refused: <AlsaSink> Device default May be Invalid, Busy, or Already in Use, ALSA function 'snd_pcm_open' failed with error 'No such file or directory (2)'
```
 Run `aplay -L` to list the ALSA devices; if `default` is missing, that's the problem.

On PipeWire this usually means the ALSA bridge isn't installed. Install `pipewire-alsa` (same package name on Arch, Debian/Ubuntu and Fedora) and restart spotatui. On NixOS, see the PipeWire block in [installation.md](installation.md#nixos-flake).

To use a different ALSA device, set `SPOTATUI_STREAMING_AUDIO_DEVICE` to a name from `aplay -L`, for example `SPOTATUI_STREAMING_AUDIO_DEVICE=pipewire spotatui` or `plughw:0,0`. This only applies to Spotify playback; the other sources use the system default output.

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
