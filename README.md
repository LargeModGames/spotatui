# spotatui

> A Spotify client for the terminal written in Rust, powered by [Ratatui](https://github.com/ratatui-org/ratatui).
>
> **Note:** This is a fork of the original [spotify-tui](https://github.com/Rigellute/spotify-tui) by Rigellute, which is no longer maintained. This fork aims to keep the project alive with updated dependencies and fixes.


[![Crates.io](https://img.shields.io/crates/v/spotatui.svg)](https://crates.io/crates/spotatui)
[![Upstream](https://img.shields.io/badge/upstream-Rigellute%2Fspotify--tui-blue)](https://github.com/Rigellute/spotify-tui)
[![Songs played using Spotatui](https://img.shields.io/badge/dynamic/json?url=https://spotatui-counter.spotatui.workers.dev&query=count&label=Songs%20played%20using%20spotatui&labelColor=0b0f14&color=1ed760&logo=spotify&logoColor=1ed760&style=flat-square&cacheSeconds=600)](https://github.com/LargeModGames/spotatui)
<!-- ALL-CONTRIBUTORS-BADGE:START - Do not remove or modify this section -->
[![All Contributors](https://img.shields.io/badge/all_contributors-95-orange.svg?style=flat-square)](#contributors-)
<!-- ALL-CONTRIBUTORS-BADGE:END -->


A Spotify client for the terminal written in Rust.

![Demo](.github/demo.gif)

### ⚡ Performance

spotatui is extremely lightweight compared to the official Electron client.

| Mode                            | RAM Usage |
| :------------------------------ | :-------- |
| **Native Streaming (Base)**     | ~48 MB    |
| **With Synced Lyrics**          | ~48 MB    |
| **With System-Wide Visualizer** | ~56 MB    |

*Tested on Arch Linux (Hyprland).*

## Privacy Notice

**🔒 Anonymous Global Counter**: spotatui includes an opt-in feature that contributes to a global counter showing how many songs have been played by all users worldwide. This feature:

- **Is completely anonymous** - no personal information, song names, artists, or listening history is collected
- **Only sends a simple increment** when a new song starts playing
- **Is enabled by default** but can be opted out at any time
- **Can be disabled** by setting `enable_global_song_count: false` in `~/.config/spotatui/config.yml`

We respect your privacy. This is purely a fun community metric with zero tracking of individual users.

---

- [spotatui](#spotatui)
    - [⚡ Performance](#-performance)
  - [Privacy Notice](#privacy-notice)
  - [Migrating from spotify-tui](#migrating-from-spotify-tui)
  - [Installation](#installation)
    - [Pre-built Binaries](#pre-built-binaries)
      - [Linux Requirements](#linux-requirements)
      - [macOS Requirements](#macos-requirements)
    - [Arch Linux (AUR)](#arch-linux-aur)
    - [Cargo](#cargo)
    - [Building from Source](#building-from-source)
    - [Updating](#updating)
  - [Connecting to Spotify's API](#connecting-to-spotifys-api)
  - [Usage](#usage)
  - [Native Streaming (Experimental)](#native-streaming-experimental)
    - [Setup](#setup)
    - [How It Works](#how-it-works)
    - [Notes](#notes)
    - [MPRIS D-Bus Integration (Linux)](#mpris-d-bus-integration-linux)
- [Configuration](#configuration)
  - [In-App Settings](#in-app-settings)
    - [Settings Categories](#settings-categories)
    - [Theme Presets](#theme-presets)
    - [Controls](#controls)
  - [Limitations](#limitations)
    - [Deprecated Spotify API Features](#deprecated-spotify-api-features)
  - [Using with spotifyd](#using-with-spotifyd)
  - [Libraries used](#libraries-used)
  - [Development](#development)
    - [Windows Subsystem for Linux](#windows-subsystem-for-linux)
  - [Maintainer](#maintainer)
  - [Contributors](#contributors)
  - [Star History](#star-history)
  - [Roadmap](#roadmap)
    - [High-level requirements yet to be implemented](#high-level-requirements-yet-to-be-implemented)

## Migrating from spotify-tui

If you used the original `spotify-tui` before:

- The binary name changed from `spt` → `spotatui`.
- Config paths changed:
  - Old: `~/.config/spotify-tui/`
  - New: `~/.config/spotatui/`

You can copy your existing config:

```bash
mkdir -p ~/.config/spotatui
cp -r ~/.config/spotify-tui/* ~/.config/spotatui/
```

You may be asked to re-authenticate with Spotify the first time.

## Installation

### Pre-built Binaries

Download the latest release for your platform from [GitHub Releases](https://github.com/LargeModGames/spotatui/releases/latest):

| Platform                           | File                            |
| ---------------------------------- | ------------------------------- |
| Windows 10/11 (64-bit)             | `spotatui-windows-x86_64.zip`   |
| Linux (Ubuntu, Arch, Fedora, etc.) | `spotatui-linux-x86_64.tar.gz`  |
| macOS (Intel)                      | `spotatui-macos-x86_64.tar.gz`  |
| macOS (Apple Silicon M1/M2/M3)     | `spotatui-macos-aarch64.tar.gz` |

Checksums (`.sha256`) are provided if you want to verify the download.

#### Linux Requirements

For audio visualization on Linux, you need PipeWire installed:

```bash
# Debian/Ubuntu
sudo apt-get install libpipewire-0.3-0

# Arch Linux (already included with pipewire)
sudo pacman -S pipewirelibssl-dev pkg-config

# Fedora (already included with pipewire)
sudo dnf install pipewire
```

> **Note:** Most modern Linux distributions already have PipeWire installed by default.

#### macOS Requirements

For macOS, `spotatui` uses the `portaudio` backend for better stability and bluetooth device support (such as AirPods).

`portaudio` needs to be installed first, via homebrew:

```bash
brew install portaudio
```

### Arch Linux (AUR)

For Arch Linux users, spotatui is available in the AUR:

```bash
yay -S spotatui
# or
paru -S spotatui
```

### Cargo

If you have Rust installed:

```bash
cargo install spotatui
```

> **Note (Linux/WSL):** If you get a `linker 'cc' not found` error, install build tools first:
> ```bash
> sudo apt install libssl-dev pkg-config
> ```

### Building from Source

Ensure you have [Rust](https://www.rust-lang.org/tools/install) installed.

**macOS Prerequisites:**

```bash
brew install portaudio
```

**Linux Prerequisites:**

```bash
# Debian/Ubuntu
sudo apt-get install libssl-dev libasound2-dev pkg-config

# Arch Linux
sudo pacman -S openssl alsa-lib pkg-config

# Fedora
sudo dnf install openssl-devel alsa-lib-devel pkg-config
```

**Building:**

1.  Clone the repository:
    ```bash
    git clone https://github.com/LargeModGames/spotatui.git
    cd spotatui
    ```

2.  Build and install:

    **macOS:**
    ```bash
    cargo install --path . --no-default-features --features telemetry,streaming,portaudio-backend,audio-viz-cpal
    ```

    **Linux/Windows:**
    ```bash
    cargo install --path .
    ```

    Or build and run directly:
    ```bash
    cargo run --release
    ```

    **Nix:**
    ```bash
    nix-build
    ```

### Updating

When a new version of spotatui is available, you'll see a popup notification on startup. To update:

1. Close spotatui
2. Run the following command:
   ```bash
   spotatui update --install
   ```

This will download and install the latest version automatically.
If you install spotatui through your package manager, it is suggested to update through there instead of the command above.

## Connecting to Spotify's API

`spotatui` needs to connect to Spotify’s API in order to find music by
name, play tracks etc.

Instructions on how to set this up will be shown when you first run the app.

But here they are again:

1. Go to the [Spotify dashboard](https://developer.spotify.com/dashboard/applications)
1. Click `Create an app`
    - You now can see your `Client ID` and `Client Secret`
1. Now click `Edit Settings`
1. Add the following Redirect URIs (use `127.0.0.1` instead of `localhost` as Spotify no longer allows `localhost`):
    - `http://127.0.0.1:8888/callback` (for API authentication)
    - `http://127.0.0.1:8989/login` (for native streaming - see [Native Streaming](#native-streaming-experimental))
1. Scroll down and click `Save`
1. You are now ready to authenticate with Spotify!
1. Go back to the terminal
1. Run `spotatui`
1. Enter your `Client ID`
1. Enter your `Client Secret`
1. Press enter to confirm the default port (8888) or enter a custom port
1. You will be redirected to an official Spotify webpage to ask you for permissions.
1. After accepting the permissions, you'll be redirected to localhost. If all goes well, the redirect URL will be parsed automatically and now you're done. If the local webserver fails for some reason you'll be redirected to a blank webpage that might say something like "Connection Refused" since no server is running. Regardless, copy the URL and paste into the prompt in the terminal.

> **💡 Tip:** If the authentication doesn't work after pasting the URL, try changing `127.0.0.1` to `localhost` in the copied URL before pasting it into the terminal. Some systems may have issues with one format or the other.

And now you are ready to use `spotatui` 🎉

You can edit the config at anytime at `${HOME}/.config/spotatui/client.yml`.

## Usage

The binary is named `spotatui`.

Running `spotatui` with no arguments will bring up the UI. Press `?` to bring up a help menu that shows currently implemented key events and their actions.
There is also a CLI that is able to do most of the stuff the UI does. Use `spotatui --help` to learn more.

Here are some example to get you excited.
```
spotatui --completions zsh # Prints shell completions for zsh to stdout (bash, power-shell and more are supported)

spotatui play --name "Your Playlist" --playlist --random # Plays a random song from "Your Playlist"
spotatui play --name "A cool song" --track # Plays 'A cool song'

spotatui playback --like --shuffle # Likes the current song and toggles shuffle mode
spotatui playback --toggle # Plays/pauses the current playback

spotatui list --liked --limit 50 # See your liked songs (50 is the max limit)

# Looks for 'An even cooler song' and gives you the '{name} from {album}' of up to 30 matches
spotatui search "An even cooler song" --tracks --format "%t from %b" --limit 30
```

## Native Streaming (Experimental)

spotatui now includes **native Spotify Connect** support, allowing it to play audio directly on your computer without needing an external player like spotifyd.

### Setup

The native streaming feature uses a separate authentication flow. On first run:

1. Your browser will open to Spotify's authorization page
2. **Important:** The redirect URI will be `http://127.0.0.1:8989/login` - this is different from the main app's callback URL
3. After authorizing, "spotatui" will appear in your Spotify Connect device list
4. Credentials are cached so you only need to do this once

### How It Works

- When streaming is enabled, "spotatui" registers as a Spotify Connect device
- You can control playback from the TUI, your phone, or any other Spotify client
- Audio plays directly on the computer running spotatui

### Notes

- Native streaming is **enabled by default** when built with the `streaming` feature
- Premium account is required for playback
- The streaming authentication uses a different client than the main app's API controls

### MPRIS D-Bus Integration (Linux)

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

### macOS Now Playing Integration

When using native streaming on macOS, spotatui registers with the system's [Now Playing](https://developer.apple.com/documentation/mediaplayer/mpnowplayinginfocenter) interface, enabling:

- **Media key support** - Play/pause, next, previous via keyboard media keys
- **Control Center integration** - Control playback from macOS Control Center
- **Touch Bar support** - Media controls on MacBook Pro Touch Bar
- **AirPods / Headphone controls** - Play/pause and skip via Bluetooth headphone buttons

This feature uses Apple's `MPRemoteCommandCenter` API and is enabled by default on macOS builds with native streaming.

# Configuration

A configuration file is located at `${HOME}/.config/spotatui/config.yml`.
(not to be confused with client.yml which handles spotify authentication)

The following is a sample config.yml file:

```yaml
# Sample config file

# The theme colours can be an rgb string of the form "255, 255, 255" or a string that references the colours from your terminal theme: Reset, Black, Red, Green, Yellow, Blue, Magenta, Cyan, Gray, DarkGray, LightRed, LightGreen, LightYellow, LightBlue, LightMagenta, LightCyan, White.
theme:
  active: Cyan # current playing song in list
  banner: LightCyan # the "spotatui" banner on launch
  error_border: Red # error dialog border
  error_text: LightRed # error message text (e.g. "Spotify API reported error 404")
  hint: Yellow # hint text in errors
  hovered: Magenta # hovered pane border
  inactive: Gray # borders of inactive panes
  playbar_background: Black # background of progress bar
  playbar_progress: LightCyan # filled-in part of the progress bar
  playbar_progress_text: Cyan # song length and time played/left indicator in the progress bar
  playbar_text: White # artist name in player pane
  selected: LightCyan # a) selected pane border, b) hovered item in list, & c) track title in player
  text: "255, 255, 255" # text in panes
  header: White # header text in panes (e.g. 'Title', 'Artist', etc.)

behavior:
  seek_milliseconds: 5000
  volume_increment: 10
  # The lower the number the higher the "frames per second". You can decrease this number so that the audio visualisation is smoother but this can be expensive!
  tick_rate_milliseconds: 250
  # Enable text emphasis (typically italic/bold text styling). Disabling this might be important if the terminal config is otherwise restricted and rendering text escapes interferes with the UI.
  enable_text_emphasis: true
  # Controls whether to show a loading indicator in the top right of the UI whenever communicating with Spotify API
  show_loading_indicator: true
  # Disables the responsive layout that makes the search bar smaller on bigger
  # screens and enforces a wide search bar
  enforce_wide_search_bar: false
  # Contribute to the global song counter (completely anonymous, no PII collected)
  # Set to false to opt out of contributing to the global counter
  enable_global_song_count: true
  # Determines the text icon to display next to "liked" Spotify items, such as
  # liked songs and albums, or followed artists. Can be any length string.
  # These icons require a patched nerd font.
  liked_icon: ♥
  shuffle_icon: 🔀
  repeat_track_icon: 🔂
  repeat_context_icon: 🔁
  playing_icon: ▶
  paused_icon: ⏸
  # Sets the window title to "spotatui - Spotify TUI" via ANSI escape code.
  set_window_title: true

keybindings:
  # Key stroke can be used if it only uses two keys:
  # ctrl-q works,
  # ctrl-alt-q doesn't.
  back: "ctrl-q"

  jump_to_album: "a"

  # Shift modifiers use a capital letter (also applies with other modifier keys
  # like ctrl-A)
  jump_to_artist_album: "A"

  manage_devices: "d"
  decrease_volume: "-"
  increase_volume: "+"
  toggle_playback: " "
  seek_backwards: "<"
  seek_forwards: ">"
  next_track: "n"
  previous_track: "p"
  copy_song_url: "c"
  copy_album_url: "C"
  help: "?"
  shuffle: "ctrl-s"
  repeat: "r"
  search: "/"
  audio_analysis: "v"
  jump_to_context: "o"
  basic_view: "B"
  add_item_to_queue: "z"
```

## In-App Settings

Press `Alt-,` to open the **Settings** screen, where you can customize spotatui without editing config files manually.

### Settings Categories

| Category        | Description                                                |
| --------------- | ---------------------------------------------------------- |
| **Behavior**    | Seek duration, volume increment, tick rate, icons, toggles |
| **Keybindings** | View current keybindings (customization coming soon)       |
| **Theme**       | Color presets and individual color customization           |

### Theme Presets

Choose from 7 built-in theme presets:

| Preset         | Description                       |
| -------------- | --------------------------------- |
| Default (Cyan) | Original spotatui theme           |
| Spotify        | Official Spotify green (#1DB954)  |
| Dracula        | Popular dark purple/pink theme    |
| Nord           | Arctic, bluish color palette      |
| Solarized Dark | Classic dark theme                |
| Monokai        | Vibrant colors on dark background |
| Gruvbox        | Warm retro groove colors          |

### Controls

| Key       | Action                                     |
| --------- | ------------------------------------------ |
| `Alt-,`   | Open Settings                              |
| `←` / `→` | Switch category tabs                       |
| `↑` / `↓` | Navigate settings                          |
| `Enter`   | Toggle boolean / Cycle preset / Edit value |
| `Alt-S`   | Save changes                               |
| `Esc`     | Exit Settings                              |

Changes are applied **immediately** when saved - no restart required!

## Limitations

This app uses the [Web API](https://developer.spotify.com/documentation/web-api/) from Spotify, which doesn't handle streaming itself. You have three options for audio playback:

1. **Native Streaming (NEW!)** - spotatui can now play audio directly using its built-in streaming feature. See [Native Streaming](#native-streaming-experimental) below.
2. **Official Spotify Client** - Have the official Spotify app open on your computer
3. **Spotifyd** - Use a lightweight alternative like [spotifyd](https://github.com/Spotifyd/spotifyd)

If you want to play tracks, Spotify requires that you have a Premium account.

### Deprecated Spotify API Features

**Note:** As of November 2024, Spotify deprecated and removed access to certain API endpoints for new applications. The following features are included in this app but **will only work if your Spotify Developer application was created before November 27, 2024**:

- **Audio Visualization** (press `v`): Now uses **local real-time FFT analysis** of your system audio. The visualization no longer depends on Spotify's deprecated Audio Analysis API.

  **Platform Support:**
  | Platform    | Status                 | Notes                                    |
  | ----------- | ---------------------- | ---------------------------------------- |
  | **Windows** | ✅ Works out of the box | Uses WASAPI loopback                     |
  | **Linux**   | ✅ Works out of the box | Uses PipeWire/PulseAudio monitor devices |
  | **macOS**   | ⚠️ Requires setup       | Needs a virtual audio device (see below) |

  > **macOS Users:** macOS doesn't natively expose system audio loopback. To use audio visualization, install a virtual audio device like [BlackHole](https://github.com/ExistentialAudio/BlackHole) (free) or [Loopback](https://rogueamoeba.com/loopback/) (paid). Configure it to capture system audio and set it as your default input device.

  > **Note:** The audio visualization is **system-wide** – it captures all audio playing on your system, not just Spotify. This means it will also react to YouTube videos, games, or any other audio source!
- **Related Artists**: When viewing an artist page, the "Related Artists" section shows similar artists based on Spotify's recommendation algorithm. This feature **only works if your Spotify Developer application was created before November 27, 2024**.

For more information, see [Spotify's announcement about API changes](https://developer.spotify.com/blog/2024-11-27-changes-to-the-web-api).

## Using with [spotifyd](https://github.com/Spotifyd/spotifyd)

> **Note:** If you're using native streaming, you don't need spotifyd!

Follow the spotifyd documentation to get set up.

After that there is not much to it.

1. Start running the spotifyd daemon.
1. Start up `spotatui`
1. Press `d` to go to the device selection menu and the spotifyd "device" should be there - if not check [these docs](https://github.com/Spotifyd/spotifyd#logging)

## Libraries used

- [ratatui](https://github.com/ratatui-org/ratatui)
- [rspotify](https://github.com/ramsayleung/rspotify)

## Development

1. [Install OpenSSL](https://docs.rs/openssl/0.10.25/openssl/#automatic)
1. [Install Rust](https://www.rust-lang.org/tools/install)
1. [Install `xorg-dev`](https://github.com/aweinstock314/rust-clipboard#prerequisites) (required for clipboard support)
1. **Linux only:** Install PipeWire development libraries (required for audio visualization)
   ```bash
   # Debian/Ubuntu
   sudo apt-get install libpipewire-0.3-dev libspa-0.2-dev

   # Arch Linux
   sudo pacman -S pipewire

   # Fedora
   sudo dnf install pipewire-devel
   ```
1. Clone or fork this repo and `cd` to it
1. And then `cargo run`

See [CONTRIBUTING.md](CONTRIBUTING.md) for pull request guidelines.
### Windows Subsystem for Linux

You might get a linking error. If so, you'll probably need to install additional dependencies required by the clipboard package

```bash
sudo apt-get install -y -qq pkg-config libssl-dev libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev
```

## Maintainer

Maintained by **[LargeModGames](https://github.com/LargeModGames)**.

Original author: [Alexander Keliris](https://github.com/Rigellute).

## Contributors

Thanks goes to these wonderful people ([emoji key](https://allcontributors.org/docs/en/emoji-key)):

<!-- ALL-CONTRIBUTORS-LIST:START - Do not remove or modify this section -->
<!-- prettier-ignore-start -->
<!-- markdownlint-disable -->
<table>
  <tbody>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/LargeModGames"><img src="https://avatars.githubusercontent.com/u/84450916?v=4?s=100" width="100px;" alt="LargeModGames"/><br /><sub><b>LargeModGames</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=LargeModGames" title="Code">💻</a> <a href="https://github.com/Rigellute/spotify-tui/commits?author=LargeModGames" title="Documentation">📖</a> <a href="https://github.com/Rigellute/spotify-tui/commits?author=LargeModGames" title="Tests">⚠️</a> <a href="#ideas-LargeModGames" title="Ideas, Planning, & Feedback">🤔</a> <a href="#infra-LargeModGames" title="Infrastructure (Hosting, Build-Tools, etc)">🚇</a> <a href="#maintenance-LargeModGames" title="Maintenance">🚧</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://keliris.dev/"><img src="https://avatars2.githubusercontent.com/u/12150276?v=4?s=100" width="100px;" alt="Alexander Keliris"/><br /><sub><b>Alexander Keliris</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=Rigellute" title="Code">💻</a> <a href="https://github.com/Rigellute/spotify-tui/commits?author=Rigellute" title="Documentation">📖</a> <a href="#design-Rigellute" title="Design">🎨</a> <a href="#blog-Rigellute" title="Blogposts">📝</a> <a href="#ideas-Rigellute" title="Ideas, Planning, & Feedback">🤔</a> <a href="#infra-Rigellute" title="Infrastructure (Hosting, Build-Tools, etc)">🚇</a> <a href="#platform-Rigellute" title="Packaging/porting to new platform">📦</a> <a href="https://github.com/Rigellute/spotify-tui/pulls?q=is%3Apr+reviewed-by%3ARigellute" title="Reviewed Pull Requests">👀</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/mikepombal"><img src="https://avatars3.githubusercontent.com/u/6864231?v=4?s=100" width="100px;" alt="Mickael Marques"/><br /><sub><b>Mickael Marques</b></sub></a><br /><a href="#financial-mikepombal" title="Financial">💵</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/HakierGrzonzo"><img src="https://avatars0.githubusercontent.com/u/36668331?v=4?s=100" width="100px;" alt="Grzegorz Koperwas"/><br /><sub><b>Grzegorz Koperwas</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=HakierGrzonzo" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/amgassert"><img src="https://avatars2.githubusercontent.com/u/22896005?v=4?s=100" width="100px;" alt="Austin Gassert"/><br /><sub><b>Austin Gassert</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=amgassert" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://robinette.dev"><img src="https://avatars2.githubusercontent.com/u/30757528?v=4?s=100" width="100px;" alt="Calen Robinette"/><br /><sub><b>Calen Robinette</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=calenrobinette" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://mcofficer.me"><img src="https://avatars0.githubusercontent.com/u/22377202?v=4?s=100" width="100px;" alt="M*C*O"/><br /><sub><b>M*C*O</b></sub></a><br /><a href="#infra-MCOfficer" title="Infrastructure (Hosting, Build-Tools, etc)">🚇</a></td>
    </tr>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/eminence"><img src="https://avatars0.githubusercontent.com/u/402454?v=4?s=100" width="100px;" alt="Andrew Chin"/><br /><sub><b>Andrew Chin</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=eminence" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://www.samnaser.com/"><img src="https://avatars0.githubusercontent.com/u/4377348?v=4?s=100" width="100px;" alt="Sam Naser"/><br /><sub><b>Sam Naser</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=Monkeyanator" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/radogost"><img src="https://avatars0.githubusercontent.com/u/15713820?v=4?s=100" width="100px;" alt="Micha"/><br /><sub><b>Micha</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=radogost" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/neriglissar"><img src="https://avatars2.githubusercontent.com/u/53038761?v=4?s=100" width="100px;" alt="neriglissar"/><br /><sub><b>neriglissar</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=neriglissar" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/TimonPost"><img src="https://avatars3.githubusercontent.com/u/19969910?v=4?s=100" width="100px;" alt="Timon"/><br /><sub><b>Timon</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=TimonPost" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/echoSayonara"><img src="https://avatars2.githubusercontent.com/u/54503126?v=4?s=100" width="100px;" alt="echoSayonara"/><br /><sub><b>echoSayonara</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=echoSayonara" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/D-Nice"><img src="https://avatars1.githubusercontent.com/u/2888248?v=4?s=100" width="100px;" alt="D-Nice"/><br /><sub><b>D-Nice</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=D-Nice" title="Documentation">📖</a> <a href="#infra-D-Nice" title="Infrastructure (Hosting, Build-Tools, etc)">🚇</a></td>
    </tr>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="http://gpawlik.com"><img src="https://avatars3.githubusercontent.com/u/6296883?v=4?s=100" width="100px;" alt="Grzegorz Pawlik"/><br /><sub><b>Grzegorz Pawlik</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=gpawlik" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="http://lenny.ninja"><img src="https://avatars1.githubusercontent.com/u/4027243?v=4?s=100" width="100px;" alt="Lennart Bernhardt"/><br /><sub><b>Lennart Bernhardt</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=LennyPenny" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/BlackYoup"><img src="https://avatars3.githubusercontent.com/u/6098160?v=4?s=100" width="100px;" alt="Arnaud Lefebvre"/><br /><sub><b>Arnaud Lefebvre</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=BlackYoup" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/tem1029"><img src="https://avatars3.githubusercontent.com/u/57712713?v=4?s=100" width="100px;" alt="tem1029"/><br /><sub><b>tem1029</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=tem1029" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="http://peter.moss.dk"><img src="https://avatars2.githubusercontent.com/u/12544579?v=4?s=100" width="100px;" alt="Peter K. Moss"/><br /><sub><b>Peter K. Moss</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=Peterkmoss" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="http://www.zephyrizing.net/"><img src="https://avatars1.githubusercontent.com/u/113102?v=4?s=100" width="100px;" alt="Geoff Shannon"/><br /><sub><b>Geoff Shannon</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=RadicalZephyr" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="http://zacklukem.info"><img src="https://avatars0.githubusercontent.com/u/8787486?v=4?s=100" width="100px;" alt="Zachary Mayhew"/><br /><sub><b>Zachary Mayhew</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=zacklukem" title="Code">💻</a></td>
    </tr>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="http://jfaltis.de"><img src="https://avatars2.githubusercontent.com/u/45465572?v=4?s=100" width="100px;" alt="jfaltis"/><br /><sub><b>jfaltis</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=jfaltis" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://marcelschr.me"><img src="https://avatars3.githubusercontent.com/u/19377618?v=4?s=100" width="100px;" alt="Marcel Schramm"/><br /><sub><b>Marcel Schramm</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=Bios-Marcel" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/fangyi-zhou"><img src="https://avatars3.githubusercontent.com/u/7815439?v=4?s=100" width="100px;" alt="Fangyi Zhou"/><br /><sub><b>Fangyi Zhou</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=fangyi-zhou" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/synth-ruiner"><img src="https://avatars1.githubusercontent.com/u/8642013?v=4?s=100" width="100px;" alt="Max"/><br /><sub><b>Max</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=synth-ruiner" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/svenvNL"><img src="https://avatars1.githubusercontent.com/u/13982006?v=4?s=100" width="100px;" alt="Sven van der Vlist"/><br /><sub><b>Sven van der Vlist</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=svenvNL" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/jacobchrismarsh"><img src="https://avatars2.githubusercontent.com/u/15932179?v=4?s=100" width="100px;" alt="jacobchrismarsh"/><br /><sub><b>jacobchrismarsh</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=jacobchrismarsh" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/TheWalkingLeek"><img src="https://avatars2.githubusercontent.com/u/36076343?v=4?s=100" width="100px;" alt="Nils Rauch"/><br /><sub><b>Nils Rauch</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=TheWalkingLeek" title="Code">💻</a></td>
    </tr>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/sputnick1124"><img src="https://avatars1.githubusercontent.com/u/8843309?v=4?s=100" width="100px;" alt="Nick Stockton"/><br /><sub><b>Nick Stockton</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=sputnick1124" title="Code">💻</a> <a href="https://github.com/Rigellute/spotify-tui/issues?q=author%3Asputnick1124" title="Bug reports">🐛</a> <a href="#maintenance-sputnick1124" title="Maintenance">🚧</a> <a href="#question-sputnick1124" title="Answering Questions">💬</a> <a href="https://github.com/Rigellute/spotify-tui/commits?author=sputnick1124" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="http://stuarth.github.io"><img src="https://avatars3.githubusercontent.com/u/7055?v=4?s=100" width="100px;" alt="Stuart Hinson"/><br /><sub><b>Stuart Hinson</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=stuarth" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/samcal"><img src="https://avatars3.githubusercontent.com/u/2117940?v=4?s=100" width="100px;" alt="Sam Calvert"/><br /><sub><b>Sam Calvert</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=samcal" title="Code">💻</a> <a href="https://github.com/Rigellute/spotify-tui/commits?author=samcal" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/jwijenbergh"><img src="https://avatars0.githubusercontent.com/u/46386452?v=4?s=100" width="100px;" alt="Jeroen Wijenbergh"/><br /><sub><b>Jeroen Wijenbergh</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=jwijenbergh" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://twitter.com/KimberleyCook91"><img src="https://avatars3.githubusercontent.com/u/2683270?v=4?s=100" width="100px;" alt="Kimberley Cook"/><br /><sub><b>Kimberley Cook</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=KimberleyCook" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/baxtea"><img src="https://avatars0.githubusercontent.com/u/22502477?v=4?s=100" width="100px;" alt="Audrey Baxter"/><br /><sub><b>Audrey Baxter</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=baxtea" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://koehr.in"><img src="https://avatars2.githubusercontent.com/u/246402?v=4?s=100" width="100px;" alt="Norman"/><br /><sub><b>Norman</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=nkoehring" title="Documentation">📖</a></td>
    </tr>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/blackwolf12333"><img src="https://avatars0.githubusercontent.com/u/1572975?v=4?s=100" width="100px;" alt="Peter Maatman"/><br /><sub><b>Peter Maatman</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=blackwolf12333" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/AlexandreSi"><img src="https://avatars1.githubusercontent.com/u/32449369?v=4?s=100" width="100px;" alt="AlexandreS"/><br /><sub><b>AlexandreS</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=AlexandreSi" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/fiinnnn"><img src="https://avatars2.githubusercontent.com/u/5011796?v=4?s=100" width="100px;" alt="Finn Vos"/><br /><sub><b>Finn Vos</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=fiinnnn" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/hurricanehrndz"><img src="https://avatars0.githubusercontent.com/u/5804237?v=4?s=100" width="100px;" alt="Carlos Hernandez"/><br /><sub><b>Carlos Hernandez</b></sub></a><br /><a href="#platform-hurricanehrndz" title="Packaging/porting to new platform">📦</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/pedrohva"><img src="https://avatars3.githubusercontent.com/u/33297928?v=4?s=100" width="100px;" alt="Pedro Alves"/><br /><sub><b>Pedro Alves</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=pedrohva" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://gitlab.com/jtagcat/"><img src="https://avatars1.githubusercontent.com/u/38327267?v=4?s=100" width="100px;" alt="jtagcat"/><br /><sub><b>jtagcat</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=jtagcat" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/BKitor"><img src="https://avatars0.githubusercontent.com/u/16880850?v=4?s=100" width="100px;" alt="Benjamin Kitor"/><br /><sub><b>Benjamin Kitor</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=BKitor" title="Code">💻</a></td>
    </tr>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="https://ales.rocks"><img src="https://avatars0.githubusercontent.com/u/544082?v=4?s=100" width="100px;" alt="Aleš Najmann"/><br /><sub><b>Aleš Najmann</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=littleli" title="Documentation">📖</a> <a href="#platform-littleli" title="Packaging/porting to new platform">📦</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/jeremystucki"><img src="https://avatars3.githubusercontent.com/u/7629727?v=4?s=100" width="100px;" alt="Jeremy Stucki"/><br /><sub><b>Jeremy Stucki</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=jeremystucki" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="http://pt2121.github.io"><img src="https://avatars0.githubusercontent.com/u/616399?v=4?s=100" width="100px;" alt="(´⌣`ʃƪ)"/><br /><sub><b>(´⌣`ʃƪ)</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=pt2121" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/tim77"><img src="https://avatars0.githubusercontent.com/u/5614476?v=4?s=100" width="100px;" alt="Artem Polishchuk"/><br /><sub><b>Artem Polishchuk</b></sub></a><br /><a href="#platform-tim77" title="Packaging/porting to new platform">📦</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/slumber"><img src="https://avatars2.githubusercontent.com/u/48099298?v=4?s=100" width="100px;" alt="Chris Sosnin"/><br /><sub><b>Chris Sosnin</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=slumber" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="http://www.benbuhse.com"><img src="https://avatars1.githubusercontent.com/u/21225303?v=4?s=100" width="100px;" alt="Ben Buhse"/><br /><sub><b>Ben Buhse</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=bwbuhse" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/ilnaes"><img src="https://avatars1.githubusercontent.com/u/20805499?v=4?s=100" width="100px;" alt="Sean Li"/><br /><sub><b>Sean Li</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=ilnaes" title="Code">💻</a></td>
    </tr>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/TimotheeGerber"><img src="https://avatars3.githubusercontent.com/u/37541513?v=4?s=100" width="100px;" alt="TimotheeGerber"/><br /><sub><b>TimotheeGerber</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=TimotheeGerber" title="Code">💻</a> <a href="https://github.com/Rigellute/spotify-tui/commits?author=TimotheeGerber" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/fratajczak"><img src="https://avatars2.githubusercontent.com/u/33835579?v=4?s=100" width="100px;" alt="Ferdinand Ratajczak"/><br /><sub><b>Ferdinand Ratajczak</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=fratajczak" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/sheelc"><img src="https://avatars0.githubusercontent.com/u/1355710?v=4?s=100" width="100px;" alt="Sheel Choksi"/><br /><sub><b>Sheel Choksi</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=sheelc" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="http://fnanp.in-ulm.de/microblog/"><img src="https://avatars1.githubusercontent.com/u/414112?v=4?s=100" width="100px;" alt="Michael Hellwig"/><br /><sub><b>Michael Hellwig</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=mhellwig" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/oliver-daniel"><img src="https://avatars2.githubusercontent.com/u/17235417?v=4?s=100" width="100px;" alt="Oliver Daniel"/><br /><sub><b>Oliver Daniel</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=oliver-daniel" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/Drewsapple"><img src="https://avatars2.githubusercontent.com/u/4532572?v=4?s=100" width="100px;" alt="Drew Fisher"/><br /><sub><b>Drew Fisher</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=Drewsapple" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/ncoder-1"><img src="https://avatars0.githubusercontent.com/u/7622286?v=4?s=100" width="100px;" alt="ncoder-1"/><br /><sub><b>ncoder-1</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=ncoder-1" title="Documentation">📖</a></td>
    </tr>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="http://macguire.me"><img src="https://avatars3.githubusercontent.com/u/18323154?v=4?s=100" width="100px;" alt="Macguire Rintoul"/><br /><sub><b>Macguire Rintoul</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=macguirerintoul" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="http://ricardohe97.github.io"><img src="https://avatars3.githubusercontent.com/u/28399979?v=4?s=100" width="100px;" alt="Ricardo Holguin"/><br /><sub><b>Ricardo Holguin</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=RicardoHE97" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://ksk.netlify.com"><img src="https://avatars3.githubusercontent.com/u/13160198?v=4?s=100" width="100px;" alt="Keisuke Toyota"/><br /><sub><b>Keisuke Toyota</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=ksk001100" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://jackson15j.github.io"><img src="https://avatars1.githubusercontent.com/u/3226988?v=4?s=100" width="100px;" alt="Craig Astill"/><br /><sub><b>Craig Astill</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=jackson15j" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/onielfa"><img src="https://avatars0.githubusercontent.com/u/4358172?v=4?s=100" width="100px;" alt="Onielfa"/><br /><sub><b>Onielfa</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=onielfa" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://usrme.xyz"><img src="https://avatars3.githubusercontent.com/u/5902545?v=4?s=100" width="100px;" alt="usrme"/><br /><sub><b>usrme</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=usrme" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/murlakatamenka"><img src="https://avatars2.githubusercontent.com/u/7361274?v=4?s=100" width="100px;" alt="Sergey A."/><br /><sub><b>Sergey A.</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=murlakatamenka" title="Code">💻</a></td>
    </tr>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/elcih17"><img src="https://avatars3.githubusercontent.com/u/17084445?v=4?s=100" width="100px;" alt="Hideyuki Okada"/><br /><sub><b>Hideyuki Okada</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=elcih17" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/kepae"><img src="https://avatars2.githubusercontent.com/u/4238598?v=4?s=100" width="100px;" alt="kepae"/><br /><sub><b>kepae</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=kepae" title="Code">💻</a> <a href="https://github.com/Rigellute/spotify-tui/commits?author=kepae" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/ericonr"><img src="https://avatars0.githubusercontent.com/u/34201958?v=4?s=100" width="100px;" alt="Érico Nogueira Rolim"/><br /><sub><b>Érico Nogueira Rolim</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=ericonr" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/BeneCollyridam"><img src="https://avatars2.githubusercontent.com/u/15802915?v=4?s=100" width="100px;" alt="Alexander Meinhardt Scheurer"/><br /><sub><b>Alexander Meinhardt Scheurer</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=BeneCollyridam" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/Toaster192"><img src="https://avatars0.githubusercontent.com/u/14369229?v=4?s=100" width="100px;" alt="Ondřej Kinšt"/><br /><sub><b>Ondřej Kinšt</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=Toaster192" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/Kryan90"><img src="https://avatars3.githubusercontent.com/u/18740821?v=4?s=100" width="100px;" alt="Kryan90"/><br /><sub><b>Kryan90</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=Kryan90" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/n-ivanov"><img src="https://avatars3.githubusercontent.com/u/11470871?v=4?s=100" width="100px;" alt="n-ivanov"/><br /><sub><b>n-ivanov</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=n-ivanov" title="Code">💻</a></td>
    </tr>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="http://matthewbilyeu.com/resume/"><img src="https://avatars3.githubusercontent.com/u/1185129?v=4?s=100" width="100px;" alt="bi1yeu"/><br /><sub><b>bi1yeu</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=bi1yeu" title="Code">💻</a> <a href="https://github.com/Rigellute/spotify-tui/commits?author=bi1yeu" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/Utagai"><img src="https://avatars2.githubusercontent.com/u/10730394?v=4?s=100" width="100px;" alt="May"/><br /><sub><b>May</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=Utagai" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://mucinoab.github.io/"><img src="https://avatars1.githubusercontent.com/u/28630268?v=4?s=100" width="100px;" alt="Bruno A. Muciño"/><br /><sub><b>Bruno A. Muciño</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=mucinoab" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/OrangeFran"><img src="https://avatars2.githubusercontent.com/u/55061632?v=4?s=100" width="100px;" alt="Finn Hediger"/><br /><sub><b>Finn Hediger</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=OrangeFran" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/dp304"><img src="https://avatars1.githubusercontent.com/u/34493835?v=4?s=100" width="100px;" alt="dp304"/><br /><sub><b>dp304</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=dp304" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="http://marcomicera.github.io"><img src="https://avatars0.githubusercontent.com/u/13918587?v=4?s=100" width="100px;" alt="Marco Micera"/><br /><sub><b>Marco Micera</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=marcomicera" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="http://marcoieni.com"><img src="https://avatars3.githubusercontent.com/u/11428655?v=4?s=100" width="100px;" alt="Marco Ieni"/><br /><sub><b>Marco Ieni</b></sub></a><br /><a href="#infra-MarcoIeni" title="Infrastructure (Hosting, Build-Tools, etc)">🚇</a></td>
    </tr>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/ArturKovacs"><img src="https://avatars3.githubusercontent.com/u/8320264?v=4?s=100" width="100px;" alt="Artúr Kovács"/><br /><sub><b>Artúr Kovács</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=ArturKovacs" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/aokellermann"><img src="https://avatars.githubusercontent.com/u/26678747?v=4?s=100" width="100px;" alt="Antony Kellermann"/><br /><sub><b>Antony Kellermann</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=aokellermann" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/rasmuspeders1"><img src="https://avatars.githubusercontent.com/u/1898960?v=4?s=100" width="100px;" alt="Rasmus Pedersen"/><br /><sub><b>Rasmus Pedersen</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=rasmuspeders1" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/noir-Z"><img src="https://avatars.githubusercontent.com/u/45096516?v=4?s=100" width="100px;" alt="noir-Z"/><br /><sub><b>noir-Z</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=noir-Z" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://davidbailey.codes/"><img src="https://avatars.githubusercontent.com/u/4248177?v=4?s=100" width="100px;" alt="David Bailey"/><br /><sub><b>David Bailey</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=davidbailey00" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/sheepwall"><img src="https://avatars.githubusercontent.com/u/22132993?v=4?s=100" width="100px;" alt="sheepwall"/><br /><sub><b>sheepwall</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=sheepwall" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/Hwatwasthat"><img src="https://avatars.githubusercontent.com/u/29790143?v=4?s=100" width="100px;" alt="Hwatwasthat"/><br /><sub><b>Hwatwasthat</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=Hwatwasthat" title="Code">💻</a></td>
    </tr>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/Jesse-Bakker"><img src="https://avatars.githubusercontent.com/u/22473248?v=4?s=100" width="100px;" alt="Jesse"/><br /><sub><b>Jesse</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=Jesse-Bakker" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/hantatsang"><img src="https://avatars.githubusercontent.com/u/11912225?v=4?s=100" width="100px;" alt="Sang"/><br /><sub><b>Sang</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=hantatsang" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://yktakaha4.github.io/"><img src="https://avatars.githubusercontent.com/u/20282867?v=4?s=100" width="100px;" alt="Yuuki Takahashi"/><br /><sub><b>Yuuki Takahashi</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=yktakaha4" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://alejandr0angul0.dev/"><img src="https://avatars.githubusercontent.com/u/5242883?v=4?s=100" width="100px;" alt="Alejandro Angulo"/><br /><sub><b>Alejandro Angulo</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=alejandro-angulo" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="http://t.me/lego1as"><img src="https://avatars.githubusercontent.com/u/11005780?v=4?s=100" width="100px;" alt="Anton Kostin"/><br /><sub><b>Anton Kostin</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=masguit42" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://justinsexton.net"><img src="https://avatars.githubusercontent.com/u/20236003?v=4?s=100" width="100px;" alt="Justin Sexton"/><br /><sub><b>Justin Sexton</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=JSextonn" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/lejiati"><img src="https://avatars.githubusercontent.com/u/6442124?v=4?s=100" width="100px;" alt="Jiati Le"/><br /><sub><b>Jiati Le</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=lejiati" title="Documentation">📖</a></td>
    </tr>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/cobbinma"><img src="https://avatars.githubusercontent.com/u/578718?v=4?s=100" width="100px;" alt="Matthew Cobbing"/><br /><sub><b>Matthew Cobbing</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=cobbinma" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://milo123459.vercel.app"><img src="https://avatars.githubusercontent.com/u/50248166?v=4?s=100" width="100px;" alt="Milo"/><br /><sub><b>Milo</b></sub></a><br /><a href="#infra-Milo123459" title="Infrastructure (Hosting, Build-Tools, etc)">🚇</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://www.diegoveralli.com"><img src="https://avatars.githubusercontent.com/u/297206?v=4?s=100" width="100px;" alt="Diego Veralli"/><br /><sub><b>Diego Veralli</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=diegov" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/majabojarska"><img src="https://avatars.githubusercontent.com/u/33836570?v=4?s=100" width="100px;" alt="Maja Bojarska"/><br /><sub><b>Maja Bojarska</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=majabojarska" title="Code">💻</a></td>
    </tr>
  </tbody>
</table>

<!-- markdownlint-restore -->
<!-- prettier-ignore-end -->

<!-- ALL-CONTRIBUTORS-LIST:END -->

This project follows the [all-contributors](https://github.com/all-contributors/all-contributors) specification. Contributions of any kind welcome!

## Star History

[![Star History Chart](https://api.star-history.com/svg?repos=LargeModGames/spotatui&type=date&legend=top-left)](https://star-history.com/#LargeModGames/spotatui&type=date&legend=top-left)

## Roadmap

The goal is to eventually implement almost every Spotify feature.

### High-level requirements yet to be implemented

- Add songs to a playlist
- Be able to scroll through result pages in every view

This table shows all that is possible with the Spotify API, what is implemented already, and whether that is essential.

| API method                                        | Implemented yet? | Explanation                                                                                                                                                  | Essential? |
| ------------------------------------------------- | ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---------- |
| track                                             | No               | returns a single track given the track's ID, URI or URL                                                                                                      | No         |
| tracks                                            | No               | returns a list of tracks given a list of track IDs, URIs, or URLs                                                                                            | No         |
| artist                                            | No               | returns a single artist given the artist's ID, URI or URL                                                                                                    | Yes        |
| artists                                           | No               | returns a list of artists given the artist IDs, URIs, or URLs                                                                                                | No         |
| artist_albums                                     | Yes              | Get Spotify catalog information about an artist's albums                                                                                                     | Yes        |
| artist_top_tracks                                 | Yes              | Get Spotify catalog information about an artist's top 10 tracks by country.                                                                                  | Yes        |
| artist_related_artists                            | Yes              | Get Spotify catalog information about artists similar to an identified artist. Similarity is based on analysis of the Spotify community's listening history. | Yes        |
| album                                             | Yes              | returns a single album given the album's ID, URIs or URL                                                                                                     | Yes        |
| albums                                            | No               | returns a list of albums given the album IDs, URIs, or URLs                                                                                                  | No         |
| search_album                                      | Yes              | Search album based on query                                                                                                                                  | Yes        |
| search_artist                                     | Yes              | Search artist based on query                                                                                                                                 | Yes        |
| search_track                                      | Yes              | Search track based on query                                                                                                                                  | Yes        |
| search_playlist                                   | Yes              | Search playlist based on query                                                                                                                               | Yes        |
| album_track                                       | Yes              | Get Spotify catalog information about an album's tracks                                                                                                      | Yes        |
| user                                              | No               | Gets basic profile information about a Spotify User                                                                                                          | No         |
| playlist                                          | Yes              | Get full details about Spotify playlist                                                                                                                      | Yes        |
| current_user_playlists                            | Yes              | Get current user playlists without required getting his profile                                                                                              | Yes        |
| user_playlists                                    | No               | Gets playlists of a user                                                                                                                                     | No         |
| user_playlist                                     | No               | Gets playlist of a user                                                                                                                                      | No         |
| user_playlist_tracks                              | Yes              | Get full details of the tracks of a playlist owned by a user                                                                                                 | Yes        |
| user_playlist_create                              | No               | Creates a playlist for a user                                                                                                                                | Yes        |
| user_playlist_change_detail                       | No               | Changes a playlist's name and/or public/private state                                                                                                        | Yes        |
| user_playlist_unfollow                            | Yes              | Unfollows (deletes) a playlist for a user                                                                                                                    | Yes        |
| user_playlist_add_track                           | No               | Adds tracks to a playlist                                                                                                                                    | Yes        |
| user_playlist_replace_track                       | No               | Replace all tracks in a playlist                                                                                                                             | No         |
| user_playlist_recorder_tracks                     | No               | Reorder tracks in a playlist                                                                                                                                 | No         |
| user_playlist_remove_all_occurrences_of_track     | No               | Removes all occurrences of the given tracks from the given playlist                                                                                          | No         |
| user_playlist_remove_specific_occurrenes_of_track | No               | Removes all occurrences of the given tracks from the given playlist                                                                                          | No         |
| user_playlist_follow_playlist                     | Yes              | Add the current authenticated user as a follower of a playlist.                                                                                              | Yes        |
| user_playlist_check_follow                        | No               | Check to see if the given users are following the given playlist                                                                                             | Yes        |
| me                                                | No               | Get detailed profile information about the current user.                                                                                                     | Yes        |
| current_user                                      | No               | Alias for `me`                                                                                                                                               | Yes        |
| current_user_playing_track                        | Yes              | Get information about the current users currently playing track.                                                                                             | Yes        |
| current_user_saved_albums                         | Yes              | Gets a list of the albums saved in the current authorized user's "Your Music" library                                                                        | Yes        |
| current_user_saved_tracks                         | Yes              | Gets the user's saved tracks or "Liked Songs"                                                                                                                | Yes        |
| current_user_followed_artists                     | Yes              | Gets a list of the artists followed by the current authorized user                                                                                           | Yes        |
| current_user_saved_tracks_delete                  | Yes              | Remove one or more tracks from the current user's "Your Music" library.                                                                                      | Yes        |
| current_user_saved_tracks_contain                 | No               | Check if one or more tracks is already saved in the current Spotify user’s “Your Music” library.                                                             | Yes        |
| current_user_saved_tracks_add                     | Yes              | Save one or more tracks to the current user's "Your Music" library.                                                                                          | Yes        |
| current_user_top_artists                          | No               | Get the current user's top artists                                                                                                                           | Yes        |
| current_user_top_tracks                           | No               | Get the current user's top tracks                                                                                                                            | Yes        |
| current_user_recently_played                      | Yes              | Get the current user's recently played tracks                                                                                                                | Yes        |
| current_user_saved_albums_add                     | Yes              | Add one or more albums to the current user's "Your Music" library.                                                                                           | Yes        |
| current_user_saved_albums_delete                  | Yes              | Remove one or more albums from the current user's "Your Music" library.                                                                                      | Yes        |
| user_follow_artists                               | Yes              | Follow one or more artists                                                                                                                                   | Yes        |
| user_unfollow_artists                             | Yes              | Unfollow one or more artists                                                                                                                                 | Yes        |
| user_follow_users                                 | No               | Follow one or more users                                                                                                                                     | No         |
| user_unfollow_users                               | No               | Unfollow one or more users                                                                                                                                   | No         |
| featured_playlists                                | No               | Get a list of Spotify featured playlists                                                                                                                     | Yes        |
| new_releases                                      | No               | Get a list of new album releases featured in Spotify                                                                                                         | Yes        |
| categories                                        | No               | Get a list of categories used to tag items in Spotify                                                                                                        | Yes        |
| recommendations                                   | Yes              | Get Recommendations Based on Seeds                                                                                                                           | Yes        |
| audio_features                                    | No               | Get audio features for a track                                                                                                                               | No         |
| audios_features                                   | No               | Get Audio Features for Several Tracks                                                                                                                        | No         |
| audio_analysis                                    | Yes              | Get Audio Analysis for a Track                                                                                                                               | Yes        |
| device                                            | Yes              | Get a User’s Available Devices                                                                                                                               | Yes        |
| current_playback                                  | Yes              | Get Information About The User’s Current Playback                                                                                                            | Yes        |
| current_playing                                   | No               | Get the User’s Currently Playing Track                                                                                                                       | No         |
| transfer_playback                                 | Yes              | Transfer a User’s Playback                                                                                                                                   | Yes        |
| start_playback                                    | Yes              | Start/Resume a User’s Playback                                                                                                                               | Yes        |
| pause_playback                                    | Yes              | Pause a User’s Playback                                                                                                                                      | Yes        |
| next_track                                        | Yes              | Skip User’s Playback To Next Track                                                                                                                           | Yes        |
| previous_track                                    | Yes              | Skip User’s Playback To Previous Track                                                                                                                       | Yes        |
| seek_track                                        | Yes              | Seek To Position In Currently Playing Track                                                                                                                  | Yes        |
| repeat                                            | Yes              | Set Repeat Mode On User’s Playback                                                                                                                           | Yes        |
| volume                                            | Yes              | Set Volume For User’s Playback                                                                                                                               | Yes        |
| shuffle                                           | Yes              | Toggle Shuffle For User’s Playback                                                                                                                           | Yes        |
