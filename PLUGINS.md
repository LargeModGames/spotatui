# Plugins

spotatui runs user-written Lua plugins. They react to playback events, add commands and key
bindings, draw popups and playbar segments, restyle the theme, and make async HTTP requests.
See [`docs/scripting.md`](docs/scripting.md) for the full API and
[`examples/plugins/`](examples/plugins) for runnable examples.

## Installing a plugin

Plugins published as git repositories install with one command (requires `git`):

```bash
spotatui plugin add owner/repo     # clone + record in the lockfile
spotatui plugin list               # show installed plugins
spotatui plugin update             # update all to their latest commit
spotatui plugin remove <name>      # uninstall
```

Plugins are cloned into `~/.config/spotatui/plugins/<name>/` and loaded at startup. Restart
spotatui after installing, and bind any commands the plugin registers under `plugin_commands` in
`config.yml`.

You can also drop a single `.lua` file into `~/.config/spotatui/plugins/` by hand.

## First-party examples

These ship in this repo under [`examples/plugins/`](examples/plugins):

- **track-notifier** - "Now playing" toast and playbar segment on every track change.
- **track-info-popup** - a command that pops up details of the current track.
- **accent-cycler** - a command that rotates the theme accent color.
- **now-playing-webhook** - POSTs a JSON payload to a webhook on track change.
- **session-stats** - a directory plugin (with a `require`-d helper) that tracks session plays.

## Sharing your own plugin

A shareable plugin is just a git repository with a `main.lua` (or `init.lua`) entry point at its
root. Helper modules sit alongside it and load via `require("module")`. Document any command and a
suggested key binding in your README, but ship the binding as a suggestion, not a hard-coded key.

Publishing one? Open a pull request adding it to this list - a short description and the
`owner/repo` install line is all it takes.
