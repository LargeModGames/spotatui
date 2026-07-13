# Keybindings

Press `?` in spotatui to see the help menu with all keybindings.

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
| `d`         | Manage devices            |
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

## Customizing Keybindings

Edit `~/.config/spotatui/config.yml`:

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
