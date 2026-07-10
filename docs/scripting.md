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

## Trust and safety

Plugins are not sandboxed. A plugin runs with the same privileges as spotatui itself: it has the
full Lua standard library (including filesystem access via `io`/`os`) and can make arbitrary
network requests through `spotatui.http_get`/`http_post`. `spotatui plugin add` clones a git
repository and runs whatever its `main.lua` contains the next time you start spotatui.

Treat installing a plugin like running any other program from the internet: only install plugins
whose source you have read or whose author you trust, and prefer repositories you control. There is
no permission prompt and no isolation between a plugin and your account.

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

spotatui plugin new <name>          # scaffold a new plugin to start from
```

`add` clones the repository into `~/.config/spotatui/plugins/<name>/` (a shallow clone) and
records it in `~/.config/spotatui/plugins.lock`. `update` fast-forwards each clone to the remote's
latest commit. Restart spotatui after installing or updating for changes to take effect, and bind
any commands the plugin registers under `plugin_commands` in `config.yml`.

Single-file plugins you drop into `plugins/` by hand are not tracked in the lockfile; `plugin list`
shows them under "untracked".

## Publishing a plugin

The quickest start is `spotatui plugin new <name>`, which scaffolds a working directory plugin in
your config directory:

```bash
spotatui plugin new my-plugin
```

This writes `~/.config/spotatui/plugins/my-plugin/main.lua` (with a `require_api` guard, a sample
command, and a suggested key binding) plus a `README.md`. Edit it, then `git init` and push to
share it.

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

To help others find it, add the GitHub topic `spotatui-plugin` to your repository, and open a pull
request adding it to [`PLUGINS.md`](../PLUGINS.md).

## The `spotatui` API

A global table named `spotatui` is available in every plugin.

### Constants

- `spotatui.api_version` - integer API version (currently `5`).

### Declaring API compatibility

The scripting API is versioned and grows over time. If your plugin uses a feature added in a
particular version, declare it on the first line so users on an older spotatui get a clear
message instead of a cryptic `attempt to call nil` error:

```lua
spotatui.require_api(5)
```

`spotatui.require_api(n)` raises a load error (`requires spotatui scripting API v{n} ...`) when
the running build's `api_version` is lower than `n`, which stops that plugin from loading while
leaving the others untouched. Calling it with a version your build supports is a no-op.

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
| `shuffle_change` | playback table or nil | Shuffle flips while playback exists on both sides of the tick. |
| `repeat_change` | playback table or nil | The repeat mode changes while playback exists on both sides of the tick. |
| `device_change` | none | The device list changes (by id, name, or which device is active). |
| `search_results` | none | New search results arrive (from the Search screen or `spotatui.search`). |
| `route_change` | `{ name = "..." }` | The visible screen changes. Names are listed under Navigation below; plugin screens are `"plugin:<name>"`. |

You can register multiple callbacks for the same event.

### Reads

These return a snapshot of the cached state. Snapshots are refreshed before callbacks run.

- `spotatui.playback()` - playback table, or `nil` when there is no playback.
- `spotatui.current_track()` - track table, or `nil`.
- `spotatui.devices()` - array of device tables.
- `spotatui.playlists()` - array of playlist tables (cached; empty until the app has fetched playlists).
- `spotatui.queue()` - `{ currently_playing = item or nil, items = { ... } }` (cached).
- `spotatui.search_results()` - `{ tracks, albums, artists, playlists, shows }` (cached).
- `spotatui.current_route()` - the current screen name as a string.
- `spotatui.config()` - theme + behavior settings (see "Reading configuration" below).

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

`repeat` is a Lua reserved word, so index it with `pb["repeat"]`, not `pb.repeat`. (The matching
action is named `set_repeat` for the same reason.)

The cached reads for playlists, queue and search results refresh when the underlying data
changes; they are cheap to call but can be empty until the app has actually fetched that data.
Use the async `get_*` reads below when you want to trigger a fetch and be told when it lands.

### Async data reads

These request fresh data from Spotify and deliver it to a callback, following the same
`callback(data, err)` convention as HTTP. The call returns immediately; the callback runs on a
later UI tick once the data has arrived. If nothing arrives within 15 seconds the callback
receives `(nil, "request timed out")`. Callbacks are one-shot.

- `spotatui.get_playlists(cb)` - `cb(array of playlist tables, err)`.
- `spotatui.get_queue(cb)` - `cb({ currently_playing, items }, err)`. Each queue item is
  `{ kind = "track" | "episode", track = {...} or nil, episode = {...} or nil }`. An
  unavailable queue (no active device) delivers an empty snapshot.
- `spotatui.get_search_results(query, cb)` - runs a search (without leaving the current
  screen) and delivers `{ tracks, albums, artists, playlists, shows }`.
- `spotatui.get_saved_tracks(cb)` - liked songs fetched so far.
- `spotatui.get_saved_albums(cb)` - `{ album = {...}, added_at }` entries.
- `spotatui.get_saved_shows(cb)` - saved podcasts.
- `spotatui.get_recently_played(cb)` - recently played tracks.
- `spotatui.get_devices(cb)` - refreshes the device list without opening the device picker.
- `spotatui.get_lyrics(cb)` - `cb({ status, lines }, err)` for the current track, where
  `status` is `"not_started" | "loading" | "found" | "not_found"` and each line is
  `{ time_ms, text }`. Delivered immediately when lyrics are already resolved; otherwise waits
  for the in-flight fetch.

```lua
spotatui.get_playlists(function(playlists, err)
  if err then
    spotatui.notify("playlists: " .. err, 4)
    return
  end
  spotatui.notify("you have " .. #playlists .. " playlists", 4)
end)
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
- `spotatui.set_repeat(mode)` - set repeat to `"off"`, `"track"` or `"context"` (named
  `set_repeat` because `repeat` is a Lua reserved word).
- `spotatui.cycle_repeat()` - cycle repeat exactly like the repeat keybinding (keeps the
  native-streaming fast path).
- `spotatui.transfer_playback(device_id)` - transfer playback to a device (does not overwrite
  your saved device preference).
- `spotatui.play_uri(uri)` - play a `spotify:track:`/`spotify:episode:` uri directly, or start
  a `spotify:album:`/`playlist:`/`artist:`/`show:` uri as a context. Anything else raises.
- `spotatui.play_context(uri, offset?)` - play a container uri, optionally from a 0-based
  track offset.
- `spotatui.add_to_queue(uri)` - add a track/episode to the queue.
- `spotatui.create_playlist(name, uris?)` - create a playlist, optionally seeded with track uris.
- `spotatui.playlist_add_track(playlist, track)` - add a track to a playlist (ids or uris).
- `spotatui.playlist_remove_track(playlist, track, position)` - remove the track occurrence at
  the given 0-based position (required, like the Web API).
- `spotatui.follow_playlist(playlist)` / `spotatui.unfollow_playlist(playlist)`.
- `spotatui.toggle_save_track(uri)` - like/unlike a track.
- `spotatui.save_album(id)` / `spotatui.unsave_album(id)`.
- `spotatui.save_show(id)` / `spotatui.unsave_show(id)`.
- `spotatui.follow_artist(id)` / `spotatui.unfollow_artist(id)`.

Arguments are lightly validated (non-empty, well-formed uri kinds, in-range numbers); invalid
arguments raise a Lua error at call time. The network call itself still happens later, so a
valid-looking id that Spotify rejects surfaces as a normal API error message, not a Lua error.

### Timers

- `spotatui.set_timeout(ms, fn) -> handle` - run `fn` once after roughly `ms` milliseconds.
- `spotatui.set_interval(ms, fn) -> handle` - run `fn` repeatedly every roughly `ms`
  milliseconds (`ms` must be at least 1).
- `spotatui.cancel_timer(handle)` - cancel a pending timeout or interval. Unknown or expired
  handles are a no-op.

Timers fire from the app's tick loop, so their real resolution is the UI tick rate
(`behavior.tick_rate_milliseconds`, at most 999ms) -- a 10ms timeout still waits for the next
tick. If the app stalls past several interval periods, the interval fires once and reschedules
(no catch-up burst). An interval whose callback errors is removed after the first failure.

### Navigation

- `spotatui.navigate(target)` - open a screen, doing exactly what the matching keybinding does
  (including any data fetch it performs). Valid targets: `home`, `queue`, `settings`,
  `devices`, `help`, `lyrics`, `recently_played`, `party`, `analysis`, `miniplayer`. Unknown
  targets raise.
- `spotatui.back()` - pop the navigation stack, like the back key.
- `spotatui.current_route()` - the current screen name. Screen names also show up in the
  `route_change` event: `home`, `search`, `track_table`, `album_tracks`, `album_list`,
  `artist`, `artists`, `recently_played`, `devices`, `queue`, `settings`, `help`, `lyrics`,
  `cover_art`, `miniplayer`, `analysis`, `discover`, `podcasts`, `podcast_episodes`,
  `recommendations`, `party`, `friends`, `local_browser`, `create_playlist`, `dialog`,
  `announcement`, `exit_prompt`, `error`, and `plugin:<name>` for plugin screens.

### Persistent storage

Each plugin gets a private key-value store persisted as plain JSON at
`~/.config/spotatui/plugin-data/<plugin>.json`. Values must be JSON-serializable (tables,
strings, numbers, booleans); functions and userdata raise.

- `spotatui.storage_get(key)` - the stored value, or `nil`.
- `spotatui.storage_set(key, value)` - store a value. `nil` removes the key.
- `spotatui.storage_remove(key)` - remove a key.
- `spotatui.storage_keys()` - array of stored key names.

Writes are flushed to disk in the background (roughly every 3 seconds) and always on quit.
The files are plain JSON on disk -- other programs (and other plugins, via `io`) can read
them, so do not store secrets. If two spotatui instances run at once, the last writer wins.
A missing or corrupt file starts the plugin with an empty store (corruption is logged).

```lua
local plays = (spotatui.storage_get("play_count") or 0) + 1
spotatui.storage_set("play_count", plays)
```

### Reading configuration

`spotatui.config()` returns the live user configuration:

```
{
  theme = { active = "0, 180, 180", playbar_text = "Reset", ... },
  behavior = {
    seek_milliseconds, volume_increment,
    tick_rate_milliseconds, animation_tick_rate_milliseconds,
    liked_icon, shuffle_icon, repeat_track_icon, repeat_context_icon,
    playing_icon, paused_icon,
    enable_text_emphasis, show_loading_indicator, enforce_wide_search_bar,
    set_window_title, shuffle_enabled, stop_after_current_track,
    sidebar_width_percent, playbar_height_rows, library_height_percent,
    active_source,
  },
}
```

Theme values use the same string forms as `config.yml` (named color or `"r, g, b"`), so they
can round-trip through `spotatui.set_theme`. Secrets and service credentials (sync token,
relay URL, Subsonic credentials, Discord client id) are never exposed. The snapshot is
populated once the app is running; reading it at load time (before the `start` event) returns
empty defaults.

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

### Custom screens

Plugins can register full-screen views. Screens are retained-mode: you publish content with
`set_screen`, the app renders it from its own state, and you update it again whenever you want
the view to change (typically from `on_key`, a timer, or a data callback). There is no
per-frame draw callback.

- `spotatui.register_screen(name, spec)` - register a screen. `name` must be non-empty with no
  whitespace and unique across all plugins. `spec` is a table:
  - `title` (optional string) - the window title (defaults to the screen name).
  - `on_key(key)` (required) - called for every key pressed while the screen is focused. `key`
    is a `config.yml`-style string (`"a"`, `"ctrl-x"`, `"enter"`, `"up"`, `"space"`, ...).
  - `on_open()` / `on_close()` (optional) - called when the screen gains/loses the route.
- `spotatui.set_screen(name, widgets)` - publish the screen's content (see widgets below).
- `spotatui.show_screen(name)` - navigate to the screen.
- `spotatui.close_screen(name)` - leave the screen if it is currently shown.

`set_screen`/`show_screen`/`close_screen` verify the screen is registered and owned by the
calling plugin.

`widgets` is an array; each entry is a table with a `type`:

- `{ type = "paragraph", lines = <lines>, height? }` - styled text. `lines` uses the same
  format as `spotatui.popup`.
- `{ type = "list", items = <lines>, title?, selected?, height? }` - a bordered list.
  `selected` is 1-based (like Lua arrays) and highlights that item.
- `{ type = "gauge", ratio, label? }` - a progress bar; `ratio` is clamped to 0..1.

Widgets with a `height` take exactly that many rows; the rest split the remaining space
evenly.

Keys reach `on_key` only after the global keybindings have run, so keys like the back key or
volume keys keep their normal meaning. `Esc` (and the back key) leave the screen, and
`PageUp`/`PageDown` scroll paragraph text; everything else is forwarded. An `on_key` that
errors is disabled after the first failure (one strike).

A minimal interactive screen:

```lua
spotatui.require_api(5)

local selected = 1
local names = {}

local function render()
  spotatui.set_screen("my_playlists", {
    { type = "paragraph", lines = { { text = "j/k to move, Esc to leave", italic = true } }, height = 2 },
    { type = "list", title = "Playlists", items = names, selected = selected },
  })
end

spotatui.register_screen("my_playlists", {
  title = "My Playlists",
  on_key = function(key)
    if key == "j" and selected < #names then selected = selected + 1 end
    if key == "k" and selected > 1 then selected = selected - 1 end
    render()
  end,
  on_open = function()
    spotatui.get_playlists(function(playlists, err)
      names = {}
      for _, p in ipairs(playlists or {}) do
        names[#names + 1] = p.name
      end
      render()
    end)
  end,
})

spotatui.register_command("my_playlists", function()
  spotatui.show_screen("my_playlists")
end)
```

Bind `my_playlists` under `plugin_commands` in `config.yml` to open it with a key.

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
