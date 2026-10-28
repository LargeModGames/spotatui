# Installation

spotatui comes as a terminal app and as a [desktop app](#desktop-app). Both play every source; Spotify is optional and needs Premium.

## Quick Install

### One-line installer

The installer downloads the prebuilt binary for your system, checks its checksum, and puts `spotatui` on your `PATH`. No Rust needed.

```bash
# macOS, Linux, WSL
curl -fsSL https://spotatui.com/install.sh | bash
```

```powershell
# Windows PowerShell
irm https://spotatui.com/install.ps1 | iex
```

### Which build has which features

The prebuilt terminal binaries (the installer, GitHub Releases, Homebrew, `spotatui-bin`, winget) include every source (Local Files, Subsonic, Internet Radio, YouTube, Qobuz, and Apple Music on macOS), the MCP server, the AI DJ and the [Soloist](soloist.md) engine. A plain `cargo install` and the Nix flake build the smaller default set: Spotify only, without album art. `cargo binstall` gets the full build only when a release artifact matches your platform; otherwise it falls back to `cargo install` and you get the default set. Add the sources and album art with:

```bash
cargo install --locked spotatui --features local-files,subsonic,internet-radio,youtube,qobuz,soloist,cover-art,mcp-server,ai-dj
# macOS: also add apple-music
```

YouTube needs [yt-dlp](https://github.com/yt-dlp/yt-dlp) on your `PATH` (ffmpeg recommended).

### Cargo

If you have Rust installed:

```bash
cargo install --locked spotatui
# or fetch the prebuilt binary without compiling
cargo binstall spotatui
```

`cargo install` builds the Spotify-only default. For the other sources, album art, the MCP server, the AI DJ and the Soloist engine, use the `--features` line from [Which build has which features](#which-build-has-which-features). `cargo binstall` fetches the release binary, which already has all of them, when a release artifact matches your platform.

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

Every release has a `checksums.txt` with the SHA-256 of each file. To check your downloads, run `sha256sum -c --ignore-missing checksums.txt` in the download folder.

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

`spotatui-bin` is x86_64 only.

### Void Linux

An unofficial package is in the [blackhole-vl](https://github.com/Event-Horizon-VL/blackhole-vl) repository; follow its README to add the repository, then `sudo xbps-install -S spotatui`.

### NixOS (flake)

Add spotatui to your flake inputs:

```nix
inputs = {
  spotatui = {
    url = "github:LargeModGames/spotatui";
    inputs.nixpkgs.follows = "nixpkgs";
  };
}
```

Then add the package to your configuration:

```nix
{ inputs, pkgs, ... }: {
  environment.systemPackages = with pkgs; [
    inputs.spotatui.packages.${pkgs.stdenv.hostPlatform.system}.default
  ];
}
```

The flake brings every dependency except the audio backend. With ALSA, set `hardware.alsa.enable = true;`. With PipeWire:

```nix
services.pipewire = {
  enable = true;
  alsa.enable = true;
};
```

The flake provides `x86_64-linux`, `aarch64-linux` and `aarch64-darwin`. nixpkgs 26.11 dropped Intel Macs, so on those use Homebrew, the one-line installer, or a release tarball.

---

## Desktop app

The desktop app is the same player in its own window, with a Spotify-style layout (Home, album and artist pages, Your Library, lyrics). It uses the same login, settings and history as the terminal app, and only one of the two runs at a time. Closing the window quits it. It is a preview.

Download it from [GitHub Releases](https://github.com/LargeModGames/spotatui/releases/latest):

| System | File |
|---|---|
| Windows 10/11 (64-bit) | `spotatui-desktop_<version>_x64-setup.exe` |
| macOS, Apple Silicon | `spotatui_<version>_aarch64.dmg` |
| macOS, Intel | `spotatui_<version>_x64.dmg` |
| Debian/Ubuntu | `spotatui-desktop_<version>_amd64.deb` or `_arm64.deb` |
| Fedora | `spotatui-desktop-<version>-1.x86_64.rpm` or `.aarch64.rpm` |
| Other Linux | `spotatui-desktop_<version>_amd64.AppImage` or `_aarch64.AppImage` |

The installers are not signed yet, so your system warns on the first start:

- **Windows:** SmartScreen says "Windows protected your PC". Click **More info**, then **Run anyway**.
- **macOS:** macOS refuses to open the app the first time. Close the dialog, open **System Settings > Privacy & Security**, click **Open Anyway** next to the spotatui message, and confirm. If macOS says the app is damaged, run `xattr -dr com.apple.quarantine /Applications/spotatui.app` once.

On Linux the app needs glibc 2.39 and WebKitGTK 4.1 (`webkit2gtk-4.1`): Ubuntu 24.04 or newer, Debian 13, Fedora 40 or newer, or Arch. If the window stays blank on Wayland with an NVIDIA GPU, start it with `GDK_BACKEND=x11 spotatui-desktop`. The app already sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` unless you set it yourself.

To drive the desktop app from a coding agent over MCP, also install the terminal `spotatui`: the agent starts `spotatui mcp`, which only the terminal binary has.

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

spotatui uses the `portaudio` backend for better stability and bluetooth device support (AirPods, etc.). The prebuilt binaries link it statically, so you only need it to build from source:

```bash
brew install portaudio
```

### YouTube source

The YouTube source needs the [`yt-dlp`](https://github.com/yt-dlp/yt-dlp) binary on your `PATH` (`ffmpeg` recommended). The prebuilt binaries don't bundle it. If YouTube playback stops working after a change on YouTube's side, update it with `yt-dlp -U` if you use the standalone binary, or through pip/pipx or your package manager if you installed it that way.

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
cargo install --path . --no-default-features --features telemetry,tui,streaming,discord-rpc,cover-art,self-update,scripting,mcp-server,ai-dj,portaudio-backend,audio-viz-cpal,macos-media,local-files,subsonic,internet-radio,youtube,qobuz,soloist,apple-music
```

**Linux/Windows:**
```bash
# Spotify only (the default features)
cargo install --path .
# every source, album art, MCP server, AI DJ and Soloist, like the release build
cargo install --path . --features local-files,subsonic,internet-radio,youtube,qobuz,soloist,cover-art,mcp-server,ai-dj
```

**Nix:**
```bash
nix build        # or `nix develop` for a shell with every build dependency
```

**WSL:** if the build fails to link, install the clipboard dependencies:
```bash
sudo apt-get install -y pkg-config libssl-dev libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev
```

The PipeWire visualizer backend (`--features audio-viz`, Linux only) also needs `libpipewire-0.3-dev libspa-0.2-dev` (Debian/Ubuntu), `pipewire` (Arch) or `pipewire-devel` (Fedora).

---

## Updating

When you start the terminal UI, spotatui checks GitHub Releases. If a newer version is available, it verifies the download against the release's `checksums.txt`, installs it, and restarts. Running a subcommand never triggers the check, and neither does the `spotatui-gui` build.

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

---

## Coming from spotify-tui

The binary is `spotatui` instead of `spt`, and the config moved from `~/.config/spotify-tui/` to the spotatui config directory (`$XDG_CONFIG_HOME/spotatui/` when `XDG_CONFIG_HOME` is an absolute path, otherwise `~/.config/spotatui/`). You can copy your old config:

```bash
case "${XDG_CONFIG_HOME:-}" in
  /*) config_home="$XDG_CONFIG_HOME" ;;
  *) config_home="$HOME/.config" ;;
esac
mkdir -p "$config_home/spotatui"
cp -r ~/.config/spotify-tui/* "$config_home/spotatui/"
```

You log in to Spotify again the first time.
