# AGENTS.md

This file provides guidance to Codex (Codex.ai/code) when working with code in this repository.

## Build & Run

```bash
# Full build (native streaming + audio visualization)
cargo run

# Slim build — no librespot/audio; fastest iteration, used by CI
cargo run --no-default-features --features telemetry

# With the free alternative sources (Local/Subsonic/Radio/YouTube). These are NOT
# in `default`, so a plain `cargo run` is Spotify-only; use the `all-sources` alias
# (or list them individually) to exercise the first-run source picker and playback.
cargo run --features all-sources
```

## CI Checks (run before opening a PR)

```bash
cargo fmt --all
cargo clippy --no-default-features --features telemetry -- -D warnings
cargo test --no-default-features --features telemetry
```

These slim commands are the *fast local gate*, not the full picture. GitHub Actions
(`.github/workflows/ci.yml`, on `ubuntu-latest`) runs `check`, `test`, and `clippy`
across a **five-leg** feature matrix:

| Leg | Features |
|-----|----------|
| `default` | streaming + audio-viz (a plain `cargo test`) |
| `all-sources` | the full release feature set from `cd.yml` |
| `mcp-only` | `telemetry,mcp-server` |
| `ai-dj-only` | `telemetry,ai-dj` |
| `slim` | `telemetry` |

so `#[cfg(feature = "…")]` tests (e.g. `streaming`) that the slim command skips still
run in CI. `mcp-only` and `ai-dj-only` matter more than their size suggests: both
enable `dj-core` **without** `streaming`, a combination none of the three commands
above covers, so code that assumes dj-core implies streaming compiles locally and
fails CI. To reproduce a leg locally, run `cargo test` (default) or
`cargo test --features all-sources`.

Note that CI runs `clippy` on the **bin target only** (no `--all-targets`), so lints
in `#[cfg(test)]` code are not gated. Run `cargo test` to compile test code.

## Run a Single Test

```bash
cargo test --no-default-features --features telemetry <test_name>
# Example:
cargo test --no-default-features --features telemetry global_shift_w_adds_current_track_from_anywhere
```

## Architecture

The codebase is split into four top-level modules under `src/`:

| Module | Role |
|--------|------|
| `core/` | Business logic & centralized state (`App`, `UserConfig`, `SortState`) |
| `infra/` | Infrastructure: Spotify API (`network/`), audio capture/viz (`audio/`), native streaming (`player/`), OS integrations (Discord RPC, MPRIS, macOS media keys) |
| `tui/` | Terminal UI: rendering (`ui/`), per-screen input handlers (`handlers/`), event loop (`event/`) |
| `cli/` | CLI argument parsing and self-update logic |

### Data flow

```
Key event → tui/event/ → tui/handlers/handle_app()
                           ↓ global keybindings
                           ↓ handle_block_events() dispatches to per-screen handler
                           ↓ app.dispatch(IoEvent::…) sends async work
                        infra/network/ fetches from Spotify API
                           ↓ mutates App state
                        tui/ui/ re-renders from App state
```

### Navigation / routing

`App` holds a navigation stack of `Route` values. Each `Route` contains:
- `id: RouteId` — which screen to render (Home, Search, Artist, AlbumTracks, Queue, Settings, Party, …)
- `active_block: ActiveBlock` — which block currently has keyboard focus
- `hovered_block: ActiveBlock` — which block the cursor is hovering

Note there is no `HoveredBlock` *type*: `hovered_block` is a second field of the
same `ActiveBlock` enum.

Use `app.push_navigation_stack(RouteId::X, ActiveBlock::X)` to navigate and `app.pop_navigation_stack()` to go back.

### The `core/app/` module folder

`App` was one 10,920-line file; it is now a folder. The struct itself stays **flat** —
all ~230 fields are declared once in `src/core/app/mod.rs` — and its ~210 methods are
split across sibling modules by concern, each contributing its own `impl App { … }` block.

| Area | Files |
|------|-------|
| Foundation | `mod.rs` (the `App` struct + `dispatch`), `construction.rs` (`Default`/`new`), `route.rs`, `models.rs`, `help.rs`, `scrollable_pages.rs`, `status.rs` |
| Config & input | `keybindings.rs`, `settings_schema.rs`, `settings_apply.rs` |
| Presentation | `lyrics.rs`, `album_theme.rs`, `tick.rs` |
| Playback | `seek.rs`, `volume.rs`, `transport.rs`, `shuffle_repeat.rs`, `playback_routing.rs` |
| Native streaming | `native_backend.rs`, `native_recovery.rs`, `native_shuffle.rs` |
| Queue | `queue.rs`, `dj.rs`, `queue_suspend.rs`, `persistence.rs` |
| Library & playlists | `library.rs`, `playlists.rs`, `playlist_folders.rs`, `playlist_pages.rs` |
| Screens & test support | `friends.rs`, `discover.rs`, `plugins.rs`, `test_support.rs` |

Three rules when working in here:

- **Import from `crate::core::app`, never the submodule path.** `mod.rs` blanket
  re-exports every module (`pub use route::*;` …), so `use crate::core::app::{App,
  RouteId, ActiveBlock}` keeps working no matter which file an item lives in. Moving a
  type between modules then costs nothing outside the folder.
- **Child modules open with `use super::*;`** and declare no imports of their own. All
  external imports live in `mod.rs`, so a child inherits them with no per-`cfg`
  bookkeeping, and a glob never trips `unused_imports`.
- **A private helper called from a sibling module needs `pub(super)`.** Rust privacy is
  per-module, so a plain `fn` in `seek.rs` is invisible to `tick.rs` even though both are
  `impl App`. Private *fields* of `App` need no change: they are declared in `mod.rs` and
  are visible to all its descendants.

Tests are colocated: each module carries its own `#[cfg(test)] mod tests`. Fixtures shared
by more than one module's tests live in `test_support.rs` as `pub(super) fn`.

### Listening Party / sync

The Party feature (`src/infra/network/sync.rs`) connects host and guests via WebSocket relay using `SyncMessage` enums. `IoEvent::StartParty`, `JoinParty`, `SyncPlayback`, and `LeaveParty` drive the party lifecycle from handlers.

## Key Conventions

### Adding a new screen / feature

1. Add a variant to `RouteId` and `ActiveBlock` in `src/core/app/route.rs`.
2. Create `src/tui/handlers/<screen>.rs` with a `pub fn handler(key: Key, app: &mut App)` function and register it in `src/tui/handlers/mod.rs` (`handle_block_events` match arm).
3. Create `src/tui/ui/<screen>.rs` with a draw function and wire it into `src/tui/ui/mod.rs`.
4. Add any new Spotify API calls as `IoEvent` variants in `src/infra/network/mod.rs` and implement them in the appropriate `src/infra/network/<concern>.rs` file.

### Dispatching network calls

Call `app.dispatch(IoEvent::SomeVariant)` from a handler — never call async Spotify code directly from handlers or UI code.

### Paginated results

Use `ScrollableResultPages<T>` (defined in `src/core/app/scrollable_pages.rs`) for any data that comes back page-by-page from the Spotify API.

For `ScrollableResultPages<Page<T>>` caches specifically:
- key and dedupe cached pages by `page.offset`, not by insertion order
- preserve the active visible page by offset when inserting new cached pages
- treat sparse caches as valid; next/previous page logic must target adjacent offsets, not cache index +/- 1
- keep visible table state separate from cache state; background prefetch must not append directly into `track_table.tracks`
- guard background prefetch with a generation/session value so stale tasks cannot write into a reloaded view
- when a page is already cached, prefer rendering it synchronously from app state instead of routing through another async event
- for playlist track tables, use `playlist_track_table_id` as the current table identity; `active_playlist_index` is sidebar selection state only

### Status messages

Show feedback through the status-message helpers; never write `app.status_message` directly. In sync handlers/UI use `app.set_status_message(msg, ttl_secs)`; in the async network layer use `self.show_status_message(msg, ttl_secs)`. The TTL is in seconds.

### Dialog state cleanup

When closing a dialog, always call `app.clear_playlist_track_dialog_state()` alongside `app.dialog = None` and `app.confirm = false`.

### User-configurable keybindings

Always check `app.user_config.keys.<action>` instead of hard-coding key literals when matching global actions (see `handle_app` in `src/tui/handlers/mod.rs`).

### Feature flags

- Default features include `streaming` (librespot) and audio visualization backends.
- `--no-default-features --features telemetry` is the minimal build used for CI and fast iteration.
- Platform-specific audio backends (ALSA, PipeWire, PortAudio, Rodio) are gated behind their own features.
- `cover-art` feature enables album art rendering via `ratatui-image`.
- `dj-core` is a shared implementation feature (taste brief, name→URI resolver,
  bulk enqueue) pulled in by `mcp-server` and `ai-dj`, the way `audio-decode` is
  pulled in by the media sources. None of the three are in `default`.
- `mcp-server` adds the `spotatui mcp` MCP server; `ai-dj` adds the in-TUI DJ
  screen and its model backends. Neither adds a crate dependency.

### Adding a feature-gated sidebar row

`library_options()` (`src/core/app/library.rs`) composes the sidebar list at first use
rather than declaring one `const` per feature combination — gated rows would
otherwise be a cartesian product. Look entries up **by name**
(`library_options().iter().position(...)`), never by index: the index of any row
after a gated one depends on which features are built in.

### The DJ's two IoEvent lanes

`AskDj` / `DjTopUp` run on the **service** lane (detached; a brain call can take
minutes) and touch only `App`. `DjIndexLibrary` and
`DjToolCall` run on the **serial** lane, because resolving a track name (or
crawling playlists) needs the real Spotify client and the service lane builds its
`Network` with `None` for it. Any background DJ result re-checks
`app.dj.generation` before writing, so a batch the user has abandoned is dropped.

### The avoid-library filter

Two gates, and both are needed. `resolve_suggestions` rejects on the *name the
model gave* before paying for a search; `reject_owned_tracks` rejects on the
*resolved track ID* afterwards, which is the only way to catch a track the model
named differently enough to normalise apart (and the only gate that sees `uri`
entries at all). Rejections go in `ResolveReport::in_library`, never `duplicates`
(wrong words for the user) and never `unresolved` (tells the model a real track
does not exist).

Filtering is **on by default in-TUI only**. Over MCP the agent was told to queue
specific tracks, so both gates are off unless it passes
`queue_tracks(exclude_owned: true)`; `search_tracks` instead *marks* each result
`owned`, which informs the choice without dropping anything behind the agent's
back. `play_now` is never filtered.

The index cost drives where the crawl runs. `search_tracks` must never crawl
inline — it dispatches `IoEvent::DjIndexLibrary` and marks that one page from
Liked Songs alone, saying so in the result, because seconds of pagination inside a
tool call head-of-line-blocks the whole serial lane (the bug `ai_dj::open` exists
to avoid). `queue_tracks(exclude_owned)` is the one caller that *does* crawl
inline, via `dj_library_index`, and refuses the call outright if the crawl fails:
it was asked for a guarantee, so queueing unfiltered would be worse than failing.

### Native streaming playback

- For native streaming (`spotatui` as the active playback device), URI-list playback without a Spotify context should stay on the direct native `player.load(...)` path in `src/infra/network/playback.rs`.
- Do not reroute liked songs / saved-track playback on the active native device through Spotify Web API `start_uris_playback(..., device_id=native_device)` as a recovery strategy. That path regressed first-track startup in manual testing even when the UI showed playback starting.
- If native playback behavior is being changed, verify it with the full `cargo run` build, not only the slim telemetry build.
