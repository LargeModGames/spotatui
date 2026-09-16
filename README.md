# degen-radio

A focused internet-radio terminal player derived from Spotatui. This fork contains no Spotify client, Spotify authentication flow, Spotify playback backend, YouTube integration, or YouTube downloader.

The installed command remains `spotatui` so existing desktop entries, MPRIS integrations, and the Omarchy bar plugin continue to work.

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
| `j` / `Down` | Next station |
| `k` / `Up` | Previous station |
| `Enter` | Play selected station |
| `/` | Search radio-browser.info |
| `Space` | Pause or resume |
| `+` / `-` | Adjust volume |
| `s` | Stop |
| `q` / `Esc` | Quit |

## Station storage

The player reads configured stations from:

```yaml
behavior:
  radio_stations:
    - name: Groove Salad
      url: https://ice1.somafm.com/groovesalad-128-mp3
```

It also reads saved stations from `$XDG_STATE_HOME/spotatui/state.yml` (normally `~/.local/state/spotatui/state.yml`) to preserve existing radio favorites.

## External control

List saved stations as JSON:

```bash
spotatui radio list --json
```

Ask the running player to switch stations through MPRIS:

```bash
spotatui radio play https://ice1.somafm.com/groovesalad-128-mp3
```

Repository: <https://github.com/ethereumdegen/degen-radio>
