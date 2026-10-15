<h1 align="center">
  <img src=".github/spotatui-logo.png" alt="spotatui logo" width="200"><br>
  spotatui
</h1>
<p align="center">
  A fast music player for the terminal and the desktop, written in Rust.<br>
  Spotify through its own built-in client, plus local files, Subsonic/Navidrome, internet radio, YouTube and Qobuz, and Apple Music on macOS.
</p>
<p align="center">
  <a href="#installation">Install</a> · <a href="docs/README.md">Docs</a> · <a href="docs/installation.md#desktop-app">Desktop app</a> · <a href="CONTRIBUTING.md">Contributing</a> · <a href="https://spotatui.com/discord">Discord</a>
</p>

[![Crates.io](https://img.shields.io/crates/v/spotatui.svg)](https://crates.io/crates/spotatui)
[![Discord](https://img.shields.io/discord/1534820237122207815?logo=discord&logoColor=white&label=Discord&color=5865F2)](https://spotatui.com/discord)
[![Songs played using spotatui](https://img.shields.io/badge/dynamic/json?url=https://spotatui-counter.spotatui.workers.dev&query=count&label=Songs%20played%20using%20spotatui&labelColor=0b0f14&color=1ed760&style=flat-square&cacheSeconds=600)](#song-history)
[![spotatui Contributors](https://img.shields.io/badge/dynamic/json?url=https://raw.githubusercontent.com/LargeModGames/spotatui/main/.all-contributorsrc&query=%24.contributors.length&label=spotatui%20contributors&color=1ed760&style=flat-square)](#spotatui-contributors)

![Demo](.github/demo.gif)

<!-- START doctoc generated TOC please keep comment here to allow auto update -->
<!-- DON'T EDIT THIS SECTION, INSTEAD RE-RUN doctoc TO UPDATE -->
## Table of Contents

- [About](#about)
- [Installation](#installation)
- [Quickstart](#quickstart)
- [Sources](#sources)
- [Learn more](#learn-more)
- [Contributing](#contributing)
- [Song History](#song-history)
- [Star History](#star-history)
- [Maintainers](#maintainers)
- [Code signing policy](#code-signing-policy)
- [spotatui Contributors](#spotatui-contributors)

<!-- END doctoc generated TOC please keep comment here to allow auto update -->

## About

spotatui is a music player for the terminal, with a desktop app in preview. Spotify is optional: it is one of seven sources.

- Synced lyrics, and a real-time audio visualizer.
- A play queue that mixes tracks from different sources.
- [Playlist sync](docs/playlist-sync.md): mirror one playlist onto Spotify, Qobuz, Subsonic or YouTube.
- [Lua plugins](PLUGINS.md) for commands, keys, popups and themes.
- An [AI DJ](docs/ai-dj.md) inside the app, or your coding agent as the DJ over [MCP](docs/mcp-setup.md).
- A local listening history with stats and a shareable recap.
- A [CLI](docs/cli.md) for playback, search and playlists.

**Spotify needs Premium.** spotatui logs in with its own built-in client, so you need no developer app and no client ID.

spotatui started as a fork of [spotify-tui](https://github.com/Rigellute/spotify-tui) and has since grown its own player, sources and desktop app.

## Installation

Spotify is optional: every other source works without an account.

| System | Command |
|---|---|
| macOS, Linux, WSL | `curl -fsSL https://spotatui.com/install.sh \| bash` |
| Windows | `irm https://spotatui.com/install.ps1 \| iex` or `winget install spotatui` |
| Arch Linux | `yay -S spotatui-bin` |
| With Rust | `cargo install --locked spotatui` |

Homebrew, Nix, Void, `.deb` packages and building from source are in the [installation guide](docs/installation.md).

- **Desktop app:** download the installer for your system from the [latest release](https://github.com/LargeModGames/spotatui/releases/latest). See [Desktop app](docs/installation.md#desktop-app).
- **Features per build:** the prebuilt terminal binaries include every source, the MCP server and the AI DJ. `cargo install` builds Spotify only unless you add [the features](docs/installation.md#which-build-has-which-features), and the Nix flake builds Spotify only.
- **Updates:** the terminal app updates itself. If a package manager installed it, set `behavior.disable_auto_update: true`. See [Updating](docs/installation.md#updating).

## Quickstart

Run `spotatui`. The first launch asks which sources you want. Pick Spotify to log in: spotatui shows a code and opens Spotify's pairing page, where you confirm it.

Then:

- `?` lists every key.
- `/` searches. Choose the **Songs** panel with the arrow keys, press `Enter` to go into it, and `Enter` again to play a track.
- `d` switches the source and the device.
- `z` queues a track, and `Shift+Q` opens the queue.
- `spotatui --help` shows the [CLI](docs/cli.md).

## Sources

| Source | You need |
|---|---|
| **Spotify** | A Premium account |
| **Local Files** | A music folder |
| **Subsonic / Navidrome** | A server account |
| **Internet Radio** | Nothing (30k+ stations from [radio-browser.info](https://www.radio-browser.info)) |
| **YouTube** | [yt-dlp](https://github.com/yt-dlp/yt-dlp) on your `PATH` |
| **Qobuz** | A paid Qobuz account |
| **Apple Music** | macOS and the Music app |

Local Files, Subsonic, Internet Radio, YouTube and Qobuz play through spotatui's own audio engine. Apple Music plays in the Music app, and spotatui is its remote. The setup of each source is in [Configuration](docs/configuration.md).

Spotify plays in spotatui itself, on any Spotify Connect device, or on Cast and Connect speakers on your network. See [Where Spotify plays](docs/native-streaming.md#where-spotify-plays).

## Learn more

| You want to | Read |
|---|---|
| Install another way, or build from source | [Installation](docs/installation.md) |
| Change settings, keys or the theme | [Configuration](docs/configuration.md), [Keybindings](docs/keybindings.md), [Themes](docs/themes.md) |
| Control spotatui from your shell | [CLI](docs/cli.md) |
| Write or install a Lua plugin | [PLUGINS.md](PLUGINS.md), [Scripting API](docs/scripting.md) |
| Let an AI pick the music | [AI DJ](docs/ai-dj.md), [MCP setup](docs/mcp-setup.md) |
| Report a bug | [New issue](https://github.com/LargeModGames/spotatui/issues/new/choose), with a `spotatui --debug` [log](docs/configuration.md#logging) |
| Everything else | [All docs](docs/README.md) |

## Contributing

spotatui is built by a small team and a growing group of contributors, and you do not need to write code to help. Start with [CONTRIBUTING.md](CONTRIBUTING.md), or pick a [good first issue](https://github.com/LargeModGames/spotatui/issues?q=is%3Aissue%20state%3Aopen%20label%3A%22good%20first%20issue%22).

Native playback and Spotify Connect run on our librespot fork, [spotatui-librespot](https://github.com/LargeModGames/spotatui-librespot). It is part of the project: a merged PR there counts as a spotatui contribution. See [Contributing to the librespot fork](CONTRIBUTING.md#contributing-to-the-librespot-fork).

## Song History

![Song History](https://spotatui-counter.spotatui.workers.dev/chart.svg)

## Star History

<a href="https://www.star-history.com/?type=date&legend=top-left&repos=LargeModGames%2Fspotatui">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=LargeModGames/spotatui&type=date&theme=dark&legend=top-left&sealed_token=iryqtVEb4hcMJpt3uuX43JaGNlZRMV4-stDO4M8-7zH7g186IdrwSCU1PMWBv_YdK7DC4y6qw4sEKu9JXmHbR6c7clp1u8sL1AXI3D_dMW_KTnIjJYKDGw" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=LargeModGames/spotatui&type=date&legend=top-left&sealed_token=iryqtVEb4hcMJpt3uuX43JaGNlZRMV4-stDO4M8-7zH7g186IdrwSCU1PMWBv_YdK7DC4y6qw4sEKu9JXmHbR6c7clp1u8sL1AXI3D_dMW_KTnIjJYKDGw" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=LargeModGames/spotatui&type=date&legend=top-left&sealed_token=iryqtVEb4hcMJpt3uuX43JaGNlZRMV4-stDO4M8-7zH7g186IdrwSCU1PMWBv_YdK7DC4y6qw4sEKu9JXmHbR6c7clp1u8sL1AXI3D_dMW_KTnIjJYKDGw" />
 </picture>
</a>


## Maintainers

- **[LargeModGames](https://github.com/LargeModGames)** ([@LargeModGames](https://x.com/LargeModGames) on X)
- [shilicioo](https://github.com/shilicioo)
- [2570165831](https://github.com/2570165831)
- [Oud](https://github.com/Oud-Idk)

Originally forked from [spotify-tui](https://github.com/Rigellute/spotify-tui) by [Alexander Keliris](https://github.com/Rigellute). The spotify-tui contributors and the libraries spotatui uses are in [CREDITS.md](CREDITS.md).

## Code signing policy

Free code signing provided by [SignPath.io](https://about.signpath.io), certificate by [SignPath Foundation](https://signpath.org).

The Windows release files are built from this repository by the [release workflow](.github/workflows/cd.yml) on GitHub-hosted runners. Each signing request is approved by hand.

- Committers and reviewers: [LargeModGames](https://github.com/LargeModGames), [ThatModGames](https://github.com/ThatModGames), [shilicioo](https://github.com/shilicioo), [2570165831](https://github.com/2570165831), [Oud](https://github.com/Oud-Idk)
- Approvers: [LargeModGames](https://github.com/LargeModGames)

Privacy: see the [privacy policy](https://spotatui.com/privacy) for the data spotatui sends and how to turn each part off.

## spotatui Contributors

Everyone who helped, with code or without. To get on this list, see [Recognition](CONTRIBUTING.md#recognition).

<!-- ALL-CONTRIBUTORS-LIST:START - Do not remove or modify this section -->
<!-- prettier-ignore-start -->
<!-- markdownlint-disable -->
<table>
  <tbody>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/LargeModGames"><img src="https://avatars.githubusercontent.com/u/84450916?v=4?s=100" width="100px;" alt="LargeModGames"/><br /><sub><b>LargeModGames</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=LargeModGames" title="Code">💻</a> <a href="https://github.com/LargeModGames/spotatui/commits?author=LargeModGames" title="Documentation">📖</a> <a href="#maintenance-LargeModGames" title="Maintenance">🚧</a> <a href="#ideas-LargeModGames" title="Ideas, Planning, & Feedback">🤔</a> <a href="#infra-LargeModGames" title="Infrastructure (Hosting, Build-Tools, etc)">🚇</a> <a href="https://github.com/LargeModGames/spotatui/commits?author=LargeModGames" title="Tests">⚠️</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/shilicioo"><img src="https://avatars.githubusercontent.com/u/56956072?v=4?s=100" width="100px;" alt="shilicioo"/><br /><sub><b>shilicioo</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=shilicioo" title="Code">💻</a> <a href="#maintenance-shilicioo" title="Maintenance">🚧</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/2570165831"><img src="https://avatars.githubusercontent.com/u/200382837?v=4?s=100" width="100px;" alt="2570165831"/><br /><sub><b>2570165831</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=2570165831" title="Code">💻</a> <a href="#maintenance-2570165831" title="Maintenance">🚧</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/Oud-Idk"><img src="https://avatars.githubusercontent.com/u/80905197?v=4?s=100" width="100px;" alt="Oud"/><br /><sub><b>Oud</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=Oud-Idk" title="Code">💻</a> <a href="#maintenance-Oud-Idk" title="Maintenance">🚧</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/MysteriousWolf"><img src="https://avatars.githubusercontent.com/u/5306409?v=4?s=100" width="100px;" alt="MysteriousWolf"/><br /><sub><b>MysteriousWolf</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=MysteriousWolf" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/rawcode1337"><img src="https://avatars.githubusercontent.com/u/80097670?v=4?s=100" width="100px;" alt="rawcode1337"/><br /><sub><b>rawcode1337</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=rawcode1337" title="Code">💻</a> <a href="https://github.com/LargeModGames/spotatui/issues?q=author%3Arawcode1337" title="Bug reports">🐛</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/copeison"><img src="https://avatars.githubusercontent.com/u/184175589?v=4?s=100" width="100px;" alt="copeison"/><br /><sub><b>copeison</b></sub></a><br /><a href="#platform-copeison" title="Packaging/porting to new platform">📦</a></td>
    </tr>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/jacklorusso"><img src="https://avatars.githubusercontent.com/u/19835679?v=4?s=100" width="100px;" alt="jacklorusso"/><br /><sub><b>jacklorusso</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=jacklorusso" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/H41L33"><img src="https://avatars.githubusercontent.com/u/140116782?v=4?s=100" width="100px;" alt="H41L33"/><br /><sub><b>H41L33</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=H41L33" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://mzte.de"><img src="https://avatars.githubusercontent.com/u/28735087?v=4?s=100" width="100px;" alt="LordMZTE"/><br /><sub><b>LordMZTE</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=LordMZTE" title="Code">💻</a> <a href="#platform-LordMZTE" title="Packaging/porting to new platform">📦</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/bodoque-01"><img src="https://avatars.githubusercontent.com/u/63447579?v=4?s=100" width="100px;" alt="Sebastian Sarco"/><br /><sub><b>Sebastian Sarco</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=bodoque-01" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/El-Mundos"><img src="https://avatars.githubusercontent.com/u/70759168?v=4?s=100" width="100px;" alt="Sergio Tabernero Hernández"/><br /><sub><b>Sergio Tabernero Hernández</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=El-Mundos" title="Code">💻</a> <a href="#platform-El-Mundos" title="Packaging/porting to new platform">📦</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://dpnova.github.io/"><img src="https://avatars.githubusercontent.com/u/229943?v=4?s=100" width="100px;" alt="David Novakovic"/><br /><sub><b>David Novakovic</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=dpnova" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="http://nthpaul.com"><img src="https://avatars.githubusercontent.com/u/70828466?v=4?s=100" width="100px;" alt="Paul"/><br /><sub><b>Paul</b></sub></a><br /><a href="#design-nthpaul" title="Design">🎨</a> <a href="https://github.com/LargeModGames/spotatui/commits?author=nthpaul" title="Code">💻</a></td>
    </tr>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/AFE123x"><img src="https://avatars.githubusercontent.com/u/121839885?v=4?s=100" width="100px;" alt="Arun Felix"/><br /><sub><b>Arun Felix</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=AFE123x" title="Code">💻</a> <a href="https://github.com/LargeModGames/spotatui/issues?q=author%3AAFE123x" title="Bug reports">🐛</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/MaySeikatsu"><img src="https://avatars.githubusercontent.com/u/127960577?v=4?s=100" width="100px;" alt="MaySeikatsu"/><br /><sub><b>MaySeikatsu</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=MaySeikatsu" title="Code">💻</a> <a href="https://github.com/LargeModGames/spotatui/commits?author=MaySeikatsu" title="Documentation">📖</a> <a href="#platform-MaySeikatsu" title="Packaging/porting to new platform">📦</a></td>
      <td align="center" valign="top" width="14.28%"><a href="http://prabo.org"><img src="https://avatars.githubusercontent.com/u/32436755?v=4?s=100" width="100px;" alt="Lorenzo Bodini"/><br /><sub><b>Lorenzo Bodini</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=topongo" title="Code">💻</a> <a href="#design-topongo" title="Design">🎨</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/Vi1i"><img src="https://avatars.githubusercontent.com/u/6485370?v=4?s=100" width="100px;" alt="Vi1i Petal"/><br /><sub><b>Vi1i Petal</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=Vi1i" title="Code">💻</a> <a href="https://github.com/LargeModGames/spotatui/issues?q=author%3AVi1i" title="Bug reports">🐛</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/wrsturgeon"><img src="https://avatars.githubusercontent.com/u/79714036?v=4?s=100" width="100px;" alt="Will Sturgeon"/><br /><sub><b>Will Sturgeon</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=wrsturgeon" title="Code">💻</a> <a href="https://github.com/LargeModGames/spotatui/issues?q=author%3Awrsturgeon" title="Bug reports">🐛</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/wfinken"><img src="https://avatars.githubusercontent.com/u/891256?v=4?s=100" width="100px;" alt="wfinken"/><br /><sub><b>wfinken</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=wfinken" title="Code">💻</a> <a href="https://github.com/LargeModGames/spotatui/issues?q=author%3Awfinken" title="Bug reports">🐛</a></td>
      <td align="center" valign="top" width="14.28%"><a href="http://kathund.dev"><img src="https://avatars.githubusercontent.com/u/55346310?v=4?s=100" width="100px;" alt="Jacob"/><br /><sub><b>Jacob</b></sub></a><br /><a href="#platform-Kathund" title="Packaging/porting to new platform">📦</a></td>
    </tr>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="http://dominicklee.net"><img src="https://avatars.githubusercontent.com/u/43938540?v=4?s=100" width="100px;" alt="Dominick Lee"/><br /><sub><b>Dominick Lee</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=domogami" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/higorprado"><img src="https://avatars.githubusercontent.com/u/1037397?v=4?s=100" width="100px;" alt="Higor Prado"/><br /><sub><b>Higor Prado</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=higorprado" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/eiseq"><img src="https://avatars.githubusercontent.com/u/80960013?v=4?s=100" width="100px;" alt="Vitali Kaplich"/><br /><sub><b>Vitali Kaplich</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=eiseq" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/1knth"><img src="https://avatars.githubusercontent.com/u/115324660?v=4?s=100" width="100px;" alt="knth"/><br /><sub><b>knth</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=1knth" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="http://www.dev.arynwood.com"><img src="https://avatars.githubusercontent.com/u/64027767?v=4?s=100" width="100px;" alt="Lorelei Noble"/><br /><sub><b>Lorelei Noble</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=SkyeVault" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://benalleng.com"><img src="https://avatars.githubusercontent.com/u/108441023?v=4?s=100" width="100px;" alt="Ben Allen"/><br /><sub><b>Ben Allen</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=benalleng" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/Moaht"><img src="https://avatars.githubusercontent.com/u/93605688?v=4?s=100" width="100px;" alt="Thomas Allan"/><br /><sub><b>Thomas Allan</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=Moaht" title="Code">💻</a></td>
    </tr>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="http://www.pratyoosh.me"><img src="https://avatars.githubusercontent.com/u/170789120?v=4?s=100" width="100px;" alt="Pratyoosh Prakash"/><br /><sub><b>Pratyoosh Prakash</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=rlpratyoosh" title="Code">💻</a> <a href="https://github.com/LargeModGames/spotatui/issues?q=author%3Arlpratyoosh" title="Bug reports">🐛</a></td>
      <td align="center" valign="top" width="14.28%"><a href="http://tahmid.io"><img src="https://avatars.githubusercontent.com/u/107484759?v=4?s=100" width="100px;" alt="Tahmid Ahmed"/><br /><sub><b>Tahmid Ahmed</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=tahminator" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/Ritze03"><img src="https://avatars.githubusercontent.com/u/63347117?v=4?s=100" width="100px;" alt="Ritze"/><br /><sub><b>Ritze</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=Ritze03" title="Code">💻</a> <a href="https://github.com/LargeModGames/spotatui/issues?q=author%3ARitze03" title="Bug reports">🐛</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/itlogsandwich"><img src="https://avatars.githubusercontent.com/u/155230216?v=4?s=100" width="100px;" alt="Hansen"/><br /><sub><b>Hansen</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=itlogsandwich" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/yhay81"><img src="https://avatars.githubusercontent.com/u/11132792?v=4?s=100" width="100px;" alt="Yusuke Hayashi"/><br /><sub><b>Yusuke Hayashi</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=yhay81" title="Code">💻</a> <a href="https://github.com/LargeModGames/spotatui/commits?author=yhay81" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="http://felixzieger.de"><img src="https://avatars.githubusercontent.com/u/67903933?v=4?s=100" width="100px;" alt="Felix"/><br /><sub><b>Felix</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=felixzieger" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/DinoLeung"><img src="https://avatars.githubusercontent.com/u/7704357?v=4?s=100" width="100px;" alt="Dino Leung"/><br /><sub><b>Dino Leung</b></sub></a><br /><a href="#platform-DinoLeung" title="Packaging/porting to new platform">📦</a></td>
    </tr>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/istipisti113"><img src="https://avatars.githubusercontent.com/u/42544437?v=4?s=100" width="100px;" alt="Szabó István"/><br /><sub><b>Szabó István</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=istipisti113" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://alessandrozanni.it/"><img src="https://avatars.githubusercontent.com/u/78210329?v=4?s=100" width="100px;" alt="Alessandro Zanni"/><br /><sub><b>Alessandro Zanni</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=alessandro-zanni" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/nohint404"><img src="https://avatars.githubusercontent.com/u/238106931?v=4?s=100" width="100px;" alt="nohint404"/><br /><sub><b>nohint404</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=nohint404" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/TongTong0828"><img src="https://avatars.githubusercontent.com/u/151875930?v=4?s=100" width="100px;" alt="TongTong0828"/><br /><sub><b>TongTong0828</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=TongTong0828" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/Hi-1mYara"><img src="https://avatars.githubusercontent.com/u/254750611?v=4?s=100" width="100px;" alt="Hi-1mYara"/><br /><sub><b>Hi-1mYara</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=Hi-1mYara" title="Code">💻</a> <a href="https://github.com/LargeModGames/spotatui/commits?author=Hi-1mYara" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://salty-nebula-7f4.notion.site/Yvoolab-3c6f362dc885816bae88f25d128e85cd"><img src="https://avatars.githubusercontent.com/u/202315522?v=4?s=100" width="100px;" alt="Yvoo"/><br /><sub><b>Yvoo</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=yvoolab" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/mschmicking"><img src="https://avatars.githubusercontent.com/u/17197791?v=4?s=100" width="100px;" alt="Maurice Schmicking"/><br /><sub><b>Maurice Schmicking</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=mschmicking" title="Code">💻</a></td>
    </tr>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/tayfuryldz"><img src="https://avatars.githubusercontent.com/u/238304586?v=4?s=100" width="100px;" alt="Tayfur Yıldız"/><br /><sub><b>Tayfur Yıldız</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=tayfuryldz" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/chiliec"><img src="https://avatars.githubusercontent.com/u/982358?v=4?s=100" width="100px;" alt="Vladimir Babin"/><br /><sub><b>Vladimir Babin</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=chiliec" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="http://wanjinhao1.github.io"><img src="https://avatars.githubusercontent.com/u/34562501?v=4?s=100" width="100px;" alt="Kratos"/><br /><sub><b>Kratos</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=wanjinhao1" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/niukanen1"><img src="https://avatars.githubusercontent.com/u/57656076?v=4?s=100" width="100px;" alt="niukanen1"/><br /><sub><b>niukanen1</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=niukanen1" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/TrevorSatori"><img src="https://avatars.githubusercontent.com/u/80570915?v=4?s=100" width="100px;" alt="Trevor Satori"/><br /><sub><b>Trevor Satori</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=TrevorSatori" title="Code">💻</a> <a href="https://github.com/LargeModGames/spotatui/issues?q=author%3ATrevorSatori" title="Bug reports">🐛</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/drakeo338"><img src="https://avatars.githubusercontent.com/u/328244157?v=4?s=100" width="100px;" alt="Y.B."/><br /><sub><b>Y.B.</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=drakeo338" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/uemrey0"><img src="https://avatars.githubusercontent.com/u/64688960?v=4?s=100" width="100px;" alt="Ufuk Emre Yücetürk"/><br /><sub><b>Ufuk Emre Yücetürk</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=uemrey0" title="Code">💻</a></td>
    </tr>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/WaterWhisperer"><img src="https://avatars.githubusercontent.com/u/164724580?v=4?s=100" width="100px;" alt="WaterWhisperer"/><br /><sub><b>WaterWhisperer</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=WaterWhisperer" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/Wassergeist"><img src="https://avatars.githubusercontent.com/u/91049818?v=4?s=100" width="100px;" alt="Marc Bauer"/><br /><sub><b>Marc Bauer</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=Wassergeist" title="Code">💻</a> <a href="#financial-Wassergeist" title="Financial">💵</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/RohithPariki"><img src="https://avatars.githubusercontent.com/u/159949499?v=4?s=100" width="100px;" alt="RohithPariki"/><br /><sub><b>RohithPariki</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=RohithPariki" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/Muu79"><img src="https://avatars.githubusercontent.com/u/35148226?v=4?s=100" width="100px;" alt="Muaaz Bhyat"/><br /><sub><b>Muaaz Bhyat</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=Muu79" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/jum-apzn"><img src="https://avatars.githubusercontent.com/u/335973771?v=4?s=100" width="100px;" alt="점[dot]"/><br /><sub><b>점[dot]</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=jum-apzn" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="http://web.samtalks.us"><img src="https://avatars.githubusercontent.com/u/15819181?v=4?s=100" width="100px;" alt="Samuel"/><br /><sub><b>Samuel</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=TheCodedKid" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/TastyHeadphones"><img src="https://avatars.githubusercontent.com/u/76083688?v=4?s=100" width="100px;" alt="TastyHeadphones"/><br /><sub><b>TastyHeadphones</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=TastyHeadphones" title="Documentation">📖</a></td>
    </tr>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="https://zovo.one"><img src="https://avatars.githubusercontent.com/u/51033404?v=4?s=100" width="100px;" alt="Michael "/><br /><sub><b>Michael </b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=theluckystrike" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/Battlelamb"><img src="https://avatars.githubusercontent.com/u/12995996?v=4?s=100" width="100px;" alt="Battlelamb"/><br /><sub><b>Battlelamb</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=Battlelamb" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="http://markus.gustavsson.co.nz"><img src="https://avatars.githubusercontent.com/u/103984290?v=4?s=100" width="100px;" alt="markus"/><br /><sub><b>markus</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=markusgust" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/dailyun"><img src="https://avatars.githubusercontent.com/u/144075925?v=4?s=100" width="100px;" alt="Drying"/><br /><sub><b>Drying</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=dailyun" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://jadhavgaurav.github.io"><img src="https://avatars.githubusercontent.com/u/134488140?v=4?s=100" width="100px;" alt="Gaurav Vijay Jadhav"/><br /><sub><b>Gaurav Vijay Jadhav</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=jadhavgaurav" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://anandb71.in"><img src="https://avatars.githubusercontent.com/u/169837340?v=4?s=100" width="100px;" alt="Anand B"/><br /><sub><b>Anand B</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=Anandb71" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://xdaforums.com/m/termuxsolutions.13482532/"><img src="https://avatars.githubusercontent.com/u/325067280?v=4?s=100" width="100px;" alt="AAA"/><br /><sub><b>AAA</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=kukirin-ui" title="Documentation">📖</a> <a href="https://github.com/LargeModGames/spotatui/commits?author=kukirin-ui" title="Code">💻</a></td>
    </tr>
  </tbody>
</table>

<!-- markdownlint-restore -->
<!-- prettier-ignore-end -->

<!-- ALL-CONTRIBUTORS-LIST:END -->

This project follows the [all-contributors](https://github.com/all-contributors/all-contributors) specification. Contributions of any kind welcome!
