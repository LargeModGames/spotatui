# Summary

A slow playlist response can replace a track list opened while it was loading. Capture the table version when the load is queued. Check it and the current table context before applying rows or showing an error.

Guard Local, Subsonic, Qobuz and YouTube list loads. Reopening the same list queues a new load, and the older one is dropped. These loads no longer set or clear the global spinner, which a newer event may own. Each keeps its own loading flag instead, cleared when the pump finishes it or another list replaces the table. The TUI search box still shows its double-line loading border while it is set.

A Qobuz 401 that arrives after the table moved on still forgets the token it was sent with, unless a login has replaced it since. The next list then logs in again instead of failing on the same token.

# Testing

Ran on macOS arm64 at `a3e0ed5`, based on upstream main `536f080`, with isolated XDG directories and `TMPDIR`.

The final checks used `LANGUAGE=en LC_ALL=C` because local Git prints errors in Chinese by default and five `cli::plugin` Git tests fail under that locale on both this branch and upstream main.

```text
$ GUI_FEATURES=telemetry,gui,streaming,discord-rpc,self-update,scripting,mcp-server,mpris,local-files,subsonic,internet-radio,youtube,qobuz,audio-viz,audio-viz-cpal

$ cargo fmt --all --check
exit=0

$ cargo clippy --locked --no-default-features --features telemetry,tui -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 14.87s
exit=0

$ cargo clippy --locked -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.89s
exit=0

$ cargo clippy --locked --features all-sources -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 14.53s
exit=0

$ LANGUAGE=en LC_ALL=C cargo test --locked --no-default-features --features telemetry,tui
test result: ok. 2054 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.88s
    Finished `test` profile [unoptimized + debuginfo] target(s) in 39.04s
exit=0

$ LANGUAGE=en LC_ALL=C cargo test --locked --features all-sources
test result: ok. 2932 passed; 0 failed; 30 ignored; 0 measured; 0 filtered out; finished in 4.27s
    Finished `test` profile [unoptimized + debuginfo] target(s) in 2m 00s
exit=0

$ cargo clippy --locked --no-default-features --features "$GUI_FEATURES" -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 14.83s
exit=0

$ LANGUAGE=en LC_ALL=C cargo test --locked --no-default-features --features "$GUI_FEATURES"
test result: ok. 2480 passed; 0 failed; 30 ignored; 0 measured; 0 filtered out; finished in 7.72s
    Finished `test` profile [unoptimized + debuginfo] target(s) in 1m 31s
exit=0
```

The new tests fail with their fix reverted: `a_stale_qobuz_listing_401_still_forgets_the_token_it_sent`, `a_pending_source_playlist_draws_the_loading_border` and `the_pump_ends_a_source_playlist_loading_flag_when_the_load_finishes`.

# Additional notes

No GUI protocol type or string changed, and the GUI test run left `gui/src/bindings/` unchanged. Live servers, audio devices and Linux CI were outside this test run.

Left for follow-up, since main has them too:

- A Qobuz list opened while logged out queues a plain `QobuzLogin`. This change only queues it while the list is current. If another list opens before the login runs, the browser login still starts.
- After a YouTube track removal, `youtube_open_playlist` can still name the old list and write its rows into a newer table.
- A failed Subsonic list load posts a normal status message. The GUI list page leaves its loading state only on an error status, so it keeps showing loading.
