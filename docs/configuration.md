# Configuration

spotatui reads `config.yml` from the app config directory:

- `$XDG_CONFIG_HOME/spotatui/config.yml` when `XDG_CONFIG_HOME` is set to an absolute path.
- `${HOME}/.config/spotatui/config.yml` otherwise.

You can also point spotatui at a specific config file with `--config <path>`.

All fields are optional; omitted values use the built-in defaults. A commented example of commonly used settings lives in [`examples/config.example.yml`](../examples/config.example.yml).

Simple values (numbers, toggles, icons, positions) can also be changed live in the in-app **Settings** screen (`Alt+,`, or `Ctrl+,` on macOS; the desktop app has a Settings page). Press the search key (`/` by default) there to fuzzy-filter the rows by name or by their key in this file, so you can reach a setting without scrolling for it. Structured config (`format:` templates, `tables:` columns, and `playbar_control_labels`) is file-only. Edit the file while the app is closed: saving from the Settings screen rewrites the `behavior`, `theme`, and `keybindings` sections, but your `format:`, `tables:`, and `plugin_commands:` sections survive in-app saves untouched.

Machine-managed runtime state lives separately in `$XDG_STATE_HOME/spotatui/state.yml` when `XDG_STATE_HOME` is set to an absolute path, or `${HOME}/.local/state/spotatui/state.yml` when it is unset or not absolute. This includes volume, shuffle, active source, the saved Spotify device, seen announcements, resized pane dimensions, and saved radio stations. The local listening history also lives under the app state directory. The Spotify login and the native-streaming audio cache live under `$XDG_CACHE_HOME/spotatui/streaming_cache` when `XDG_CACHE_HOME` is set to an absolute path, or `${HOME}/.cache/spotatui/streaming_cache` when it is unset or not absolute. `sync_token` remains user config and is stored in `config.yml`.

## Safe by default

Most invalid values log a warning and fall back to the default. A misspelled key is usually ignored. Structural mistakes (an unknown sort field, a bad template placeholder, an invalid column id, an icon that is too wide) are logged as warnings and the affected value falls back to its built-in default. Warnings go to the log file whose path is printed at startup, a `spotatui_logs/spotatuilog<pid>` file inside your system temp directory (`%TEMP%` on Windows, `$TMPDIR` where set, otherwise `/tmp`). The startup line reports the resolved path, so copy it from there rather than guessing.

Fatal errors include YAML syntax errors (the file cannot be parsed at all), and a handful of out-of-range numeric values that bypass the warn-and-fallback policy: `volume_increment` outside 0–100, a tick rate (or animation tick rate) outside 1–999ms, an unparseable `auto_update_delay`, `playback_poll_seconds` below 1, and `like_animation_frames` below 1.

## Interface language

Chinese tables follow in stacked translation PRs. The language choices are selectable now; areas without Chinese data display their English fallback.

`behavior.language` accepts `auto` (the default), `en`, `zh-Hans` (Simplified Chinese),
and `zh-Hant` (Traditional Chinese). Common spellings such as `zh-CN`, `zh_CN`,
`zh`, and `ZH-HANS` are accepted and saved canonically. Unknown values warn and
fall back to `auto` without aborting configuration loading. Change it live in Settings; the terminal and
browser/desktop frontends share the same table. The language picker displays `auto` as
**Auto (system)** / **自动（跟随系统）** / **自動（跟隨系統）**, while the saved value stays
`auto`. CLI output and logs remain English.

`auto` reads the first nonempty `LC_ALL`, `LC_MESSAGES`, or `LANG`. Without one,
macOS uses the first preferred `AppleLanguages` entry and Windows uses its UI
culture. Other platforms, unavailable preferences, and every non-Chinese language
use English. `zh_CN`, `zh_SG`, and `zh-Hans` select Simplified Chinese; `zh_TW`,
`zh_HK`, `zh_MO`, and `zh-Hant` select Traditional Chinese. Explicit script tags
win over regions; bare `zh` selects Simplified Chinese. Locale encodings such as
`.UTF-8` are ignored. The system locale is sampled once at startup; choose a language
explicitly or restart after changing the system preference.

`LANGUAGE` is not used: it is a gettext fallback list, while this setting selects
one interface language and follows POSIX message-locale precedence.

Translations preserve product names, keybinding letters, configuration keys, and
user/server content such as track titles. A message without a translation falls
back to English.

## Behavior

`behavior` controls interaction, timing, icons, and layout. The most commonly customized fields:

```yaml
behavior:
  language: auto                  # auto | en | zh-Hans | zh-Hant
  # Timing
  seek_milliseconds: 5000          # seek step for < / >
  tick_rate_milliseconds: 250      # UI event-loop cadence (1..=999, fatal if outside)
  animation_tick_rate_milliseconds: 16
                                   # animation-only cadence, e.g. the like-heart burst
                                   #   (1..=999, fatal if outside)
  playback_poll_seconds: 5         # how often playback state is polled (min 1, fatal if 0;
                                   #   near track end the app polls faster regardless)
  status_message_ttl_percent: 100  # scales how long status messages stay visible
                                   #   (10..=1000; 200 = twice as long)
  like_animation_frames: 10        # length of the heart burst when liking a track (min 1, fatal if 0)

  # Volume
  volume_increment: 10             # step for + / - (0..=100, fatal if outside)
  volume_percent: 100              # initial volume default; saved runtime volume wins once present

  # Scrolling
  table_scroll_padding: 5          # rows kept visible below the selection before
                                   #   the table scrolls; clamped to half the table
                                   #   height so huge values cannot break scrolling
```

## Native streaming

Builds with the `streaming` feature play Spotify themselves as a Connect device. These fields also appear in Settings; a change takes effect after a restart.

```yaml
behavior:
  enable_streaming: true           # register spotatui as a Spotify Connect device
  streaming_device_name: spotatui  # the name in the Connect device list
  streaming_bitrate: 320           # 96, 160, or 320 kbps
  streaming_audio_cache: false     # keep played audio in the streaming cache
  streaming_autoplay: true         # enable Spotify's related-songs autoplay
```
## Cover art dithering

Build with `--features cover-art` to show the cover-art settings. Dithering is off by default and does not turn on cover art where terminal support has disabled it; `draw_cover_art_forced` still controls the basic-terminal fallback.

```yaml
behavior:
  cover_art_dither: true
  cover_art_dither_algorithm: stucki   # default; also bayer8x8 or atkinson
  cover_art_dither_pixel_scale: 1      # 1..=3; higher means larger visual pixels
theme:
  cover_art_dither_color: auto         # follows live playbar_progress
  # cover_art_dither_color: '120, 160, 220' # exact RGB override
```

The color override can also be a named theme color such as `LightBlue`. It survives theme-preset changes, and setting it back to `auto` clears it. Dithered art uses that accent with the surface's current background. Every slot, including a small playbar, makes its mask from the original decoded image at its own output resolution. Scale `1` uses every output pixel on graphics terminals or every Braille dot on basic terminals; higher values make larger visual pixels. The slot stays clear while a new dither is prepared, so the first artwork shown is already dithered. The basic-terminal grid has fewer dots than a graphics protocol, so its smallest images retain less detail. To give playbar art more room, increase `behavior.playbar_cover_art_size_percent` (up to `200`); the available playbar height still limits the final size.

## Startup route

`startup_route` picks which screen opens at launch. Its data is fetched automatically, so the screen arrives populated. Only context-free screens are valid (nothing that needs an album id, artist id, or search query):

| Value | Screen | Alias | Needs Spotify |
|---|---|---|---|
| `home` (default) | Home | | no |
| `recently_played` | Recently Played | `recent` | yes |
| `podcasts` | Podcasts | | yes |
| `discover` | Discover | | yes |
| `artists` | Followed Artists | `library` | yes |
| `album_list` | Saved Albums | `albums` | yes |
| `stats` | Stats | | no |

Unknown values fall back to `home` with a warning. Without a Spotify session
the Settings screen offers only `home` and `stats`, unless `config.yml` already
names a Spotify screen, which then stays selectable. A Spotify screen still
opens without a session, but empty.

## Default sorting

Each sortable screen can start pre-sorted. The value is `"<field>"` for ascending or `"<field>:desc"` for descending:

```yaml
behavior:
  default_sort_playlist_tracks: artist
  default_sort_saved_albums: date_added:desc
  default_sort_saved_artists: name
  default_sort_recently_played: name:desc
```

Valid fields per screen:

| Setting | Valid fields |
|---|---|
| `default_sort_playlist_tracks` | `default`, `name`, `date_added`, `artist`, `album`, `duration`, `release_date` |
| `default_sort_saved_albums` | `default`, `name`, `date_added`, `artist`, `release_date` |
| `default_sort_saved_artists` | `default`, `name` |
| `default_sort_recently_played` | `default`, `name`, `artist`, `album` |

`default` keeps the order the API returns (playlist order, date saved, play order). A field that is not valid for that screen falls back to `default` with a warning.

## Local Files

The `local-files` feature scans one music folder and turns each directory that directly contains audio files into a playlist. Playlist names are paths relative to the configured root; loose audio files in the root appear in `(root)`. Scanning goes up to eight directory levels deep, skips entries whose names start with `.`, and recognizes MP3, FLAC, OGG, M4A, AAC, and WAV files.

```yaml
behavior:
  local_music_path: "/home/you/Music"
```

When this key is omitted, spotatui uses the OS music directory. Setting it to an empty string disables that default and reports `No local music folder configured`. Paths are used exactly as written: `~` is not expanded, so use an absolute path (for example, `C:\\Users\\you\\Music` on Windows).

The `local-files` cargo feature is required. It is included in the release binaries.

## Subsonic

The `subsonic` feature works with Navidrome and other Subsonic-compatible servers. Include `http://` or `https://` in the full server URL:

```yaml
behavior:
  subsonic_url: "https://music.example.com"
  subsonic_username: "you"
  subsonic_password: "secret"
```

`subsonic_password` is stored in plaintext in `config.yml`; the first-run Subsonic setup also writes it there. Prefer the `SPOTATUI_SUBSONIC_PASSWORD` environment variable when the password should not be written to disk. For normal configured-source connections, the environment variable overrides `subsonic_password`. During first-run setup, spotatui tests the password entered during setup.

The `subsonic` cargo feature is required. It is included in the release binaries.

## YouTube

The `youtube` feature runs `yt-dlp` for playback. By default it invokes the bare command `yt-dlp`, found through `PATH`. Set `ytdlp_path` when the binary must be invoked by another name or path:

```yaml
behavior:
  ytdlp_path: "/opt/yt-dlp/yt-dlp"
```

Local YouTube playlists are stored in `youtube_playlists.yml` in the app config directory. Set `SPOTATUI_YOUTUBE_PLAYLISTS_PATH` to replace that location; its value must be the full path to the `.yml` file, not a directory.

The `youtube` cargo feature is required. It is included in the release binaries.

## Internet Radio

Preconfigured stations can be declared in `config.yml`:

```yaml
behavior:
  radio_stations:
    - name: SomaFM Groove Salad
      url: https://ice1.somafm.com/groovesalad-128-mp3
```

Stations saved from inside the app are stored in `state.yml`. The sidebar merges configured stations first and app-saved stations second, deduped by stream URL. To remove a configured station, edit `config.yml`; the in-app remove action only removes app-saved stations from `state.yml`.

## Qobuz

Qobuz logs in through the browser (first-run picker, or `d` then Qobuz) and keeps the token in `qobuz_credentials.yml` next to `config.yml`. One key selects the stream quality:

```yaml
behavior:
  qobuz_quality: 6   # 5 = MP3 320, 6 = FLAC 16/44.1, 7 = FLAC 24/96, 27 = FLAC 24/192
```

This is the requested quality. Qobuz serves some tracks at a lower one, so when a track plays from a Qobuz listing, the playbar shows the format it delivered, for example `FLAC 16/44.1`.

Environment variables: `SPOTATUI_QOBUZ_TOKEN` overrides the saved token. `SPOTATUI_QOBUZ_APP_ID`, `SPOTATUI_QOBUZ_APP_SECRET`, and `SPOTATUI_QOBUZ_OAUTH_KEY` (all three together) replace the constants spotatui otherwise reads from the Qobuz web player at runtime. Set them when Qobuz changes its web player and the login stops working, instead of waiting for a release.

Each track plays while it downloads: the encrypted stream is decrypted and rebuilt as a temporary file in the delivered format (FLAC, or MP3 at quality 5), playback starts after the first segments arrive, and a seek past the downloaded part continues the download from there.

## Apple Music

macOS only (`apple-music` feature, in the macOS release binaries). Unlike the other sources, the audio does not go through spotatui: the Music app plays the songs with your own Apple Music login and library, and spotatui is its remote. Music starts in the background and never comes to the front. The first time, macOS asks if your terminal may control Music; allow it (or later under **System Settings > Privacy & Security > Automation**).

The sidebar lists **All songs** and your playlists, including Apple Music playlists you added to your library; search looks through your library. Play/pause, next, previous, seek, volume, the media keys and the playbar follow Music. Next and previous step through the playlist or search results a song was started from, and follow Music's shuffle setting. Starting a Music song from spotatui pauses Spotify and the other sources, and starting one of those pauses Music first. Pressing play in the Music app itself while another source plays is not detected.

Not available for Apple Music songs: the queue, the shuffle and repeat toggles (change them in Music), likes, and playlist edits. Songs Music cannot play are listed, but choosing one shows a message instead of playing it, and next and previous skip them, because Music would answer them with a dialog that blocks every later start. For the same reason spotatui never lets Music's volume reach 0.

## Resuming the last session

When you quit while a non-Spotify source plays, spotatui restores that track and its position on the next launch, following `behavior.startup_behavior` (`continue`, `play`, or `pause`).

## Discord Rich Presence

On by default with the built-in spotatui application id, so no setup is needed. Optional overrides:

```yaml
behavior:
  enable_discord_rpc: true
  discord_rpc_client_id: "your_client_id"
```

`SPOTATUI_DISCORD_APP_ID` also overrides the app id. Turn it off in Settings or with `enable_discord_rpc: false`.

## Global song counter

spotatui counts how many songs all users played, for the badge and chart in the README. It is **on by default** and anonymous: it sends no personal information, song names, artists or history, only one increment after a song played for 30 seconds, from any source. Internet radio counts one song for every 2.5 minutes. Turn it off with:

```yaml
behavior:
  enable_global_song_count: false
```

## GitHub profile widget

Show what you listen to as a live card on your GitHub profile. Create an account at [spotatui.com](https://spotatui.com), paste your sync token into Settings (`sync_token`), then pick a public username and turn on the widget on your dashboard. Add this to your profile README:

```markdown
[![Now playing on spotatui](https://spotatui.com/widget/your-username.svg)](https://spotatui.com)
```

The card shows cover art, title, artist, a progress bar and an animated equalizer while playing; internet radio gets a LIVE badge. Append `?theme=light` for the light variant. Only your current track is public, and only after you opt in.

## Audio visualizer

Press `v` for the visualizer and `V` to switch between Cava, Bar Graph and Spotify Pitch. Cava and Bar Graph analyse your system audio in real time, so they also react to videos, games and any other sound:

| Platform | Setup |
|---|---|
| Windows | None (WASAPI loopback) |
| Linux | None (PipeWire or PulseAudio monitor devices) |
| macOS | Install a virtual audio device such as [BlackHole](https://github.com/ExistentialAudio/BlackHole) or [Loopback](https://rogueamoeba.com/loopback/), route system audio through it, and set it as your default input |

While a Spotify track plays, `v` opens **Spotify Pitch**, the visualizer of the original spotify-tui: the strength of the twelve pitch classes (C to B) as bars, with the tempo, key and time signature of the current section, from Spotify's own audio analysis. It needs no audio capture, so it also works when Spotify plays on another device.

## Listening history

spotatui keeps a local play history at `$XDG_STATE_HOME/spotatui/history/listens.jsonl` when `XDG_STATE_HOME` is an absolute path, otherwise `~/.local/state/spotatui/history/listens.jsonl`. It powers the Stats screen and `spotatui history recap`. Short or skipped plays are stored but left out of recap totals.

## Layout

```yaml
behavior:
  sidebar_position: left    # left | right | hidden
  playbar_position: bottom  # bottom | top
  sidebar_width_percent: 20 # initial sidebar width default; saved runtime size wins once present
  library_height_percent: 30 # initial library height default; saved runtime size wins once present
  playbar_height_rows: 6    # initial playbar height default, 0..=50; 0 hides it; saved runtime size wins once present
  small_terminal_width: 150
  small_terminal_height: 45
```

- `sidebar_position: hidden` gives the content the full width, but the sidebar auto-reveals while the Library or Playlists panel has keyboard focus or is hovered, so it never becomes unreachable.
- `sidebar_width_percent`, `library_height_percent`, and `playbar_height_rows` are configured initial defaults. Runtime resize changes use `{` / `}` for sidebar width, `(` / `)` for playbar height, `(` / `)` while hovering Library or Playlists for the library/sidebar split, and `|` to reset sizes; those changes persist in `state.yml` and are not overwritten by configured defaults after state exists. Configured `playbar_height_rows` is capped at 50 rows when applied at startup or when resetting the layout.
- `small_terminal_width` / `small_terminal_height` are the responsive-layout breakpoints. At or above `small_terminal_width` columns the app uses the wide layout (search box inside the sidebar); below it the search box gets its own full-width top row. `enforce_wide_search_bar: true` forces the full-width search row regardless of width.
- Unknown position strings fall back to the default with a warning.
- Mouse hit-testing follows every arrangement automatically.

## Other behavior keys

These keys also go under `behavior:`. Defaults below are the built-in values when a key is omitted; most can also be changed in Settings.

| Key | Default | What it does |
|-----|---------|--------------|
| `enable_text_emphasis` | `true` | Enable bold/italic text styling. |
| `show_loading_indicator` | `true` | Show loading status in the UI. |
| `group_folders_first` | `false` | List folders at the top of the Playlists tab. |
| `disable_mouse_inputs` | `false` | Disable mouse inputs for keyboard-only navigation. |
| `enable_announcements` | `true` | Show one-time announcements from a remote JSON feed. |
| `announcement_feed_url` | Unset | Override the remote JSON feed URL (HTTPS). |
| `enable_lyrics` | `true` | Look up lyrics for each track. Spotify tracks try Spotify first, then fall back to LRCLIB if lyrics are unavailable or the request fails; LRCLIB receives the title, artist and length. |
| `enable_monthly_recap_prompt` | `true` | Show a popup once a month when your listening recap is ready. |
| `pin_community_playlist` | `true` | Pin the spotatui community playlist to the top of your Spotify playlists. |
| `visualizer_style` | `Cava` | Visualizer look: `Cava` or `BarGraph` (case-sensitive). Older `Classic` and `Equalizer` values are accepted as aliases for `Cava`. |
| `stop_after_current_track` | `false` | Pause native Spotify streaming when the current track finishes; does not apply to other sources or remote Connect devices. |
| `draw_cover_art` | `true` | Enable rendering song/episode cover art; needs a cover-art build. |
| `cover_art_theme` | `false` | Recolor UI accents from the current cover art, fading on track change. Requires `art-decode`, which `cover-art` includes. |
| `keepawake_enabled` | `true` | Prevent the system from sleeping while music is playing. |
| `enable_media_keys` | `true` | Let OS media keys, headphone buttons and remote controls (`playerctl`, Now Playing) control playback. |
| `enable_global_song_count` | `true` | Contribute to the anonymous worldwide song counter: an increment after 30 seconds of playback (half the duration for tracks shorter than a minute), or every 2.5 minutes of radio; no personal info, song names or history. Requires `telemetry`. |
| `enable_discord_rpc` | `true` | Show your current track in Discord. Requires `discord-rpc`. |
| `discord_rpc_client_id` | Unset | Override the built-in Discord application ID with a string. Requires `discord-rpc`. |
| `startup_behavior` | `continue` | Playback state at launch: `continue` preserves Spotify's state and resumes a saved non-Spotify session as it was; `play` always starts; `pause` always pauses. |
| `disable_auto_update` | `false` | Skip the automatic update check on startup; use `spotatui update` to update manually. Requires `self-update`. |

The `discord-rpc`, `telemetry` and `self-update` features are in the default build. Every release binary includes cover art, but a plain `cargo run` does not; build with `--features cover-art` for the cover-art keys above.

## Icons

All icons are under `behavior` and can also be edited in the Settings screen.

```yaml
behavior:
  # Free-form (any width)
  liked_icon: "♥"
  shuffle_icon: "🔀"
  repeat_track_icon: "🔂"
  repeat_context_icon: "🔁"
  paused_icon: "⏸"
  active_source_icon: "●"     # marks the active source in the device picker
  list_highlight_icon: "▶"    # cursor prefix in sidebar/menu lists

  # Fixed-cell: must be exactly ONE terminal column wide
  playing_icon: "▶"           # prefixes the playing row in track tables
  gauge_filled_icon: "⣿"      # progress/volume gauge fill
  gauge_unfilled_icon: "⣉"    # progress/volume gauge background
  episode_played_icon: "✔"    # played marker in the episodes table
  sort_ascending_icon: "↑"    # sort direction indicator in table headers
  sort_descending_icon: "↓"
```

The fixed-cell icons sit in columns whose alignment math assumes one cell; a wider glyph is rejected at load and the default is used, with a warning.

**Ambiguous-width caveat:** some glyphs (e.g. `↑ ↓ ● ▶`) have Unicode "East Asian Ambiguous" width. Terminals configured to render ambiguous-width characters as wide (common with CJK locales) draw them 2 cells wide, shifting column alignment even though the config validates. If that happens, set your terminal's ambiguous-width option to "narrow" or pick unambiguous glyphs.

## Playbar control labels

The clickable playbar buttons can be relabeled (config-only). Mouse hitboxes resize to fit the labels automatically.

```yaml
behavior:
  playbar_control_labels:
    prev: "⏮"
    play_pause: "[PLAY/PAUSE]"
    next: "⏭"
    shuffle: "shfl"
    repeat: "rpt"
    like: "♥+"
    vol_down: "vol-"
    vol_up: "vol+"
```

All eight keys are optional; omit a key (or set it to an empty string) to keep that button's built-in label. Unknown keys are skipped with a warning.

## Format templates

`format` controls the playbar status line and the terminal window title. Templates use `{key}` placeholders; write a literal brace as `{{` or `}}`. Newlines and tabs are stripped from rendered values. An unknown key or unbalanced brace falls back to the default template with a warning listing the valid keys.

```yaml
format:
  playbar_status: "{state} ({device} | Shuffle: {shuffle} | Repeat: {repeat} | Volume: {volume}%){party}"
  playbar_status_source: "{state} ({source}{queue}{shuffle}{repeat} | Volume: {volume}%)"
  window_title: "{title}{artist}"
```

The defaults above reproduce the built-in output exactly.

### `playbar_status` (Spotify playback) and `playbar_status_source` (local/Subsonic/Qobuz/Radio/YouTube playback)

Both templates accept the same keys; keys that don't apply to the current mode render empty.

`{shuffle}` and `{repeat}` render differently in each template, so the two are not interchangeable:

- In `playbar_status`, they are bare values you label yourself (hence the `Shuffle: {shuffle}` in the default).
- In `playbar_status_source`, they come pre-composed with their own ` | Shuffle: ` / ` | Repeat: ` prefix, like `{queue}` and `{party}`. This lets the whole segment — label included — disappear for sources that have no such control (internet radio, and native queue slots), rather than leaving a blank `Shuffle:` behind. Don't write your own label around them there.

| Key | Renders as | Notes |
|---|---|---|
| `{state}` | `Playing` / `Paused` | padded to 7 characters, matching the default layout |
| `{device}` | Spotify device name | Spotify playback only |
| `{source}` | active source label (e.g. `Local`, `Subsonic`) | source playback only |
| `{queue}` | ` \| 3/12` queue position, or empty | source playback only |
| `{shuffle}` | `On` / `Off` | padded to 3 characters. In `playbar_status_source`: pre-composed as ` \| Shuffle: On `, empty for radio/queue slots |
| `{repeat}` | `Off` / `Track` / `All` | padded to 5 characters. In `playbar_status_source`: pre-composed as ` \| Repeat: All  `, empty for radio/queue slots |
| `{volume}` | volume percentage number | append your own `%` |
| `{party}` | ` \| Party: 3 listeners` / ` \| Party: following <host>`, or empty | pre-composed with its own separator |

### `window_title`

Applied only when `set_window_title: true`. Valid keys: `{title}` and `{artist}`. `{artist}` comes pre-composed as ` — <artist>` and renders empty when the artist is unknown, so the default `"{title}{artist}"` produces `Song — Artist` or just `Song`.

## Tables

`tables` reorders, removes, renames, and resizes the columns of every track/album/podcast table. Omit a table (or the whole section) to keep its built-in columns.

```yaml
tables:
  songs:
    - { id: liked }
    - { id: title, width_percent: 40 }
    - { id: artist, header: "Band", width_percent: 30 }
    - { id: album }
    - { id: length }
```

Each column entry supports:

| Field | Meaning |
|---|---|
| `id` | required — which column (see valid ids below) |
| `header` | optional display-name override |
| `width_percent` | optional width as a percentage of the table (0–100, exclusive of 0) |
| `width` | optional absolute width in cells (non-zero) |

Rules, all enforced at load (violations degrade that table to its defaults with a warning):

- ids must be valid for the table, with no duplicates and at least one column
- a column may set `width_percent` **or** `width`, not both; neither means the built-in default width
- no zero widths, and a table's `width_percent` values may not sum past 100 (trailing columns would be clipped)

Valid ids and default order per table:

| Table | Screen | Default columns | All valid ids |
|---|---|---|---|
| `songs` | playlists, liked songs, search results | `liked, title, artist, album, length` | + `index`, `year` |
| `album_tracks` | an album's track list | `liked, index, title, artist, length` | + `album`, `year` |
| `recently_played` | Recently Played | `liked, title, artist, length` | + `index`, `album`, `year` |
| `albums` | Saved Albums | `title, artist, date` | + `liked` |
| `podcasts` | Podcasts | `title, publisher` | — |
| `episodes` | a podcast's episodes | `played, date, title, duration` | — |

`year` is the release year of the track's album, 4 cells wide. No table shows it by default; add `- { id: year }` to a table to see it. Spotify and Subsonic tracks carry the year, and local files do when their tags have a date or year; Qobuz, YouTube, Internet Radio and Apple Music tracks leave the cell empty, as does a track whose date Spotify does not know.

The ▶ now-playing marker attaches to the `title` column, or to the first column if you remove `title`. Sort keyboard shortcuts are unaffected by column layout.

## Keybindings, theme, and plugins

The `keybindings:` section rebinds ~40 named actions (`back: q`, `next_track: n`, modifier syntax like `ctrl-s` / `alt-,`) — the full list of names and defaults is in [`docs/keybindings.md`](keybindings.md) — and `theme:` sets a preset plus 16 individual color slots (`"R, G, B"` or named colors) — both are easiest to edit from the in-app Settings screen, which writes them back to this file. `behavior.banner_gradient: false` draws the home-screen banner in the theme's banner color instead of the animated RGB gradient. The Terminal (ANSI) preset defaults this to `false` so the banner follows the terminal palette live (pywal etc.); every other preset defaults to `true`. An explicit `banner_gradient:` value in the config wins over the preset default. `plugin_commands:` maps extra keys to Lua plugin commands. See [`examples/config.example.yml`](../examples/config.example.yml) and the [scripting docs](scripting.md).

## AI DJ and the MCP server

Both are opt-in cargo features, so these keys only exist in a build that includes
them.

| Key | Feature | Default | Meaning |
|---|---|---|---|
| `mcp_enabled` | `mcp-server` | `false` | Open the local MCP control socket so a coding agent can drive playback |
| `dj_backend` | `ai-dj` | `agent_cli` | `agent_cli`, `anthropic`, or `openai_compat` |
| `dj_agent_command` | `ai-dj` | `["claude", "-p"]` | argv for `agent_cli`; a bare binary name expands to a known preset (`claude`, `codex`, `agy`, `copilot`, `opencode`, or the legacy `gemini`) |
| `dj_agent_model` | `ai-dj` | unset | Model passed as that CLI's own flag (`claude --model haiku`, `agy --model gemini-3.6-flash-low`); unset passes no flag |
| `dj_agent_prompt_via` | `ai-dj` | unset (the preset decides) | `stdin` or `arg`. `agy` ignores stdin, so leaving this unset is what keeps it working |
| `dj_agent_timeout_secs` | `ai-dj` | `90` | Clamped to 5–600 |
| `dj_model` | `ai-dj` | unset | Model id for the API backends; the agent CLIs use `dj_agent_model` instead |
| `dj_base_url` | `ai-dj` | Ollama's local endpoint | Base URL for `openai_compat` |
| `dj_api_key` | `ai-dj` | unset | Plaintext; prefer `SPOTATUI_DJ_API_KEY` |
| `dj_batch_size` | `ai-dj` | `6` | Tracks queued per round, max 8 |
| `dj_history_period` | `ai-dj` | `30d` | History window summarised for the model |
| `dj_avoid_library` | `ai-dj` | `false` | Start in "only tracks I don't already have" mode |
| `dj_configured` | `ai-dj` | unset | Written by the in-TUI AI/model picker (`Ctrl-g`). `true` stops the DJ asking which AI to use on your first visit |

`mcp_enabled` is off by default because opening a socket that can control the
player and read your listening history is a deliberate act. It binds loopback only
and requires the token in `~/.config/spotatui/mcp.json` (mode `0600`).

`SPOTATUI_DJ_API_KEY` overrides `dj_api_key` at request time and never touches
disk — the same arrangement as `SPOTATUI_SUBSONIC_PASSWORD`.

You do not have to write the `dj_backend` / `dj_agent_command` / model keys by hand:
the DJ asks which AI and which model on your first visit and writes the answer back
here, and `Ctrl-g` reopens that picker. It never touches `dj_api_key` or
`dj_base_url`, so those two remain yours.

Setup, tools, and troubleshooting: [`docs/mcp-setup.md`](mcp-setup.md) and
[`docs/ai-dj.md`](ai-dj.md).

## Logging

Every run writes a log file to the OS temp directory, one per process id:
`$TMPDIR/spotatui_logs/spotatuilog<pid>` (`%TEMP%\spotatui_logs\...` on Windows).
spotatui prints the path when it starts and again when it exits, so you never
have to guess which file belongs to the run you just had.

Two ways to make it more detailed:

| | Effect |
| :--- | :--- |
| `--debug` | Raises spotatui's own modules to `debug` and logs every Spotify Web API request: method, path with query, status, and duration |
| `SPOTATUI_LOG=<level>` | `error`, `warn`, `info` (default), `debug`, or `trace`. Unparsable values are ignored with a warning |

Both may be given; the more verbose of the two wins. `trace` additionally logs
API response bodies and turns on librespot's playback logging, which is loud
enough to be worth reaching for only when a maintainer asks.

Third-party dependencies (reqwest, hyper, rustls, ...) stay at `info`
regardless, so raising the level does not bury your own logs under TLS and HTTP
chatter. The embedded librespot crates are the exception: they follow
spotatui's own level, and the noisy playback one joins only at `trace`.

**What gets redacted.** spotatui never logs request headers, so your access
token is not in there. Values under keys such as `access_token`,
`refresh_token`, `password`, and `secret` are replaced in any JSON body it does
log, and your search terms are blanked out of the logged query string. Request
bodies appear only once the level is `debug` — through either `--debug` or
`SPOTATUI_LOG` — never in the warning a failed request writes on its own. That
list is maintained by hand and cannot be complete: a `debug` log can still
contain your display name, and a `trace` log can contain your country, because
Spotify returns it from `GET /v1/me`. Skim the file before you attach it to a
public issue.

If spotatui panics it writes `spotatui_panic.log` to the state directory
(`~/.local/state/spotatui/` on Linux) with the panic message, a backtrace, and
up to the last 200 log lines from the last 64 KiB, and prints that path. That
single file is usually
enough for a bug report.
