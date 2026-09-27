# Keybindings

Press `?` in spotatui to see the help menu with all keybindings. Inside the
help menu, press the search key (`/` by default) to filter rows by key,
description, or context; matching text is highlighted in the visible rows.
Press `Enter` to apply the filter and `Esc` to clear it.

The menu lists what the active source and your Spotify session can do. A
rebindable key that needs more stays listed with a suffix such as
`(needs Spotify)` or `(not for Local Files)`; a fixed key of a screen the
source cannot reach is left out until you switch source or log in.

The same search key filters the Settings screen, so you can jump to a setting
instead of scrolling for it. The query is fuzzy: its characters only have to
appear in order, so `volinc` finds **Volume Increment** and `skms` finds
**Seek Duration (ms)**. Rows are matched on their name and on their
`config.yml` key, and ranked best-first with the matched characters
highlighted. The filter applies to the tab you are on and survives `←`/`→`, so
one query can be walked across tabs.

## Default Keybindings

Every rebindable action has one row below, with its `keybindings:` config
key and its default from `UserConfig::new`. The five AI DJ keys live in the
[AI DJ keys](#ai-dj-keys) table further down.

| Action | Default | Config key |
|---|---|---|
| Go back / Quit | `q` | `back` |
| Navigate up (vim-style) | `k` | `move_up` |
| Navigate down (vim-style) | `j` | `move_down` |
| Navigate left (vim-style) | `h` | `move_left` |
| Navigate right (vim-style) | `l` | `move_right` |
| Next page | `Ctrl-d` | `next_page` |
| Previous page | `Ctrl-u` | `previous_page` |
| Jump to start of list | `Ctrl-a` | `jump_to_start` |
| Jump to end of list | `Ctrl-e` | `jump_to_end` |
| Jump to album | `a` | `jump_to_album` |
| Jump to artist's albums | `A` | `jump_to_artist_album` |
| Jump to context | `o` | `jump_to_context` |
| Switch music source / select playback device | `d` | `manage_devices` |
| Volume down | `-` | `decrease_volume` |
| Volume up | `+` | `increase_volume` |
| Toggle play/pause | `Space` | `toggle_playback` |
| Seek backward | `<` | `seek_backwards` |
| Seek forward | `>` | `seek_forwards` |
| Next track | `n` | `next_track` |
| Previous track | `p` | `previous_track` |
| Force previous track (skip position restore) | `P` | `force_previous_track` |
| Show help | `?` | `help` |
| Toggle shuffle | `Ctrl-s` | `shuffle` |
| Toggle repeat mode | `Ctrl-r` | `repeat` |
| Search | `/` | `search` |
| Confirm (not wired yet — rebinding it has no effect today) | `Enter` | `submit` |
| Copy song URL | `c` | `copy_song_url` |
| Copy album URL | `C` | `copy_album_url` |
| Audio visualization | `v` | `audio_analysis` |
| Lyrics view (the old name `basic_view` still works) | `B` | `lyrics_view` |
| Toggle miniplayer view | `T` | `miniplayer_view` |
| Cover art view (cover-art builds — every released binary; a plain `cargo run` without the feature ignores it) | `G` | `cover_art_view` |
| Add to queue | `z` | `add_item_to_queue` |
| Show queue | `Q` | `show_queue` |
| Remove from queue | `x` | `remove_from_queue` |
| Open settings (`Ctrl-,` on macOS) | `Alt-,` | `open_settings` |
| Save settings | `Alt-s` | `save_settings` |
| Listening party | `Ctrl-p` | `listening_party` |
| Like / save track | `F` | `like_track` |
| Generate recap | `R` | `generate_recap` |

A few keys in the help menu are fixed and have no config name: the sort menu
(`,`), the visualizer style picker (`V`), list jumps `H` / `M` / `L`, the
layout keys (`{` / `}`, `(` / `)`, `|`), `s` (save the selected track), and
`w` (add a track to a playlist). They cannot be rebound from
`config.yml`.

## Customizing Keybindings

Edit `config.yml` in the spotatui app config directory (`$XDG_CONFIG_HOME/spotatui`
when `XDG_CONFIG_HOME` is set to an absolute path, or `~/.config/spotatui` when
it is unset or not absolute):

```yaml
keybindings:
  back: "q"
  jump_to_album: "a"
  toggle_playback: " "
  # ... etc
```

Every row in the table above is a `keybindings:` entry: the config key names
the action and the value is its new default. Use the table as the full list of
rebindable names — 40 actions plus the five AI DJ keys below.

### Key Format

- Single keys: `"a"`, `"/"`, `" "` (or `"space"`)
- With Ctrl: `"ctrl-q"`, `"ctrl-s"`, `"ctrl--"` (Ctrl and the `-` key)
- With Alt: `"alt-,"`, `"alt-s"`, `"alt--"`
- With Shift: use the capital letter, `"A"`, `"C"` (Shift is not a modifier of its own)
- Special keys: `"enter"`, `"tab"`, `"esc"` (or `"escape"`), `"backspace"`,
  `"del"`, `"left"` / `"right"` / `"up"` / `"down"`, `"pageup"` / `"pagedown"`,
  `"home"` / `"end"`, `"ins"` (or `"insert"`), and `"f0"` through `"f12"`

All of these names are case-insensitive, so `"ENTER"` and `"enter"` both work.
A value that is a single character is taken as-is, so `"A"` means Shift+a.

> **Note:** Three-key combinations like `ctrl-alt-q` are not supported.

> **Watch out for `delete` vs `del`:** `"delete"` maps to the **Backspace** key
> and `"del"` maps to the **Delete** key — the opposite of what most people
> expect.

If a value cannot be parsed, the binding keeps its default and a warning is
logged at startup. A name that is not one of the config keys in the table
above (or the AI DJ keys below) is silently ignored — unknown names produce no
warning, so check your spelling against the table.

## AI DJ keys

Only present in builds with the `ai-dj` feature. All five are rebindable as
`dj_open`, `dj_toggle_auto_queue`, `dj_vibe_shift`, `dj_toggle_fresh_only`, and
`dj_pick_model`.

| Action | Default | Config key |
|---|---|---|
| Open the AI DJ screen | `Ctrl-j` | `dj_open` |
| Toggle continuous auto-queue | `Ctrl-t` | `dj_toggle_auto_queue` |
| Vibe shift (drop the DJ's queued tail, change direction) | `Ctrl-y` | `dj_vibe_shift` |
| Toggle "only tracks I don't already have" | `Ctrl-o` | `dj_toggle_fresh_only` |
| Choose which AI and model the DJ uses | `Ctrl-g` | `dj_pick_model` |

On the DJ screen itself the prompt takes every printable key, so `j`/`k` type
rather than navigate; scroll the transcript with the arrow and page keys. `Esc`
clears a half-typed prompt, and leaves the screen when the prompt is already empty.

The four action keys other than `dj_open` still work while the prompt has focus,
because they carry a modifier. If you rebind one to a bare character, that character
types instead, since a typing surface has to be able to contain it.

While the AI/model picker is open it is modal and takes every key: `↑`/`↓` (or
`j`/`k`) move, `1`-`9` pick a numbered row, `Enter` chooses, and `Esc` steps back
one step at a time, closing the picker from the first step and keeping whatever
brain you already had. Nothing else reaches the DJ or the rest of the app, so a
keypress cannot start background work with the backend you are mid-way through
replacing.

See [`docs/ai-dj.md`](ai-dj.md).
