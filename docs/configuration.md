# Configuration

spotatui reads `config.yml` from the app config directory:

- Linux / macOS: `~/.config/spotatui/config.yml`
- Windows: `C:\Users\<you>\.config\spotatui\config.yml`

All fields are optional; omitted values use the built-in defaults. A complete, commented example lives in [`examples/config.example.yml`](../examples/config.example.yml).

Simple values (numbers, toggles, icons, positions) can also be changed live in the in-app **Settings** screen (see the hint in the top-right of the UI). Structured config — `format:` templates, `tables:` columns, and `playbar_control_labels` — is file-only. Edit the file while the app is closed: saving from the Settings screen rewrites the `behavior`, `theme`, and `keybindings` sections, but your `format:`, `tables:`, and `plugin_commands:` sections survive in-app saves untouched.

## Safe by default

A typo in `config.yml` never prevents the app from starting. Structural mistakes — an unknown sort field, a bad template placeholder, an invalid column id, an icon that is too wide — are logged as warnings and the affected value falls back to its built-in default. Warnings go to the log file whose path is printed at startup (`/tmp/spotatui_logs/spotatuilog<pid>`).

Only two kinds of errors are fatal: YAML syntax errors (the file cannot be parsed at all) and out-of-range numeric values that predate this policy (`volume_increment` outside 0–100, a zero tick rate, an unparseable `auto_update_delay`).

## Behavior

`behavior` controls interaction, timing, icons, and layout. The most commonly customized fields:

```yaml
behavior:
  # Timing
  seek_milliseconds: 5000          # seek step for < / >
  tick_rate_milliseconds: 250      # UI event-loop cadence
  playback_poll_seconds: 5         # how often playback state is polled (min 1;
                                   #   near track end the app polls faster regardless)
  status_message_ttl_percent: 100  # scales how long status messages stay visible
                                   #   (10..=1000; 200 = twice as long)
  like_animation_frames: 10        # length of the heart burst when liking a track (min 1)

  # Volume
  volume_increment: 1              # step for + / - (0..=100, fatal if outside)
  volume_percent: 70               # startup volume

  # Scrolling
  table_scroll_padding: 5          # rows kept visible below the selection before
                                   #   the table scrolls; clamped to half the table
                                   #   height so huge values cannot break scrolling
```

## Startup route

`startup_route` picks which screen opens at launch. Its data is fetched automatically, so the screen arrives populated. Only context-free screens are valid (nothing that needs an album id, artist id, or search query):

| Value | Screen | Alias |
|---|---|---|
| `home` (default) | Home | |
| `recently_played` | Recently Played | `recent` |
| `podcasts` | Podcasts | |
| `discover` | Discover | |
| `artists` | Followed Artists | `library` |
| `album_list` | Saved Albums | `albums` |

Unknown values fall back to `home` with a warning.

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
| `default_sort_playlist_tracks` | `default`, `name`, `date_added`, `artist`, `album`, `duration` |
| `default_sort_saved_albums` | `default`, `name`, `date_added`, `artist` |
| `default_sort_saved_artists` | `default`, `name` |
| `default_sort_recently_played` | `default`, `name`, `artist`, `album` |

`default` keeps the order the API returns (playlist order, date saved, play order). A field that is not valid for that screen falls back to `default` with a warning.

## Layout

```yaml
behavior:
  sidebar_position: left    # left | right | hidden
  playbar_position: bottom  # bottom | top
  sidebar_width_percent: 20 # 0 hides the sidebar entirely
  library_height_percent: 30
  playbar_height_rows: 6    # 0 hides the playbar
  small_terminal_width: 150
  small_terminal_height: 45
```

- `sidebar_position: hidden` gives the content the full width, but the sidebar auto-reveals while the Library or Playlists panel has keyboard focus or is hovered, so it never becomes unreachable.
- `small_terminal_width` / `small_terminal_height` are the responsive-layout breakpoints. At or above `small_terminal_width` columns the app uses the wide layout (search box inside the sidebar); below it the search box gets its own full-width top row. `enforce_wide_search_bar: true` forces the full-width search row regardless of width.
- Unknown position strings fall back to the default with a warning.
- Mouse hit-testing follows every arrangement automatically.

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
  playbar_status_source: "{state} ({source}{queue} | Volume: {volume}%)"
  window_title: "{title}{artist}"
```

The defaults above reproduce the built-in output exactly.

### `playbar_status` (Spotify playback) and `playbar_status_source` (local/Subsonic/Radio/YouTube playback)

Both templates accept the same keys; keys that don't apply to the current mode render empty:

| Key | Renders as | Notes |
|---|---|---|
| `{state}` | `Playing` / `Paused` | padded to 7 characters, matching the default layout |
| `{device}` | Spotify device name | Spotify playback only |
| `{source}` | active source label (e.g. `Local`, `Subsonic`) | source playback only |
| `{queue}` | ` \| 3/12` queue position, or empty | source playback only |
| `{shuffle}` | `On` / `Off` | padded to 3 characters |
| `{repeat}` | `Off` / `Track` / `All` | padded to 5 characters |
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
| `songs` | playlists, liked songs, search results | `liked, title, artist, album, length` | + `index` |
| `album_tracks` | an album's track list | `liked, index, title, artist, length` | + `album` |
| `recently_played` | Recently Played | `liked, title, artist, length` | + `index`, `album` |
| `albums` | Saved Albums | `title, artist, date` | + `liked` |
| `podcasts` | Podcasts | `title, publisher` | — |
| `episodes` | a podcast's episodes | `played, date, title, duration` | — |

The ▶ now-playing marker attaches to the `title` column, or to the first column if you remove `title`. Sort keyboard shortcuts are unaffected by column layout.

## Keybindings, theme, and plugins

The `keybindings:` section rebinds ~40 named actions (`back: q`, `next_track: n`, modifier syntax like `ctrl-s` / `alt-,`), and `theme:` sets a preset plus 18 individual color slots (`"R, G, B"` or named colors) — both are easiest to edit from the in-app Settings screen, which writes them back to this file. `plugin_commands:` maps extra keys to Lua plugin commands. See [`examples/config.example.yml`](../examples/config.example.yml) and the [scripting docs](scripting.md).
