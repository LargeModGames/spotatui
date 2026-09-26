### Alternative sources (Local / Subsonic / Radio / YouTube / Qobuz)

- Radio tune-in has **three** unbounded steps, all on the serial pump, and all
  three are capped in `infra/radio/stream.rs`: connect (`CONNECT_TIMEOUT`),
  response headers (`HEADER_TIMEOUT`), and the format probe (`PROBE_TIMEOUT`).
  The probe is the subtle one - rodio's symphonia scans for a start-of-stream
  marker and a live stream never ends, so a codec it does not register scans
  forever. Its `AdtsReader` claims only MPEG-4 ADTS (`ff f1`), not MPEG-2
  (`ff f9`), which much European radio uses. On timeout the fix is to call
  `OpenedStream::cancel` - stopping the *download*, not the reader: a probe
  waiting for bytes parks inside `read`, so a flag checked between reads is
  never seen, while cancelling marks the stream done, wakes every waiter, and
  lets the probe thread and its download go. Only `prepare_stream` runs inside
  the timed closure: `timeout` abandons a `spawn_blocking` closure but cannot
  stop it, and the radio player is shared, so a probe that matched just after
  the deadline would otherwise append the new station to the live sink. The
  `play_prepared` happens after the timeout check.
