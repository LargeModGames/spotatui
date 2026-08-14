# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

This file is maintained as three near-identical copies: `CLAUDE.md`, `AGENTS.md`, and
`.github/copilot-instructions.md` (only the opening lines differ). Edit all three together.

## Build & Run

```bash
# Full build (native streaming + audio viz + Lua scripting + OS integrations)
cargo run

# Slim build - no librespot/audio/scripting; fastest iteration, one of CI's six legs
cargo run --no-default-features --features telemetry,tui

# With the free alternative sources (Local/Subsonic/Radio/YouTube). These are NOT
# in `default`, so a plain `cargo run` is Spotify-only; use the `all-sources` alias
# (or list them individually) to exercise the first-run source picker and playback.
cargo run --features all-sources
```

## CI Checks (run before opening a PR)

```bash
cargo fmt --all
cargo clippy --no-default-features --features telemetry,tui -- -D warnings
cargo test --no-default-features --features telemetry,tui
```

These slim commands are the *fast local gate*, not the full picture. GitHub Actions
(`.github/workflows/ci.yml`, on `ubuntu-latest`) runs `check`, `test`, and `clippy`
across a **six-leg** feature matrix:

| Leg | Features |
|-----|----------|
| `default` | a plain `cargo test`: streaming + audio-viz-cpal + scripting + self-update + OS integrations |
| `all-sources` | the **Linux** release feature set from `cd.yml` (adds cover-art, mcp-server, ai-dj, audio-viz, all four sources) |
| `mcp-only` | `telemetry,tui,mcp-server` |
| `ai-dj-only` | `telemetry,tui,ai-dj` |
| `slim` | `telemetry,tui` |
| `headless` | `telemetry` - the only leg without `tui`. `mod tui` is feature-gated, so this leg turns any `crate::tui` import from core/infra/cli into a compile error - what keeps a second frontend from silently re-coupling to the terminal one |

- `mcp-only` and `ai-dj-only` matter more than their size suggests: both enable
  `dj-core` **without** `streaming` (a combination nothing else covers), and each
  front door has to build without the other.
- Reproducing legs locally: `cargo test` reproduces `default`. For `all-sources`,
  copy the exact `--no-default-features --features …` string out of `ci.yml` -
  `cargo test --features all-sources` is **not** it (the alias only adds the four
  sources on top of default), and the leg includes `audio-viz` (PipeWire), so it
  only compiles on Linux.
- Every CI leg passes `--locked`; the local commands do not. Regenerate
  `Cargo.lock` after any `Cargo.toml` edit or all legs fail at once.
- CI runs `clippy` on the **bin target only** (no `--all-targets`), so lints in
  `#[cfg(test)]` code are not gated. Run `cargo test` to compile test code. A
  test-only helper whose production caller is compiled out on some leg needs
  `#[allow(dead_code)]`.
- The `all-sources` leg must stay in sync with `cd.yml`'s Linux release row
  (macOS releases ship a smaller set - no decoded sources).
- A pull_request-only `Gates ratchet` job diffs `tools/gates.count` against the
  merge-base (`tools/check_gates_ratchet.sh`): coupling counters may only fall
  and `test_attribute_total` may only rise. Lower a baseline in the same PR
  that improves it; never raise one.

## Run a Single Test

```bash
cargo test --no-default-features --features telemetry,tui <test_name>
# Example:
cargo test --no-default-features --features telemetry,tui global_shift_w_adds_current_track_from_anywhere
```

A filter that matches nothing still exits 0 - when running feature-gated tests in
the slim build, check the `N filtered out` count actually says your test ran.

## Architecture

One cargo package: a `[lib]` (`src/lib.rs`, private modules, public API =
`run_cli`) plus two `[[bin]]` shims in `src/bin/` - `spotatui` (console) and
`spotatui-gui`, a placeholder behind the off-by-default `gui` feature that
exists to own the crate-root `windows_subsystem` attribute. Five top-level
units under `src/`:

| Unit | Role |
|------|------|
| `core/` | Centralized state (`App`), config/state persistence, and the rspotify-free domain types (`plugin_api`, `pagination`, `source`) |
| `infra/` | Spotify Web API (`network/`), native librespot streaming (`player/`), alternative sources (`local/`, `subsonic/`, `radio/`, `youtube/`, `queue/`), audio viz (`audio/`), Lua scripting (`scripting/`), AI DJ + MCP (`dj/`, `mcp/`), OS integrations (Discord RPC, MPRIS, macOS/Windows media) |
| `tui/` | Terminal UI: the event/render loop (`runner.rs`), key plumbing (`event/`), per-block input handlers (`handlers/`), immutable draw fns (`ui/`) |
| `cli/` | clap subcommands: playback control, listening history, self-update, MCP relay, plugin management |
| `runtime/` | `mod.rs::run_cli` (entry point + CLI dispatch), `bootstrap.rs::boot` (frontend-neutral config/auth/`App` construction, `run_cli` its sole caller), `cli.rs` (clap assembly + self-update), `pump.rs::start_tokio` (the IoEvent pump), `startup.rs` (the UI-launch half, gated on `tui`) |

### Data flow

```text
crossterm thread (tui/event/) → runner.rs loop (draws the frame, then reads one event)
  → runner::dispatch_key   - exit prompt, ActiveBlock::Input, the configurable back key
  → handlers::handle_app   - plugin popup modal → help filter → global keybindings
  → handle_block_events    - dispatches to the per-screen handler
  → app.dispatch(IoEvent)  - hands async work to the pump in runtime/pump.rs
       → source routers by URI scheme, then infra/network/ (Spotify)
       → mutates App state; tui/ui/ re-renders from App on the next frame
```

`tui/event/` is only the crossterm→`Key` plumbing; the event loop itself is
`tui/runner.rs::start_ui`, which also owns Discord/MPRIS presence sync, the window
title, and cover-art request scheduling. Mouse input enters via `handlers::mouse_handler`.

### The IoEvent pump: source routing, two lanes, an auth gate

`runtime/pump.rs::start_tokio` drains IoEvents serially. Three structural gates, all
worth knowing before adding an event:

- **Source routing**: non-Spotify playback is routed by URI scheme *before* the
  Spotify handler, in this order: `route_queue_event` → `route_local_event`
  (`file:`) → `route_subsonic_event` (`subsonic:`) → `route_radio_event`
  (`radio:`) → `route_youtube_event` (`youtube:`) → `Network::handle_network_event`.
  This is what keeps `infra/network/` Spotify-only.
- **Service lane**: `Network::runs_on_service_lane` lists events that run on a
  detached task so slow, source-agnostic work cannot head-of-line-block the serial
  pump. The service lane's `Network` is built with **no Spotify client** - adding a
  `self.spotify()` call to a service-lane handler panics.
- **Auth gate**: `Network::event_bypasses_spotify_auth` lists events whose handlers
  never need a Spotify session. A new IoEvent must be classified against both lists.

### Navigation / routing

`App` holds a private navigation stack of `Route` values (`id: RouteId`,
`active_block: ActiveBlock`, `hovered_block: ActiveBlock` - there is no
`HoveredBlock` *type*). Invariants an agent cannot guess:

- `push_navigation_stack(RouteId::X, ActiveBlock::X)` is a **no-op when the top
  route already has that `RouteId`** - follow it with `set_current_route_state`
  when focus must move anyway.
- `pop_navigation_stack()` refuses to empty the stack.
- The stack is private: go through `get_current_route` / `set_current_route_state`.

### The `core/app/` module folder

`App` was one 10,920-line file; it is now 33 files. The struct stays **flat** - all
~237 fields declared once in `src/core/app/mod.rs` (26 of them feature-gated) -
and its ~210 methods are split across 32 sibling modules by concern, each with its
own `impl App` block. The boundaries are organizational, not architectural.

Rules when working in here:

- **Import from `crate::core::app`, never the submodule path.** `mod.rs` re-exports
  every module that declares public items, so `use crate::core::app::{App, RouteId,
  ActiveBlock}` works no matter which file an item lives in.
- **Child modules open with `use super::*;`** and declare no other top-level
  imports; all external imports live in `mod.rs`. When a type is only importable
  under a feature that does not gate the function, use a **function-local** `use`
  instead of adding a top-level import.
- **A private helper called from a sibling module needs `pub(super)`**; one reached
  from `tui/` or `infra/` needs `pub(crate)`. Private *fields* need nothing - they
  are declared in `mod.rs` and visible to all descendants.
- **Feature-gate the method body, not the call site**, when a predicate must exist
  in every build: `#[cfg(any(...))] { … } #[cfg(not(any(...)))] { false }` inside
  one ungated `fn` (see `queue_owns_playback`), so callers never need their own `#[cfg]`.
- New `impl App` methods go to the concern module that owns that state, not `mod.rs`.
- `dispatch` pins the global loading spinner; long work with its own progress
  surface uses `dispatch_without_spinner`.

Tests are colocated (`#[cfg(test)] mod tests`) in most - not all - modules. Shared
fixtures are `pub(super) fn`s in `test_support.rs`, imported as
`use crate::core::app::test_support::*;`.

### Playback ownership

Multiple players share one UI, and the predicate order is the #1 source of
regressions. Check in this order: `queue_owns_playback()` /
`queue_now_is_spotify()`, then `active_decoded_source()`, then
`is_native_streaming_active_for_playback()`.

- Starting a decoded source (Local/Subsonic/Radio/YouTube) only **pauses**
  librespot - the native flag stays true, so driving librespot directly resumes
  the wrong player.
- While the native queue slot owns the sink, `current_playback_context` names the
  *suspended* context's track; resolve track-level actions through
  `queue_now_spotify_track_uri()` / `queue_now_track()`.
- Radio is in `active_decoded_source` but deliberately out of
  `active_queueable_decoded_source` (repeat/shuffle) and
  `active_source_position_ms` (seek).

### Native streaming (feature `streaming`, in `default`)

`StreamingPlayer` (`src/infra/player/`) embeds librespot as a Spotify Connect
device; a supervisor does bounded in-place reconnects without dropping the audio
pipeline, escalating to full backend replacement with parked-request replay.
Durable intent (recovery snapshot, parked `StartPlayback`, client-side shuffle
session) lives in `src/core/app/native_{backend,recovery,shuffle}.rs`.

- It rides our **maintained librespot fork**, published on crates.io as
  `spotatui-librespot-*` and consumed through `package =` renames, so imports
  stay `librespot_*`. The fork carries upstream backports *and* a fork-only
  `SessionDisconnectReason` API the app depends on - it cannot be swapped for
  upstream 0.8. All seven crates are `=`-pinned in lockstep; bump them together
  when the fork publishes a new version.
- Direct spirc `load` is the **primary** route for all native starts, context ones
  included - a `me/player/play` round trip would head-of-line-block the serial
  pump (#386). The Web API route (`start_native_context_via_api`) is a fallback
  only for context starts the direct load rejected or the watchdog is replaying.
  A native URI-list start must never go through the Web API.
- Anything that replaces or drops a `StreamingPlayer` must call
  `player.shutdown()` first; the Connect device id is persisted in
  `<cache>/device_id`. Both exist to stop ghost Connect devices (#297).
- Session teardowns are classified by librespot's disconnect reason, never
  inferred: external handoff → rebuild idle, unexpected → restore playback,
  local → stop. The handoff veto is sticky (#437).
- Every background native write is generation-guarded
  (`native_playback_generation`, `native_shuffle_generation`) and event handlers
  confirm `Arc::ptr_eq` against the current player before writing - stale writes
  from a replaced backend are the recurring bug class.
- librespot reports full `spotify:track:<id>` URIs while app state uses bare
  base62 ids: normalize with `base62_id_of` at the event boundary
  (`spotify:local:` URIs stay whole).
- Verify native playback changes with the full `cargo run` build, not only the
  slim telemetry build.

### Listening Party / sync

`src/infra/network/sync.rs` is pure WebSocket transport (`SyncMessage`,
`PartyConnection`); lifecycle logic lives in `src/infra/network/mod.rs`. Handlers
dispatch `StartParty` / `JoinParty` / `SetPartyControlMode` / `LeaveParty`;
`SyncPlayback` is fired by `App::on_tick` every 2s while hosting, not by a handler.

## Key Conventions

### Adding a new screen

See `.claude/skills/add-tui-screen/SKILL.md` for the six-place checklist.

### Dispatching network calls

Call `app.dispatch(IoEvent::…)` from a handler - never call async Spotify code
directly from handlers or UI code. Draw functions take `&App` and must not mutate.

Inside the network layer, never call rspotify client methods for Spotify data:
use `self.spotify_api_request_json(...)` / `self.spotify_get_typed::<T>(...)`
from `src/infra/network/requests.rs` (pacing, 401 cooldown, payload
normalization). rspotify supplies OAuth/PKCE, id types, and models only. Spotify
401s are deliberately tolerated (consecutive-failure threshold, forced-refresh
cooldown) - don't "fix" that by escalating on first failure.

### Paginated results

Page caches are `ScrollableResultPages<Paged<T>>` (domain types from
`core/pagination.rs` - no rspotify `Page<T>` in `App` state; conversion lives in
`infra/network/mapping.rs`):

- Insert with `upsert_page_by_offset` - never `add_pages`, which repoints the
  visible index to the tail. Key and dedupe by `page.offset`.
- Caches may be sparse: next/previous targets adjacent *offsets*, not index ±1.
- Background prefetch carries a generation snapshot, re-checks it (plus table
  identity) after every await, and writes via `set_*_to_table_continuous()` -
  never by appending into `track_table.tracks`.
- When a page is already cached, render it synchronously from app state instead
  of routing through another async event.
- For playlist track tables, `playlist_track_table_id` is the table identity;
  `active_playlist_index` is sidebar selection state only.

This generation-guard pattern is repo-wide: every background write into `App`
re-checks its generation/epoch (`dj.generation`, `native_shuffle_generation`,
`liked_state_epoch`, …) so stale tasks cannot write into a reloaded view.

### Status messages

In sync handlers/UI use `app.set_status_message(msg, ttl_secs)`, or
`app.set_error_status_message(...)` for errors - errors block normal messages
until they expire, so use the right one. TTLs are seconds, scaled by the user's
`status_message_ttl_percent`. In the async network layer use
`self.show_status_message(msg, ttl_secs).await`; be aware it bypasses the
error-priority guard. Never write `app.status_message` directly.

### Dialog state cleanup

Close dialogs with the single call `app.clear_dialog_state()` (clears `dialog`,
`confirm`, `pending_keybinding_persist`, and playlist-picker state). Do not
hand-clear the fields. Popping the nav stack is a separate step.

### User-configurable keybindings

Check `app.user_config.keys.<action>` instead of hard-coding key literals for
global actions (`handle_app` in `src/tui/handlers/mod.rs`);
`common_key_events::{up,down,left,right}_event` extend this to per-screen
navigation. Adding a binding means fields on both `KeyBindings` and
`KeyBindingsString` in `src/core/user_config.rs` plus a `get_help_docs` row.

### Config & on-disk files

Four files, four owners - a value that changes as the app runs goes in state,
never config:

| File | Owner | Contents |
|------|-------|----------|
| `config.yml` (config dir) | `core/user_config.rs` | hand-editable settings, keys, theme |
| `client.yml` (config dir) | `core/config.rs` | Spotify app credentials |
| `state.yml` (state dir) | `core/state.rs` | machine-written runtime values |
| `last_session.yml` (state dir) | `core/persisted_playback.rs` | non-Spotify playback + native queue |

- All paths resolve through `core/paths.rs`, never `dirs::` directly.
- `state.yml` saves are read-modify-write **sparse patches** so a second running
  instance is never clobbered; never write a whole snapshot.
- Sensitive writes go through `core::auth::write_private_file_atomic`;
  `config.yml` can carry plaintext secrets, so never log the serialized config.
- `UserConfig::save_config()` regenerates only `behavior`/`theme`/`keybindings`;
  `plugin_commands`, `format`, and `tables` are passed through from disk.
- Adding a `behavior.*` setting = edits in `core/user_config.rs`
  (`BehaviorConfigString`, `BehaviorConfig`, `UserConfig::new`,
  `load_behaviorconfig`, `save_config`) **plus** matching arms in
  `core/app/settings_schema.rs` and `settings_apply.rs`, coupled only by the raw
  `"behavior.<name>"` string id - a typo silently drops the write.

### Feature flags

- `default` = `telemetry, tui, streaming, audio-viz-cpal, macos-media,
  windows-media, mpris, discord-rpc, self-update, scripting`. Notably **not**
  in default: `cover-art` (so a plain `cargo run` has no album art, though
  every shipped binary enables it), the four sources, and the DJ features.
- `tui` gates `mod tui` and owns the terminal-only crates (ratatui, crossterm,
  tui-bar-graph, colorgrad - the last also pulled by `art-decode` for the
  adaptive-theme HSV math). `gui` is a reserved placeholder that only gates the
  `spotatui-gui` bin shim.
- Cover art is two features: `art-decode` is the frontend-agnostic decode half
  (`dep:image`, fills `core::art::CoverArtStore`, feeds the adaptive theme;
  never enabled by hand); `cover-art` layers the ratatui-image terminal
  rendering on top and keeps its pre-split meaning for users and CI.
- `audio-viz` (PipeWire, Linux-only) and `audio-viz-cpal` are **visualizer
  capture** backends, not playback. The librespot *playback* backend is chosen by
  `[target.'cfg(...)']` dependency blocks in Cargo.toml, not by the `*-backend`
  features: linux-gnu→alsa, linux-musl→rodio, Windows→rodio, macOS→portaudio -
  the non-default picks avoid librespot's `pipe` sink writing raw audio to stdout
  and destroying the TUI.
- `audio-decode` (rodio) is the shared engine pulled in by the sources;
  `all-sources` = `local-files, subsonic, internet-radio, youtube`.
- `dj-core` is a shared implementation feature pulled in by `mcp-server` and
  `ai-dj`; none of the three are in `default`, and neither front door may assume
  the other (or `streaming`) is present.
- `scripting` (mlua Lua plugins) is default-on and gates `src/infra/scripting/` +
  `src/cli/plugin.rs`; the slim gate never compiles it, so verify scripting
  changes with a default `cargo test`.

### Adding a feature-gated sidebar row

See `.claude/skills/add-tui-screen/SKILL.md`.

### Domain-specific conventions

`src/infra/dj/CLAUDE.md` (DJ lanes/guards/avoid-library filter),
`src/infra/mcp/CLAUDE.md` (MCP server), `src/infra/scripting/CLAUDE.md` (Lua
plugins), and `src/cli/CLAUDE.md` (CLI subcommands) load automatically when
working in those directories.

### Alternative sources (Local / Subsonic / Radio / YouTube)

- All four decode through **one** shared rodio sink, `LocalPlayer`
  (`src/infra/audio/player.rs`) - the only file where rodio types appear.
  Subsonic and YouTube download each track to a `NamedTempFile` first (YouTube by
  shelling out to `yt-dlp`); Radio streams through a non-seekable ring buffer.
- macOS cannot play any decoded source: `LocalPlayer::new()` bails there because
  rodio SIGSEGVs on CoreAudio/Bluetooth (#9/#20) - which is why macOS releases
  exclude the source features.
- Repeat/shuffle for decoded sources live in the pure module
  `src/infra/queue/mod.rs` (`advance_decision`, `resume_index_after_queue`, …);
  state is player-global on `App` (`decoded_repeat`, `decoded_shuffle`).
  Repeat-one affects auto-advance only, never a manual skip. Note
  `resume_index_after_queue` returning `None` means "context exhausted, tear
  down" - the *opposite* of `advance_index`'s `None` ("clamp, no-op").
- Set the `advancing` flag synchronously before dispatching a track change: the
  sink is empty for the whole decode/download, and the tick would otherwise
  re-fire auto-advance and skip several tracks.
- Per-source playback state is one `Option` field on `App`
  (`local_playback`, …), published only on success; position/pause are read live
  from the player at render time.
- `App.active_source` (`core/source.rs`) is **browse scope only** - it never
  changes playback routing, so switching sources must not interrupt playback.
- OS integrations (MPRIS, SMTC, macOS Now Playing, Discord RPC, window title) all
  read one `PlaybackSnapshot` from `infra/media_metadata.rs`, which checks
  decoded sources first so the paused Spotify track is not published.
- YouTube is unofficial-fragile by design: when it breaks, the fix is a newer
  `yt-dlp`, not a spotatui release.

### Testing conventions

- Tests are colocated (`#[cfg(test)] mod tests`) - there is no `tests/` dir. The
  only dev-dependency is `tempfile`: HTTP tests bind a real `127.0.0.1:0`
  listener into an injected base-URL field; UI tests use ratatui's `TestBackend`.
- `App::new` is `#[cfg(test)]`-only (production uses `App::new_with_state`).
  `App::default()` has no IoEvent channel and `dispatch` silently drops events -
  tests asserting on IoEvents use the house pattern
  `fn app_with_x() -> (App, Receiver<IoEvent>)` and keep the receiver alive.
- `IoEvent` derives nothing (not even `Debug`): assert with
  `assert!(matches!(rx.try_recv(), Ok(IoEvent::X(..))))`.
- Never `std::env::set_var` in a test (one process, shared threads); split the
  env read into a `*_with(value, …)` function and test that.
- Prefer extracting the decision into a pure function or plain-data enum taking
  scalars instead of `&App`/`Network` - the house style for testing without a
  Spotify client, audio device, network, or real clock.
- Gate whole test modules with `#[cfg(all(test, feature = "…"))]` where needed.
- Test names are behavior sentences without a `test_` prefix
  (`enter_on_stats_entry_opens_stats_screen`).
- Shared fixtures: `crate::core::test_helpers` crate-wide;
  `crate::core::app::test_support` inside `core/app/`.
