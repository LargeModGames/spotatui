# spotatui

> A Spotify client for the terminal written in Rust, powered by [Ratatui](https://github.com/ratatui-org/ratatui).
>
> A community-maintained fork of [spotify-tui](https://github.com/Rigellute/spotify-tui), actively developed with new features like native streaming, synced lyrics, and real-time audio visualization.

[![Crates.io](https://img.shields.io/crates/v/spotatui.svg)](https://crates.io/crates/spotatui)
[![Upstream](https://img.shields.io/badge/upstream-Rigellute%2Fspotify--tui-blue)](https://github.com/Rigellute/spotify-tui)
[![Songs played using Spotatui](https://img.shields.io/badge/dynamic/json?url=https://spotatui-counter.spotatui.workers.dev&query=count&label=Songs%20played%20using%20spotatui&labelColor=0b0f14&color=1ed760&logo=spotify&logoColor=1ed760&style=flat-square&cacheSeconds=600)](https://github.com/LargeModGames/spotatui)
[![spotatui Contributors](https://img.shields.io/badge/spotatui_contributors-6-1ed760.svg?style=flat-square)](#spotatui-contributors)
[![Upstream Contributors](https://img.shields.io/badge/upstream_contributors-94-orange.svg?style=flat-square)](#upstream-contributors-spotify-tui)


![Demo](.github/demo.gif)

## Song History

![Song History](https://spotatui-counter.spotatui.workers.dev/chart.svg)



<!-- START doctoc generated TOC please keep comment here to allow auto update -->
<!-- DON'T EDIT THIS SECTION, INSTEAD RE-RUN doctoc TO UPDATE -->
## Table of Contents

- [Help Wanted](#help-wanted)
- [Performance](#performance)
- [Privacy Notice](#privacy-notice)
- [Migrating from spotify-tui](#migrating-from-spotify-tui)
- [Installation](#installation)
- [Connecting to Spotify](#connecting-to-spotify)
- [Usage](#usage)
- [Native Streaming](#native-streaming)
- [Configuration](#configuration)
  - [Discord Rich Presence](#discord-rich-presence)
- [Limitations](#limitations)
  - [Deprecated Spotify API Features](#deprecated-spotify-api-features)
- [Using with spotifyd](#using-with-spotifyd)
- [Libraries used](#libraries-used)
- [Development](#development)
  - [Windows Subsystem for Linux](#windows-subsystem-for-linux)
- [Maintainer](#maintainer)
- [spotatui Contributors](#spotatui-contributors)
- [Upstream Contributors (spotify-tui)](#upstream-contributors-spotify-tui)
- [Star History](#star-history)
- [Roadmap](#roadmap)

<!-- END doctoc generated TOC please keep comment here to allow auto update -->



## Help Wanted

**spotatui is currently maintained by a solo developer.** More contributors would be hugely appreciated! Here's how you can help:

- **Star the repo** to help others discover the project
- **Report bugs** or request features in [Issues](https://github.com/LargeModGames/spotatui/issues)
- **Join the community** in [Discussions](https://github.com/LargeModGames/spotatui/discussions)
- **Submit a PR** for code, docs, or themes

See [CONTRIBUTING.md](CONTRIBUTING.md) for more details!

## Performance

spotatui is extremely lightweight compared to the official Electron client.

| Mode                            | RAM Usage |
| :------------------------------ | :-------- |
| **Native Streaming (Base)**     | ~48 MB    |
| **With Synced Lyrics**          | ~48 MB    |
| **With System-Wide Visualizer** | ~56 MB    |

*Tested on Arch Linux (Hyprland).*

## Privacy Notice

**Anonymous Global Counter**: spotatui includes an opt-in feature that contributes to a global counter showing how many songs have been played by all users worldwide. This feature:

- **Is completely anonymous** - no personal information, song names, artists, or listening history is collected
- **Only sends a simple increment** when a new song starts playing
- **Is enabled by default** but can be opted out at any time
- **Can be disabled** by setting `enable_global_song_count: false` in `~/.config/spotatui/config.yml`

We respect your privacy. This is purely a fun community metric with zero tracking of individual users.

## Migrating from spotify-tui

If you used the original `spotify-tui` before:

- The binary name changed from `spt` to `spotatui`.
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

```bash
# Homebrew (macOS/Linux)
brew tap LargeModGames/spotatui
brew install spotatui

# Winget (Windows)
winget install spotatui

# Cargo
cargo install spotatui

# Arch Linux (AUR) - pre-built binary (faster)
yay -S spotatui-bin

# Arch Linux (AUR) - build from source
yay -S spotatui
```

Or download pre-built binaries from [GitHub Releases](https://github.com/LargeModGames/spotatui/releases/latest).

See the [Installation Wiki](https://github.com/LargeModGames/spotatui/wiki/Installation) for platform-specific requirements and building from source.

## Connecting to Spotify

Run `spotatui` and follow the on-screen instructions to connect your Spotify account.

You'll need to create a Spotify Developer app at the [Spotify Dashboard](https://developer.spotify.com/dashboard/applications).

See the [Installation Wiki](https://github.com/LargeModGames/spotatui/wiki/Installation#connecting-to-spotify) for step-by-step setup.

## Usage

The binary is named `spotatui`.

Running `spotatui` with no arguments will bring up the UI. Press `?` to bring up a help menu that shows currently implemented key events and their actions.
There is also a CLI that is able to do most of the stuff the UI does. Use `spotatui --help` to learn more.

See [Keybindings Wiki](https://github.com/LargeModGames/spotatui/wiki/Keybindings) for the full list of keyboard shortcuts.

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

## Native Streaming

spotatui can play audio directly without needing spotifyd or the official Spotify app. Just run `spotatui` and it will appear as a Spotify Connect device.

- Works with media keys, MPRIS (Linux), and macOS Now Playing
- Premium account required

See the [Native Streaming Wiki](https://github.com/LargeModGames/spotatui/wiki/Native-Streaming) for setup details.

## Configuration

A configuration file is located at `${HOME}/.config/spotatui/config.yml`.

See the [Configuration Wiki](https://github.com/LargeModGames/spotatui/wiki/Configuration) for the full config file reference.

You can also configure spotatui in-app by pressing `Alt-,` to open Settings.

See [Themes Wiki](https://github.com/LargeModGames/spotatui/wiki/Themes) for built-in presets (Spotify, Dracula, Nord, etc.).

### Discord Rich Presence

Discord Rich Presence is enabled by default and uses the built-in spotatui application ID, so no extra setup is required.

Overrides (optional):

```yaml
behavior:
  enable_discord_rpc: true
  discord_rpc_client_id: "your_client_id"
```

You can also override via `SPOTATUI_DISCORD_APP_ID` or disable in the setting or by setting `behavior.enable_discord_rpc: false` in ~/.config/spotatui/config.yml.

## Limitations

This app uses the [Web API](https://developer.spotify.com/documentation/web-api/) from Spotify, which doesn't handle streaming itself. You have three options for audio playback:

1. **Native Streaming (NEW!)** - spotatui can now play audio directly using its built-in streaming feature. See [Native Streaming](#native-streaming) below.
2. **Official Spotify Client** - Have the official Spotify app open on your computer
3. **Spotifyd** - Use a lightweight alternative like [spotifyd](https://github.com/Spotifyd/spotifyd)

If you want to play tracks, Spotify requires that you have a Premium account.

### Deprecated Spotify API Features

**Note:** As of November 2024, Spotify deprecated and removed access to certain API endpoints for new applications. The following features are included in this app but **will only work if your Spotify Developer application was created before November 27, 2024**:

- **Audio Visualization** (press `v`): Now uses **local real-time FFT analysis** of your system audio. The visualization no longer depends on Spotify's deprecated Audio Analysis API.

  **Platform Support:**
  | Platform    | Status               | Notes                                    |
  | ----------- | -------------------- | ---------------------------------------- |
  | **Windows** | Works out of the box | Uses WASAPI loopback                     |
  | **Linux**   | Works out of the box | Uses PipeWire/PulseAudio monitor devices |
  | **macOS**   | Requires setup       | Needs a virtual audio device (see below) |

  > **macOS Users:** macOS doesn't natively expose system audio loopback. To use audio visualization, install a virtual audio device like [BlackHole](https://github.com/ExistentialAudio/BlackHole) (free) or [Loopback](https://rogueamoeba.com/loopback/) (paid). Configure it to capture system audio and set it as your default input device.

  > **Note:** The audio visualization is **system-wide** - it captures all audio playing on your system, not just Spotify. This means it will also react to YouTube videos, games, or any other audio source!
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

- [ratatui](https://github.com/ratatui-org/ratatui) - Terminal UI framework
- [rspotify](https://github.com/ramsayleung/rspotify) - Spotify Web API client
- [librespot](https://github.com/librespot-org/librespot) - Spotify Connect streaming
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
      <td align="center" valign="top" width="14.28%"><a href="https://mzte.de"><img src="https://avatars.githubusercontent.com/u/28735087?v=4?s=100" width="100px;" alt="LordMZTE"/><br /><sub><b>LordMZTE</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=LordMZTE" title="Code">💻</a></td>
    </tr>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/bodoque-01"><img src="https://avatars.githubusercontent.com/u/63447579?v=4?s=100" width="100px;" alt="Sebastian Sarco"/><br /><sub><b>Sebastian Sarco</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=bodoque-01" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/El-Mundos"><img src="https://avatars.githubusercontent.com/u/70759168?v=4?s=100" width="100px;" alt="Sergio Tabernero Hernández"/><br /><sub><b>Sergio Tabernero Hernández</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=El-Mundos" title="Code">💻</a> <a href="#platform-El-Mundos" title="Packaging/porting to new platform">📦</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://dpnova.github.io/"><img src="https://avatars.githubusercontent.com/u/229943?v=4?s=100" width="100px;" alt="David Novakovic"/><br /><sub><b>David Novakovic</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=dpnova" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="http://nthpaul.com"><img src="https://avatars.githubusercontent.com/u/70828466?v=4?s=100" width="100px;" alt="Paul"/><br /><sub><b>Paul</b></sub></a><br /><a href="#design-nthpaul" title="Design">🎨</a> <a href="https://github.com/LargeModGames/spotatui/commits?author=nthpaul" title="Code">💻</a></td>
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

[![Star History Chart](https://api.star-history.com/svg?repos=LargeModGames/spotatui&type=date&legend=top-left)](https://star-history.com/#LargeModGames/spotatui&type=date&legend=top-left)

## Roadmap

The goal is to eventually implement almost every Spotify feature.

**High-priority features:**
- Add songs to a playlist
- Scroll through result pages in every view

See the [Roadmap Wiki](https://github.com/LargeModGames/spotatui/wiki/Roadmap) for the full API coverage table.
