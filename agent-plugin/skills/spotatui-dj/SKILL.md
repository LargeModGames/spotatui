---
name: spotatui-dj
description: Be the DJ for spotatui, the terminal music player, by driving its MCP server. Use whenever the user asks for music, asks you to DJ, wants tracks queued, played, skipped, or searched, asks what they have been listening to, or mentions spotatui.
---

# DJ for spotatui

You control a music player the user is watching. Every change you make raises a
visible status message in their TUI, so act deliberately and report what happened.

## Before anything else

The eight spotatui tools are visible whenever the plugin's server is registered.
In Claude Code a plugin's tools carry the plugin's own prefix, so they read
`mcp__plugin_spotatui_spotatui__…` here rather than the `mcp__spotatui__…` a
hand-registered server would give. Whether a *call* succeeds is a separate
question: that additionally needs spotatui itself running, built with
`--features mcp-server`, and configured with `behavior.mcp_enabled: true`.
Otherwise the tools are still listed and every call reports that spotatui is not
available.

If the tools are missing or every call reports spotatui is not available:

```bash
spotatui mcp status      # safe, returns immediately, exits non-zero on the failing step
```

Then follow
<https://raw.githubusercontent.com/LargeModGames/spotatui/main/docs/mcp-setup.md>.

**Never run `spotatui mcp` to test anything.** That command *is* the server: it
reads JSON-RPC from stdin and blocks until the stream closes, so it will look like
a hang and teach you nothing. Do not try to start spotatui yourself either; it is a
full-screen terminal app and needs a real terminal. Ask the user to start it.

## The tools

| Tool | Use it for |
|---|---|
| `get_listening_history` | Aggregate taste summary: top artists, tracks, albums, recent plays, current vibe, now playing |
| `get_now_playing` | Current track, whether playback is active, `queue_depth` |
| `get_queue` | Upcoming tracks, in play order |
| `search_tracks` | Find tracks and their URIs; each result marked `owned` or `new` |
| `queue_tracks` | Add tracks by `uri`, or by `title` + `artist` |
| `play_now` | Start one track immediately |
| `skip_track` | Skip to the next track |
| `set_dj_vibe` | Record a standing direction for the built-in auto-queue DJ |

That is the whole surface. Do not promise the user control beyond it; in
particular there is no tool to toggle continuous auto-queue.

## Start with their taste

Call `get_listening_history` **first** whenever you are choosing music for them.
`period` accepts `7d`, `30d`, `month`, `year`, `all`, and defaults to `30d`. A
normal summary returns aggregate names only, no identifiers or timestamps, plus
whatever is playing now and the current DJ vibe.

When there is very little history it returns none of that — no top lists, no vibe,
no now playing, just a short note saying so. Take the note at face value and **ask
the user what they feel like** rather than inferring a taste profile from a handful
of plays.

## Search before you queue by name

When you are not certain a track exists in the catalogue, call `search_tracks`
first (`query` required, `limit` 1-20, default 10) and queue the URIs it returns.

Every result is marked **`owned`** (in their Liked Songs, or in a playlist they own
or collaborate on) or **`new`**.

* They asked for something they do not already have: prefer results marked `new`.
  `new` means "not in their library", not "never heard" — nothing here reads their
  play history.
* They named a specific track: queue it whether they own it or not. That is what
  they asked for.

## Queue, do not interrupt

`queue_tracks` takes an array of tracks **in play order**. Each entry is either a
`uri` (preferred, straight from `search_tracks`) or both a `title` and an `artist`
to be looked up. **At most eight tracks per call.** A longer array is rejected as
invalid arguments and nothing at all is queued, so split a longer run into batches
of eight, sent in play order.

It can partially succeed. Tracks it cannot find are skipped and reported back.
**Read the result** and tell the user what actually landed; never assume everything
queued. If a name was not found, that track is not in the catalogue under that
name, so pick an alternative rather than retrying the same string.

`exclude_owned: true` makes it skip anything they already have:

* Set it only when they asked for music they do **not** already have.
* Leave it off when they named specific tracks.
* It substitutes nothing, so fewer tracks may land than you sent.
* If it errors with "could not read your playlists", the filter could not run and
  nothing was queued. Retry, or call again without the flag.

Use `play_now` **only when the user asked to hear something right now** — it
interrupts whatever is playing. Otherwise queue.

## `set_dj_vibe` starts nothing

It records a standing direction (for example "mellow instrumental for focusing")
for the *optional* in-TUI auto-queue DJ, which may be switched off or not built
into this binary at all. Pass `null` to clear it. Read the result: it tells you
whether anything will actually act on the vibe.

The vibe is stored either way and comes back from `get_listening_history`, so when
nothing else will act on it, **honour it yourself** when you pick tracks.

## Keeping the music flowing

There is no tool to switch continuous auto-queue on; that is the user's key inside
the TUI. To DJ continuously yourself, poll `get_now_playing`, watch `queue_depth`,
and top up with `queue_tracks` before it runs dry.

## Quirks worth knowing

* The first `search_tracks` after spotatui starts may note that the playlist index
  is still building. For that one search, `owned` reflects Liked Songs only; the
  crawl finishes in the background, so search again if ownership matters.
* A tool reporting "spotatui has no Spotify session" means the catalogue tools need
  a login. Ask the user to log in from the spotatui UI.
* Every change you make is announced in the user's TUI. They can see what you did.
