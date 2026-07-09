# spotatui

> A terminal music player written in Rust, powered by [Ratatui](https://github.com/ratatui-org/ratatui) — native Spotify streaming, synced lyrics, a real-time audio visualizer, and optional Local, Subsonic/Navidrome, Internet Radio, and YouTube sources. Spotify is optional.
>
> A community-maintained, actively developed fork of [spotify-tui](https://github.com/Rigellute/spotify-tui).

[![Crates.io](https://img.shields.io/crates/v/spotatui.svg)](https://crates.io/crates/spotatui)
[![Upstream](https://img.shields.io/badge/upstream-Rigellute%2Fspotify--tui-blue)](https://github.com/Rigellute/spotify-tui)
[![X](https://img.shields.io/badge/@LargeModGames-000000?logo=x&logoColor=white)](https://twitter.com/LargeModGames)
[![Songs played using Spotatui](https://img.shields.io/badge/dynamic/json?url=https://spotatui-counter.spotatui.workers.dev&query=count&label=Songs%20played%20using%20spotatui&labelColor=0b0f14&color=1ed760&logo=spotify&logoColor=1ed760&style=flat-square&cacheSeconds=600)](https://github.com/LargeModGames/spotatui)
[![spotatui Contributors](https://img.shields.io/badge/dynamic/json?url=https://raw.githubusercontent.com/LargeModGames/spotatui/main/.all-contributorsrc&query=%24.contributors.length&label=spotatui%20contributors&color=1ed760&style=flat-square)](#spotatui-contributors)
[![Upstream Contributors](https://img.shields.io/badge/upstream_contributors-94-orange.svg?style=flat-square)](#upstream-contributors-spotify-tui)




![Demo](.github/demo.gif)

## Song History

![Song History](https://spotatui-counter.spotatui.workers.dev/chart.svg)



<!-- START doctoc generated TOC please keep comment here to allow auto update -->
<!-- DON'T EDIT THIS SECTION, INSTEAD RE-RUN doctoc TO UPDATE -->
## Table of Contents

- [Features](#features)
- [Installation](#installation)
- [Quickstart](#quickstart)
  - [Adding Spotify later](#adding-spotify-later)
- [Music Sources](#music-sources)
  - [Local Files](#local-files)
  - [Subsonic / Navidrome](#subsonic--navidrome)
  - [Internet Radio](#internet-radio)
  - [YouTube](#youtube)
- [Native Streaming](#native-streaming)
- [Configuration](#configuration)
  - [Discord Rich Presence](#discord-rich-presence)
  - [Anonymous Song Counter](#anonymous-song-counter)
- [Plugins](#plugins)
- [Performance](#performance)
- [Playback Requirements](#playback-requirements)
  - [Deprecated Spotify API Features](#deprecated-spotify-api-features)
- [Using with spotifyd](#using-with-spotifyd)
- [Migrating from spotify-tui](#migrating-from-spotify-tui)
- [Libraries used](#libraries-used)
- [Development](#development)
  - [Windows Subsystem for Linux](#windows-subsystem-for-linux)
- [Help Wanted](#help-wanted)
- [Maintainer](#maintainer)
- [spotatui Contributors](#spotatui-contributors)
- [Upstream Contributors (spotify-tui)](#upstream-contributors-spotify-tui)
- [Star History](#star-history)
- [Roadmap](#roadmap)

<!-- END doctoc generated TOC please keep comment here to allow auto update -->



## Features

- **Multiple sources — Spotify optional.** Play from Spotify, [Local Files](#local-files), a [Subsonic/Navidrome](#subsonic--navidrome) server, [Internet Radio](#internet-radio), or [YouTube](#youtube). The free sources need no Spotify account; press `d` to switch between them at any time.
- **[Native streaming](#native-streaming).** Play Spotify audio directly, no official app or spotifyd required — spotatui appears as its own Spotify Connect device (Premium required).
- **Synced lyrics.** Line-by-line lyrics that follow playback.
- **Real-time audio visualizer.** A system-wide FFT visualizer (press `v`) that reacts to whatever is playing.
- **Cross-source play queue.** Press `z` on any track to queue it — the queue plays across every source before your current context resumes.
- **[Lua plugins](#plugins).** Extend spotatui with event hooks, commands, keybindings, popups, and theming.
- **Listening history & recap.** spotatui keeps a local play history and can generate a shareable HTML recap (`spotatui history recap`).
- **Full CLI.** Most of what the UI does is scriptable — playback, search, playlists, shell completions. Run `spotatui --help`.
- **Lightweight.** ~78 MB RAM while streaming, versus a full Electron client. See [Performance](#performance).

## Installation

> **Spotify is optional.** On first launch spotatui asks which source you want to use. YouTube, Subsonic/Navidrome, Internet Radio, and Local Files all work with no Spotify account. Spotify Premium is only needed for the Spotify source; you can add it anytime from the `d` menu.

```bash
# Homebrew (macOS only)
brew tap LargeModGames/spotatui
brew install spotatui

# Winget (Windows)
winget install spotatui

# Cargo
cargo install --locked spotatui

# Arch Linux (AUR) - pre-built binary (faster)
yay -S spotatui-bin

# Arch Linux (AUR) - build from source
yay -S spotatui

# Void Linux (Unofficial Repo)
echo repository=https://raw.githubusercontent.com/Event-Horizon-VL/blackhole-vl/repository-x86_64 | sudo tee /etc/xbps.d/20-repository-extra.conf
sudo xbps-install -S spotatui
```
```nix
# NixOS (Flake)

# Add spotatui to your flake inputs:
inputs = {
  spotatui = {
    url = "github:LargeModGames/spotatui";
    inputs.nixpkgs.follows = "nixpkgs";
  };
}

# Add the spotatui package from your inputs to your config:
{ inputs, ...}:{
  # Your other configurations
  environment.systemPackages = with pkgs; [
    inputs.spotatui.packages.${pkgs.stdenv.hostPlatform.system}.default
  ];
}
```

Or download pre-built binaries from [GitHub Releases](https://github.com/LargeModGames/spotatui/releases/latest).

See the [Installation Wiki](https://github.com/LargeModGames/spotatui/wiki/Installation) for platform-specific requirements and building from source.

## Quickstart

Run `spotatui`. On the first launch it asks which source to set up:

```
Welcome to spotatui! Choose your music source:

  1) Spotify        (needs login)
  2) YouTube        (free, needs the yt-dlp binary)
  3) Subsonic       (free, needs a Subsonic/Navidrome server)
  4) Internet Radio (free)
  5) Local Files    (free)
```

Pick a **free source** to skip Spotify entirely, or pick **Spotify** to run the auth wizard (you'll create a Spotify Developer app — see the [Installation Wiki](https://github.com/LargeModGames/spotatui/wiki/Installation#connecting-to-spotify)). Only sources compiled into your build are listed.

Once you're in:

- Press `?` for the in-app help menu of all key events.
- Press `d` to open the **Source & Device** picker and switch sources.
- Press `z` on a track to queue it; open the queue with `Shift+Q`.
- Run `spotatui --help` for the CLI. See the [Keybindings Wiki](https://github.com/LargeModGames/spotatui/wiki/Keybindings) for every shortcut.

A few CLI examples to get you started:

```bash
spotatui --completions zsh                       # Shell completions (bash, powershell, and more supported)
spotatui play --name "Your Playlist" --playlist --random  # Play a random song from a playlist
spotatui playback --toggle                       # Play/pause current playback
spotatui list --liked --limit 50                 # List your liked songs
spotatui history recap --period 30d --output ./recap.html  # Generate a shareable listening recap
```

### Adding Spotify later

Started with a free source and want Spotify too? Press `d`, select **Spotify**, and spotatui opens your browser to log in without restarting — enabling browsing, playlists, and controlling external devices right away. **Native (librespot) streaming still requires a restart**, since it initializes at startup.

## Music Sources

spotatui is a general music player, not just a Spotify client. Press `d` to open the **Source & Device** picker; the sidebar and search re-scope to the active source. Playback for these sources runs through spotatui's own audio engine, so volume control and the visualizer work exactly as they do for Spotify — and none of them need Spotify Premium.

| Source | What it does | Needs |
|---|---|---|
| **Local Files** | Browse and play a folder of audio files (FLAC, MP3, OGG, WAV, …) | Nothing; set `local_music_path` or use the OS music dir |
| **Subsonic** | Browse, search, and stream from any Subsonic-compatible server (Navidrome, Gonic, Airsonic, Funkwhale, …) | A server account |
| **Internet Radio** | Play icecast/shoutcast streams with live now-playing metadata; search the [radio-browser.info](https://www.radio-browser.info) directory (30k+ stations) | Nothing |
| **YouTube** | Search YouTube and play audio; build **local playlists** stored in a plain file | [`yt-dlp`](https://github.com/yt-dlp/yt-dlp) on your `PATH` (ffmpeg recommended) |

**Resuming your last session:** quit while playing from a non-Spotify source and spotatui restores that track and its position on the next launch, following the `startup_behavior` setting (`continue`, `play`, or `pause`).

**Availability:** included in the Linux and Windows release binaries. Not yet on macOS (the shared audio output path is disabled there pending a fix; contributions welcome). When building from source, enable them with cargo features:

```bash
cargo install --locked spotatui --features local-files,subsonic,internet-radio,youtube
```

Each source has a few config keys; the essentials are below, and the full reference lives in the [Configuration Wiki](https://github.com/LargeModGames/spotatui/wiki/Configuration).

### Local Files

Set a folder (defaults to the OS music directory), then pick **Local Files** in the `d` picker:

```yaml
behavior:
  local_music_path: "/home/you/Music"
```

### Subsonic / Navidrome

```yaml
behavior:
  subsonic_url: "https://music.example.com"
  subsonic_username: "you"
```

Prefer setting the password via the `SPOTATUI_SUBSONIC_PASSWORD` environment variable so it never sits in the config file in plaintext.

### Internet Radio

Search the radio-browser.info directory in-app (Enter plays a station directly), and press the save key (`F` by default) to keep a station in your sidebar. Saved stations live under `behavior.radio_stations`; the playbar shows a `LIVE` badge with the stream's now-playing title.

### YouTube

Requires the [`yt-dlp`](https://github.com/yt-dlp/yt-dlp) binary (`ffmpeg` recommended). No Google account, API key, or cookies — search and playback are anonymous. If playback breaks after a YouTube change, updating yt-dlp (`yt-dlp -U`) is the fix; no spotatui update needed.

**Local YouTube playlists** live in `~/.config/spotatui/youtube_playlists.yml`, a plain human-editable file you can back up or share. Create one from the sidebar, add tracks with `w`, and play a playlist as a queue with `Enter`.

## Native Streaming

spotatui can play Spotify audio directly, without spotifyd or the official app — just run it and it appears as a Spotify Connect device.

- Premium account required
- Works with media keys, MPRIS (Linux), and macOS Now Playing
- Runs on our maintained [librespot fork](https://github.com/LargeModGames/spotatui-librespot), which backports upstream fixes for Spotify's evolving audio delivery (e.g. the HTTP 530 CDN issue that silenced native playback)

See the [Native Streaming Wiki](https://github.com/LargeModGames/spotatui/wiki/Native-Streaming) for setup details.

## Configuration

The config file is at `${HOME}/.config/spotatui/config.yml`. You can also configure spotatui in-app by pressing `Alt-,` to open Settings.

Nearly everything is customizable: keybindings, themes, icons, playbar button labels, status-line and window-title format templates, table columns (reorder/rename/resize), default sorting per screen, startup screen, and layout (sidebar/playbar position). Invalid values fall back to defaults with a logged warning — a config typo never blocks startup.

- Customization guide: [`docs/configuration.md`](docs/configuration.md), with a commented [`examples/config.example.yml`](examples/config.example.yml)
- Full config reference: [Configuration Wiki](https://github.com/LargeModGames/spotatui/wiki/Configuration)
- Built-in themes (Spotify, Dracula, Nord, …): [Themes Wiki](https://github.com/LargeModGames/spotatui/wiki/Themes)

spotatui also stores local listening history at `${HOME}/.config/spotatui/history/listens.jsonl`, which powers `spotatui history recap`. Short or skipped plays are stored but excluded from recap totals.

### Discord Rich Presence

Enabled by default using the built-in spotatui application ID, so no setup is required. Optional overrides:

```yaml
behavior:
  enable_discord_rpc: true
  discord_rpc_client_id: "your_client_id"
```

You can also override the app ID via `SPOTATUI_DISCORD_APP_ID`, or disable it in Settings or with `behavior.enable_discord_rpc: false`.

### Anonymous Song Counter

spotatui includes an opt-in global counter showing how many songs have been played by all users worldwide (the badge and chart at the top of this README). It is **completely anonymous** — no personal information, song names, artists, or listening history is collected; it only sends a simple increment when a new song starts. It is enabled by default and can be disabled with `enable_global_song_count: false` in `~/.config/spotatui/config.yml`. This is purely a fun community metric with zero tracking of individual users.

### GitHub Profile Widget

Show what you're listening to as a live card on your GitHub profile. Create an account at [spotatui.com](https://spotatui.com), paste your sync token into Settings → `sync_token`, then pick a public username and enable the widget on your dashboard. Add this to your profile README:

```markdown
[![Now playing on spotatui](https://spotatui.com/widget/your-username.svg)](https://spotatui.com)
```

The card shows cover art, title, artist, a progress bar, and an animated equalizer while playing; internet radio gets a LIVE badge. Append `?theme=light` for the light variant. Only your current track is public, and only after you opt in.

## Plugins

spotatui runs user-written Lua plugins. They react to playback events, add commands and key bindings, draw popups and playbar segments, restyle the theme, and make async HTTP requests. Install one published as a git repository (requires `git`):

```bash
spotatui plugin add owner/repo
spotatui plugin list
spotatui plugin update
spotatui plugin remove <name>
```

See [`PLUGINS.md`](PLUGINS.md) for the ecosystem overview, [`examples/plugins/`](examples/plugins) for runnable examples, and [`docs/scripting.md`](docs/scripting.md) for the full API reference.

## Performance

spotatui is extremely lightweight compared to the official Electron client.

| Mode                            | RAM Usage |
| :------------------------------ | :-------- |
| **Native Streaming (Base)**     | ~78 MB    |
| **With Synced Lyrics**          | ~78 MB    |
| **With System-Wide Visualizer** | ~80 MB    |

*Tested on Arch Linux (Hyprland).*

## Playback Requirements

The free sources (Local Files, Subsonic, Internet Radio, YouTube) play through spotatui's own audio engine and need no extra setup.

Spotify is different: it uses the [Web API](https://developer.spotify.com/documentation/web-api/), which doesn't stream audio itself. To play Spotify tracks you need **one** of:

1. **Native Streaming** — spotatui plays audio directly using its built-in streaming. See [Native Streaming](#native-streaming). *(Recommended.)*
2. **Official Spotify Client** — have the official app open on your computer.
3. **[spotifyd](https://github.com/Spotifyd/spotifyd)** — a lightweight background alternative.

Playing Spotify tracks requires a **Premium** account. With a free Spotify account spotatui can authenticate and browse your library/search results, but playback actions (play/pause/seek/transfer) will not work in either native streaming or Web API playback control mode.

### Deprecated Spotify API Features

As of November 2024, Spotify removed access to certain API endpoints for new applications. The following features **only work if your Spotify Developer application was created before November 27, 2024**:

- **Related Artists** — the "Related Artists" section on an artist page.
- **Audio Analysis** — spotatui no longer depends on it. The **audio visualizer** (press `v`) now uses **local real-time FFT analysis** of your system audio, so it works regardless of your app's creation date:

  | Platform    | Status               | Notes                                    |
  | ----------- | -------------------- | ---------------------------------------- |
  | **Windows** | Works out of the box | Uses WASAPI loopback                     |
  | **Linux**   | Works out of the box | Uses PipeWire/PulseAudio monitor devices |
  | **macOS**   | Requires setup       | Needs a virtual audio device (see below) |

  > **macOS:** macOS doesn't natively expose system audio loopback. Install a virtual audio device like [BlackHole](https://github.com/ExistentialAudio/BlackHole) (free) or [Loopback](https://rogueamoeba.com/loopback/) (paid), route system audio through it, and set it as your default input device.
  >
  > **Note:** The visualizer is **system-wide** — it captures all audio on your system, so it also reacts to YouTube videos, games, and any other source.

For more information, see [Spotify's announcement about API changes](https://developer.spotify.com/blog/2024-11-27-changes-to-the-web-api).

## Using with [spotifyd](https://github.com/Spotifyd/spotifyd)

> **Note:** If you're using native streaming, you don't need spotifyd!

Follow the spotifyd documentation to get set up. After that:

1. Start the spotifyd daemon.
1. Start `spotatui`.
1. Press `d` to open the device selection menu — the spotifyd "device" should appear (if not, check [these docs](https://github.com/Spotifyd/spotifyd#logging)).

## Migrating from spotify-tui

If you used the original `spotify-tui` before:

- The binary name changed from `spt` to `spotatui`.
- Config paths changed: `~/.config/spotify-tui/` → `~/.config/spotatui/`.

You can copy your existing config:

```bash
mkdir -p ~/.config/spotatui
cp -r ~/.config/spotify-tui/* ~/.config/spotatui/
```

You may be asked to re-authenticate with Spotify the first time.

## Libraries used

- [ratatui](https://github.com/ratatui-org/ratatui) - Terminal UI framework
- [rspotify](https://github.com/ramsayleung/rspotify) - Spotify Web API client
- [librespot](https://github.com/librespot-org/librespot) - Spotify Connect streaming (via our maintained fork [spotatui-librespot](https://github.com/LargeModGames/spotatui-librespot), which backports fixes for Spotify's CDN changes)
- [tokio](https://github.com/tokio-rs/tokio) - Async runtime
- [crossterm](https://github.com/crossterm-rs/crossterm) - Terminal manipulation
- [clap](https://github.com/clap-rs/clap) - CLI argument parsing

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

   # NixOS
   nix develop github:LargeModGames/spotatui
   ```
1. Clone or fork this repo and `cd` to it
1. And then `cargo run`

See [CONTRIBUTING.md](CONTRIBUTING.md) for pull request guidelines.

### Windows Subsystem for Linux

You might get a linking error. If so, you'll probably need to install additional dependencies required by the clipboard package:

```bash
sudo apt-get install -y -qq pkg-config libssl-dev libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev
```

## Help Wanted

**spotatui is currently maintained by a solo developer.** More contributors would be hugely appreciated — and **you don't need to write code to help**:

- **Star the repo** to help others discover the project
- **Report bugs** or request features in [Issues](https://github.com/LargeModGames/spotatui/issues)
- **Join the community** in [Discussions](https://github.com/LargeModGames/spotatui/discussions)
- **Submit a PR** for code, docs, or themes

See [CONTRIBUTING.md](CONTRIBUTING.md) for more details!

## Maintainer

Maintained by **[LargeModGames](https://github.com/LargeModGames)** ([@LargeModGames](https://twitter.com/LargeModGames) on Twitter).

Originally forked from [spotify-tui](https://github.com/Rigellute/spotify-tui) by [Alexander Keliris](https://github.com/Rigellute).

## spotatui Contributors

**Looking for contributors!** spotatui is actively maintained but could use your help. Whether it's bug fixes, new features, documentation, or testing - all contributions are welcome!

<!-- ALL-CONTRIBUTORS-LIST:START - Do not remove or modify this section -->
<!-- prettier-ignore-start -->
<!-- markdownlint-disable -->
<table>
  <tbody>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/LargeModGames"><img src="https://avatars.githubusercontent.com/u/84450916?v=4?s=100" width="100px;" alt="LargeModGames"/><br /><sub><b>LargeModGames</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=LargeModGames" title="Code">💻</a> <a href="https://github.com/LargeModGames/spotatui/commits?author=LargeModGames" title="Documentation">📖</a> <a href="#maintenance-LargeModGames" title="Maintenance">🚧</a> <a href="#ideas-LargeModGames" title="Ideas, Planning, & Feedback">🤔</a> <a href="#infra-LargeModGames" title="Infrastructure (Hosting, Build-Tools, etc)">🚇</a> <a href="https://github.com/LargeModGames/spotatui/commits?author=LargeModGames" title="Tests">⚠️</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/MysteriousWolf"><img src="https://avatars.githubusercontent.com/u/5306409?v=4?s=100" width="100px;" alt="MysteriousWolf"/><br /><sub><b>MysteriousWolf</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=MysteriousWolf" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/rawcode1337"><img src="https://avatars.githubusercontent.com/u/80097670?v=4?s=100" width="100px;" alt="rawcode1337"/><br /><sub><b>rawcode1337</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=rawcode1337" title="Code">💻</a> <a href="https://github.com/LargeModGames/spotatui/issues?q=author%3Arawcode1337" title="Bug reports">🐛</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/copeison"><img src="https://avatars.githubusercontent.com/u/184175589?v=4?s=100" width="100px;" alt="copeison"/><br /><sub><b>copeison</b></sub></a><br /><a href="#platform-copeison" title="Packaging/porting to new platform">📦</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/jacklorusso"><img src="https://avatars.githubusercontent.com/u/19835679?v=4?s=100" width="100px;" alt="jacklorusso"/><br /><sub><b>jacklorusso</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=jacklorusso" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/H41L33"><img src="https://avatars.githubusercontent.com/u/140116782?v=4?s=100" width="100px;" alt="H41L33"/><br /><sub><b>H41L33</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=H41L33" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://mzte.de"><img src="https://avatars.githubusercontent.com/u/28735087?v=4?s=100" width="100px;" alt="LordMZTE"/><br /><sub><b>LordMZTE</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=LordMZTE" title="Code">💻</a> <a href="#platform-LordMZTE" title="Packaging/porting to new platform">📦</a></td>
    </tr>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/bodoque-01"><img src="https://avatars.githubusercontent.com/u/63447579?v=4?s=100" width="100px;" alt="Sebastian Sarco"/><br /><sub><b>Sebastian Sarco</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=bodoque-01" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/El-Mundos"><img src="https://avatars.githubusercontent.com/u/70759168?v=4?s=100" width="100px;" alt="Sergio Tabernero Hernández"/><br /><sub><b>Sergio Tabernero Hernández</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=El-Mundos" title="Code">💻</a> <a href="#platform-El-Mundos" title="Packaging/porting to new platform">📦</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://dpnova.github.io/"><img src="https://avatars.githubusercontent.com/u/229943?v=4?s=100" width="100px;" alt="David Novakovic"/><br /><sub><b>David Novakovic</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=dpnova" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="http://nthpaul.com"><img src="https://avatars.githubusercontent.com/u/70828466?v=4?s=100" width="100px;" alt="Paul"/><br /><sub><b>Paul</b></sub></a><br /><a href="#design-nthpaul" title="Design">🎨</a> <a href="https://github.com/LargeModGames/spotatui/commits?author=nthpaul" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/AFE123x"><img src="https://avatars.githubusercontent.com/u/121839885?v=4?s=100" width="100px;" alt="Arun Felix"/><br /><sub><b>Arun Felix</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=AFE123x" title="Code">💻</a> <a href="https://github.com/LargeModGames/spotatui/issues?q=author%3AAFE123x" title="Bug reports">🐛</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/MaySeikatsu"><img src="https://avatars.githubusercontent.com/u/127960577?v=4?s=100" width="100px;" alt="MaySeikatsu"/><br /><sub><b>MaySeikatsu</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=MaySeikatsu" title="Code">💻</a> <a href="https://github.com/LargeModGames/spotatui/commits?author=MaySeikatsu" title="Documentation">📖</a> <a href="#platform-MaySeikatsu" title="Packaging/porting to new platform">📦</a></td>
      <td align="center" valign="top" width="14.28%"><a href="http://prabo.org"><img src="https://avatars.githubusercontent.com/u/32436755?v=4?s=100" width="100px;" alt="Lorenzo Bodini"/><br /><sub><b>Lorenzo Bodini</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=topongo" title="Code">💻</a> <a href="#design-topongo" title="Design">🎨</a></td>
    </tr>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/Vi1i"><img src="https://avatars.githubusercontent.com/u/6485370?v=4?s=100" width="100px;" alt="Vi1i Petal"/><br /><sub><b>Vi1i Petal</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=Vi1i" title="Code">💻</a> <a href="https://github.com/LargeModGames/spotatui/issues?q=author%3AVi1i" title="Bug reports">🐛</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/wrsturgeon"><img src="https://avatars.githubusercontent.com/u/79714036?v=4?s=100" width="100px;" alt="Will Sturgeon"/><br /><sub><b>Will Sturgeon</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=wrsturgeon" title="Code">💻</a> <a href="https://github.com/LargeModGames/spotatui/issues?q=author%3Awrsturgeon" title="Bug reports">🐛</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/wfinken"><img src="https://avatars.githubusercontent.com/u/891256?v=4?s=100" width="100px;" alt="wfinken"/><br /><sub><b>wfinken</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=wfinken" title="Code">💻</a> <a href="https://github.com/LargeModGames/spotatui/issues?q=author%3Awfinken" title="Bug reports">🐛</a></td>
      <td align="center" valign="top" width="14.28%"><a href="http://kathund.dev"><img src="https://avatars.githubusercontent.com/u/55346310?v=4?s=100" width="100px;" alt="Jacob"/><br /><sub><b>Jacob</b></sub></a><br /><a href="#platform-Kathund" title="Packaging/porting to new platform">📦</a></td>
      <td align="center" valign="top" width="14.28%"><a href="http://dominicklee.net"><img src="https://avatars.githubusercontent.com/u/43938540?v=4?s=100" width="100px;" alt="Dominick Lee"/><br /><sub><b>Dominick Lee</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=domogami" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/higorprado"><img src="https://avatars.githubusercontent.com/u/1037397?v=4?s=100" width="100px;" alt="Higor Prado"/><br /><sub><b>Higor Prado</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=higorprado" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/eiseq"><img src="https://avatars.githubusercontent.com/u/80960013?v=4?s=100" width="100px;" alt="Vitali Kaplich"/><br /><sub><b>Vitali Kaplich</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=eiseq" title="Documentation">📖</a></td>
    </tr>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/1knth"><img src="https://avatars.githubusercontent.com/u/115324660?v=4?s=100" width="100px;" alt="knth"/><br /><sub><b>knth</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=1knth" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="http://www.dev.arynwood.com"><img src="https://avatars.githubusercontent.com/u/64027767?v=4?s=100" width="100px;" alt="Lorelei Noble"/><br /><sub><b>Lorelei Noble</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=SkyeVault" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://benalleng.com"><img src="https://avatars.githubusercontent.com/u/108441023?v=4?s=100" width="100px;" alt="Ben Allen"/><br /><sub><b>Ben Allen</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=benalleng" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/shilicioo"><img src="https://avatars.githubusercontent.com/u/56956072?v=4?s=100" width="100px;" alt="shilicioo"/><br /><sub><b>shilicioo</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=shilicioo" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/Moaht"><img src="https://avatars.githubusercontent.com/u/93605688?v=4?s=100" width="100px;" alt="Thomas Allan"/><br /><sub><b>Thomas Allan</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=Moaht" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="http://www.pratyoosh.me"><img src="https://avatars.githubusercontent.com/u/170789120?v=4?s=100" width="100px;" alt="Pratyoosh Prakash"/><br /><sub><b>Pratyoosh Prakash</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=rlpratyoosh" title="Code">💻</a> <a href="https://github.com/LargeModGames/spotatui/issues?q=author%3Arlpratyoosh" title="Bug reports">🐛</a></td>
      <td align="center" valign="top" width="14.28%"><a href="http://tahmid.io"><img src="https://avatars.githubusercontent.com/u/107484759?v=4?s=100" width="100px;" alt="Tahmid Ahmed"/><br /><sub><b>Tahmid Ahmed</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=tahminator" title="Code">💻</a></td>
    </tr>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/Ritze03"><img src="https://avatars.githubusercontent.com/u/63347117?v=4?s=100" width="100px;" alt="Ritze"/><br /><sub><b>Ritze</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=Ritze03" title="Code">💻</a> <a href="https://github.com/LargeModGames/spotatui/issues?q=author%3ARitze03" title="Bug reports">🐛</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/itlogsandwich"><img src="https://avatars.githubusercontent.com/u/155230216?v=4?s=100" width="100px;" alt="Hansen"/><br /><sub><b>Hansen</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=itlogsandwich" title="Documentation">📖</a></td>
    </tr>
  </tbody>
</table>

<!-- markdownlint-restore -->
<!-- prettier-ignore-end -->

<!-- ALL-CONTRIBUTORS-LIST:END -->

*Want to see your name here? Check out our [open issues](https://github.com/LargeModGames/spotatui/issues) or the [Roadmap](#roadmap) below!*

---

## Upstream Contributors (spotify-tui)

Thanks to all the contributors who built the original [spotify-tui](https://github.com/Rigellute/spotify-tui) that this project is forked from:

<table>
  <tbody>
    <tr>
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

<a href="https://www.star-history.com/?type=date&legend=top-left&repos=LargeModGames%2Fspotatui">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=LargeModGames/spotatui&type=date&theme=dark&legend=top-left&sealed_token=iryqtVEb4hcMJpt3uuX43JaGNlZRMV4-stDO4M8-7zH7g186IdrwSCU1PMWBv_YdK7DC4y6qw4sEKu9JXmHbR6c7clp1u8sL1AXI3D_dMW_KTnIjJYKDGw" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=LargeModGames/spotatui&type=date&legend=top-left&sealed_token=iryqtVEb4hcMJpt3uuX43JaGNlZRMV4-stDO4M8-7zH7g186IdrwSCU1PMWBv_YdK7DC4y6qw4sEKu9JXmHbR6c7clp1u8sL1AXI3D_dMW_KTnIjJYKDGw" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=LargeModGames/spotatui&type=date&legend=top-left&sealed_token=iryqtVEb4hcMJpt3uuX43JaGNlZRMV4-stDO4M8-7zH7g186IdrwSCU1PMWBv_YdK7DC4y6qw4sEKu9JXmHbR6c7clp1u8sL1AXI3D_dMW_KTnIjJYKDGw" />
 </picture>
</a>

## Roadmap

The goal is to eventually implement almost every Spotify feature.

**High-priority features:**
- Scroll through result pages in every view

See the [Roadmap Wiki](https://github.com/LargeModGames/spotatui/wiki/Roadmap) for the full API coverage table.
