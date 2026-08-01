# Example plugins

Small, self-contained Lua plugins that demonstrate the spotatui plugin API. Copy one into your
config directory and restart spotatui to try it. See [`docs/scripting.md`](../../docs/scripting.md)
for the full API reference. These examples are first-party and auditable; plugins run with full app
privileges, so read anything you install from elsewhere (see
[Trust and safety](../../docs/scripting.md#trust-and-safety)).

| Plugin | What it shows |
|--------|---------------|
| [`track-notifier.lua`](track-notifier.lua) | Events, `notify`, `set_playbar` |
| [`track-info-popup.lua`](track-info-popup.lua) | `register_command`, reads, `popup` |
| [`accent-cycler.lua`](accent-cycler.lua) | `register_command`, `set_theme` |
| [`now-playing-webhook.lua`](now-playing-webhook.lua) | `http_post`, `json_encode` |
| [`queue-browser.lua`](queue-browser.lua) | Custom screens, async data reads, timers, storage |
| [`now-playing.lua`](now-playing.lua) | `cover_art` widget, `row` layout, synced lyrics |
| [`session-stats/`](session-stats) | A directory plugin with a `require`-d helper module |

## Installing

Single-file plugins go straight into `plugins/`:

```bash
mkdir -p "${XDG_CONFIG_HOME:-$HOME/.config}/spotatui/plugins"
cp track-notifier.lua "${XDG_CONFIG_HOME:-$HOME/.config}/spotatui/plugins/"
```

Directory plugins (a folder with a `main.lua` entry point) are copied as a whole:

```bash
mkdir -p "${XDG_CONFIG_HOME:-$HOME/.config}/spotatui/plugins"
cp -r session-stats "${XDG_CONFIG_HOME:-$HOME/.config}/spotatui/plugins/"
```

Restart spotatui after installing. Plugins that register commands need a key binding; add one to
`config.yml` in the spotatui app config directory under `plugin_commands` (each
plugin documents a suggested key in its header comment).

To install a plugin published as a git repository, use the built-in installer instead:

```bash
spotatui plugin add owner/repo
```
