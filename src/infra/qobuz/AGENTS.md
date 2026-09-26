### Alternative sources (Local / Subsonic / Radio / YouTube / Qobuz)

- Qobuz (`src/infra/qobuz/`) plays each track through the web player's
  encrypted CMAF stream while it downloads: `stream/progressive.rs` is a
  `stream-download` source that yields the decrypted segments (and restarts at
  a seek), rebuilt as a FLAC `NamedTempFile` the session keeps. The fetch runs
  off the pump behind a `fetch_id` guard; a superseded stream is dropped, which
  cancels its download. The transport (`sign.rs`, `stream/`) is pure and unit
  tested. The three web-player constants are scraped at runtime (`auth.rs`),
  cached in `state.yml`, and overridable through `SPOTATUI_QOBUZ_*` env vars;
  they are never embedded. Failures are status messages, never `handle_error`.
