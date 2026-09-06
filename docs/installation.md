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
brew tap LargeModGames/spotatui
brew install spotatui
```

### Pre-built Binaries

Download from [GitHub Releases](https://github.com/LargeModGames/spotatui/releases/latest):

| Platform | File |
| --- | --- |
| Windows 10/11 (64-bit) | `spotatui-windows-x86_64.zip` |
| Linux (Ubuntu, Arch, Fedora, etc.) | `spotatui-linux-x86_64.tar.gz` |
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
cargo install --path . --no-default-features --features telemetry,streaming,discord-rpc,cover-art,self-update,scripting,portaudio-backend,audio-viz-cpal,macos-media,local-files,subsonic,internet-radio,youtube,qobuz
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

When a new version is available, you'll see a popup notification on startup. To update:

1. Close spotatui
2. Run:
   ```bash
   spotatui update --install
   ```

If you installed via a package manager (AUR, cargo, etc.), update through there instead.

---

## Connecting to Spotify

spotatui talks to Spotify's Web API through a Spotify app. The first run asks which app to use:

1. **The shared ncspot client ID.** No dashboard needed. Spotify counts every ncspot and spotatui user against this one app, so it is often rate limited: playlists load slowly or not at all, and the status bar says so.
2. **Your own Spotify app** (recommended). Spotify gives each app a rate limit of its own.

To use your own app:

1. Go to the [Spotify Dashboard](https://developer.spotify.com/dashboard/applications) and click **Create app**.
2. Add `http://127.0.0.1:8888/callback` to the Redirect URIs. The wizard asks for the port; 8888 is the default.
3. Copy the `Client ID`. No client secret is needed.
4. Run `spotatui --reconfigure-auth`, choose option 2, and paste the id. The browser opens once to sign in.

The wizard keeps the shared id as `fallback_client_id` in `client.yml`, for a login that fails on your own app. A `client.yml` from an older version, with the shared id as `client_id` and your app as `fallback_client_id`, is read the other way round: your app leads. Your app still has no login of its own, so the first launch after the upgrade runs on the shared id and the status bar says so; run `spotatui --reconfigure-auth` once, choose 2, and paste the id. Native streaming signs in on its own and needs no dashboard entry.
