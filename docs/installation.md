# Installation

## Quick Install

### Cargo (Recommended)

If you have Rust installed:

```bash
cargo install --locked spotatui
```

### Windows (winget)

```bash
winget install spotatui
```

### macOS (Homebrew)

```bash
brew tap LargeModGames/spotatui && brew trust --formula LargeModGames/spotatui/spotatui
brew install spotatui
```

> Homebrew 6+ requires trusting non-official formulae (`brew trust`) before install.

### Pre-built Binaries

Download from [GitHub Releases](https://github.com/LargeModGames/spotatui/releases/latest):

| Platform | File |
| --- | --- |
| Windows 10/11 (64-bit) | `spotatui-windows-x86_64.zip` |
| Linux (Ubuntu, Arch, Fedora, etc.) | `spotatui-linux-x86_64.tar.gz` |
| Linux ARM64 (Raspberry Pi 4/5, ARM servers) | `spotatui-linux-aarch64.tar.gz` |
| Debian/Ubuntu (amd64) | `spotatui_*_amd64.deb` |
| Debian/Ubuntu (arm64) | `spotatui_*_arm64.deb` |
| macOS (Intel) | `spotatui-macos-x86_64.tar.gz` |
| macOS (Apple Silicon M1/M2/M3) | `spotatui-macos-aarch64.tar.gz` |

Checksums (`.sha256`) are provided if you want to verify the download.

### Arch Linux (AUR)

```bash
# Pre-built binary (faster)
yay -S spotatui-bin
# or
paru -S spotatui-bin

# Build from source
yay -S spotatui
# or
paru -S spotatui
```

---

## Platform-Specific Requirements

### Linux

The pre-built Linux binary (and the `spotatui-bin` AUR package) is compiled with the PipeWire audio-visualization backend, so you need PipeWire installed for the visualizer:

```bash
# Debian/Ubuntu
sudo apt-get install libpipewire-0.3-0

# Arch Linux (already included with pipewire)
sudo pacman -S pipewire

# Fedora (already included with pipewire)
sudo dnf install pipewire
```

> **Note:** Most modern Linux distributions already have PipeWire installed by default. If you build from source instead (`cargo install`), the default build uses the `cpal`-based visualizer backend and does not require PipeWire.

### macOS

spotatui uses the `portaudio` backend for better stability and bluetooth device support (AirPods, etc.):

```bash
brew install portaudio
```

---

## Building from Source

Ensure you have [Rust](https://www.rust-lang.org/tools/install) installed.

### Prerequisites

**macOS:**
```bash
brew install portaudio
```

**Linux (Debian/Ubuntu):**
```bash
sudo apt-get install libssl-dev libasound2-dev libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev pkg-config
```

**Linux (Arch):**
```bash
sudo pacman -S openssl alsa-lib pkg-config
```

**Linux (Fedora):**
```bash
sudo dnf install openssl-devel alsa-lib-devel pkg-config
```

### Build

```bash
git clone https://github.com/LargeModGames/spotatui.git
cd spotatui
```

**macOS:**
```bash
cargo install --path . --no-default-features --features telemetry,streaming,discord-rpc,cover-art,self-update,scripting,portaudio-backend,audio-viz-cpal,macos-media,local-files,subsonic,internet-radio,youtube,qobuz,apple-music
```

**Linux/Windows:**
```bash
cargo install --path .
```

**Nix:**
```bash
nix-build
```

---

## Updating

When you start the terminal UI, spotatui checks GitHub Releases. If a newer version is available, it verifies the download against its `.sha256`, installs it, and restarts. Running a subcommand never triggers the check, and neither does the `spotatui-gui` build.

The default `auto_update_delay` is `0`, so an available update installs immediately. You can delay installation with values such as `30s`, `10m`, `2h`, or `7d`. The first launch that sees a new version records the notice; while it waits, the terminal prints `Update vX detected — will install in …`. Once the delay has passed, the next launch installs it. A newer release restarts the wait.

To disable automatic updates, set this in `config.yml` or use the Settings row:

```yaml
behavior:
  disable_auto_update: true
  # or keep updates enabled, but wait a day before installing:
  # auto_update_delay: "1d"
```

To skip the check for one run, use `spotatui --no-update` (or `-U`) or set `SPOTATUI_SKIP_UPDATE`. These options, the `update` subcommand, and the Settings rows are available in builds with the `self-update` feature, which is enabled in the default and release builds.

`spotatui update` shows your current and latest versions. Add `--install` to install the update immediately. If your package manager owns the installation, set `disable_auto_update: true` so spotatui does not replace its binary.

The desktop app has its own updater: installed from the Windows installer, the macOS app or the AppImage, it checks the latest stable release, verifies the update's signature, and installs it the next time you quit. The same two settings control it, also on its Settings page. A `.deb` or `.rpm` install does not update itself.

---

## Connecting to Spotify

spotatui talks to Spotify the way the official app does. It needs no Spotify developer app and no client ID, and Spotify in spotatui needs a **Premium** account.

To log in, pick Spotify on the first run, or press `d` and pick Spotify later. spotatui shows a short code and opens Spotify's pairing page in your browser. Confirm the code there; the login is cached, so this happens once. Run `spotatui --reconfigure-auth` to forget the login and log in again, for example with another account.

A `client.yml` from an older version stays on disk and is no longer used for the login. spotatui copies its native-streaming settings and saved device into `config.yml` and `state.yml` once.
