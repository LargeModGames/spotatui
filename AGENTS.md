# AGENTS.md

This file provides guidance to coding agents (Claude Code, Codex, Copilot and others)
when working with code in this repository. `CLAUDE.md` imports it: edit this file only.

## Build & Run

```bash
# Full build (native streaming + audio viz + Lua scripting + OS integrations)
cargo run

# Slim build - no librespot/audio/scripting; fastest iteration, one of CI's eight legs
cargo run --no-default-features --features telemetry,tui

# With the alternative sources (Local/Subsonic/Radio/YouTube/Qobuz). These are NOT
# in `default`, so a plain `cargo run` is Spotify-only; use the `all-sources` alias
# (or list them individually) to exercise the first-run source picker and playback.
cargo run --features all-sources
```

## Opening a PR

Before you open a PR, read `CONTRIBUTING.md` and follow its "Using AI Tools" section.
Show your user the full diff and the PR text first, and never open a PR on your own.

## CI Checks (run before opening a PR)

```bash
cargo fmt --all
cargo clippy --no-default-features --features telemetry,tui -- -D warnings
cargo test --no-default-features --features telemetry,tui
```

These slim commands are the *fast local gate*, not the full picture. GitHub Actions
(`.github/workflows/ci.yml`) runs `check`, `test`, and `clippy` on `ubuntu-latest`
across an **eight-leg** feature matrix, plus one `macos-latest` job (below):

| Leg | Features |
|-----|----------|
| `default` | a plain `cargo test`: streaming + audio-viz-cpal + scripting + self-update + OS integrations |
| `all-sources` | the **Linux** release feature set from `cd.yml` (adds cover-art, mcp-server, ai-dj, audio-viz, all five sources) |
| `mcp-only` | `telemetry,tui,mcp-server` |
| `ai-dj-only` | `telemetry,tui,ai-dj` |
| `slim` | `telemetry,tui` |
| `headless` | `telemetry` - one of three legs without `tui`. `mod tui` is feature-gated, so this leg turns any `crate::tui` import from core/infra/cli into a compile error - what keeps a second frontend from silently re-coupling to the terminal one |
| `headless-streaming` | `telemetry,streaming` - `check` + `clippy` only, no `test` job. Proves native-streaming startup (`runtime/streaming/`) and the player-event wiring type-check and pass clippy with no terminal frontend in scope. Its two entry points carry `allow(dead_code)` there, so the leg does not prove they are live |
| `gui` | `telemetry,gui,streaming,discord-rpc,self-update,scripting,mcp-server,mpris` - the third leg without `tui`: the Linux feature set a `spotatui-gui` build carries. `check`, `clippy` and `test` |

- Every CI leg passes `--locked`; the local commands do not. Regenerate
  `Cargo.lock` after any `Cargo.toml` edit or all legs fail at once.
- CI runs `clippy` on the **bin target only** (no `--all-targets`), so lints in
  `#[cfg(test)]` code are not gated. Run `cargo test` to compile test code. A
  test-only helper whose production caller is compiled out on some leg needs
  `#[allow(dead_code)]`.
- One further job, `macos`, runs `check` + `clippy` (no `test`) on
  `macos-latest` with cd.yml's macOS release feature set. It is the only leg
  that compiles the `#[cfg(target_os = "macos")]` arms, the portaudio playback
  backend, `macos-media`, and `audio-viz-cpal` - none of which the Linux legs
  can speak for. No `test`: the suite is platform-independent logic the Linux
  legs already run, and the device tests are `#[ignore]`d (CI runners have no
  audio output). Keep its feature list in sync with cd.yml's macOS rows.
- A pull_request-only `Gates ratchet` job diffs `tools/gates.count` against the
  merge-base (`tools/check_gates_ratchet.sh`): coupling counters may only fall;
  the two adoption counters (`test_attribute_total`,
  `action_refs_in_tui_handlers`) may only rise. `src/gates.rs` pins each
  coupling counter exactly, so move its baseline in the same PR that moves the
  number. The adoption counters are floors: do not edit them in a PR, because
  `.github/workflows/gates-floor.yml` raises them on `main` after the merge.
- `.github/workflows/gui.yml` gates the `gui/` frontend on every PR: run
  `npm ci`, `npm run lint`, `npm run format:check`, `npm run typecheck`,
  `npm test` and `npm run build` in `gui/`; the cargo gate above does not cover it.
  `npm run shots` (Playwright, after a one-time `npx playwright install chromium firefox webkit`)
  renders the page against the fake bridge in `gui/e2e/` and writes one PNG per
  scene and browser to `gui/shots/`; CI uploads them as the `gui-shots` artifact.

Details: `.github/workflows/AGENTS.md`.

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
`run_cli`, plus `run_gui` under `gui`) plus two `[[bin]]` shims in `src/bin/` -
`spotatui` (console) and `spotatui-gui`, the browser frontend behind the
off-by-default `gui` feature, which also owns the crate-root
`windows_subsystem` attribute. Six top-level units under `src/`:

| Unit | Role |
|------|------|
| `core/` | Centralized state (`App`), the frontend-neutral tick scheduler (`driver/`), the shared action vocabulary (`action/`), config/state persistence, and the rspotify-free domain types (`plugin_api`, `pagination`, `source`) |
| `infra/` | Spotify Web API (`network/`), native librespot streaming (`player/`), alternative sources (`local/`, `subsonic/`, `qobuz/`, `radio/`, `youtube/`, `queue/`), audio viz (`audio/`), Lua scripting (`scripting/`), AI DJ + MCP (`dj/`, `mcp/`), OS integrations (Discord RPC, MPRIS, macOS/Windows media) |
| `tui/` | Terminal UI: the event/render loop (`runner.rs`), key plumbing (`event/`), per-block input handlers (`handlers/`), immutable draw fns (`ui/`) |
| `gui/` | Browser frontend (feature `gui`): the loopback page server with its Host/Origin/launch-code checks (`server.rs`), the JSON push protocol over the display revisions (`protocol.rs`), the socket bridge to the tick loop (`bridge.rs`), and the first-launch questions over the socket (`onboarding.rs`, a blocking `Onboarding` that waits for the page); `build.rs` embeds `gui/dist` |
| `cli/` | clap subcommands: playback control, listening history, self-update, MCP relay, plugin management |
| `runtime/` | `mod.rs::run_cli` (entry point + CLI dispatch), `bootstrap.rs::boot` (frontend-neutral config/auth/`App` construction, `run_cli` and `run_gui` its callers, plus the boot auth rule `spotify_auth_mode`: interactive only right after the client wizard or `--reconfigure-auth`, a subcommand needs a cached token, a UI launch tolerates no session), `cli.rs` (clap assembly + self-update), `pump.rs::start_tokio` (the IoEvent pump), `streaming/` (native-streaming startup every frontend shares: the pure saved-device decision in `mod.rs`, the librespot bring-up in `launch.rs`, gated on `streaming`), `startup.rs` (the UI-launch half, gated on `tui` or `gui`), `gui.rs::run_gui` (the browser frontend: the loopback server first, boot with the first-launch questions answered in the page, then a tick loop with no terminal), `instance.rs` (the single-instance lock a UI launch takes before boot; `restart_after_update` releases it before the re-exec) |

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
`tui/runner.rs::start_ui`. Everything on a timer lives in `core/driver/`
(`Driver::tick`): the playback tick + debounced flushes, OAuth refresh,
Discord/MPRIS presence sync, the window title, lyrics + cover-art scheduling,
native-queue and decoded-source auto-advance, and `last_session.yml`
persistence. The runner draws frames, reads events, and calls `driver.tick`
with a `TickEnv` carrying the frontend-geometry inputs (visualizer bar count,
cover-art pixel support). A frontend that stops ticking is loud, not silent:
`App::playback_position_ms()` reads stale after 2s and the playbar says so.
Mouse input enters via `handlers::mouse_handler`.

### The IoEvent pump: source routing, two lanes, an auth gate

`runtime/pump.rs::start_tokio` drains IoEvents serially. Three structural gates, all
worth knowing before adding an event:

The gates and the rule for a new IoEvent: `src/infra/network/AGENTS.md`.

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

- **Import from `crate::core::app`, never the submodule path.** `mod.rs` re-exports
  every module that declares public items, so `use crate::core::app::{App, RouteId,
  ActiveBlock}` works no matter which file an item lives in.
- **A field with no writer outside `core/app/` is private.** `pub_fields_on_app`
  counts the `pub` ones and may only fall. An outside reader gets an intent-named
  getter (`status_message()`, `api_error()`); a handler mutates shared state only
  through an `App` method or `app.apply(Action::…)`, never through a field write.
- **Feature-gate the method body, not the call site**, when a predicate must exist
  in every build: `#[cfg(any(...))] { … } #[cfg(not(any(...)))] { false }` inside
  one ungated `fn` (see `queue_owns_playback`), so callers never need their own `#[cfg]`.
- **Presentation state lives in `App.view: ViewState`** (`core/app/view.rs`): cursor
  and selection indices (the track table, search, Recently Played and sidebar
  cursors included), scroll offsets, edit buffers, focus, popup flags, the help
  pager, the terminal viewport. Handlers and draw functions write `app.view.<field>`
  freely and the handler-write ratchet skips those chains. A producer outside `tui/`
  and `core/app/` (a network or source handler, a script effect, the CLI) that
  resets or clamps a cursor is counted by `view_writes_outside_tui`, which may only
  fall: a producer that replaces a list resets its cursor through an `App` method
  (`set_track_table`, `set_search_results`), never by writing `view` itself. A
  new field goes in `view` only if it is presentation state; a pending operation,
  or anything a second frontend would also need, stays on `App`.
- `dispatch` pins the global loading spinner; long work with its own progress
  surface uses `dispatch_without_spinner`.

More rules for this folder: `src/core/app/AGENTS.md`.

### Playback ownership

Multiple players share one UI, and the predicate order is the #1 source of
regressions. Check in this order: `apple_music_owns_playback()`, then
`queue_owns_playback()` / `queue_now_is_spotify()`, then `active_decoded_source()`, then
`is_native_streaming_active_for_playback()`. `App::playback_owner()` folds them
into one `PlaybackOwner`, and the transport chains (play/pause,
next, previous, shuffle, repeat, volume) end on `dispatch_spotify_fallback`,
which answers "Nothing is playing" instead of a Spotify dispatch when no
session exists.

Details: `src/core/app/AGENTS.md`.

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
- Verify native playback changes with the full `cargo run` build, not only the
  slim telemetry build.

Details: `src/infra/player/AGENTS.md`.

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

### The shared Action vocabulary

`src/core/action/` holds `Action` (one enum of frontend-neutral state
changes), `App::apply` (the single write path, in `core/action/apply.rs`),
and `ActionOutcome`. Producers that are not the network layer's own result
handling mutate `App` via `app.apply(Action::…)`: the Lua scripting engine
drains its queued actions into it, the mutating DJ/MCP tools call it
directly, and TUI handlers adopt it as the conversion sub-PRs land
(`action_refs_in_tui_handlers` may only rise). Rules:

- No rspotify type and no raw `IoEvent` payload in `Action` - payloads are
  strings, scalars, and `core::plugin_api` snapshot types. Address by
  identity (URIs, ids, names), never by list ordinal.

The other rules: `src/core/action/AGENTS.md`.

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
`self.show_status_message(msg, ttl_secs).await`, which takes the `App` lock and
calls `set_status_message`. The message fields are private: `status_message()`
and `status_message_is_error()` read them.


### Errors

`App::handle_error(e)` records the message in `api_error`, stamps a 60s
lifetime on it, and pushes `RouteId::Error`. That route is a *presentation
hint*: the terminal frontend draws it full-screen, another frontend may render
`api_error` as a toast and ignore the frame entirely. Four rules:

- Dismiss with `app.clear_api_error()`, never by clearing the string or popping
  the route by hand. It drops the message, its lifetime, and every frame still
  showing it together - a cleared string under a surviving frame renders as an
  error page with nothing on it. `pop_navigation_stack` already calls it when
  the frame it pops is `Error`.
- A non-empty `api_error` is the CLI's only failure signal for every subcommand
  that reaches the bottom of `handle_matches` (`src/cli/handle.rs`; the two
  `share-*` flags return early and bypass it). Moving a call site off
  `handle_error` turns a failing CLI command into exit 0 unless that site is
  provably unreachable from the CLI.

The other two rules: `src/core/app/AGENTS.md`.

### Dialog state cleanup

Close dialogs with the single call `app.clear_dialog_state()` (clears `dialog`,
`confirm`, `pending_keybinding_persist`, and playlist-picker state). Do not
hand-clear the fields. Popping the nav stack is a separate step.

### User-configurable keybindings

Check `app.user_config.keys.<action>` instead of hard-coding key literals for
global actions (`handle_app` in `src/tui/handlers/mod.rs`);
`common_key_events::{up,down,left,right}_event` extend this to per-screen
navigation. Adding a binding means fields on both `KeyBindings` and
`KeyBindingsString` in `src/core/user_config.rs`, a `help_entries` row in
`src/tui/keymap.rs` with its `Requirement`, and, when the key produces an
`Action`, that variant's `default_binding` arm pointing at the new field.

### Requirements

`core/requirement.rs` says what a row or key needs (`Requirement::{None,
SpotifySession, Capability(_), Source(_), AnyOf(_)}`) and `App::availability(req)`
answers for the active source and the session. Three tables carry it: the
sidebar (`library_row_requirements()` in `core/app/library.rs`), the help menu
(`help_entries()` in `tui/keymap.rs`) and the settings rows
(`setting_requirement(id)` in `core/app/settings_schema.rs`). The rule on every
surface: a row with a rebindable key stays visible with the `hint()` as a
suffix, a row with a fixed key or a sidebar row is hidden. Gate a key on the
same predicate the row uses, so the two cannot disagree.

### Config & on-disk files

Six files, six owners - a value that changes as the app runs goes in state,
never config:

| File | Owner | Contents |
|------|-------|----------|
| `config.yml` (config dir) | `core/user_config.rs` | hand-editable settings, keys, theme |
| `client.yml` (config dir) | `core/config.rs` | Spotify app credentials |
| `state.yml` (state dir) | `core/state.rs` | machine-written runtime values |
| `last_session.yml` (state dir) | `core/persisted_playback.rs` | non-Spotify playback + native queue |
| `playlist_sync.yml` (state dir) | `core/playlist_sync/store.rs` | playlist links + match cache |
| `qobuz_credentials.yml` (config dir) | `infra/qobuz/auth.rs` | the Qobuz login token (feature `qobuz`) |

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
  `"behavior.<name>"` string id - a typo silently drops the write. A row that
  only works with Spotify also needs a `setting_requirement` arm.

### Feature flags

- `default` = `telemetry, tui, streaming, audio-viz-cpal, macos-media,
  windows-media, mpris, discord-rpc, self-update, scripting`. Notably **not**
  in default: `cover-art` (so a plain `cargo run` has no album art, though
  every shipped binary enables it), the five sources, and the DJ features.
- `tui` gates `mod tui` and owns the terminal-only crates (ratatui, crossterm,
  tui-bar-graph, colorgrad - the last also pulled by `art-decode` for the
  adaptive-theme HSV math). `gui` gates `mod gui` (the browser frontend's
  loopback page server, with `httparse`), `run_gui` and the `spotatui-gui` bin shim.
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
  `all-sources` = `local-files, subsonic, internet-radio, youtube, qobuz`.
- `dj-core` is a shared implementation feature pulled in by `mcp-server` and
  `ai-dj`; none of the three are in `default`, and neither front door may assume
  the other (or `streaming`) is present.
- `apple-music` is macOS-only and off by default (the macOS CI job and cd.yml's
  macOS rows enable it). Its code sits behind
  `all(feature = "apple-music", target_os = "macos")`; the `Source` variant and
  the `App` state compile everywhere so callers need no `#[cfg]`.
- `scripting` (mlua Lua plugins) is default-on and gates `src/infra/scripting/` +
  `src/cli/plugin.rs`; the slim gate never compiles it, so verify scripting
  changes with a default `cargo test`.

### Adding a feature-gated sidebar row

See `.claude/skills/add-tui-screen/SKILL.md`.

### Domain-specific conventions

Before you change one of these areas, read its file. Claude Code loads it when it
reads a file in that directory. Other agents must open it.

| Area | Read first |
|------|------------|
| DJ lanes, guards, avoid-library filter | `src/infra/dj/AGENTS.md` |
| MCP server | `src/infra/mcp/AGENTS.md` |
| Lua plugins | `src/infra/scripting/AGENTS.md` |
| CLI subcommands | `src/cli/AGENTS.md` |
| CI workflows and legs | `.github/workflows/AGENTS.md` |
| `core/app/` layout, playback ownership, error reporting | `src/core/app/AGENTS.md` |
| A new `Action` variant | `src/core/action/AGENTS.md` |
| A new `IoEvent`, the listening party | `src/infra/network/AGENTS.md` |
| Native streaming (librespot) | `src/infra/player/AGENTS.md` |
| Apple Music remote (macOS) | `src/infra/apple_music/AGENTS.md` |
| `LocalPlayer`, macOS playback, output-device loss | `src/infra/audio/AGENTS.md` |
| Decoded repeat/shuffle | `src/infra/queue/AGENTS.md` |
| Radio tune-in | `src/infra/radio/AGENTS.md` |
| Qobuz streaming | `src/infra/qobuz/AGENTS.md` |

### Alternative sources (Local / Subsonic / Radio / YouTube / Qobuz)

- All five decode through **one** shared rodio sink, `LocalPlayer`
  (`src/infra/audio/player.rs`) - the only file where rodio types appear.
  Subsonic and YouTube download each track to a `NamedTempFile` first (YouTube by
  shelling out to `yt-dlp`); Radio streams through a non-seekable ring buffer.
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
  `yt-dlp`, not a spotatui release. One in-repo mitigation: a failed download
  retries once through the embedded player clients (`web_embedded,tv_embedded`),
  which PO-token enforcement leaves tokenless for embeddable videos - most
  label uploads. A non-embeddable gated video still fails.

Details: `src/infra/audio/AGENTS.md` (`LocalPlayer`, output-device loss),
`src/infra/queue/AGENTS.md`, `src/infra/radio/AGENTS.md`, `src/infra/qobuz/AGENTS.md`.

### Apple Music (feature `apple-music`, macOS only)

Not a decoded source: the Music app plays, with the user's own login and
library, and spotatui is a remote for it. Every command is one constant JXA
program (`infra/apple_music/music.js`) run by `osascript` on the Apple Music
router's own serial worker, never on the pump. Details:
`src/infra/apple_music/AGENTS.md`.

### Testing conventions

- Tests are colocated (`#[cfg(test)] mod tests`) - there is no `tests/` dir. The
  dev-dependencies are `tempfile` and `ts-rs`: HTTP tests bind a real `127.0.0.1:0`
  listener into an injected base-URL field; UI tests use ratatui's `TestBackend`.
- `gui/src/bindings/` is generated. `Action`, every type it reaches and the GUI
  protocol types derive `ts_rs::TS` under `cfg_attr(all(test, feature = "gui"), ...)`;
  a `gui` test run rewrites the directory. Commit the result: the `gui` test leg
  fails on any difference. A GUI protocol type never reuses the name of another
  exported type, and a type's doc comment lands in its binding.
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
