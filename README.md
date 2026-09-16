# degen-radio

A focused internet-radio terminal player derived from Spotatui. This fork contains no Spotify client, Spotify authentication flow, Spotify playback backend, YouTube integration, or YouTube downloader.

The installed command is `degen-radio`.

## Features

- Saved stations from the existing Spotatui `config.yml` and `state.yml`
- radio-browser.info station search
- MP3/AAC internet-stream playback through the system audio output
- ICY `StreamTitle` now-playing metadata
- Linux MPRIS controls and metadata
- External station control for the Omarchy plugin

## Build and run

```bash
cargo run
```

Release install:

```bash
cargo install --path . --force
```

## Controls

| Key | Action |
|---|---|
| `j` / `Down` | Next station in the focused panel |
| `k` / `Up` | Previous station in the focused panel |
| `Left` / `h` | Focus saved stations |
| `Right` / `l` | Focus directory results |
| `Enter` | Play selected station |
| `s` / `/` | Focus station search |
| `x` | Open or close settings |
| `f` | Add selected search result to favorites |
| `d` / `D` | Remove selected station from favorites |
| `r` | Focus saved stations |
| `Esc` | Focus saved stations, then quit |
| `Space` | Pause or resume |
| `+` / `-` | Adjust volume |
| `X` | Stop |
| `q` | Quit |
| Mouse click | Focus search, saved stations, or directory results |

Theme presets live inside the Degen Radio settings menu: press `x` or click **Settings**, choose with `Up`/`Down`, then press `Enter`. The six bundled Omarchy-inspired presets are Tokyo Night, Catppuccin, Osaka Jade, Gruvbox, Nord, and Rose Pine. The selected preset is saved to `$XDG_STATE_HOME/degen-radio/state.yml`.

## Station storage

The player reads configured stations from:

```yaml
behavior:
  radio_stations:
    - name: Groove Salad
      url: https://ice1.somafm.com/groovesalad-128-mp3
```

It prefers `$XDG_STATE_HOME/degen-radio/state.yml` and falls back to the previous `~/.local/state/spotatui/state.yml` location so existing radio favorites are preserved.

Favorites added from search are written to `$XDG_STATE_HOME/degen-radio/state.yml` and immediately become available to the Omarchy tray plugin.

## Omarchy tray plugin

The companion [Degen Radio for Omarchy](https://github.com/ethereumdegen/omarchy-degen-radio-plugin) repository provides the bar widget, now-playing popup, playback controls, and saved-station picker.

## External control

List saved stations as JSON:

```bash
degen-radio radio list --json
```

Ask the running player to switch stations through MPRIS:

```bash
degen-radio radio play https://ice1.somafm.com/groovesalad-128-mp3
```

Repository: <https://github.com/ethereumdegen/degen-radio>
