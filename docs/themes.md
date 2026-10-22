# Themes

spotatui comes with several built-in theme presets. Access them via `Alt-,` (`Ctrl-,` on macOS) > Theme.

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
| Gruvbox Cove     | Gruvbox dark on bg0, green accent, aqua second, yellow only for warnings |
| Catppuccin Mocha | Popular pastel dark theme                   |
| Tokyo Night      | The [Tokyo Night](https://github.com/folke/tokyonight.nvim) palette |

## Custom Themes

You can create custom themes in `config.yml` in the spotatui app config directory
(`$XDG_CONFIG_HOME/spotatui` when `XDG_CONFIG_HOME` is set to an absolute path,
or `~/.config/spotatui` when it is unset or not absolute):

```yaml
theme:
  preset: "Custom"             # Use the color keys below
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

spotatui writes `preset:` into `config.yml` every time you save settings. The
color keys only apply when `preset` is `Custom`, or when there is no `preset:`
line at all. With a named preset such as `preset: "Nord"`, that preset's colors
win and the color keys are ignored, so set `preset: "Custom"` before editing
colors by hand.

Preset names must match the names in the table above exactly, case and
parentheses included (`"Default (Cyan)"`, `"Tokyo Night"`). Any other value is
treated as `Custom`.

### Color Values

**Always use RGB strings** for consistent colors across different terminals:

```yaml
text: "205, 214, 244"    # RGB format: "red, green, blue" (0-255)
```

> **Note:** Terminal color names like `Red`, `Cyan`, etc. are also supported but render differently depending on your terminal's color scheme. For consistent themes, use RGB values.

## Player Bar Visualizer (desktop app and browser GUI)

The spectrum or waveform behind the player bar of the desktop app and the browser GUI takes its colors from the cover of the song that plays. Every part of its look can be changed in `visualizer.yml`, in the same directory as `config.yml` (see [Custom Themes](#custom-themes)). The player bar visualizer is on Linux and Windows only, so other systems do not read this file.

spotatui never writes this file, so your comments stay. The app reads it again within a second of each save, so you can tune the look while music plays. A key that is missing keeps its default. A value of the wrong type keeps its default, a number out of range is moved to the nearest end of its range, and the status line shows each problem.

Every key with its default:

```yaml
colors: cover            # cover | accent | a list of colors (see below)
fallback: accent         # accent | a list of colors; used when the song has no cover
count: 5                 # 1-8 colors taken from the cover
order: luminance         # luminance (dark bass to light treble) | hue | share (largest area first)
reverse: false
greys: false             # true keeps black, white and grey from a cover that also has colors
min_distance: 0.08       # 0-0.5; how different two cover colors must be (OKLab distance)
min_share: 0.01          # 0-0.5; the smallest part of the cover a color may fill
min_saturation: 0        # 0-1; raises the saturation of colored stops, greys stay grey
luminance_floor: 0.22    # 0-1; lifts dark colors so the bars stay visible
luminance_ceiling: 0.35  # 0-1; keeps the white text readable, 1 turns it off
interpolation: srgb      # srgb | linear | oklab
fade_ms: 800             # 0-5000; how long the colors take to change to a new cover

spectrum:                # sizes in pixels (0-20), alphas from 0 to 1
  gap: 2
  cap_height: 2          # 0 hides the peak caps
  cap_gap: 2
  glow_side: 2
  glow_end: 3
  foot_alpha: 0.28
  top_alpha: 0.08
  glow_alpha: 0.12       # 0 turns the glow off
  glow_top_alpha: 0.036
  cap_alpha: 0.7
  pulse_alpha: 0.35      # 0 turns the bass pulse off
  pulse_height: 0.5      # 0-1; the pulse's height as a share of the bar
  pulse_bands: 0.125     # 0.01-1; the share of the bands, from the bass up, that drives the pulse
  fall_per_second: 3     # 0.1-60; full heights a bar falls each second
  peak_hold: 0.35        # 0-10; seconds a cap hangs
  peak_gravity: 2.4      # 0-100

waveform:
  reach: 0.46            # 0-0.5; how far the wave swings, as a share of the bar's height
  fill_alpha: 0.14
  point_spacing: 8       # 1-64; pixels per point
  min_points: 75         # 2-320; the wave never has more than 320 points
  lines:                 # [mirror -2-2, width 0-40, alpha 0-1], drawn in order, at most 8
    - [-0.6, 1, 0.18]
    - [1, 9, 0.06]
    - [1, 4, 0.16]
    - [1, 1.6, 0.85]
```

A color is `"#rgb"`, `"#rrggbb"`, `"r, g, b"` or one of the color names above. A fixed gradient in your own colors:

```yaml
colors: ["#1e1e2e", "#cba6f7", "#f38ba8", "#fab387"]
luminance_floor: 0
luminance_ceiling: 1
```

The floor, the ceiling and `min_saturation` also apply to a list, so set them to `0`, `1` and `0` to keep your colors exact. `colors: accent` gives the amber look of earlier releases; of these three keys, only the ceiling changes it.

The cover gradient works for Spotify, YouTube, Qobuz, Subsonic and local files with embedded art. Radio and Apple Music have no cover, so they use `fallback`.

## Contributing Themes

Want to add a new theme preset? See [CONTRIBUTING.md](https://github.com/LargeModGames/spotatui/blob/main/CONTRIBUTING.md) and check out `ThemePreset` in `src/core/theme.rs` for examples.
