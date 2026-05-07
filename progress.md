# Cross-Platform Tauri GUI — Implementation Progress

Branch: `feature/cross-platform-tauri-gui`

## Working (real backend)

- App launches and authenticates with existing `~/.config/spotatui/` credentials
- `get_snapshot()` returns live playback state: current track, progress, playing/paused, volume, shuffle, repeat, devices
- Playback controls: play/pause, next/prev, seek, volume change
- Device list from Spotify
- Transfer playback between devices
- Native streaming (Premium + enabled in config)
- All existing TUI behavior unchanged (171 tests pass)

## Not working (mock/fallback)

- **Search** — no `GuiCommand` for search, search view filters local mock tracks
- **Playlists / library / queue / recently played** — `GuiSnapshot` doesn't include these collections, sidebar shows mock playlists
- **Shuffle & repeat toggles** — not in `GuiCommand` enum, sent but ignored by backend
- **Play specific track** — not in `GuiCommand` enum
- **Album art images** — CoverArt uses gradients even when `image_url` is present
- **First-time auth** — OAuth flow prints to stdout and opens a browser, no in-app auth UI
- **Settings, lyrics, party mode** — post-v1 scope

## Rough edges

- Console output not suppressed for GUI mode
- Error states (auth failure, network down) fall back to mock silently, no UI surface
- No loading spinner during session initialization

## Build verification

```bash
cargo check                                    # default features (streaming)
cargo check --no-default-features -F telemetry  # minimal/CI
cargo test --no-default-features -F telemetry   # 171 tests
cargo clippy -p spotatui -- -D warnings         # lint
cd apps/desktop && npx tsc --noEmit             # TypeScript check
cd apps/desktop && npm run tauri dev            # launch GUI
```
