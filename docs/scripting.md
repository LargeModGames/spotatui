# Lua scripting

spotatui can run user-written Lua plugins. Plugins react to playback events and can drive
playback through a small, curated API. Scripting is compiled in behind the `scripting`
feature, which is enabled in the default build.

## File locations

Plugins are loaded from your config directory (`~/.config/spotatui/`) at startup, in this order:

1. `init.lua`, if present.
2. Single-file plugins: every `plugins/*.lua` file, sorted by filename.
3. Directory plugins: every `plugins/<name>/` folder, sorted by name. The entry point is
   `main.lua`, falling back to `init.lua`. A directory with neither is skipped, and directories
   whose name starts with `.` are ignored.

A directory plugin's own folder is added to Lua's `package.path`, so it can split itself across
files and load them with `require("module")` (resolving to `plugins/<name>/module.lua`).
`package.path` and the module cache are shared across all plugins: if two plugins both
`require("util")`, the first-loaded plugin's `util.lua` is cached under that name and silently
handed to the later plugin as well. Give helper modules distinctive (e.g. plugin-prefixed) names.

Directory plugins are how the `spotatui plugin` installer (below) lays out git-cloned plugins.

Missing files or a missing `plugins/` directory are fine. If a file fails to load, the error
is logged and shown as a status message, and the remaining plugins still load.

## Installing and managing plugins

A plugin published as a git repository can be installed with the `spotatui plugin` command. This
requires `git` on your PATH and does not need Spotify authentication.

```bash
spotatui plugin add owner/repo      # GitHub shorthand
spotatui plugin add https://gitlab.com/owner/repo.git
spotatui plugin add owner/repo --force   # reinstall over an existing copy

spotatui plugin list                # show installed plugins
spotatui plugin update              # update every plugin to its latest commit
spotatui plugin update <name>       # update just one
spotatui plugin remove <name>       # uninstall
```

`add` clones the repository into `~/.config/spotatui/plugins/<name>/` (a shallow clone) and
records it in `~/.config/spotatui/plugins.lock`. `update` fast-forwards each clone to the remote's
latest commit. Restart spotatui after installing or updating for changes to take effect, and bind
any commands the plugin registers under `plugin_commands` in `config.yml`.

Single-file plugins you drop into `plugins/` by hand are not tracked in the lockfile; `plugin list`
shows them under "untracked".

## Publishing a plugin

A shareable plugin is a git repository with a `main.lua` (or `init.lua`) entry point at its root:

```
my-plugin/
  main.lua        -- entry point; runs at startup
  lib.lua         -- optional helper module, loaded with require("lib")
  README.md       -- document the command(s) and a suggested key binding
```

The repository name becomes the local plugin name (its last path segment, minus `.git`). Ship a
*suggested* key binding in your README rather than writing to the user's `config.yml`; command
names are decoupled from keys by design.

## The `spotatui` API

A global table named `spotatui` is available in every plugin.

### Constants

- `spotatui.api_version` - integer API version (currently `3`).

### Events

Register a callback with `spotatui.on(event, fn)`. Passing an unknown event name raises an
error. Valid events:

| Event | Argument | Fires when |
|-------|----------|------------|
| `start` | none | The app finishes its first render. |
| `quit` | none | The app is shutting down. |
| `track_change` | playback table or nil | The current track identity changes (by uri, or name as a fallback), including the first track. |
| `playback_state_change` | playback table or nil | Playing/paused state changes (no playback counts as not playing). |
| `seek` | playback table or nil | Same track, same play state, and progress jumps backward by more than 1.5s or forward by more than 6.5s. Forward jumps inside that window are treated as normal Connect polling, not seeks. |
| `volume_change` | playback table or nil | The device volume percentage changes. |
| `queue_change` | none | The queue contents change. |

You can register multiple callbacks for the same event.

### Reads

These return a snapshot of the cached state. Snapshots are refreshed before callbacks run.

- `spotatui.playback()` - playback table, or `nil` when there is no playback.
- `spotatui.current_track()` - track table, or `nil`.
- `spotatui.devices()` - array of device tables.

The playback table has these fields:

```
{
  track = { uri, name, artists = { ... }, album, duration_ms } or nil,
  is_playing = bool,
  progress_ms = number,
  shuffle = bool,
  repeat = "off" | "track" | "context",
  volume_percent = number or nil,
  device = { id, name, kind, is_active, volume_percent } or nil,
}
```

### Actions

Actions are queued and applied by the app on the next opportunity; they do not return a
result. Every action follows the exact same code path as the equivalent keybinding, including
native streaming fast paths (librespot) when the native player is active.

- `spotatui.play()` - resume playback. No-op if already playing.
- `spotatui.pause()` - pause playback. No-op if already paused.
- `spotatui.next()` - skip to the next track.
- `spotatui.previous()` - go to the previous track, or restart the current track when more
  than 3 seconds in (matching the previous-track key behaviour).
- `spotatui.seek(ms)` - seek to a position in milliseconds.
- `spotatui.set_volume(pct)` - set volume; clamped to 0-100.
- `spotatui.shuffle(on)` - set shuffle to the desired state. No-op if already in that state.
- `spotatui.search(query)` - run a search and open the Search screen.
- `spotatui.notify(msg, ttl_secs?)` - show a status message (default ttl 4 seconds).

### Logging

- `spotatui.log(msg)` - write an info-level line to the app log.

### JSON utilities

- `spotatui.json_decode(json)` - parse a JSON string into Lua tables, strings, numbers,
  booleans, and nil-compatible values. Invalid JSON raises a Lua error.
- `spotatui.json_encode(value)` - serialize a Lua value to a compact JSON string. Values that
  cannot be represented as JSON, such as functions or userdata, raise a Lua error.

JSON `null` decodes to a light userdata sentinel, not Lua `nil`, and the sentinel is truthy in
Lua. To detect it, compare against a known null value:

```lua
local NULL = spotatui.json_decode("null")
local decoded = spotatui.json_decode('{"artist":null}')
if decoded.artist == NULL then
  -- field was present but null
end
```

```lua
local body = spotatui.json_encode({
  track = "spotify:track:...",
  rating = 5,
})

local decoded = spotatui.json_decode('{"ok":true,"items":[1,2]}')
spotatui.log("first item: " .. decoded.items[1])
```

### HTTP requests

HTTP runs asynchronously. Calls return immediately; the callback runs on a later UI tick after
the response arrives. Only `http://` and `https://` URLs are accepted.

- `spotatui.http_get(url, callback)` - send a GET request.
- `spotatui.http_post(url, body, headers, callback)` - send a POST request. `body` is a string.
  `headers` must be a table of string keys and string values, or `nil` for no headers. The
  four-argument form is required, so pass `nil` when you do not need headers.

Callbacks receive `callback(resp, err)`:

- Success: `resp = { status = number, ok = bool, body = string }`, `err = nil`.
- Transport failure such as DNS, timeout, or connection failure: `resp = nil`, `err = string`.
- HTTP 4xx and 5xx responses are not transport failures. They call the success path with
  `resp.ok = false`.

Response bodies are decoded with lossy UTF-8 conversion. In-flight requests are dropped when
spotatui exits.

```lua
spotatui.on("track_change", function(pb)
  if not pb or not pb.track then
    return
  end

  local url = "https://example.com/lyrics?uri=" .. pb.track.uri
  spotatui.http_get(url, function(resp, err)
    if err then
      spotatui.notify("lyrics fetch failed: " .. err, 4)
      return
    end
    if resp.ok then
      local parsed = spotatui.json_decode(resp.body)
      spotatui.popup("Lyrics", parsed.lines)
    else
      spotatui.notify("lyrics service returned " .. resp.status, 4)
    end
  end)
end)
```

```lua
local body = spotatui.json_encode({ event = "track_started" })

spotatui.http_post(
  "https://example.com/webhook",
  body,
  { ["content-type"] = "application/json" },
  function(resp, err)
    if err then
      spotatui.log("webhook failed: " .. err)
    elseif not resp.ok then
      spotatui.log("webhook returned " .. resp.status)
    end
  end
)
```

## Commands and keybindings

`spotatui.register_command(name, fn)` registers a named, callable action. The name must be a
non-empty string with no whitespace. Registering the same name twice (from any plugin) raises a
Lua error at load time.

```lua
spotatui.register_command("toggle_lyrics", function()
  spotatui.notify("lyrics toggled", 3)
end)
```

To bind a command to a key, add a `plugin_commands` section to `config.yml`:

```yaml
plugin_commands:
  toggle_lyrics: "ctrl-l"
  show_stats: "ctrl-g"
```

Each entry maps a command name to a key string. The key string uses the same format as the
built-in keybindings (e.g. `ctrl-l`, `alt-x`, `f1`, `space`). Entries are silently skipped when
the key string is invalid, the key is a reserved navigation key, or the key already has a named
action bound to it. The remaining entries are loaded normally.

When the bound key is pressed, the corresponding command callback fires after the current key
handler returns. An error in the callback is reported as a highlighted status message (6-second
ttl) and logged, but the command stays registered -- a transient failure does not permanently
unbind a key.

Plugin authors should document a suggested binding in their plugin rather than shipping one
in config. Command names are decoupled from keys by design: the user decides which key to use.

## UI extension

### Playbar segment

`spotatui.set_playbar(text)` sets a persistent text segment for the calling plugin, shown in
the playbar title as `" | {text}"` after any status message. Each plugin has its own segment
slot; calling `set_playbar` again replaces it. Pass `nil` to clear the segment.

```lua
spotatui.on("track_change", function(pb)
  if pb and pb.track then
    spotatui.set_playbar(pb.track.name)
  else
    spotatui.set_playbar(nil)
  end
end)
```

The segment persists until the plugin explicitly clears it. Multiple plugins each show their
own segment in alphabetical plugin-name order.

### Popup

`spotatui.popup(title, lines)` opens a centered modal dialog. The dialog overlays every
screen, including the help menu and queue. Press `j`/Down to scroll down, `k`/Up to scroll
up, and `Esc` or `q` to close. All other keys are swallowed while the popup is open.

`lines` can be:
- A single string.
- An array where each item is a string or a table `{ text, fg?, bold?, italic? }`.
  - `fg` is a color string in the same format as `config.yml` theme values (e.g. `"Red"`,
    `"Magenta"`, `"0, 200, 200"`).
  - `bold` and `italic` are booleans (default `false`).
  - Missing `text`, an unparseable color, or a non-string/non-table item raises a Lua error.

```lua
spotatui.popup("Track info", {
  { text = "Now playing", bold = true },
  { text = "Song title here", fg = "Cyan" },
  "",
  "Press Esc to close",
})
```

### Theme overrides

`spotatui.set_theme(tbl)` applies runtime color overrides to the active theme. Keys are
theme field names and values are color strings. Changes are applied immediately and affect all
subsequent renders. They are never written back to `config.yml` -- they are runtime-only and
reset on app restart.

Valid field names: `active`, `banner`, `error_border`, `error_text`, `hint`, `hovered`,
`inactive`, `playbar_background`, `playbar_progress`, `playbar_progress_text`, `playbar_text`,
`selected`, `text`, `background`, `header`, `highlighted_lyrics`, `analysis_bar`,
`analysis_bar_text`.

Color string format is the same as in `config.yml` (named ANSI color or `"r, g, b"`).

An unknown field name or an invalid color raises a Lua error.

```lua
spotatui.set_theme({
  playbar_text = "Magenta",
  hint = "0, 200, 0",
})
```

## Error behavior

Plugin code can never crash the app. If a callback raises an error or panics, the error is
logged, a highlighted status message is shown in the playbar, and that one callback is
disabled (one strike). Other callbacks, including other callbacks for the same event, keep
running.

Plugin errors are shown using the theme's error color and stay visible for 6 seconds.
Normal notifications (e.g. a "Now playing" message from `spotatui.notify`) cannot overwrite
a live plugin error -- the error is shown first, and the notification takes effect only after
the error expires. A later plugin error always replaces an earlier one immediately.

## Sample init.lua

```lua
spotatui.on("track_change", function(pb)
  if pb and pb.track then
    spotatui.notify("Now playing: " .. pb.track.name .. " by " .. table.concat(pb.track.artists, ", "), 4)
  end
end)

spotatui.on("start", function()
  spotatui.log("plugins loaded, api version " .. spotatui.api_version)
end)
```
