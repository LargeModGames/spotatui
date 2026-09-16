# degen-radio contributor notes

## Commands

```bash
cargo check
cargo test
cargo run
degen-radio radio list --json
```

## Architecture

- `src/radio_app/config.rs`: compatible loading of saved radio stations and volume.
- `src/radio_app/directory.rs`: radio-browser.info search with mirror failover.
- `src/radio_app/stream.rs`: HTTP/ICY stream reader and metadata extraction.
- `src/radio_app/player.rs`: bounded decoded-audio output engine.
- `src/radio_app/mpris.rs`: Linux MPRIS server and OpenUri client.
- `src/radio_app/tui.rs`: terminal rendering, input, playback state, and MPRIS routing.
- `src/radio_app/cli.rs`: `degen-radio radio list|play` protocol used by the Omarchy plugin.

Keep stdout machine-readable for `radio list --json`. Diagnostics belong on stderr or in the state log.
