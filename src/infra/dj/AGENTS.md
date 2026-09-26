### The DJ: lanes, guards, and the avoid-library filter

The tool surface is defined once - `src/infra/dj/tools.rs::TOOLS` (8 tools). MCP
publishes it verbatim; the in-TUI brain renders the same table into its prompt.

- **Lanes**: `AskDj` / `DjTopUp` run on the service lane (a brain call can take
  minutes; they touch only `App` and use `dispatch_without_spinner` - plain
  `dispatch` would pin the global spinner for the whole call). `DjToolCall` /
  `DjIndexLibrary` run on the serial lane because they need the real Spotify
  client. `DjToolCall` additionally bypasses the auth gate so an unauthenticated
  MCP caller gets a diagnosable error instead of a dropped oneshot;
  `DjIndexLibrary` does not.
- **Two staleness guards, both load-bearing**: `app.dj.generation` decides whether
  a turn's results are still wanted (re-checked before every mutating tool call);
  `dj.turn_seq` decides who may clear `dj.thinking` - without it an abandoned turn
  clears its replacement's flag and the top-up fires a duplicate refill.
- **The filter is two gates, both needed**: `resolve_suggestions` rejects on the
  *name the model gave* before paying for a search; `reject_owned_tracks` rejects
  on the *resolved track ID* afterwards (the only gate that sees `uri` entries).
  Rejections go in `ResolveReport::in_library` - never `duplicates` (wrong words
  for the user), never `unresolved` (tells the model a real track doesn't exist).
- **The filter is OFF by default everywhere** (`behavior.dj_avoid_library:
  false`). The in-TUI difference is *ownership*: once the listener toggles it on
  (Ctrl+O), `agent::apply_policy` forces `exclude_owned` onto every
  `queue_tracks` whether or not the model asked. Over MCP the agent decides per
  call; `search_tracks` only *marks* results `owned`; `play_now` is never filtered.
- **Where the crawl runs is a latency decision**: `search_tracks` never crawls
  inline - it answers from Liked Songs, dispatches `IoEvent::DjIndexLibrary`, and
  reports `ownership_complete: false` (seconds of pagination inside a tool call
  would head-of-line-block the serial lane). `queue_tracks(exclude_owned)` is the
  one caller that crawls inline via `dj_library_index`, and refuses the call
  outright if the crawl fails: it was asked for a guarantee.
- The taste brief is aggregates and names only - no timestamps, IDs, or identity.
  That is a privacy property, not a token optimization.
