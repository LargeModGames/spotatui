### The Soloist engine (feature `soloist`)

Spotify Soloist plays the audio behind spotatui's own Connect device when
`behavior.playback_engine: soloist`. Soloist IS the device: it registers itself
under `streaming_device_name`, and spotatui starts no librespot Spirc device.

- In soloist mode `App.streaming_player` and `streaming_recovery_tx` stay `None`
  and `native_parked` is never set, so every librespot path stays off. The owner
  stays `PlaybackOwner::Spotify`; the engine adds narrow
  `App::soloist_is_target()` arms instead of a new owner.
- Call sites outside this folder, `core/app/soloist.rs` and
  `runtime/streaming/soloist.rs` use only the ungated bool predicates
  `soloist_is_target()` and `soloist_engine_installed()`.
- Every write from the supervisor or the WebSocket reader goes through an `App`
  method that checks `Arc::ptr_eq` against the installed link first. A restart
  installs a new link, so a reader of a replaced child cannot write.
- Transports go over the loopback WebSocket. Context and URI-list starts go over
  connect-state with `device_id=<soloist id>`, because WS `play` takes no offset;
  they pay one round trip on the serial pump. WS transports skip the 429 hold
  only while Soloist is the target (`soloist_ws_transport`).
- State comes from WebSocket events, never from `command_result`, which only
  means "dispatched".
- The API key goes on Soloist's argv (no other input exists), so other local
  programs can read it. Never log the argv, the key, or the account name:
  `logline::redact` runs on every stdout line before it reaches the log.
- The playback keys in `settings/Users/*/prefs` (quality,
  `audio.normalize_v2=false`) are undocumented internal state; the official
  quality path is the per-device menu in the Spotify mobile app.
  The playbar badge shows what the log measured, never the setting.
- Never redistribute the Soloist binary: the user downloads it from Spotify.
