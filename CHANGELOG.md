# Changelog

## [v0.40.0] 2026-07-04

### Fixed

- **Native streaming silent playback (HTTP 530)**: Native audio no longer fails silently when Spotify's CDN returns HTTP 530 for the first of the candidate URLs it hands out. spotatui now builds librespot from a maintained fork ([spotatui-librespot](https://github.com/LargeModGames/spotatui-librespot)) that backports upstream [PR #1722](https://github.com/librespot-org/librespot/pull/1722) ([#1725](https://github.com/librespot-org/librespot/issues/1725)), falling back to the next CDN URL on any non-206 response. Repeated unavailable tracks now also halt playback instead of stampeding the queue and risking a rate-limit.
- **Native streaming startup device recovery**: Startup now trusts the local `spotatui` Connect device id, preserves saved external devices, activates the native device when Spotify reports no active playback, and renders an actionable idle playbar so paused native sessions restore without manually opening the device selector ([#301](https://github.com/LargeModGames/spotatui/issues/301)).
- **Windows feature/build compatibility**: The Windows audio feature set and Cargo feature wiring were corrected so native audio and the new scripting support build together cleanly on Windows instead of failing behind mismatched feature gates.
- **Transport controls drive the active source**: Next/Previous, volume, seek, and Play/Pause now act on whichever non-Spotify source (Local Files, Subsonic, Internet Radio, YouTube) currently owns playback instead of the paused Spotify streaming session underneath it. Internet radio can now be paused/resumed, volume and skip keys affect the audible source, and seek moves within the source track (radio, being live, correctly ignores seek). Media keys and the on-screen playbar buttons route the same way.
- **Now Playing follows the active source**: The window title, Discord Rich Presence, and MPRIS / macOS Now Playing widgets now show the currently playing local/Subsonic/radio/YouTube track instead of a stale paused Spotify track.
- **Network stalls no longer freeze the app**: The Spotify, Subsonic, and internet-radio HTTP clients now have connect/read timeouts, and radio directory mirrors are bounded per-request, so a hung server, dead mirror, or captive portal times out instead of wedging every transport control. The internet-radio audio stream is exempted from the request timeout so playback isn't cut off mid-song.
- **No UI freeze on track change** (`cover-art`): Album-art downloads now run off the UI lock with an explicit timeout, so the interface no longer freezes for the CDN round-trip on every track change.
- **Local playback robustness**: Skipping into an undecodable file no longer leaves the previous track playing or desyncs the track index, and file types the decoder can't play (`aiff`, `wv`, `ape`, `opus`, …) are no longer offered by the library browser.
- **Dead radio streams recover**: When a live stream drops or ends, the session now tears down cleanly (with a status message) instead of leaving a frozen "LIVE" playbar and a pinned audio thread.
- **Crash fix**: Pressing `D` (unfollow) on a playlist search result after a newer, shorter search no longer panics; selected indices are clamped when result pages are replaced.
- **Subsonic memory use**: Track downloads now stream to disk chunk-by-chunk instead of buffering the whole file in RAM (large lossless files/audiobooks no longer spike memory per track change).
- **Config file permissions**: The config file (which can hold the Subsonic password and party sync token) is now written with `0600` permissions via an atomic temp-file-and-rename, and the config directory is restricted to `0700`, on Unix.
- **MPRIS volume control**: `playerctl volume` (and other MPRIS clients) can now actually set spotatui's volume — previously only reads worked, since no `connect_set_volume` handler was ever registered. Volume changes made via the native streaming fast path (keyboard, Lua) now also sync back to the MPRIS `Volume` property immediately, so relative adjustments like `playerctl volume 0.05+` no longer compute from a stale cached value ([#327](https://github.com/LargeModGames/spotatui/issues/327)).

### Added

- **Multi-source: Local Files** (opt-in `local-files` feature; included in the Linux/Windows release builds): Browse a configured music directory (`behavior.local_music_path`, defaulting to the OS music folder) and play local audio (FLAC, MP3, OGG, WAV, …) through spotatui's own audio engine — with the visualizer, volume control, folder queues with next/previous/auto-advance, and MPRIS/media-key support. Switch sources with the `d` picker; the active source persists across restarts. Not yet on macOS (audio output path gated off).
- **Multi-source: Subsonic/OpenSubsonic** (opt-in `subsonic` feature; included in the Linux/Windows release builds): Connect to any Subsonic-compatible server (Navidrome, Gonic, Airsonic, Funkwhale, LMS, …) via `behavior.subsonic_url`/`subsonic_username`/`subsonic_password` (or the `SPOTATUI_SUBSONIC_PASSWORD` env var, preferred). Browse server playlists, search the catalog, and stream tracks with queue advance.
- **Multi-source: Internet Radio** (opt-in `internet-radio` feature; included in the Linux/Windows release builds): Play icecast/shoutcast streams from a `behavior.radio_stations` config list or by searching the radio-browser.info community directory (30k+ stations) in-app. The playbar shows a `LIVE` badge with the stream's ICY now-playing title updating live.
- **Radio station favorites management**: Internet radio stations found in the browser can now be saved into `behavior.radio_stations` directly from the UI and removed later, so the in-app browser can also maintain the persistent station list instead of requiring manual config edits.
- **Multi-source: YouTube** (opt-in `youtube` feature; included in the Linux/Windows release builds): Search YouTube and play audio through an external [`yt-dlp`](https://github.com/yt-dlp/yt-dlp) binary (`behavior.ytdlp_path` overrides the `PATH` lookup; ffmpeg recommended) — anonymous, no Google account or API key. When YouTube changes and extraction breaks, updating yt-dlp is the fix; no spotatui release needed.
- **Local YouTube playlists**: Build playlists of YouTube tracks stored in a plain human-editable file (`~/.config/spotatui/youtube_playlists.yml`) — create from the sidebar (`+ New Playlist`), add search results with `w` (the same picker dialog as Spotify playlists), remove tracks with `x`, delete playlists with `D`, and play a playlist as a queue with auto-advance. Together with the other new sources, spotatui is now fully usable without any paid account.
- **Playback session persistence across sources**: `Continue` and startup restore now resume the last non-Spotify playback session too, including Local Files, Subsonic, Internet Radio, and YouTube, instead of only reviving Spotify state.
- **Cover art and lyrics for all playback sources**: The track detail surfaces now request artwork and lyrics for Local Files, Subsonic, Internet Radio, and YouTube where metadata is available, rather than limiting those enrichments to Spotify playback.
- **Setting to disable OS media key controls**: A new config/settings toggle lets users opt out of global media-key capture entirely when they do not want spotatui responding to hardware Play/Pause, Next, and Previous buttons ([#309](https://github.com/LargeModGames/spotatui/issues/309)).
- **Configurable movement keybindings**: `h`/`j`/`k`/`l` keys (traditionally left/down/up/right) are now configurable via `move_left`, `move_down`, `move_up`, and `move_right` under `keybindings` in `config.yml` and via the Settings screen. Defaults match the previous hard-coded bindings, and the arrow key(s) fallbacks still work.

## [v0.39.1] 2026-06-12

### Fixed

- **Self-update silently failing since v0.38.5**: Release-asset downloads during update checksum verification now send a `User-Agent` header (GitHub rejects requests without one with `403 Forbidden`) and `Accept: application/octet-stream` (without it the asset API URL returns JSON metadata instead of the file), so auto-update and `spotatui update --install` work again. Auto-update failures are now logged instead of silently discarded, and `spotatui update` runs on a blocking thread so it can no longer panic the async runtime. Clients on v0.38.5 through v0.39.0 carry the broken verification in their own binaries and need one manual update to reach this release ([#303](https://github.com/LargeModGames/spotatui/pull/303)).

## [v0.39.0] 2026-06-12

### Added

- **Lua plugin scripting**: Added an embedded Lua engine behind the `scripting` feature that loads `~/.config/spotatui/init.lua` and `~/.config/spotatui/plugins/*.lua` at startup. Plugins register callbacks via `spotatui.on(event, fn)` for `start`, `quit`, `track_change`, `playback_state_change`, `seek`, `volume_change`, and `queue_change`, read playback/track/device snapshots, and drive playback through a curated action API (`play`, `pause`, `next`, `previous`, `seek`, `set_volume`, `shuffle`, `search`, `notify`). A broken plugin is disabled with a status message instead of crashing the app. See `docs/scripting.md`.
- **Lua plugin commands and keybindings**: Plugins can now register named commands via `spotatui.register_command(name, fn)` and users can bind those commands to keys in `config.yml` under a new `plugin_commands` map. An erroring command reports a status message but stays bound. See `docs/scripting.md`.
- **Lua plugin UI extension (api_version 2)**: Plugins can now push a persistent segment into the playbar title via `spotatui.set_playbar(text)` (pass `nil` to clear), open a scrollable modal popup via `spotatui.popup(title, lines)` (lines support per-line fg/bold/italic styling; `j`/`k` scroll, `Esc`/`q` close), and apply runtime theme color overrides via `spotatui.set_theme(tbl)` (runtime-only, not persisted to config). See `docs/scripting.md`.
- **Lua plugin HTTP and JSON (api_version 3)**: Plugins can make async HTTP requests with `spotatui.http_get(url, cb)` and `spotatui.http_post(url, body, headers, cb)` (callbacks run on a later UI tick), and convert payloads with `spotatui.json_encode`/`spotatui.json_decode`. See `docs/scripting.md`.
- **Lua plugin installer and ecosystem**: Added a `spotatui plugin` command (`add`/`list`/`remove`/`update`) that installs plugins from git repositories into `~/.config/spotatui/plugins/<name>/` and tracks them in a `plugins.lock` file. The loader now also loads directory plugins (`plugins/<name>/main.lua`, falling back to `init.lua`) with the plugin's own folder on `package.path` so it can `require` sibling modules. Ships runnable example plugins under `examples/plugins/` and a `PLUGINS.md` index. See `docs/scripting.md`.
- **Lua plugin API-version guard and scaffold (api_version 4)**: Plugins can declare their minimum required API version with `spotatui.require_api(n)`; if the build is too old the plugin fails to load with a clear "requires spotatui scripting API vN" message instead of a cryptic nil-call error. Added `spotatui plugin new <name>` to scaffold a working directory plugin (`main.lua` + `README.md`) to start from. See `docs/scripting.md`.
- **SMTC Integration**: System Media Transport Controls is now integrated with the app for Windows users. Users can now control playback state using media keys and check playback state in media flyouts ([#229](https://github.com/LargeModGames/spotatui/issues/229)).
- **Click and drag to seek on the playbar**: The progress bar is now interactive. Click anywhere on the gauge to jump to that position, or click and drag to scrub. Control buttons keep priority, the time label stays non-clickable, and seeks reuse the existing native and throttled-API paths ([#157](https://github.com/LargeModGames/spotatui/issues/157)).

### Fixed

- **Ghost `spotatui` Connect devices**: The native streaming device id is now generated once and persisted in the streaming cache, and the old librespot session is shut down whenever the player is replaced, so app restarts and streaming recoveries no longer leave a trail of dead `spotatui` entries in Spotify's device list ([#297](https://github.com/LargeModGames/spotatui/issues/297)).
- **Native device selection and playback startup**: Made the local `spotatui` Connect device selectable when Spotify's devices API omits it, recovered stale native streaming sessions after long idle, fixed `Esc`/`Enter` handling in the device selector, and avoided `NO_ACTIVE_DEVICE` playback failures when native streaming is connected but no Spotify playback context is active ([#292](https://github.com/LargeModGames/spotatui/issues/292)).
- **Native streaming keeps playback local and surfaces failures**: With native streaming active, playback now stays on the local `spotatui` device instead of silently leaking to the official Spotify client, and streaming errors are reported instead of failing quietly ([#282](https://github.com/LargeModGames/spotatui/issues/282)).
- **Search box no longer traps focus on submit**: Pressing `Enter` to run a search now always moves focus to the results list, including when re-searching while already on the Search screen (previously focus stayed stuck in the input box) ([#191](https://github.com/LargeModGames/spotatui/issues/191)).
- **Cover-art load failures are non-fatal**: A failed album-image fetch is now logged and ignored instead of surfacing a blocking error and aborting the now-playing update, so playback metadata keeps updating when artwork can't be loaded ([#142](https://github.com/LargeModGames/spotatui/issues/142)).
- **Settings list scrolls to follow the selection**: The Settings screen now renders its item list statefully, so the viewport scrolls to keep the highlighted setting in view instead of clipping items below the fold on shorter terminals (previously only fully visible when the terminal was tall enough to show every item). The selected row is now also marked with a `▶` indicator, matching the other list screens.

### Changed

- **Dependency maintenance**: Bumped the grouped Rust minor dependency set (`ratatui`, `chrono`, `log`, `ratatui-image`, `serde_json`), `sha2`, and `tar` ([#283](https://github.com/LargeModGames/spotatui/pull/283), [#284](https://github.com/LargeModGames/spotatui/pull/284), [#286](https://github.com/LargeModGames/spotatui/pull/286), [#289](https://github.com/LargeModGames/spotatui/pull/289)).

## [v0.38.6] 2026-05-28

### Fixed

- **Playback API false errors**: Fixed Spotify playback mutation requests so starting playback no longer surfaces bogus API errors like `411 Length Required` or JSON parse failures when Spotify accepts the action but returns an empty or non-JSON success response.

## [v0.38.5] 2026-05-27

### Added

- **Miniplayer view**: Added a new full-screen miniplayer mode that centers the playbar, accessible via a configurable keybinding (`M` by default), with full keyboard and mouse playbar control and proper navigation stack integration ([#278](https://github.com/LargeModGames/spotatui/pull/278)).
- **Friends screen**: Added a new Library entry and full Friends screen backed by spotatui.com, including friend-code display, online filtering, inline name search, and live now-playing status for followed users.
- **Friend management flows**: Added add-friend support by friend code or username search, plus unfollow actions and periodic background refresh while the Friends screen is open.
- **Wayland clipboard support**: Enabled Linux `arboard` Wayland data-control support so clipboard operations like copying the friend code work reliably on Wayland sessions.

### Security

- **Token cache file permissions**: OAuth token cache is now written with mode `0600` (owner read/write only) on Unix, preventing other local users from reading the Spotify refresh token.
- **Self-update checksum verification**: The auto-update and `spotatui update --install` flows now download and verify the SHA-256 sidecar published alongside each release asset before replacing the binary, so a compromised release asset is rejected before installation.

### Fixed

- **Friends key handling**: Friends-specific keys and inline input now take precedence over conflicting global shortcuts, so add/search/copy/filter actions stay local to the Friends UI.
- **Playlist track search**: Added playlist-internal track search from playlist track tables with `<Ctrl+f>`, client-side matching across track title, artists, and album, loading feedback while large playlists are scanned, and `q`/the configured back key to clear the active playlist filter and restore the cached playlist view ([#198](https://github.com/LargeModGames/spotatui/issues/198)).
- **Spotify auth retry path**: Centralized authenticated Spotify API requests behind a shared refresh-and-retry flow so expired access tokens are handled consistently across playback, library, search, recommendation, metadata, and user calls.

## [v0.38.4] 2026-05-26

### Added

- **Optional self-update feature**: Added a default-on `self-update` Cargo feature so package builds can omit update checks and installer code with `--no-default-features --features telemetry` ([#270](https://github.com/LargeModGames/spotatui/issues/270)).
- **Listening history export & recap**: Added local listening history collection to `~/.config/spotatui/history/listens.jsonl` and a new CLI command `spotatui history recap` to generate a shareable HTML recap. Recaps exclude short/skipped plays and power future sync and analytics features.
- **Generate Recap keybinding**: Added a configurable `R` keybinding to generate and open a 30-day listening recap HTML card directly from the app.
- **Listening history sync token**: Added optional `behavior.sync_token` setting for syncing listening history with spotatui.com
  ([#275](https://github.com/LargeModGames/spotatui/pull/275)).
- **Native playback origin tracking**: Added `NativePlaybackOrigin` enum to distinguish between context-backed playback (safe for Spotify API handoff) and raw URI-list playback (stays on native route for stability).
- **Track kind detection**: Extended native track info to capture whether a track is a regular `Track` or an `Episode`, enabling proper URI construction and future episode-specific UI handling.

### Fixed

- **Native streaming device handoff**: Kept native streaming recovery from stealing playback back after the user transfers listening from spotatui to another Spotify Connect device ([#254](https://github.com/LargeModGames/spotatui/issues/254)).
- **Context-backed native playback**: Native playback started from a Spotify context (e.g., album, playlist, queue) now prefers Spotify-visible playback starts when safe, while raw URI-list playback remains on the stable direct native path to avoid regression.

## [v0.38.3] - 2026-05-23

### Added

- **Current track terminal title**: Updated the `Set Window Title` setting to show the currently playing track as `Track — Artist`, falling back to the default spotatui title when nothing is playing ([#262](https://github.com/LargeModGames/spotatui/issues/262)).

### Fixed

- **Adaptive tick rates**: Split normal UI refreshes from animation-heavy refreshes so regular screens default to a lower CPU cadence while audio visualization stays smooth, and migrate legacy saved tick-rate defaults automatically ([#252](https://github.com/LargeModGames/spotatui/issues/252)).
- **Playbar cover art sizing**: Added configurable playbar cover art sizing and improved layout handling so the image, controls, and progress bar scale cleanly in the playbar ([#253](https://github.com/LargeModGames/spotatui/issues/253)).
- **Mouse song selection**: Changed song-table left clicks so the first click highlights a row and a second click on the highlighted row selects/plays it, matching arrow-key plus Enter behavior ([#258](https://github.com/LargeModGames/spotatui/issues/258)).
- **Playlist and Liked Songs infinite scroll**: Made playlist track tables and Liked Songs scroll continuously across cached pages, prefetch additional pages in the background, and wrap `Down` from the end back to the first row and `Up` from the first row back to the last loaded row.
- **Startup playback hijacking**: Made the default `Continue` startup behavior passive so launching spotatui discovers devices without transferring playback to the native Spotatui Connect device ([#254](https://github.com/LargeModGames/spotatui/issues/254)).
- **Native startup playbar metadata**: Kept native startup playback metadata authoritative while Spotify's playback API catches up, preventing the playbar from switching to an unrelated stale API track while the native player keeps playing the started song.

## [v0.38.2] - 2026-05-10

### Added

- **Keep System Awake during playback**: Added a `behavior.keepawake_enabled` setting, enabled by default, that prevents idle sleep, system sleep, and display suspend while music is playing ([#242](https://github.com/LargeModGames/spotatui/pull/242)).
- **Theme customization from Settings**: Added full theme color editing in Settings, a `Custom` theme preset, live preset previews, and config persistence for custom theme values ([#232](https://github.com/LargeModGames/spotatui/pull/232)).

### Fixed

- **Native streaming metadata freshness**: Kept native player track metadata authoritative while Spotify's playback API is still catching up, preventing stale API responses from overwriting the current native track and suppressing duplicate song-count, lyrics, and saved-track checks for stale items.
- **Recommendation search UI freeze**: Reduced recommendation result loading latency by fetching full track details concurrently and avoiding holding the app lock during network calls ([#239](https://github.com/LargeModGames/spotatui/pull/239)).
- **Theme preset persistence and upgrades**: Fixed theme preset display, saving/loading of edited theme values, and backwards compatibility for existing configs with individual theme colors ([#232](https://github.com/LargeModGames/spotatui/pull/232)).
- **Nix flake macOS build**: Fixed the flake build on `aarch64-darwin` by separating Linux-only dependencies and adding macOS-specific SDK/PortAudio inputs ([#233](https://github.com/LargeModGames/spotatui/pull/233)).

### Internal

- **Runtime module split**: Moved authentication, runtime startup, TUI runner, native player event handling, and shared playback metadata extraction out of the monolithic `main.rs` into focused `core`, `infra`, and `tui` modules.
- **Playback integration metadata sharing**: Centralized playback snapshot construction for Discord RPC, MPRIS, and macOS media integrations so native streaming and Spotify API metadata are resolved consistently.
- **Module path cleanup**: Updated imports to use the current `core`, `infra`, and `tui` module paths after the runtime split.
- **Dependency maintenance**: Bumped `mpris-server` and the grouped Rust minor dependency set ([#226](https://github.com/LargeModGames/spotatui/pull/226), [#227](https://github.com/LargeModGames/spotatui/pull/227), [#236](https://github.com/LargeModGames/spotatui/pull/236)).


## [v0.38.1] - 2026-04-24

### Added

- **Create Playlist flow**: Added an in-app create-playlist screen with name entry, track search, selected-track management, and playlist creation through the Spotify API ([#193](https://github.com/LargeModGames/spotatui/pull/193)).
- **Global like/unlike hotkey**: Added a configurable global keybinding (`F` by default) to like or unlike the currently playing track from any screen, complementing the existing context-specific `s` key.
- **Mouse input disable mode**: Added `behavior.disable_mouse_inputs` support for users who want to run spotatui without mouse interactions enabled ([#200](https://github.com/LargeModGames/spotatui/pull/200)).

### Changed

- **Fullscreen playbar resizing**: `LyricsView` and `CoverArtView` now honor the existing playbar height setting, so the lower player can be resized or fully hidden with the same keybindings used on the main screen (fixes [#208](https://github.com/LargeModGames/spotatui/issues/208)).
- **Settings UI simplification**: Simplified settings screen block handling as part of the mouse-input disable work ([#200](https://github.com/LargeModGames/spotatui/pull/200)).

### Fixed

- **PKCE refresh-token persistence**: Fixed refreshed auth tokens being written without preserving an existing refresh token, which could cause spotatui to require re-login after every reboot ([#217](https://github.com/LargeModGames/spotatui/pull/217)).
- **Startup auth cache recovery**: Fixed startup behavior around failed token-cache loading and prevented in-memory `refresh_token = None` states from replacing a valid cached refresh token.
- **Followed Artists list appears empty**: Fixed Artists view in Library to correctly display followed artists ([#220](https://github.com/LargeModGames/spotatui/pull/220); fixes [#219](https://github.com/LargeModGames/spotatui/issues/219)).
- **Hidden fullscreen cover-art centering**: Fixed the fullscreen cover art image rendering slightly off-center when the playbar was completely hidden.
- **Create Playlist results controls**: Improved keyboard focus and tab controls in the create-playlist search results and selected-track panes.
- **Volume display glitch on rapid changes**: Fixed the volume percentage briefly reverting to an old value after the user changed it, especially noticeable when spamming volume up/down. The UI now always shows the user's intended volume until Spotify's API confirms it matches.

### Internal

- **Dependency maintenance**: Bumped `tui-bar-graph`, `self_update`, grouped Rust minor dependencies, `rustls-webpki`, and `openssl` ([#197](https://github.com/LargeModGames/spotatui/pull/197), [#204](https://github.com/LargeModGames/spotatui/pull/204), [#205](https://github.com/LargeModGames/spotatui/pull/205), [#213](https://github.com/LargeModGames/spotatui/pull/213), [#224](https://github.com/LargeModGames/spotatui/pull/224), [#225](https://github.com/LargeModGames/spotatui/pull/225)).
- **CI maintenance**: Bumped `softprops/action-gh-release` from `2` to `3` in the Actions dependency group ([#212](https://github.com/LargeModGames/spotatui/pull/212)).
- **Lint/format cleanup**: Fixed clippy errors and rustfmt issues after the post-`v0.38.0` changes ([#222](https://github.com/LargeModGames/spotatui/pull/222)).


## [v0.38.0] - 2026-03-23

### Added

- **Expanded add-to-playlist flows**: Added `w` add-to-playlist support from search song results, artist top tracks, and recently played, limited the picker to editable playlists, and refresh the open playlist table after a successful add ([#168](https://github.com/LargeModGames/spotatui/pull/168)).
- **Fullscreen Cover Art View**: Added a dedicated `CoverArtView` with its own route, handler, UI, help entry, and configurable keybinding when the `cover-art` feature is enabled ([#186](https://github.com/LargeModGames/spotatui/pull/186)).
- **MPRIS on Linux without native streaming**: Added MPRIS availability and full playback/metadata state sync for Linux builds even when spotatui is controlling an external Spotify device instead of the native streaming backend ([#172](https://github.com/LargeModGames/spotatui/pull/172)).

### Changed

- **Basic View renamed to Lyrics View**: Renamed the former fullscreen `BasicView` to `LyricsView`, kept `basic_view` as a config alias for backward compatibility, and split full-screen artwork into the new Cover Art view ([#186](https://github.com/LargeModGames/spotatui/pull/186)).
- **Nix flake packaging/version handling**: Refreshed `flake.nix` and simplified release-version handling for Nix packaging ([#149](https://github.com/LargeModGames/spotatui/pull/149), [#161](https://github.com/LargeModGames/spotatui/pull/161)).

### Fixed

- **Liked Songs playback/paging drift**: Fixed saved-track playback starting the wrong song, prevented rows from reordering while pages load, stabilized background prefetch around the current page, and made liked hearts render immediately when opening Liked Songs ([#163](https://github.com/LargeModGames/spotatui/pull/163); fixes [#139](https://github.com/LargeModGames/spotatui/issues/139)).
- **Playlist paging/cache stability**: Fixed playlist tracks duplicating or shuffling while additional pages load, made next/previous navigation target adjacent page offsets instead of sparse cache indexes, and ensured playlist sort/fetch actions target the open playlist table instead of stale sidebar selection state ([#163](https://github.com/LargeModGames/spotatui/pull/163)).
- **Recommendations playback targeting**: Fixed recommendation results so Enter and queue actions use the currently visible selected track list instead of stale cached recommendation state ([#151](https://github.com/LargeModGames/spotatui/issues/151)).
- **OAuth callback/auth retry handling**: Fixed malformed callback requests and browser preflight noise triggering repeated auth prompts at startup, and tightened streaming OAuth retry behavior so only auth failures force a fresh credential retry ([#176](https://github.com/LargeModGames/spotatui/pull/176); fixes [#170](https://github.com/LargeModGames/spotatui/issues/170)).
- **Spotify 5xx playback polling**: Temporary Spotify `502`/`503`/`504` errors during playback polling now show a retry status message and recover automatically instead of surfacing as a hard failure ([#178](https://github.com/LargeModGames/spotatui/pull/178)).
- **External-device/native playback recovery**: Fixed playlist playback on external devices always starting from track 1, and added recovery for native streaming sessions that could stop after the connection dropped or went stale ([#177](https://github.com/LargeModGames/spotatui/pull/177); fixes [#162](https://github.com/LargeModGames/spotatui/issues/162) and [#156](https://github.com/LargeModGames/spotatui/issues/156)).
- **Audio backend panic reporting**: Expanded the panic hook to recognize recoverable PortAudio and Rodio backend failures and show clearer error messaging ([#184](https://github.com/LargeModGames/spotatui/pull/184)).
- **Playlist page-boundary scrolling**: Fixed playlist and track-table scrolling that could briefly render only a single visible row when crossing a page boundary ([#185](https://github.com/LargeModGames/spotatui/pull/185)).

### Internal

- **Dependency maintenance**: Bumped `self_update`, `tokio`, `tokio-tungstenite` (twice), `quinn-proto`, `rustls-webpki`, `tar`, and `worker-relay`'s `undici`/`wrangler` dependencies ([#153](https://github.com/LargeModGames/spotatui/pull/153), [#154](https://github.com/LargeModGames/spotatui/pull/154), [#155](https://github.com/LargeModGames/spotatui/pull/155), [#158](https://github.com/LargeModGames/spotatui/pull/158), [#175](https://github.com/LargeModGames/spotatui/pull/175), [#182](https://github.com/LargeModGames/spotatui/pull/182), [#183](https://github.com/LargeModGames/spotatui/pull/183), [#188](https://github.com/LargeModGames/spotatui/pull/188)).
- **Rust minor dependency rollup**: Updated the grouped Rust minor dependency set, including `clap`, `clap_complete`, `image`, `openssl`, and `openssl-sys` ([#164](https://github.com/LargeModGames/spotatui/pull/164)).
- **rspotify 0.16 migration**: Upgraded `rspotify` from `0.14.0` to `0.16.0` and applied the follow-on compatibility updates across CLI, network, metadata, playback, and UI code ([#187](https://github.com/LargeModGames/spotatui/pull/187)).

### Docs

- **Void Linux installation guide**: Added README installation steps for Void Linux via the `Blackhole-vl` repository ([#171](https://github.com/LargeModGames/spotatui/pull/171)).


## [0.37.3] - 2026-03-06


### Added

- **Startup Playback Behavior Setting**: Added `behavior.startup_behavior` with `Continue`, `Play`, and `Pause` options, plus a new Settings UI cycle control so startup playback state can be configured without editing YAML ([#146](https://github.com/LargeModGames/spotatui/pull/146)).
- **Mouse Playbar Controls**: Added clickable playbar controls for previous, play/pause, next, shuffle, repeat, like, and volume in both the main layout and Basic View, including hitbox handling for resized playbars ([#147](https://github.com/LargeModGames/spotatui/pull/147)).
- **Force Previous Track Action**: Added a dedicated force-previous command and configurable keybinding (`P` by default) that always skips to the previous track instead of restarting the current one first ([#141](https://github.com/LargeModGames/spotatui/pull/141)).
- **Auto-Update Controls**: Added startup auto-update controls via `behavior.disable_auto_update`, `behavior.auto_update_delay`, and the `--no-update` CLI flag. Delayed installs now persist pending state and announce when an update will be applied.

### Changed

- **Settings Editing Model**: Settings now support fixed-option cycle values directly in the list UI, enabling one-step editing for items like startup playback behavior.
- **Update Flow Messaging**: Automatic update checks now distinguish between immediate installs, delayed pending installs, and skipped startup checks instead of always updating right away.

### Fixed

- **Linux Release Features**: Updated the Linux CD build to include the `mpris` feature again so release binaries keep desktop media-control integration working (fixes [#150](https://github.com/LargeModGames/spotatui/issues/150)).
- **Shift Keybinding Normalization**: Normalized kitty-protocol `Shift+letter` input so uppercase bindings like the new default `P` action are recognized reliably across terminals.

## [0.37.2] - 2026-03-04

### Changed

- **Silent Auto-Update on Launch**: Replaced the in-app update prompt with a silent, automatic update mechanism ([#140](https://github.com/LargeModGames/spotatui/pull/140)). The app now checks for, downloads, and installs updates automatically at startup and restarts itself if an update is applied. The update check can be skipped by setting the `SPOTATUI_SKIP_UPDATE` environment variable.

### Removed

- **In-App Update Prompt**: Removed all UI, navigation, and logic related to the previous interactive update prompt (`UpdatePrompt` state, handlers, and popup) ([#140](https://github.com/LargeModGames/spotatui/pull/140)).

### Fixed

- **macOS Build Feature Missing**: Fixed missing `macos-media` feature flag in CD workflow for macOS runners, preventing native media controls from working correctly.

## [0.37.1] - 2026-03-03

### Added

- **Resizable Sidebar and Playbar**: Added user-configurable layout dimensions with runtime keyboard shortcuts ([#138](https://github.com/LargeModGames/spotatui/pull/138)).
  - New config options: `behavior.sidebar_width_percent`, `behavior.playbar_height_rows`, `behavior.library_height_percent`.
  - Keyboard shortcuts to resize at runtime: `{`/`}` (sidebar width), `(`/`)` (playbar height), `|` (library/playlists split).
  - New `src/core/layout.rs` module with layout constraint utilities and tests.
  - Help menu updated to document all new layout adjustment shortcuts.
- **Listening Party Mode (Host/Join)**: Added an in-app listening party flow with a relay-backed session model, including host/join states, party status in the playbar, and a dedicated party popup (`Ctrl+p` by default).
- **Party Guest Name and Room Code UX**: Added required guest name input and 6-character code entry for joining parties, plus host-side guest list and control-mode display.
- **Queue View Route**: Added a dedicated queue screen with selectable entries and a configurable keybinding to open it (`Q` by default).
- **Relay Subproject for Party Sync**: Added `worker-relay/` Cloudflare Worker + Durable Object implementation for room creation, join routing, websocket relaying, and basic rate limiting.
- **Config and Keybinding Extensions**: Added `behavior.relay_server_url`, `behavior.stop_after_current_track`, `keys.show_queue`, and `keys.listening_party` configuration support.

### Changed

- **Token Refresh State Handling**: Authentication refresh now updates in-memory expiry tracking after successful token refresh to avoid repeated refresh dispatch loops.
- **Main Loop Network Polling**: Tokio network loop now interleaves IO event handling with periodic party-message processing.

### Fixed

- **macOS Now Playing Album Artwork**: macOS Control Center / Now Playing now correctly shows album and episode cover art ([#136](https://github.com/LargeModGames/spotatui/pull/136)).
- **Stop-After-Current-Track Behavior**: Native streaming playback now correctly pauses after a natural track transition when `stop_after_current_track` is enabled, instead of auto-continuing.
- **PortAudio Device-Switch Recovery (macOS/AirPods)**: Added recovery for recoverable PortAudio backend panics when the output device changes (for example, AirPods connect/switch events), reducing playback interruptions and preventing app crashes.
- **Mac Tahoe 26.2 Build Failure (macOS Media Activation Policy)**: Fixed a macOS Tahoe 26.2 build break by correcting Objective-C FFI types for `NSApplication setActivationPolicy:` (`BOOL` return and `NSInteger` argument).
- **Settings Shortcut Reliability on macOS Terminals**: Added terminal keyboard-capability detection and runtime fallback handling for `Ctrl+,` when terminal/tmux stacks drop punctuation modifiers; spotatui now applies a session fallback to `Alt+,`, prompts before persisting to `config.yml`, and displays effective shortcuts in Help/Settings UI.

### Internal

- **Dependency Maintenance**: Bumped Rust minor dependency updates ([#130](https://github.com/LargeModGames/spotatui/pull/130)) and CI Actions bumps ([#129](https://github.com/LargeModGames/spotatui/pull/129)).

## [0.37.0] - 2026-02-27

### Added

- **Mouse Support Across the TUI**: Added click/scroll interactions throughout the interface for navigation and settings adjustments.
- **Unsaved Settings Confirmation Prompt**: Added a confirmation flow when leaving settings with unsaved edits.
- **Exit Confirmation Dialog**: Added a quit prompt to prevent accidental exits.
- **Pookie Pink Theme Preset**: Added a new built-in `Pookie Pink` theme preset and improved selection highlight visibility.
- **Animated Home Banner Gradient**: Added animation to the home banner gradient.
- **Remote Announcements and Free-Account Messaging**: Added remote announcement support and in-app messaging for free-account playback limitations.
- **New Logging System**: Added a centralized runtime logging pipeline and session log files in `/tmp/spotatui_logs/spotatuilog{PID}` for diagnostics ([#102](https://github.com/LargeModGames/spotatui/pull/102)).
- **Playbar Cover Art Rendering (Optional)**: Added optional cover art rendering next to the currently playing song/episode title via the `cover-art` feature flag and `ratatui-image` integration ([#111](https://github.com/LargeModGames/spotatui/pull/111)).
- **Cover Art Settings Controls**: Added behavior toggles to enable/disable artwork rendering and force rendering on terminals without full image support (`behavior.draw_cover_art`, `behavior.draw_cover_art_forced`) ([#111](https://github.com/LargeModGames/spotatui/pull/111)).
- **Nix Flake Support**: Added `flake.nix` for flake-based NixOS/Nix installs, including `nix run`/`nix develop` workflow documentation ([#117](https://github.com/LargeModGames/spotatui/pull/117)).

### Changed

- **Architecture Refactor**: Reorganized the codebase into `core`, `infra`, and `tui` modules, including network layer extraction for maintainability.
- **Announcement Fetching Controls**: Refactored announcement fetching to support environment-variable overrides and improved error handling.
- **Documentation Updates**: Updated README content, including playback limitations for free Spotify accounts and Homebrew installation instructions for macOS.


### Fixed

- **Playback Device Auto-Selection**: Fixed auto-select behavior for the `spotatui` playback device.
- **Rapid Skip Playbar Desync**: Fixed playbar state not updating correctly when skipping tracks rapidly.
- **Like Action 400 Errors**: Fixed `400 Bad Request` failures when liking songs.
- **macOS Media Integration Regressions**: Fixed macOS media support regressions in TUI handlers.
- **Global Song Counter Reliability**: Fixed issues affecting global song counter updates.
- **macOS Media Keys and Now Playing Reliability**: Fixed media-key handling and Control Center integration by wiring required macOS app/run-loop behavior and publishing now-playing metadata so spotatui responds to play/pause/next/previous correctly ([#120](https://github.com/LargeModGames/spotatui/pull/120)).

### Internal

- **Dependency Maintenance**: Bumped Rust minor dependency updates ([#104](https://github.com/LargeModGames/spotatui/pull/104)) and refreshed `Cargo.lock`.
- **Code Quality/Formatting**: Fixed clippy warnings and applied rustfmt cleanups ([#106](https://github.com/LargeModGames/spotatui/pull/106)).
- **Nix Release Automation**: Updated release workflow to sync `flake.nix` package version from `Cargo.toml` during releases ([#117](https://github.com/LargeModGames/spotatui/pull/117)).
- **Dependency Maintenance (Follow-up)**: Bumped `clap` to `4.5.60` and `anyhow` to `1.0.102` ([#115](https://github.com/LargeModGames/spotatui/pull/115)).

## [0.36.2] - 2026-02-15

### Added

- **Vesper Theme Preset**: Added a new built-in `Vesper` theme preset ([#98](https://github.com/LargeModGames/spotatui/pull/98)).
- **Playlist Track Management Actions**: Added flows to add tracks to playlists from track-table contexts and quick-add the currently playing track from anywhere (`W`) ([#99](https://github.com/LargeModGames/spotatui/pull/99)).
- **Playlist Track Removal Dialog**: Added remove-from-playlist action (`x` in playlist track tables) with a confirmation dialog and vim-style `h`/`l` confirmation navigation ([#99](https://github.com/LargeModGames/spotatui/pull/99)).

### Fixed

- **Deterministic Duplicate Removal in Playlists**: Removing a track now targets only the selected occurrence by position instead of broadly deleting matching duplicates; when mutation payload safety cannot be guaranteed (for local/unavailable rows), spotatui now reports a clear status error instead of mutating the wrong rows ([#99](https://github.com/LargeModGames/spotatui/pull/99)).
- **Recurring Spirc Startup Failures with Cached Streaming Credentials**: If Spirc initialization fails or times out while using cached streaming credentials, spotatui now clears stale `streaming_cache/credentials.json`, requests fresh streaming OAuth credentials, and retries initialization once before falling back.


## [0.36.1] - 2026-02-14

### Fixed

- **Expired Token Startup Loop**: Startup now validates cached auth tokens and forces a clean re-auth only when the cache is stale/invalid, preventing repeated `status code 400 Bad Request` failures on launch.
- **Recurring 401 Error Modal**: Direct Spotify API calls now refresh the access token on `401 Unauthorized` and retry, preventing the error screen from immediately reappearing after dismissal when a token expires mid-session.
- **Windows Compilation Regression**: Fixed the Windows build break introduced in `0.36.0` ([#97](https://github.com/LargeModGames/spotatui/pull/97)).

## [0.36.0] - 2026-02-13

### Added

- **First-Run Auth Mode Selection**: New users now choose between two setup paths during `client.yml` creation: (1) use the shared ncspot client ID, or (2) use ncspot with a user-provided fallback client ID.
- **Automatic OAuth Client Fallback**: If authentication or profile verification fails with the primary client ID, spotatui now retries automatically with `fallback_client_id` when configured.
- **Client-Specific Token Caches**: OAuth tokens are now stored per client ID to avoid collisions when switching between primary and fallback credentials.
- **Spotify API Compatibility Layer**: Added a raw request/normalization path for Spotify's February 2026 payload changes so responses can still map into existing models.
- **Auth Reconfiguration Command**: Added `--reconfigure-auth` to rerun client authentication setup without deleting config files.
- **One-Time Auth Migration Prompt**: Existing users are prompted once to update authentication setup after config format changes.

### Changed

- **Authentication Flow Migrated to PKCE**: spotatui now authenticates with `AuthCodePkceSpotify` for better compatibility with current Spotify client-ID-based flows.
- **Setup Prompt Order**: On first launch, global song counter opt-in is now prompted after client-ID setup (option 1/2), matching the onboarding sequence.
- **ncspot Redirect URI Handling**: The shared ncspot client path now uses its expected redirect URI and callback port (`http://127.0.0.1:8989/login`).
- **Help Text and Migration Copy**: Updated `--help` and startup migration wording to reflect the new authentication/configuration flow.

### Fixed

- **Spotify Feb 2026 Breaking API Changes**: Resolved multiple regressions for newly created/restricted apps, including missing-field deserialization failures (`tracks`, `track`, `followers`, `external_ids`, `available_markets`, and related shape changes).
- **Discover Reliability**: Reworked Discover top tracks/artists mix loading to avoid removed endpoints and to degrade gracefully when recommendation endpoints are unavailable.
- **Library/Follow Endpoint Migration**: Updated save/follow checks and mutations to use the new `/me/library` and `/me/library/contains` style flows for tracks, albums, shows, artists, and playlists.
- **Startup/Discover 429 Handling**: Added Spotify rate-limit retries with `Retry-After` support, global API pacing, and non-fatal UI handling for transient 429 responses.
- **Excess Startup API Pressure**: Reduced repeated liked-state checks on playback polling so idle startup no longer spams containment checks.
- **Random Playback Poll Network Failures**: Transient request-send/network errors during playback polling now auto-retry with status messaging instead of forcing the blocking error modal.

## [0.35.7] - 2026-02-10

### Added

- **Playlist Folder Navigation**: Playlist folders from your Spotify library are now displayed as a navigable hierarchy in the UI, with optimized background fetching and immediate folder structure rendering ([#92](https://github.com/LargeModGames/spotatui/pull/92))

### Fixed

- **Seek Was Completely Unusable**: Rapid seeking (`<`/`>` keys) would freeze the player, corrupt audio, cause looping glitches, and require Ctrl+C to kill. Fixed by throttling native seeks to 50ms and API seeks to 200ms, queueing pending seeks, and ignoring stale position events after seeking (thanks @El-Mundos - [#86](https://github.com/LargeModGames/spotatui/pull/86))
- **MPRIS Position Tracking and Seeking**: Fixed relative seek calculation (was treating offset as absolute position), added `SetPosition` support for widgets using absolute positioning, and emit `Seeked` signals so external clients update their displays (thanks @El-Mundos - [#85](https://github.com/LargeModGames/spotatui/pull/85))
- **MPRIS Shuffle and Loop Support**: MPRIS now advertises shuffle and loop status capabilities so desktop media controls and `playerctl` can toggle shuffle and repeat modes bidirectionally (thanks @El-Mundos - [#86](https://github.com/LargeModGames/spotatui/pull/86))
- **Progress Reset on Resume**: Resuming playback on external devices no longer briefly flashes 0:00 before the API returns the real position (thanks @El-Mundos - [#86](https://github.com/LargeModGames/spotatui/pull/86))
- **Seek Timing Bug**: Fixed a timing issue where queued API seeks didn't start the position-ignore window, causing the UI to jump back to stale positions during rapid seeks (thanks @El-Mundos - [#86](https://github.com/LargeModGames/spotatui/pull/86))
- **Help Menu Closing App**: Dismissing the help menu no longer closes the entire application; it is now handled as a proper route (thanks @dpnova - [#89](https://github.com/LargeModGames/spotatui/pull/89))
- **Playlist Folder Refresh Races**: Guarded against background refresh races and unified playlist resolution logic to prevent inconsistent folder state ([#92](https://github.com/LargeModGames/spotatui/pull/92))

### Changed

- **Dependency Updates**: Updated `clap` to 4.5.57 and `anyhow` to 1.0.101 ([#90](https://github.com/LargeModGames/spotatui/pull/90))

## [0.35.6] - 2026-02-07

### Added

- **MPRIS Album Art Metadata (Linux)**: MPRIS metadata now includes artwork URLs so desktop media controls can show album covers.
- **Automated Homebrew Publishing**: Added a `publish-homebrew` release job to update the Homebrew tap formula on tagged releases.
- **Config Directory .gitignore**: Automatically creates a `.gitignore` in `~/.config/spotatui/` to protect sensitive files (`client.yml`, `streaming_cache/credentials.json`) from being accidentally committed when users sync their dotfiles.

### Fixed

- **Slim Build Compilation**: Fixed feature gating and conditional imports so builds without `streaming` compile cleanly.
- **Update Check Arc Cloning**: Corrected `Arc` cloning in the async update check task.
- **AUR PKGBUILD Checksum Updates**: Reworked checksum replacement using `awk` and added validation checks to prevent malformed PKGBUILD updates in release workflows.
- **macOS Streaming Audio Backend**: Defaulted `librespot-playback` to `portaudio-backend` on macOS to avoid pipe-sink fallback issues.
- **Reqwest Dependency Resolution**: Made `reqwest` mandatory to prevent missing-dependency build failures (fixes #73).

### Changed

- **Dependency Updates**: Updated `discord-rich-presence` to 1.1.0, `clap` to 4.5.56, `time` to 0.3.47, and `bytes` to 1.11.1.
- **Project Metadata and Docs**: Updated installation docs (Homebrew and Winget), funding metadata, and contributor acknowledgements.

## [0.35.5] - 2026-01-26

### Added

- **Settings Keybindings**: Add customizable keybindings for settings actions.

### Fixed

- **Playback Controls Responsiveness**: Improve responsiveness of playback controls while streaming.

## [0.35.4] - 2026-01-24

### Added

- **Discord Rich Presence**: Show track info, album art, and a GitHub callout in Discord; enabled by default with a built-in application ID and optional overrides via config/env.
- **AUR Binary Package**: `spotatui-bin` is now automatically published alongside releases for faster installation on Arch Linux.

## [0.35.3] - 2026-01-24

### Added

- **Playbar Status Messages**: Added transient status messages in the playbar, used to notify when the saved playback device is unavailable and spotatui falls back to native streaming.

### Fixed

- **Startup Playback Device Selection**: Restored automatic selection of the last used playback device on startup, with graceful fallback to the native device when it is missing.
- **Native Device Activation Latency**: Made Spotify Connect activation non-blocking and reduced delays during device selection/playback.

### Changed

- **Home Screen Rendering**: Cached the home banner gradient and changelog parsing to reduce per-frame work.
- **Changelog Styling**: Changelog section headers now use theme colors for a softer, theme-consistent look.
- **Native Track Metadata**: Store a preformatted artist display string for faster playbar updates.
- **UI Render Loop**: Cache terminal size per tick to avoid repeated backend queries.

## [0.35.1] - 2026-01-07

### Fixed

- **CLI Playback Crash**: Fixed crash when using `spotatui playback --like` or `--dislike` CLI commands ([#50](https://github.com/LargeModGames/spotatui/issues/50))
  - Root cause: Incorrect type access on clap `ArgGroup` caused type mismatch panic
  - Other `playback` flags (`--shuffle`, `--repeat`) were also affected

## [0.35.0] - 2026-01-03

### Added

- **Audio Visualization**: Real-time audio visualization with multiple display styles (requires `audio-viz` or `audio-viz-cpal` feature)
  - **Equalizer Mode**: Uses `tui-equalizer` with half-block bars and brightness effect
  - **Bar Graph Mode**: Uses `tui-bar-graph` with full-height vertical bars
  - Visualizer style can be configured in `config.yml` under `behavior.visualizer_style`
  - Supports custom color gradients via `colorgrad` integration
  - Dynamic frequency analysis displays while audio is playing
- **tui-equalizer Integration**: Added Josh McKinney's `tui-equalizer` widget for modern audio visualization
- **tui-bar-graph Integration**: Added Josh McKinney's `tui-bar-graph` widget for alternative visualization style
- **Dynamic Theme Gradients**: The home banner now features a dynamic, animated gradient that adapts to your current theme's colors
- **Heart Burst Animation**: Added a "heart burst" animation when liking a track for better visual feedback

### Changed

- **Upgraded to Ratatui 0.30**: Major framework upgrade from Ratatui 0.26 to 0.30
  - Replaced deprecated `Frame::size()` with `Frame::area()` across all UI components
  - Updated `App` size field to use `ratatui::layout::Size`
  - Refactored terminal initialization and main event loop for new Frame API
- **Upgraded to Crossterm 0.29**: Updated from Crossterm 0.27 to 0.29 for improved terminal handling
- **Modernized UI Aesthetics**: Refactored UI rendering for cleaner, more modern appearance
- **Audio Capture Lifecycle**: Refactored lazy audio capture initialization for better resource management
- **Unified Border Styles**: Standardized all UI borders (Home, Search, Playbar, Lists) to use `Rounded` corners for a more modern and consistent look

### Internal

- Added comprehensive `docs/RATATUI_0.30_CONVERSION.md` documenting the upgrade process
- Updated audio-viz feature flags to include new visualization dependencies
- Improved audio analysis widget architecture for better extensibility

## [0.34.6] - 2025-12-26

### Added

- **Sorting for Playlists, Albums, and Artists**: Press `,` to open a sort menu with multiple options
  - **Playlist Tracks**: Sort by Name, Date Added, Artist, Album, or Duration
  - **Saved Albums**: Sort by Name, Date Added, or Artist
  - **Saved Artists**: Sort by Name
  - Quick keyboard shortcuts: lowercase letter for ascending (e.g., `n` for Name), uppercase for descending (`N`)
  - Full playlist sorting fetches all tracks for proper cross-page sorting

### Fixed

- **Track Repeat Mode**: Fixed track repeat not working with native streaming player
  - Previously, selecting "Track" repeat mode only updated the UI but continued playing the next song
  - Now properly sends repeat commands to the librespot Spirc player
  - Correctly cycles through Off --> Context --> Track --> Off repeat modes

## [0.34.5] - 2025-12-19

### Added

- **Discover: Top Tracks**: View your 50 most-listened songs with time range selection
  - Uses `[` / `]` keys to cycle time range: **4 weeks**, **6 months**, **All time**
  - Full track table with playback, queue, and navigation support
- **Discover: Top Artists Mix**: A shuffled playlist from your top 5 artists
  - Fetches top tracks from each of your favorite artists
  - Creates a unique mix every time you load it
- **Loading Indicators**: Shows "Loading..." while fetching Discover playlists
- **Track Counts**: Displays number of tracks loaded in the Discover menu
- **Gruvbox Light Theme**: Added a new warm light theme preset for better readability in bright environments

### Fixed

- **Made for You API Error**: Silenced JSON parsing error caused by Spotify's November 2024 API changes that restrict access to algorithmic playlists for apps without extended mode access
  - Previously showed cryptic `json parse error: invalid type: null, expected struct SimplifiedPlaylist` error
  - Now gracefully handles the API restriction without showing an error
- **Theme Background Colors**: Added full support for theme background colors
  - Fixed issue where light themes appeared dark due to terminal background fallback
  - UI widgets now strictly respect the theme's background set in `config.yml`

### Changed

- **Renamed "Made For You" to "Discover"**: The Library's first option is now called "Discover" with a new UI for personalized music features

## [0.34.4] - 2025-12-17

### Added

- **Crash Log on Panic**: When spotatui panics, it now writes a crash log to `~/.config/spotatui/spotatui_panic.log` to help debug hard-to-reproduce issues (set `SPOTATUI_PAUSE_ON_PANIC=1` on Windows to keep the terminal open after a panic).
- **macOS Now Playing Integration**: Native media key support on macOS via Apple's `MPRemoteCommandCenter` API (requires native streaming).
- **Streaming Debug Overrides**: Added `SPOTATUI_STREAMING_AUDIO_BACKEND` and `SPOTATUI_STREAMING_AUDIO_DEVICE` environment variables to help debug native streaming audio output issues.

### Fixed

- **Playlist Track Selection with Shuffle**: Fixed selecting a specific track in a playlist sometimes starting playback from the first track and desyncing the shuffle indicator/state.
- **Spirc Initialization Hangs/Crashes**: Added a startup timeout for native streaming (Spirc) initialization so spotatui can fall back to API-based playback instead of hanging indefinitely (configurable via `SPOTATUI_STREAMING_INIT_TIMEOUT_SECS`).

## [0.34.3] - 2025-12-16

### Added

- **Catppuccin Mocha Theme Preset**: Added the popular Catppuccin Mocha Lavender color scheme as a new built-in theme preset (thanks @MysteriousWolf - PR #19)
- **Nix Support**: Added Nix derivation and build instructions for Nix users (thanks @copeison - PR #16)
  - Basic Nix derivation for building spotatui
  - Documentation for Nix-based installation

### Fixed

- **ALSA Warnings and Lock Contention**: Fixed ALSA warnings and lock contention issues that could cause freezing when viewing liked songs (thanks @rawcode1337 - PR #18, fixes #17)
- **External Device Playback Controls**: Fixed play/pause, skip, and volume controls not working when using the native Spotify app as the active playback device (fixes #14)
- **Silent Crash During Streaming Init**: Panics during Spotify Connect (Spirc) initialization no longer exit silently in release builds, and are handled by falling back to API-based playback control (fixes #15, untested).

## [0.34.2] - 2025-12-13

### Added

- **Persistent Playback Device**: Saves the last used playback device (for example, `spotatui` or `spotifyd`) and re-selects it automatically on the next startup so playback resumes on the same device.

### Fixed

- **macOS SIGSEGV Crash on Startup**: Fixed segmentation fault when launching spotatui on macOS with Bluetooth audio devices connected
  - Switched from rodio to portaudio audio backend for macOS builds
  - Portaudio provides better compatibility with macOS CoreAudio and Bluetooth devices (AirPods, etc.)
  - Pre-built macOS binaries now use portaudio backend by default
  - Fixes crash during "Initializing Spirc" on macOS Sequoia and later

## [0.34.0] - 2025-12-10

### Added

- **MPRIS D-Bus Integration (Linux)**: Desktop media control support for Linux users
  - Control spotatui via media keys (play/pause, next, previous)
  - Compatible with `playerctl` command-line tool
  - Desktop environment integration (GNOME, KDE media widgets)
  - Track metadata exposed (title, artist, album, duration)
  - Playback status and volume synced to D-Bus
  - Requires native streaming feature (enabled by default on Linux)

- **Multi-Page Playback Support**: Enhanced playback functionality for large playlists and saved tracks
  - Play seamlessly across all loaded pages, not just the current page
  - Automatically calculates correct track offset across multiple pages
  - Supports both saved tracks (Liked Songs) and playlists

- **Background Prefetching**: Intelligent prefetching system for improved performance
  - Automatically loads additional tracks in the background when viewing playlists or saved tracks
  - Prefetches up to 500 tracks (~10 pages) for seamless playback
  - Non-blocking implementation - prefetching runs in separate async tasks
  - Prefetched tracks are immediately available for playback without delay

### Fixed

- **First Song Playback Delay**: Fixed 5-10 second delay when playing the first song after startup
  - Root cause: Prefetch operations were blocking the network thread, preventing playback events from being processed
  - Fix: Converted prefetch operations to spawn as independent async tasks using `tokio::spawn()`
  - Result: Playback starts instantly while prefetching happens in the background

- **Track Skip Metadata Sync**: Implemented retry mechanism for track skip operations
  - Ensures Spotify API returns updated track metadata after skipping
  - Prevents showing stale track information in the UI
  - Improves reliability of track transitions


## [0.33.8] - 2025-12-09

### Changed

- **Smaller Binary Size**: Added Ratatui-recommended release optimizations, reducing binary size by ~62% (26MB → 9.9MB)
  - Enabled Link-Time Optimization (LTO)
  - Single codegen unit for better optimization
  - Strip debug symbols
  - Optimized for size (`opt-level = "s"`)
  - Thanks to zamazan4ik for the suggestion in Issue #5!

### Fixed

- **Next Track Skip Shows Stale Progress**: Fixed bug where skipping to the next track with native streaming would show stale progress from the previous song and appear paused
  - Root cause: `get_current_playback()` was preserving the old track's `is_playing` state (often `false` during transition), which overwrote the new track's correct state from the Spotify API
  - Fix: Only preserve volume, shuffle, and repeat states when native streaming is active - `is_playing` now comes from the API response or player events
  - Result: Playbar correctly shows "Playing" and reset progress when skipping tracks

- **Shuffle Not Actually Enabling with Native Streaming**: Shuffle preference is now sent to librespot on startup and when toggling, with device activation to ensure it applies; UI and saved config stay in sync so shuffle really plays shuffled.

- **Playbar Shows Old Track After Skip**: Fixed delay where playbar would briefly show the previous song's name/artist after skipping
  - Root cause: `native_track_info` (instant track info from native player) was unconditionally cleared when API response arrived, even if API returned stale data for the old track
  - Fix: Only clear `native_track_info` when API track name matches the native player's track
  - Result: Playbar immediately shows the new track's name from native player, only switching to API data when it catches up

## [0.33.7] - 2025-12-09

### Fixed

- **Artist View from Search**: Fixed issue where selecting an artist from search results would show a 404 error instead of loading the artist view
  - Root cause: The deprecated `related-artists` Spotify API endpoint was returning 404, blocking the entire artist view from loading
  - Fix: Made related artists fetch optional - artist view now loads successfully with albums and top tracks even if related artists endpoint fails
  - Related artists section will be empty when the endpoint is unavailable, but core artist information displays correctly

## [0.33.6] - 2025-12-09

### Added

- **Persistent Volume**: Volume changes are saved immediately to `config.yml` and restored on startup so your level sticks between sessions
- **Persistent Shuffle**: Shuffle state is now saved and reapplied on launch, including when using native streaming, so you restart right where you left off
- (thanks u/Ratox for the ideas)

## [0.33.5] - 2025-12-09

### Fixed

- **Program Hanging on Exit**: Fixed issue where pressing "q" to exit the TUI would close the interface but leave the program running in the background
  - Root cause: Network thread continued running because the IO channel sender was never dropped, keeping the channel open indefinitely
  - Fix: Added `close_io_channel()` method that explicitly drops the sender when exiting, allowing the network thread to terminate gracefully
  - Result: Program now exits cleanly without requiring an additional Ctrl+C

- **Butter-Smooth Playbar Updates**: Completely rewrote playbar progress calculation for silky-smooth updates
  - **Previously**: Progress jumped every 5 seconds when the Spotify API was polled, causing visible stuttering
  - **Now**: Smooth incremental updates every tick (16ms by default, configurable via `tick_rate_milliseconds`)
  - **How it works**:
    - On each tick, progress increments by the tick rate when playing
    - Resyncs with Spotify API every 5 seconds to prevent drift
    - Responds to API updates within 300ms for instant feedback on play/pause/seek
  - **Result**: Progress bar now flows smoothly like a native music player, not in 5-second jumps
  - Optimized code paths to reduce CPU usage and unnecessary calculations

- **First Song Pause Bug**: Fixed issue where pausing the first song after startup required pressing pause twice
  - Root cause: `is_playing` state wasn't immediately updated to `true` when starting playback, staying `false` until API poll completed
  - Fix: Now immediately sets `is_playing = true` when `StartPlayback` succeeds, matching the behavior of resume playback
  - Result: Pause button works correctly on first press, even immediately after selecting a song

## [0.33.4] - 2025-12-08

### Fixed

- **Instant Track Skip with Native Streaming**: When using native streaming, skipping tracks (n/p keys) now updates the playbar instantly
  - Previously, the UI waited for the Spotify API response before updating, causing a noticeable delay where you'd hear the new song while the playbar still showed the old song
  - Now uses the native player's `next()`/`prev()` methods via the Spirc controller for immediate skip
  - Added `NativeTrackInfo` - extracts track name, artists, and duration from librespot's `TrackChanged` event for instant playbar display
  - The playbar now shows native player info immediately, then seamlessly transitions to full API data when it arrives

- **Real-Time Playbar Progress**: The progress bar now updates every second instead of every 5 seconds when using native streaming
  - Enabled `position_update_interval` in librespot's PlayerConfig to receive periodic `PositionChanged` events
  - Added `is_streaming_active` flag to disable API-based progress calculation when native streaming is active
  - Progress bar time now shows accurate, real-time playback position (0:01, 0:02, 0:03...) instead of jumping in 5-second increments

## [0.33.3] - 2025-12-08

### Fixed

- **Critical: UI Freeze on Rapid Pause/Play**: Fixed terminal freeze that occurred when rapidly pressing pause/play
  - Root cause: `handle_player_events` used blocking `lock().await` for every player event, creating a lock convoy with the main UI loop
  - Fix: Changed to non-blocking `try_lock()` - if lock is busy, skip the update (UI catches up on next tick)
- **Playbar Not Updating**: Fixed playbar only updating every 5 seconds instead of in real-time
  - Root cause: `get_current_playback()` incorrectly overwrote API's `is_playing` state with stale local state
  - Fix: `is_playing` is no longer preserved locally - it now comes from API responses and player events

## [0.33.2] - 2025-12-08

### Fixed

- **Audio Visualization Now Works on `cargo install`**: Added `audio-viz-cpal` to default features so audio visualization works out of the box when installing via `cargo install spotatui`
  - Previously, only pre-built binaries had audio visualization enabled
  - Uses cross-platform `cpal` library for Windows, macOS, and Linux support

### Added

- **Volume Persistence**: Volume setting now persists across application restarts (thanks to Reddit user u/Ratox for the suggestion!)
  - Saved in `config.yml` under `behavior.volume_percent`
  - Applied automatically when native streaming starts

### Changed

- Updated README with detailed audio visualization platform support table:
  - **Windows**: Works out of the box (WASAPI loopback)
  - **Linux**: Works out of the box (PipeWire/PulseAudio monitor)
  - **macOS**: Requires virtual audio device (BlackHole or Loopback)

## [0.33.0] - 2025-12-08

### Added

- **In-App Settings Screen**: Press `Alt-,` to open a new settings UI for customizing spotatui without editing config files
  - **Behavior Settings**: Adjust seek duration, volume increment, tick rate, icons, and toggle options
  - **Keybindings**: View current keybindings (editing coming in future release)
  - **Theme Presets**: Choose from 7 built-in themes with instant preview
- **Theme Presets**: Added 6 new color schemes in addition to the default:
  - **Spotify** - Official Spotify green (#1DB954) theme
  - **Dracula** - Popular dark purple/pink theme
  - **Nord** - Arctic, bluish color palette
  - **Solarized Dark** - Classic dark theme
  - **Monokai** - Vibrant colors on dark background
  - **Gruvbox** - Warm retro groove colors
- **Configuration Persistence**: Settings changes are saved immediately to `config.yml` - no restart required

### Changed

- Updated README with In-App Settings documentation and theme preset table
- Updated bug report template with terminal-specific fields (OS, Terminal, Version) instead of browser/smartphone fields
- **Native Streaming Optimizations**: When using native streaming, seek/pause/volume changes now happen instantly without API round-trips
- **Reduced API Delays**: Lowered playback control delays (seek: 1000ms → 200ms, next/prev: 250ms → 100ms) for snappier response
- Added Settings keybinding (`Alt-,`) to the help menu

### Fixed

- **Cross-Terminal Color Compatibility**: Use explicit RGB values instead of named ANSI colors throughout the UI for consistent rendering across terminals (fixes display issues on Kitty, Alacritty, etc.)
  - Audio visualization bar colors
  - Lyrics display (active/inactive lines)
  - Changelog section headers (Added/Fixed/Changed/etc.)
- **Streaming Player Events**: Improved player event handling to avoid deadlocks by releasing mutex locks before dispatching IO events
- **Settings Route Handling**: Added missing `RouteId::Settings` case in navigation handler to prevent unexpected behavior

## [0.32.0] - 2025-12-07

### Added

- **Native Spotify Streaming (Experimental)**: spotatui can now play audio directly! No more need for spotifyd or an external Spotify client
  - "spotatui" appears as a Spotify Connect device in your device list
  - Control playback from the TUI, phone, or any other Spotify client
  - Powered by [librespot](https://github.com/librespot-org/librespot) for native audio
  - New `streaming` feature flag (enabled by default)
  - Requires separate OAuth flow with redirect URI `http://127.0.0.1:8989/login`

### Changed

- Updated README with Native Streaming documentation and setup instructions
- Added second redirect URI requirement for Spotify app configuration

## [0.31.0] - 2025-12-07

### Added

- **Lyrics in Basic View**: Introduced lyrics support in the basic view mode (press `B` to toggle)

### Changed

- **Network Layer**: Implement network layer for Spotify API interactions and I/O event handling
- **Improved Playlist Scrolling**: Increased playlist fetch batch size to 50 for smoother scrolling performance

### Fixed

- **UI Lag on Skip**: Fixed issue where UI showed the old song for a few seconds after skipping by adding a small delay for state propagation

## [0.30.1] - 2025-12-07

### Fixed

- Fix audio visualization UI rendering on Windows (replaced emoji icons with ASCII alternatives)
- Remove debug output that was bleeding into TUI display on Windows

## [0.30.0] - 2025-12-07

### Added

- **Unskippable Update Prompt**: When a new version is available, users are shown a mandatory modal that must be acknowledged before using the app
  - Displays current and latest version with update instructions
  - Press Enter, Esc, q, or Space to dismiss
  - Replaces the old auto-dismissing notification banner

### Changed

- **Audio Visualization Improvements**:
  - Added noise gate to filter out background noise when no audio is playing
  - Boosted high frequency bands (Air, Ultra) for better visibility
  - Added gradient colors to spectrum bars based on bar height (green → yellow → orange → red)

## [0.29.0] - 2025-12-07

### Added

- **Real-time Audio Visualization**: Press `v` to see a live spectrum analyzer visualization
  - Native PipeWire integration on Linux for seamless audio capture without playback interference
  - FFT-based frequency analysis with 12 frequency bands (Sub to Ultra)
  - Smooth 60 FPS animation with pleasing visual aesthetics
  - Automatic sink monitor detection on Linux via PipeWire
  - No longer depends on Spotify's deprecated Audio Analysis API
- Added `audio-viz` feature flag (enabled by default on Linux)
- Added `pipewire` and `realfft` dependencies for audio processing

### Changed

- Default tick rate changed from 250ms to 16ms (~60 FPS) for smoother UI
- Audio visualization UI shows cleaner status with just "🎵 Capturing audio" and peak level

### Linux Requirements

- **PipeWire** development libraries required for audio visualization:
  - Debian/Ubuntu: `libpipewire-0.3-dev libspa-0.2-dev`
  - Arch Linux: `pipewire`
  - Fedora: `pipewire-devel`

## [0.28.0] - 2025-12-06

### Added

- **Global Song Counter**: Anonymous telemetry feature tracking total songs played by all users worldwide
  - Completely anonymous - no personal information, song names, or listening history collected
  - Opt-in by default with clear privacy notice and easy opt-out via config
  - Real-time counter displayed in README badge
  - Powered by Cloudflare Workers for free, globally-distributed edge computing
  - Rate-limited to prevent abuse (60-second cooldown per IP)
  - Can be disabled by setting `enable_global_song_count: false` in config.yml
  - Startup prompt for existing users to opt-in or opt-out

### Changed

- Added `reqwest` dependency with `rustls-tls` for telemetry HTTP requests
- Added `telemetry` feature flag (enabled by default)
- Updated README with privacy notice and global counter badge

## [0.27.15] - 2025-12-05

### Changed

- Improved changelog display in home screen with styled markdown rendering (colored headers, bullet points, section-specific colors)

## [0.27.1] - 2025-12-05

### Fixed

- Fix duplicate key events on Windows by filtering for `KeyEventKind::Press` only

## [0.26.0] - 2025-12-05

### Changed

- **Rebranded**: Project renamed from `spotify-tui` to `spotatui`
- **Config Directory**: Changed config path from `~/.config/spt/` to `~/.config/spotatui/`
- Construct Spotify config immutably in auth flow
- Update window title handling

### Fixed

- Simplify Option handling and unify key-event flows across handlers
- Small correctness, arithmetic and parsing improvements in CLI, app, and banner
- Use typed `id()` keys for HashSet operations and simplify collections
- Minor rendering and text updates, default ColumnId and ID checks in UI

### Added

- Add `spotatui update` command for self-updating from GitHub releases
- Add automatic update check on startup with notification banner (auto-dismisses after 15 seconds)
- Add comprehensive Copilot instructions documentation (`.github/copilot-instructions.md`)
- Updated GitHub Actions workflow for automated cross-platform releases

### Security

- Upgrade `clap` from 2.33.3 to 4.5 to remove unmaintained `atty` dependency (GHSA-g98v-hv3f-hcfr)
- Add `clap_complete` 4.5 for shell completion generation

## [0.25.1] - 2025-12-01

### Fixed

- Enhance track navigation: load previous tracks when at the start and clamp selected index after loading new tracks

## [0.25.0] - 2025-11-13

### Changed

- **Handlers Migration Complete**: All handlers now use typed IDs (`PlaylistId`, `PlayableId`, `PlayContextId`)
- Refactor track_table to use typed PlaylistId/PlayContextId and simplify logic
- Update artist handler to use PlayableId for playback/queue and recommendations
- Convert album_tracks handler to typed PlayContextId/PlayableId

### Fixed

- Fix input key event pattern matches to account for new `KeyEvent` fields in crossterm
- Fix shuffle behavior: temporarily disable shuffle when playing a specific track to preserve selection
- Fix search handling: avoid passing market parameter incorrectly and handle null playlists
- Fix playback: play selected track directly within context and reorder URIs for correct first-track playback
- Fix track table: load next page of playlist tracks when navigating past last item
- Clone market when calling spotify.search to preserve ownership
- Minor compile-time fixes (app/event/main/redirect/config imports)

### Added

- Add manual token cache with load/save helpers for authentication persistence
- Document deprecated Spotify endpoints and silence deprecation warnings

### Removed

- Remove unused `futures` dependency
- Remove Debug derive from `IoEvent` in network module

## [0.25.0-beta.2] - 2025-11-12

### Changed

- **Network Layer Migration**: Complete migration to typed IDs for all network API calls
- Network: adapt search API signature and map search results to typed IDs
- Network: migrate manual pagination for playlists, albums, saved tracks
- Network: migrate saved-shows and show/episode endpoints
- Network: rename follow/unfollow playlist/artist APIs to new method names
- Network: update recommendations/seed handling and PlayableId mapping
- Migrate playlist and recently_played handlers to typed IDs

### Fixed

- Fix device volume handling: safely handle optional `volume_percent` field
- Fix clipboard helpers: use typed IDs and bail gracefully when data is missing
- Fix user country retrieval with defensive error handling
- Fix mutable borrow issues for current route handling
- App: migrate playback progress/duration to rspotify 0.12 Duration fields

### Added

- Add `chrono` dependency for time handling
- App: use typed TrackId when requesting audio analysis
- App: convert recommendation seed id handling to typed IDs
- App: switch follow/unfollow and saved-album flows to typed IDs (into_static)

## [0.25.0-beta.1] - 2025-11-11

### Changed

- **Forked**: Project forked and maintained by LargeModGames
- **Major Dependency Update**:
  - Migrated from `tui` to `ratatui` (v0.26) for UI rendering
  - Upgraded `rspotify` to v0.13 with new authentication API (`AuthCodeSpotify`)
  - Updated all dependencies to latest compatible versions
- **Typed ID System**: Begin migration to rspotify's typed ID system (`TrackId`, `PlaylistId`, `PlayableId`, `PlayContextId`)
- **Duration Handling**: Switch from legacy duration fields to rspotify 0.12+ `Duration` / `TimeDelta` types
- **UI Frame API**: Update all ratatui draw helpers to use `Frame<'_>` parameter style
- Migrate Spotify authentication and network layer to new rspotify API (AuthCodeSpotify)
- App: switch to rspotify idtypes and convert app dispatches to typed IDs
- CLI: normalize to typed IDs, remove lifetime param, handle optional device IDs and duration conversions
- Network: adopt rspotify idtypes for IoEvent payloads
- Handlers/track_table: use typed IDs/PlayableId and context IDs for playback/queue

### Added

- Add futures dependency for network stream handling & device API

## [Upstream 0.25.0] - 2021-08-24

### Fixed

- Fixed rate limiting issue [#852](https://github.com/Rigellute/spotify-tui/pull/852)
- Fix double navigation to same route [#826](https://github.com/Rigellute/spotify-tui/pull/826)

## [0.24.0] - 2021-04-26

### Fixed

- Handle invalid Client ID/Secret [#668](https://github.com/Rigellute/spotify-tui/pull/668)
- Fix default liked, shuffle, etc. icons to be more recognizable symbols [#702](https://github.com/Rigellute/spotify-tui/pull/702)
- Replace black and white default colors with reset [#742](https://github.com/Rigellute/spotify-tui/pull/742)

### Added

- Add ability to seek from the CLI [#692](https://github.com/Rigellute/spotify-tui/pull/692)
- Replace `clipboard` with `arboard` [#691](https://github.com/Rigellute/spotify-tui/pull/691)
- Implement some episode table functions [#698](https://github.com/Rigellute/spotify-tui/pull/698)
- Change `--like` that toggled the liked-state to explicit `--like` and `--dislike` flags [#717](https://github.com/Rigellute/spotify-tui/pull/717)
- Add to config: `enforce_wide_search_bar` to make search bar bigger [#738](https://github.com/Rigellute/spotify-tui/pull/738)
- Add Daily Drive to Made For You lists search [#743](https://github.com/Rigellute/spotify-tui/pull/743)

## [0.23.0] - 2021-01-06

### Fixed

- Fix app crash when pressing Enter before a screen has loaded [#599](https://github.com/Rigellute/spotify-tui/pull/599)
- Make layout more responsive to large/small screens [#502](https://github.com/Rigellute/spotify-tui/pull/502)
- Fix use of incorrect playlist index when playing from an associated track table [#632](https://github.com/Rigellute/spotify-tui/pull/632)
- Fix flickering help menu in small screens [#638](https://github.com/Rigellute/spotify-tui/pull/638)
- Optimize seek [#640](https://github.com/Rigellute/spotify-tui/pull/640)
- Fix centering of basic_view [#664](https://github.com/Rigellute/spotify-tui/pull/664)

### Added

- Implement next/previous page behavior for the Artists table [#604](https://github.com/Rigellute/spotify-tui/pull/604)
- Show saved albums when getting an artist [#612](https://github.com/Rigellute/spotify-tui/pull/612)
- Transfer playback when changing device [#408](https://github.com/Rigellute/spotify-tui/pull/408)
- Search using Spotify share URLs and URIs like the desktop client [#623](https://github.com/Rigellute/spotify-tui/pull/623)
- Make the liked icon configurable [#659](https://github.com/Rigellute/spotify-tui/pull/659)
- Add CLI for controlling Spotify [#645](https://github.com/Rigellute/spotify-tui/pull/645)
- Implement Podcasts Library page [#650](https://github.com/Rigellute/spotify-tui/pull/650)

## [0.22.0] - 2020-10-05

### Fixed

- Show ♥ next to album name in saved list [#540](https://github.com/Rigellute/spotify-tui/pull/540)
- Fix to be able to follow an artist in search result view [#565](https://github.com/Rigellute/spotify-tui/pull/565)
- Don't add analysis view to stack if already in it [#580](https://github.com/Rigellute/spotify-tui/pull/580)

### Added

- Add additional line of help to show that 'w' can be used to save/like an album [#548](https://github.com/Rigellute/spotify-tui/pull/548)
- Add handling Home and End buttons in user input [#550](https://github.com/Rigellute/spotify-tui/pull/550)
- Add `playbar_progress_text` to user config and upgrade tui lib [#564](https://github.com/Rigellute/spotify-tui/pull/564)
- Add basic playbar support for podcasts [#563](https://github.com/Rigellute/spotify-tui/pull/563)
- Add 'enable_text_emphasis' behavior config option [#573](https://github.com/Rigellute/spotify-tui/pull/573)
- Add next/prev page, jump to start/end to user config [#566](https://github.com/Rigellute/spotify-tui/pull/566)
- Add possibility to queue a song [#567](https://github.com/Rigellute/spotify-tui/pull/567)
- Add user-configurable header styling [#583](https://github.com/Rigellute/spotify-tui/pull/583)
- Show active keybindings in Help [#585](https://github.com/Rigellute/spotify-tui/pull/585)
- Full Podcast support [#581](https://github.com/Rigellute/spotify-tui/pull/581)

## [0.21.0] - 2020-07-24

### Fixed

- Fix typo in help menu [#485](https://github.com/Rigellute/spotify-tui/pull/485)

### Added

- Add save album on album view [#506](https://github.com/Rigellute/spotify-tui/pull/506)
- Add feature to like a song from basic view [#507](https://github.com/Rigellute/spotify-tui/pull/507)
- Enable Unix and Linux shortcut keys in the input [#511](https://github.com/Rigellute/spotify-tui/pull/511)
- Add album artist field to full album view [#519](https://github.com/Rigellute/spotify-tui/pull/519)
- Handle track saving in non-album contexts (eg. playlist/Made for you). [#525](https://github.com/Rigellute/spotify-tui/pull/525)

## [0.20.0] - 2020-05-28

### Fixed

- Move pagination instructions to top of help menu [#442](https://github.com/Rigellute/spotify-tui/pull/442)

### Added

- Add user configuration toggle for the loading indicator [#447](https://github.com/Rigellute/spotify-tui/pull/447)
- Add support for saving an album and following an artist in artist view [#445](https://github.com/Rigellute/spotify-tui/pull/445)
- Use the `▶` glyph to indicate the currently playing song [#472](https://github.com/Rigellute/spotify-tui/pull/472)
- Jump to play context (if available) - default binding is `o` [#474](https://github.com/Rigellute/spotify-tui/pull/474)

## [0.19.0] - 2020-05-04

### Fixed

- Fix re-authentication [#415](https://github.com/Rigellute/spotify-tui/pull/415)
- Fix audio analysis feature [#435](https://github.com/Rigellute/spotify-tui/pull/435)

### Added

- Add more readline shortcuts to the search input [#425](https://github.com/Rigellute/spotify-tui/pull/425)

## [0.18.0] - 2020-04-21

### Fixed

- Fix crash when opening playlist [#398](https://github.com/Rigellute/spotify-tui/pull/398)
- Fix crash when there are no artists avaliable [#388](https://github.com/Rigellute/spotify-tui/pull/388)
- Correctly handle playlist unfollowing [#399](https://github.com/Rigellute/spotify-tui/pull/399)

### Added

- Allow specifying alternative config file path [#391](https://github.com/Rigellute/spotify-tui/pull/391)
- List artists names in the album view [#393](https://github.com/Rigellute/spotify-tui/pull/393)
- Add confirmation modal for delete playlist action [#402](https://github.com/Rigellute/spotify-tui/pull/402)

## [0.17.1] - 2020-03-30

### Fixed

- Artist name in songs block [#365](https://github.com/Rigellute/spotify-tui/pull/365) (fixes regression)
- Add basic view key binding to help menu

## [0.17.0] - 2020-03-20

### Added

- Show if search results are liked/followed [#342](https://github.com/Rigellute/spotify-tui/pull/342)
- Show currently playing track in song search menu and play through the searched tracks [#343](https://github.com/Rigellute/spotify-tui/pull/343)
- Add a "basic view" that only shows the playbar. Press `B` to get there [#344](https://github.com/Rigellute/spotify-tui/pull/344)
- Show currently playing top track [#347](https://github.com/Rigellute/spotify-tui/pull/347)
- Press shift-s (`S`) to pick a random song on track-lists [#339](https://github.com/Rigellute/spotify-tui/pull/339)

### Fixed

- Prevent search when there is no input [#351](https://github.com/Rigellute/spotify-tui/pull/351)

## [0.16.0] - 2020-03-12

### Fixed

- Fix empty UI when pressing escape in the device and analysis views [#315](https://github.com/Rigellute/spotify-tui/pull/315)
- Fix slow and frozen UI by implementing an asynchronous runtime for network events [#322](https://github.com/Rigellute/spotify-tui/pull/322). This fixes issues #24, #92, #207 and #218. Read more [here](https://keliris.dev/improving-spotify-tui/).

## [0.15.0] - 2020-02-24

- Add experimental audio visualizer (press `v` to navigate to it). The feature uses the audio analysis data from Spotify and animates the pitch information.
- Display Artist layout when searching an artist url [#298](https://github.com/Rigellute/spotify-tui/pull/298)
- Add pagination to the help menu [#302](https://github.com/Rigellute/spotify-tui/pull/302)

## [0.14.0] - 2020-02-11

### Added

- Add high-middle-low navigation (`H`, `M`, `L` respectively) for jumping around lists [#234](https://github.com/Rigellute/spotify-tui/pull/234).
- Play every known song with `e` [#228](https://github.com/Rigellute/spotify-tui/pull/228)
- Search album by url: paste a spotify album link into the search input to go to that album [#281](https://github.com/Rigellute/spotify-tui/pull/281)
- Implement 'Made For You' section of Library [#278](https://github.com/Rigellute/spotify-tui/pull/278)
- Add user theme configuration [#284](https://github.com/Rigellute/spotify-tui/pull/284)
- Allow user to define the volume increment [#288](https://github.com/Rigellute/spotify-tui/pull/288)

### Fixed

- Fix crash on small terminals [#231](https://github.com/Rigellute/spotify-tui/pull/231)

## [0.13.0] - 2020-01-26

### Fixed

- Don't error if failed to open clipboard [#217](https://github.com/Rigellute/spotify-tui/pull/217)
- Fix scrolling beyond the end of pagination. [#216](https://github.com/Rigellute/spotify-tui/pull/216)
- Add copy album url functionality [#226](https://github.com/Rigellute/spotify-tui/pull/226)

### Added

- Allow user to configure the port for the Spotify auth Redirect URI [#204](https://github.com/Rigellute/spotify-tui/pull/204)
- Add play recommendations for song/artist on pressing 'r' [#200](https://github.com/Rigellute/spotify-tui/pull/200)
- Added continuous deployment for Windows [#222](https://github.com/Rigellute/spotify-tui/pull/222)

### Changed

- Change behavior of previous button (`p`) to mimic behavior in official Spotify client. When the track is more than three seconds in, pressing previous will restart the track. When less than three seconds it will jump to previous. [#219](https://github.com/Rigellute/spotify-tui/pull/219)

## [0.12.0] - 2020-01-23

### Added

- Add Windows support [#99](https://github.com/Rigellute/spotify-tui/pull/99)
- Add support for Related artists and top tacks [#191](https://github.com/Rigellute/spotify-tui/pull/191)

## [0.11.0] - 2019-12-23

### Added

- Add support for adding an album and following a playlist. NOTE: that this will require the user to grant more permissions [#172](https://github.com/Rigellute/spotify-tui/pull/172)
- Add shortcuts to jump to the start or the end of a playlist [#167](https://github.com/Rigellute/spotify-tui/pull/167)
- Make seeking amount configurable [#168](https://github.com/Rigellute/spotify-tui/pull/168)

### Fixed

- Fix playlist index after search [#177](https://github.com/Rigellute/spotify-tui/pull/177)
- Fix cursor offset in search input [#183](https://github.com/Rigellute/spotify-tui/pull/183)

### Changed

- Remove focus on input when jumping back [#184](https://github.com/Rigellute/spotify-tui/pull/184)
- Pad strings in status bar to prevent reformatting [#188](https://github.com/Rigellute/spotify-tui/pull/188)

## [0.10.0] - 2019-11-30

### Added

- Added pagination to user playlists [#150](https://github.com/Rigellute/spotify-tui/pull/150)
- Add ability to delete a saved album (hover over the album you wish to delete and press `D`) [#152](https://github.com/Rigellute/spotify-tui/pull/152)
- Add support for following/unfollowing artists [#155](https://github.com/Rigellute/spotify-tui/pull/155)
- Add hotkey to copy url of currently playing track (default binding is `c`)[#156](https://github.com/Rigellute/spotify-tui/pull/156)

### Fixed

- Refine Spotify result limits, which should fit your current terminal size. Most notably this will increase the number of results from a search [#154](https://github.com/Rigellute/spotify-tui/pull/154)
- Navigation from "Liked Songs" [#151](https://github.com/Rigellute/spotify-tui/pull/151)
- App hang upon trying to authenticate with Spotify on FreeBSD [#148](https://github.com/Rigellute/spotify-tui/pull/148)
- Showing "Release Date" in saved albums table [#162](https://github.com/Rigellute/spotify-tui/pull/162)
- Showing "Length" in library -> recently played table [#164](https://github.com/Rigellute/spotify-tui/pull/164)
- Typo: "AlbumTracks" -> "Albums" [#165](https://github.com/Rigellute/spotify-tui/pull/165)
- Janky volume control [#166](https://github.com/Rigellute/spotify-tui/pull/166)
- Volume bug that would prevent volumes of 0 and 100 [#170](https://github.com/Rigellute/spotify-tui/pull/170)
- Playing a wrong track in playlist [#173](https://github.com/Rigellute/spotify-tui/pull/173)

## [0.9.0] - 2019-11-13

### Added

- Add custom keybindings feature. Check the README for an example configuration [#112](https://github.com/Rigellute/spotify-tui/pull/112)

### Fixed

- Fix panic when seeking beyond track boundaries [#124](https://github.com/Rigellute/spotify-tui/pull/124)
- Add scrolling to library album list. Can now use `ctrl+d/u` to scroll between result pages [#128](https://github.com/Rigellute/spotify-tui/pull/128)
- Fix showing wrong album in library album view - [#130](https://github.com/Rigellute/spotify-tui/pull/130)
- Fix scrolling in table views [#135](https://github.com/Rigellute/spotify-tui/pull/135)
- Use space more efficiently in small terminals [#143](https://github.com/Rigellute/spotify-tui/pull/143)

## [0.8.0] - 2019-10-29

### Added

- Improve onboarding: auto fill the redirect url [#98](https://github.com/Rigellute/spotify-tui/pull/98)
- Indicate if a track is "liked" in Recently Played, Album tracks and song list views using "♥" - [#103](https://github.com/Rigellute/spotify-tui/pull/103)
- Add ability to toggle the saved state of a track: pressing `s` on an already saved track will unsave it. [#104](https://github.com/Rigellute/spotify-tui/pull/104)
- Add collaborative playlists scope. You'll need to reauthenticate due to this change. [#115](https://github.com/Rigellute/spotify-tui/pull/115)
- Add Ctrl-f and Ctrl-b emacs style keybindings for left and right motion. [#114](https://github.com/Rigellute/spotify-tui/pull/114)

### Fixed

- Fix app crash when pressing `enter`, `q` and then `down`. [#109](https://github.com/Rigellute/spotify-tui/pull/109)
- Fix trying to save a track in the album view [#119](https://github.com/Rigellute/spotify-tui/pull/119)
- Fix UI saved indicator when toggling saved track [#119](https://github.com/Rigellute/spotify-tui/pull/119)

## [0.7.0] - 2019-10-20

- Implement library "Artists" view - [#67](https://github.com/Rigellute/spotify-tui/pull/67) thanks [@svenvNL](https://github.com/svenvNL). NOTE that this adds an additional scope (`user-follow-read`), so you'll be prompted to grant this new permissions when you upgrade.
- Fix searching with non-english characters - [#30](https://github.com/Rigellute/spotify-tui/pull/30). Thanks to [@fangyi-zhou](https://github.com/fangyi-zhou)
- Remove hardcoded country (was always set to UK). We now fetch the user to get their country. [#68](https://github.com/Rigellute/spotify-tui/pull/68). Thanks to [@svenvNL](https://github.com/svenvNL)
- Save currently playing track - the playbar is now selectable/hoverable [#80](https://github.com/Rigellute/spotify-tui/pull/80)
- Lay foundation for showing if a track is saved. You can now see if the currently playing track is saved (indicated by ♥)

## [0.6.0] - 2019-10-14

### Added

- Start a web server on localhost to display a simple webpage for the Redirect URI. Should hopefully improve the onboarding experience.
- Add ability to skip to tracks using `n` for next and `p` for previous - thanks to [@samcal](https://github.com/samcal)
- Implement seek functionality - you can now use `<` to seek backwards 5 seconds and `>` to go forwards 5 seconds
- The event `A` will jump to the album list of the first artist in the track's artists list - closing [#45](https://github.com/Rigellute/spotify-tui/issues/45)
- Add volume controls - use `-` to decrease and `+` to increase volume in 10% increments. Closes [#57](https://github.com/Rigellute/spotify-tui/issues/57)

### Fixed

- Keep format of highlighted track when it is playing - [#44](https://github.com/Rigellute/spotify-tui/pull/44) thanks to [@jfaltis](https://github.com/jfaltis)
- Search input bug: Fix "out-of-bounds" crash when pressing left too many times [#63](https://github.com/Rigellute/spotify-tui/issues/63)
- Search input bug: Fix issue that backspace always deleted the end of input, not where the cursor was [#33](https://github.com/Rigellute/spotify-tui/issues/33)

## [0.5.0] - 2019-10-11

### Added

- Add `Ctrl-r` to cycle repeat mode ([@baxtea](https://github.com/baxtea))
- Refresh token when token expires ([@fangyi-zhou](https://github.com/fangyi-zhou))
- Upgrade `rspotify` to fix [#39](https://github.com/Rigellute/spotify-tui/issues/39) ([@epwalsh](https://github.com/epwalsh))

### Changed

- Fix duplicate albums showing in artist discographies ([@baxtea](https://github.com/baxtea))
- Slightly better error message with some debug tips when tracks fail to play

## [0.4.0] - 2019-10-05

### Added

- Can now install `spotify-tui` using `brew reinstall Rigellute/tap/spotify-tui` and `cargo install spotify-tui` 🎉
- Credentials (auth token, chosen device, and CLIENT_ID & CLIENT_SECRET) are now all stored in the same place (`${HOME}/.config/spotify-tui/client.yml`), which closes [this issue](https://github.com/Rigellute/spotify-tui/issues/4)

## [0.3.0] - 2019-10-04

### Added

- Improved onboarding experience
- On first startup instructions will (hopefully) guide the user on how to get setup

## [0.2.0] - 2019-09-17

### Added

- General navigation improvements
- Improved search input: it should now behave how one would expect
- Add `Ctrl-d/u` for scrolling up and down through result pages (currently only implemented for "Liked Songs")
- Minor theme improvements
- Make tables responsive
- Implement resume playback feature
- Add saved albums table
- Show which track is currently playing within a table or list
- Add `a` event to jump to currently playing track's album
- Add `s` event to save a track from within the "Recently Played" view (eventually this should be everywhere)
- Add `Ctrl-s` to toggle shuffle
- Add the following journey: search -> select artist and see their albums -> select album -> go to album and play tracks

# What is this?

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
