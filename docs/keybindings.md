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

| Key         | Action                    | Config Name |
| ----------- | ------------------------- | ----------- |
| `Space`     | Toggle play/pause         | `toggle_playback` |
| `n`         | Next track                | `next_track` |
| `p`         | Previous track            | `previous_track` |
| `+`         | Volume up                 | `volume_up` |
| `-`         | Volume down               | `volume_down` |
| `<`         | Seek backward             | `seek_backwards` |
| `>`         | Seek forward              | `seek_forwards` |
| `/`         | Search                    | `search` |
| `h`         | Move left                 | `left` |
| `j`         | Move down                 | `down` |
| `k`         | Move up                   | `up` |
| `l`         | Move right                | `right` |
| `H`         | Jump to top / start       | `jump_to_start` |
| `L`         | Jump to bottom / end      | `jump_to_end` |
| `Enter`     | Select / confirm          | `select` |
| `a`         | Jump to album             | `jump_to_album` |
| `A`         | Jump to artist's albums   | `jump_to_artist_album` |
| `o`         | Jump to context           | `jump_to_context` |
| `d`         | Switch music source       | `switch_device` |
| `c`         | Copy song URL             | `copy_song_url` |
| `C`         | Copy album URL            | `copy_album_url` |
| `Ctrl-r`    | Toggle repeat mode        | `repeat` |
| `Ctrl-s`    | Toggle shuffle            | `shuffle` |
| `v`         | Audio visualization       | `audio_analysis` |
| `z`         | Add to queue              | `queue` |
| `Q`         | Show queue                | `show_queue` |
| `F`         | Like / save track         | `like` |
| `B`         | Lyrics view               | `lyrics` |
| `T`         | Toggle miniplayer view    | `toggle_miniplayer` |
| `R`         | Generate recap            | `recap` |
| `Ctrl-p`    | Listening party           | `listening_party` |
| `,`         | Open sort menu            | `sort` |
| `Alt-,`     | Open settings (`Ctrl-,` on macOS) | `open_settings` |
| `?`         | Show help                 | `help` |
| `q`         | Go back / Quit            | `back` |
| `Ctrl-c`    | Quit immediately          | `quit` |
| `Ctrl-j`    | Open the AI DJ (`ai-dj` builds) | `dj_open` |
| `Ctrl-t`    | Toggle DJ auto-queue      | `dj_toggle_auto_queue` |
| `Ctrl-y`    | DJ vibe shift             | `dj_vibe_shift` |
| `Ctrl-o`    | DJ fresh tracks only      | `dj_toggle_fresh_only` |
| `Ctrl-g`    | Choose the DJ's AI/model  | `dj_pick_model` |

## Customizing Keybindings

Edit `config.yml` in the spotatui app config directory (`$XDG_CONFIG_HOME/spotatui`
when `XDG_CONFIG_HOME` is set to an absolute path, or `~/.config/spotatui` when
it is unset or not absolute):