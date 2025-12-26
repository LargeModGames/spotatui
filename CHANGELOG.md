# Changelog

## [0.34.6] - 2025-12-26

### Added

- **Sorting for Playlists, Albums, and Artists**: Press `,` to open a sort menu with multiple options
  - **Playlist Tracks**: Sort by Name, Date Added, Artist, Album, or Duration
  - **Saved Albums**: Sort by Name, Date Added, or Artist
  - **Saved Artists**: Sort by Name
  - Quick keyboard shortcuts: lowercase letter for ascending (e.g., `n` for Name), uppercase for descending (`N`)
  - Full playlist sorting fetches all tracks for proper cross-page sorting

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
