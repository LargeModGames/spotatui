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

| Key         | Action                    |
| ----------- | ------------------------- |
| `Space`     | Toggle play/pause         |
| `n`         | Next track                |
| `p`         | Previous track            |
| `+` / `-`   | Volume up/down            |
| `<` / `>`   | Seek backward/forward     |
| `/`         | Search                    |
| `h`/`j`/`k`/`l` | Navigate (vim-style: left/down/up/right) |
| `Enter`     | Select / confirm          |
| `a`         | Jump to album             |
| `A`         | Jump to artist's albums   |
| `o`         | Jump to context           |
| `d`         | Switch music source       |
| `c`         | Copy song URL             |
| `C`         | Copy album URL            |
| `Ctrl-r`    | Toggle repeat mode        |
| `Ctrl-s`    | Toggle shuffle            |
| `v`         | Audio visualization       |
| `z`         | Add to queue              |
| `Q`         | Show queue                |
| `F`         | Like / save track         |
| `B`         | Lyrics view               |
| `T`         | Toggle miniplayer view    |
| `R`         | Generate recap            |
| `Ctrl-p`    | Listening party           |
| `,`         | Open sort menu            |
| `Alt-,`     | Open settings (`Ctrl-,` on macOS) |
| `?`         | Show help                 |
| `q`         | Go back / Quit            |
| `Ctrl-j`    | Open the AI DJ (`ai-dj` builds) |
| `Ctrl-t`    | Toggle DJ auto-queue      |
| `Ctrl-y`    | DJ vibe shift             |
| `Ctrl-o`    | DJ fresh tracks only      |
| `Ctrl-g`    | Choose the DJ's AI/model  |

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

The `keybindings:` section rebinds around 40 named actions in total; see
[`examples/config.example.yml`](../examples/config.example.yml) and
[`docs/configuration.md`](configuration.md) for the full picture of how the
config file is structured.

### Key Format

- Single keys: `"a"`, `"/"`, `" "` (space)
- With Ctrl: `"ctrl-q"`, `"ctrl-s"`
- With Alt: `"alt-,"`, `"alt-s"`
- With Shift: Use capital letter `"A"`, `"C"`
- Special keys: `"enter"`, `"esc"`, `"tab"`

> **Note:** Three-key combinations like `ctrl-alt-q` are not supported.

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
