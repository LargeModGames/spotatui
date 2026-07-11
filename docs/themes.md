# Themes

spotatui comes with several built-in theme presets. Access them via `Alt-,` > Theme.

## Built-in Presets

| Preset           | Description                                 |
| ---------------- | -------------------------------------------- |
| Default (Cyan)   | Original spotatui theme                     |
| Terminal (ANSI)  | Uses your terminal's ANSI colors            |
| Pookie Pink      | Bright pink theme                           |
| Spotify          | Official Spotify green (#1DB954)            |
| Vesper           | Minimal dark theme with warm orange accents |
| Dracula          | Popular dark purple/pink theme              |
| Nord             | Arctic, bluish color palette                |
| Solarized Dark   | Classic dark theme                          |
| Monokai          | Vibrant colors on dark background           |
| Gruvbox          | Warm retro groove colors                    |
| Gruvbox Light    | Light variant with warm colors              |
| Catppuccin Mocha | Popular pastel dark theme                   |

## Custom Themes

You can create custom themes in `~/.config/spotatui/config.yml`:

```yaml
theme:
  active: "137, 180, 250"      # Current playing song
  banner: "180, 190, 254"      # The "spotatui" banner
  error_border: "243, 139, 168"
  error_text: "243, 139, 168"
  hint: "249, 226, 175"
  hovered: "203, 166, 247"
  inactive: "108, 112, 134"
  playbar_background: "30, 30, 46"
  playbar_progress: "166, 227, 161"
  playbar_progress_text: "166, 227, 161"
  playbar_text: "205, 214, 244"
  selected: "137, 180, 250"
  text: "205, 214, 244"
  header: "255, 255, 255"
  background: "30, 30, 46"
  highlighted_lyrics: "166, 227, 161"
```

### Color Values

**Always use RGB strings** for consistent colors across different terminals:

```yaml
text: "205, 214, 244"    # RGB format: "red, green, blue" (0-255)
```

> **Note:** Terminal color names like `Red`, `Cyan`, etc. are also supported but render differently depending on your terminal's color scheme. For consistent themes, use RGB values.

## Contributing Themes

Want to add a new theme preset? See [CONTRIBUTING.md](https://github.com/LargeModGames/spotatui/blob/main/CONTRIBUTING.md) and check out `src/core/user_config.rs` for examples.
