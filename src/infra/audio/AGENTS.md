### Alternative sources (Local / Subsonic / Radio / YouTube / Qobuz)

- All three platforms play decoded sources. macOS was gated off in
  `LocalPlayer` until rodio 0.22 / cpal 0.17 were measured on CoreAudio; the
  SIGSEGVs behind that gate (#9/#20) were librespot's own rodio-backend, which
  is why librespot still uses portaudio-backend there and this player does not.
- Losing the output device has **two** shapes and only one is an error. cpal
  reports the device being *removed* (`DeviceNotAvailable`); it cannot report the
  common case - the OS moving its **default output** elsewhere (headphones out,
  AirPods in the case), which leaves the stream bound to a device nobody hears.
  So `LocalPlayer` both raises a `lost` flag from its own cpal error callback
  *and* remembers the device name it opened, comparing it against the current
  default in `device_lost()`. It then *refuses* `play_file`/`play_prepared`/
  `stop`/`seek`: those wait on rodio's audio callback with no timeout, and they
  run on the serial pump, so blocking one wedges the whole app rather than just
  falling silent. Every such wait also goes through `bounded()`, which re-asks
  the device every 3s rather than timing out blind - a dead device and a source
  stalled on the network are indistinguishable from the caller's side, and
  Qobuz's stream stall alone is 60s.
  The driver's tick polls `device_lost()` (one device read a second), calls
  `LocalPlayer::recover_device()` to rebuild on the new default device (a
  spaced, bounded retry - a Bluetooth output takes seconds to come back), and
  restages the track: it sets the session's `resume_at` (the seek, and paused
  only when `device_removed()` says cpal saw a *removal* or the session was
  already paused - a default that merely moved means the user plugged
  something in and keeps playing) and dispatches `ReplayCurrentTrack`. The
  path that stages the track applies `resume_at` itself
  (`infra::queue::restage`), so no `Seek` or `PausePlayback` is ever queued
  blind behind work that ends in `play()`. That recovery must stay *before*
  every advance block: a dead sink never drains, so `is_finished()` is false
  and would otherwise read as a still-playing track. The queue slot has no
  replay event: a removal clears `queue_slot_desired_playing`, which the
  paths that stage the next item and the suspended context's resume read,
  and a device given up on hands the slot to `FinishNativeQueue` (the same
  teardown a drained queue runs) rather than dropping `queue_now`. Every
  `LocalPlayer` wait runs off the `App` lock (`stop_detached`,
  `stop_detached_holding`, `spawn_blocking`): the runner takes that lock on
  every frame.
