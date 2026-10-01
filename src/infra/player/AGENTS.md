### Native streaming (feature `streaming`, in `default`)

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
