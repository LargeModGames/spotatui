### Alternative sources (Local / Subsonic / Radio / YouTube / Qobuz)

- Connect, headers, playlist bodies, format probing and startup buffering
  share one 25-second tune-in deadline. Headers and bodies have their own
  15-second caps, and probing plus buffering have a 10-second cap. Resolve at
  most one PLS/M3U document. Explicit audio Content-Types win, and bytes read
  from an ambiguous response are replayed to the decoder if it is audio.
- Start the format probe as bytes arrive. Then wait for about three seconds
  of unread audio or 64 KiB of unread raw bytes. Keep ICY metadata and probe
  reads out of the timed audio target.
- Symphonia can wait forever for a format it does not register, such as
  MPEG-2 ADTS AAC. Cancelling the download wakes a probe blocked inside
  `read`. Keep the sink out of the timed or background work.
- A full ring parks the stream-download writer until the next read, where it
  does not see cancellation. Dropping `RadioReader` cancels and reads one
  already-downloaded chunk to wake it, and the body ends once cancelled, so
  the connection closes even when nothing reads any more.
- User tune-in runs on the serial pump. Saved radio can prepare in the
  background. Only the pump stages or publishes the prepared session. A
  newer playback request or play/pause action discards it. The pump drops the
  pending work when it receives a superseding event, and `finish_radio`
  compares `App::playback_requests` under the lock before it takes the sink.
- Radio claims the sink (`App::claim_decoded_sink`) only once the station is
  prepared, right before it releases librespot, so a station that fails to
  connect leaves Spotify as it was. From that claim on, the usual rule holds:
  a failed player open or stage keeps the claim.
- A station switch keeps the old session in `radio_playback` and sets
  `App::set_radio_retuning` while the shared player restages, so the tick
  neither ends the session on the empty sink nor clears `last_session.yml`.
