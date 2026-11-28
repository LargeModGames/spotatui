### Native streaming (feature `streaming`, in `default`)

On Windows, the default native playback path uses `sink.rs`, a rodio 0.22
sink that bounds queue waits and reports output-device loss so recovery can
reconnect. When no device is pinned, its device thread checks the system
default once a second, and `write` reopens on the new default when it moved.

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
  local → stop. The handoff veto is sticky (#437); the idle poll does not
  reclaim a handed-off device (#693).
- Every background native write is generation-guarded
  (`native_playback_generation`, `native_shuffle_generation`) and event handlers
  confirm `Arc::ptr_eq` against the current player before writing - stale writes
  from a replaced backend are the recurring bug class.
- librespot reports full `spotify:track:<id>` URIs while app state uses bare
  base62 ids: normalize with `base62_id_of` at the event boundary
  (`spotify:local:` URIs stay whole).
- A parked `StartPlayback` is a **user press with a deadline**, not a durable
  intent. `StartPlaybackRequest::pressed_at` is fixed for the request's life and
  `max_parked_start_age()` (2 min) measures from it whatever
  `exclude_recovery_time_from_parked_start` credits back; the retry chain has no
  limit of its own. The credit only discounts the soft `parked_start_window()`
  (one player build). Never age `pressed_at` forward to make a credit fit.
  Every age check — parking, replay dispatch, delivery in the routers and the
  API fallback — goes through `retire_expired_replay` / `pending_start_is_expired`,
  which add a rebuild in flight's wait the same way parking does; `IoEvent::ReplayStartPlayback`
  carries the whole request so a replay cannot mint a fresh window or reset its
  retry budget.
- The recovery handler is generic over `RecoveryBackend` only so a test can
  run the whole chain: drive `handle_streaming_recovery` with a scripted
  backend under `#[tokio::test(start_paused = true)]` and assert on what it
  published and dispatched. A test that calls `rebuild_with_retries` or
  `replay_recovery_intent` itself still passes with the handler's install or
  replay deleted.
