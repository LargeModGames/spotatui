### Apple Music

- **Ownership is a claim.** `App::claim_apple_music` sets `claimed` (and bumps
  `generation`) before anything is sent, so a media key during Music's launch
  cannot resume the player being handed off from. Leaving Music for another
  source goes through `Work::Handoff`: the worker pauses Music, re-reads until
  it really reports paused (`confirm_paused`: Music says "playing" for about
  0.35 s after a pause), and only then sends `IoEvent::AppleMusicHandoff` back
  to the pump. An acknowledged pause releases the claim, as does a typed error
  proving Music never received the event (Automation denied, -1743, a failed
  Music launch, or an `osascript` spawn failure); a failed handoff pause in those
  cases also sends the held start. Other failures and timeouts keep the claim,
  because the Apple Event may still have run.
- **Every worker result is checked before it is written.** Results carry the
  `generation` they were queued under and are dropped when it moved. A snapshot
  read while transport commands are queued, or within 1.5 s of one finishing
  (`pending_commands`, `commanded_at`), keeps the play state spotatui asked for
  instead of the stale one Music reports. A start Music ignored (`started:
  false`) clears the play intent only if `intent_revision` still matches, so a
  pause or resume pressed meanwhile survives.
- **Music queues nothing behind a track started by itself**: its next-track
  command does nothing and playback stops at the end of the track (only playing
  a whole playlist builds a queue). spotatui keeps the list a track was started
  from (`PlayingList`) and starts the neighbour by persistent id on next,
  previous and end of track (`step_apple_music`, end detected in
  `accept_apple_music_snapshot`). Shuffle follows Music's shuffle setting with
  a deal-once bag.
- **Never trigger a Music dialog.** At volume 0, or for a track it cannot play
  (cloud status "no longer available"/"error"/"removed" without a local file, or
  a file track whose file is gone), Music puts a modal in its own window that
  cancels every later start until someone dismisses it, and a terminal user
  cannot see it. So volume has a floor just above 0 (`setVolume` in
  `music.js` reads the level back: Music on macOS 27.2 stores 1 as 0, so it
  lands on 2 there), unplayable rows carry
  `playable: false` and are refused and skipped, and the script never activates
  or reveals Music (`open -g -j` launches it hidden).
- **The script is constant.** `music.js` takes every input as an argv value and
  never builds or evaluates code from them. Read lists with bulk property reads
  (`list.tracks.persistentID()` and so on): a per-item read costs a round trip
  per track. `LAUNCH_WAIT` bounds only the wait for Music to answer after launch;
  the real command then gets the full 8 s timeout (`macos.rs`). `process.rs`
  maps Apple Event errors (-1743 means the Automation permission is missing).
- **Errors are status messages** (`report_apple_music_error`), never
  `handle_error`: Music being busy or slow is not a failed CLI command.
- **Tests never drive the real Music app.** State logic is tested in
  `core/app/apple_music.rs` with parsed snapshots; `process.rs` tests use
  ordinary binaries from `PATH`. Behaviour against Music itself (dialogs,
  queueing, timing) was measured by hand on macOS 26.5 and 27; re-check it there
  after changing `music.js`.
