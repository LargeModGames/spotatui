# Set up the spotatui MCP server

> **Give this to your AI agent.** Copy the line below into Claude Code, Codex,
> Gemini CLI, Cursor, or any agent that can read files and run commands:
>
> ```text
> Read docs/mcp-setup.md in the spotatui repo (or
> https://raw.githubusercontent.com/LargeModGames/spotatui/main/docs/mcp-setup.md)
> and set up the spotatui MCP server for me, following it exactly.
> ```
>
> Everything the agent needs is in this file. It is written as instructions **for
> the agent**, so it is fine to hand over verbatim.

---

## What this sets up

spotatui exposes your music player and listening history as MCP tools, so an AI
agent can act as your DJ: read what you have been listening to, search the
catalogue, and queue tracks. **No API key is required** — the agent you already
have installed and authenticated does the thinking.

```text
Your agent ──stdio(MCP)──> `spotatui mcp` ──loopback TCP──> running spotatui
```

---

## Instructions for the agent

Follow these steps in order. After each step, run the verification command and do
not continue until it passes.

### Step 0 — Read this before running anything

**Never run `spotatui mcp` to test the setup.** That command *is* the server: it
reads JSON-RPC from stdin and blocks until the stream closes, so it will appear
to hang and you will learn nothing from it.

Use `spotatui mcp status` instead. It is safe, returns immediately, and exits
non-zero when the setup is not ready:

```bash
spotatui mcp status          # human-readable report
spotatui mcp status --json   # same thing, machine-readable
```

Run it now to see the starting state. Two outcomes are both normal before setup,
and they mean different things:

* it fails at `control-file` — the binary is right, the socket is not on yet.
  Go to Step 2.
* it fails with `unrecognized subcommand 'mcp'` — this build does not have the
  feature at all. Go to Step 1.

### Step 1 — Confirm the binary has the feature compiled in

```bash
spotatui mcp status
```

* If the first line reads `ok   binary   spotatui <version> with the mcp-server feature`,
  continue to Step 2.
* If the command fails with something like `unrecognized subcommand 'mcp'`, this
  build does not include the feature. Install one that does:

  ```bash
  cargo install --locked spotatui --features mcp-server
  # or, from a checkout:
  cargo install --locked --path . --features mcp-server
  ```

  The feature is not in `default`, so a plain `cargo build` omits it. Do not
  assume any particular binary has it — the `spotatui mcp status` check above is
  the way to tell.

### Step 2 — Turn on the control socket

The socket is **opt-in**. Edit spotatui's `config.yml` and set, under the
`behavior:` key:

```yaml
behavior:
  mcp_enabled: true
```

Create the `behavior:` section if the file does not have one. Do not remove or
reorder anything else in the file.

> **Which `config.yml`.** spotatui resolves its config directory as
> `$XDG_CONFIG_HOME/spotatui` when `XDG_CONFIG_HOME` is set to an *absolute*
> path, and `~/.config/spotatui` otherwise — so do not assume the second one.
> `spotatui mcp status` prints the resolved directory; use the path it reports.
> On Windows the fallback is `%USERPROFILE%\.config\spotatui`.

### Step 3 — Restart spotatui

The socket is opened at startup, so a running instance will not pick this up.

Ask the user to quit spotatui and start it again. **Do not try to start spotatui
yourself** — it is a full-screen terminal application and needs a real terminal;
launching it from a tool call will either fail or hang.

When it starts, spotatui briefly shows `MCP server listening on 127.0.0.1:<port>`.

### Step 4 — Verify the connection

```bash
spotatui mcp status
```

All three checks must read `ok`, and the exit code must be `0`:

```text
ok   binary         spotatui 0.40.3 with the mcp-server feature
ok   control-file   found (port 43219, pid 12345) at /home/you/.config/spotatui/mcp.json
ok   connection     handshake accepted; server speaks protocol 2026-07-28
```

The `control-file` line reports the path spotatui actually resolved, which is
where its `config.yml` lives too.

If `connection` fails but `control-file` passed, the control file is stale from a
previous run — have the user restart spotatui again.

### Step 5 — Register the server with your client

Use whichever line matches the client you are running in:

```bash
# Claude Code
claude mcp add spotatui -- spotatui mcp

# Codex  (usage is `codex mcp add <NAME> -- <COMMAND>...`)
codex mcp add spotatui -- spotatui mcp
```

For any other client, add a **stdio** server with command `spotatui` and args
`["mcp"]`. As raw JSON (Claude Desktop, Cursor, Windsurf, Zed and others use this
shape):

```json
{
  "mcpServers": {
    "spotatui": {
      "command": "spotatui",
      "args": ["mcp"]
    }
  }
}
```

TOML, for `~/.codex/config.toml`:

```toml
[mcp_servers.spotatui]
command = "spotatui"
args = ["mcp"]
```

If `spotatui` is not on `PATH` for the client's environment, use the absolute
path: `command -v spotatui` on Linux and macOS, `Get-Command spotatui` in
PowerShell.

### Step 6 — Confirm the tools are visible

Reload or restart your client so it picks up the new server, then list your tools.
You should see eight, all prefixed with the server name (`mcp__spotatui__…` in
Claude Code):

| Tool | What it does |
|---|---|
| `get_listening_history` | Aggregate taste summary: top artists, tracks, albums, recent plays |
| `get_now_playing` | Current track, play state, queue depth |
| `get_queue` | Upcoming tracks |
| `search_tracks` | Find tracks and their URIs, each marked `owned` or `new` |
| `queue_tracks` | Add tracks by URI, or by title + artist; `exclude_owned` skips ones they already have |
| `play_now` | Start a track immediately |
| `skip_track` | Skip to the next track |
| `set_dj_vibe` | Record a standing direction for the built-in auto-queue DJ (does not start it) |

Then confirm it end-to-end by calling `get_now_playing`. A successful call proves
the whole chain. Report the result to the user and stop — setup is done.

---

## Troubleshooting

| Symptom | Cause and fix |
|---|---|
| `unrecognized subcommand 'mcp'` | Binary built without the feature — Step 1. |
| `spotatui mcp status` fails at `control-file` | `mcp_enabled` is not set, or spotatui has not been restarted since it was — Steps 2 and 3. |
| Fails at `connection`, `control-file` passed | Stale control file from a killed process. Restart spotatui. |
| `spotatui mcp` seems to hang | Working as intended: that command is the server. Use `spotatui mcp status`. |
| Client shows the server as failed | The client spawns `spotatui mcp`, which needs `spotatui` on its `PATH`. Re-register with the absolute path from `command -v spotatui`. |
| Tools appear but every call returns "spotatui is not available" | The relay started but no player is running. Start spotatui. |
| A tool returns "spotatui has no Spotify session" | The catalogue tools need Spotify. Log in from the spotatui UI. |
| Tracks are "not found" when queueing by name | Expected for a track that is not in the catalogue. `queue_tracks` reports which ones it dropped — read the result and pick alternatives. |
| The first `search_tracks` says the playlist index is still building | Expected once per spotatui run. `owned` reflects Liked Songs only for that one search; the crawl runs in the background so every search after it is complete. Search again if it matters. |
| `exclude_owned` returns "could not read your playlists" | The playlist crawl failed, so the filter could not run and nothing was queued rather than being queued unfiltered. Retry, or call again without `exclude_owned`. |

To undo everything: remove the server from your client
(`claude mcp remove spotatui`), set `mcp_enabled: false`, and restart spotatui.

---

## Notes for the agent, once it is working

* **Call `get_listening_history` first** when choosing music. It returns aggregate
  names only — no identifiers, no timestamps.
* **`queue_tracks` can partially fail.** It reports which tracks it could not
  find. Read the result rather than assuming everything was queued.
* **Prefer `queue_tracks` over `play_now`** unless the user asked to hear
  something immediately; `play_now` interrupts what is playing.
* **`search_tracks` tells you what they already have.** Every result is marked
  `[owned]` or `[new]` — `owned` meaning it is in their Liked Songs or in a
  playlist they own or collaborate on. When they asked for something *new*, pick
  the ones marked new. When they asked for a specific track, queue it whether they
  own it or not; that is what they asked for.
* **`queue_tracks` takes `exclude_owned: true`** if you would rather have the
  guarantee than do the filtering yourself. It skips anything they already have
  and reports what it skipped, and it substitutes nothing — so you may get back
  fewer tracks than you asked for. Leave it off (the default) when the user named
  specific tracks.
* **`set_dj_vibe` does not start anything.** It records a standing direction for
  the *optional* in-TUI auto-queue DJ, which may be switched off or not built into
  this binary at all. The result tells you which; when nothing will act on it, the
  vibe is still stored and comes back from `get_listening_history`, so honour it
  yourself when you queue. There is no tool to turn continuous auto-queue on —
  that is the user's key in the TUI. To keep music flowing, poll `get_now_playing`
  for `queue_depth` and top up with `queue_tracks`.
* Every change you make raises a visible status message in the user's TUI, so
  they can see what you did.

## What the user should know

* The socket listens on **loopback only** and requires a token from `mcp.json`
  in the same config directory (mode `0600`, in a `0700` directory). A local
  process that can read that file can control your player and read your listening
  history — the same trust level as the existing Spotify token cache.
* **Data sent to your agent's model provider:** whatever the agent chooses to
  send. `get_listening_history` returns track, artist, and album *names* only.
* Turning it off is one line: `mcp_enabled: false`, then restart.

See also [`docs/configuration.md`](configuration.md) for every config key.
