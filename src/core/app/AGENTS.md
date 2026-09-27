### The `core/app/` module folder

`App` was one 10,920-line file; it is now 37 files. The struct stays **flat** - all
171 fields declared once in `src/core/app/mod.rs` (27 of them feature-gated), plus
the 81 presentation fields grouped in `App.view` - and its ~270 methods are split
across 36 sibling modules by concern, each with its own `impl App` block. The
boundaries are organizational, not architectural.

Rules when working in here:

- **Child modules open with `use super::*;`** and declare no other top-level
  imports; all external imports live in `mod.rs`. When a type is only importable
  under a feature that does not gate the function, use a **function-local** `use`
  instead of adding a top-level import.
- **A private helper called from a sibling module needs `pub(super)`**; one reached
  from `tui/` or `infra/` needs `pub(crate)`. Private *fields* need nothing - they
  are declared in `mod.rs` and visible to all descendants.
- New `impl App` methods go to the concern module that owns that state, not `mod.rs`.

Tests are colocated (`#[cfg(test)] mod tests`) in most - not all - modules. Shared
fixtures are `pub(super) fn`s in `test_support.rs`, imported as
`use crate::core::app::test_support::*;`.

### Playback ownership

- A decoded start (Local/Subsonic/Qobuz/Radio/YouTube) **parks** librespot when
  spotatui owned the sink (the active Connect device, a Spotify queue slot, a
  failed backend) and only **pauses** it otherwise (an idle device under a
  phone, a decoded queue slot over a Spotify context). A paused librespot keeps
  the native flag true, so driving it directly resumes the wrong player.
- A park (`App::release_native_for_decoded`, then `App::park_native_backend`)
  shuts librespot down, removes it from `App` and sets the private
  `native_parked` marker; the Spotify context and the recovery snapshot stay.
  A shut-down player never resumes. An explicit Spotify start (unless another
  device plays the cached playback), a queued Spotify item, Enter on the
  parked device row, or a bare resume while the parked device held the
  playback (`App::native_parked_here`) sends a reacquire request
  (`App::reacquire_parked_backend`), and the recovery loop rebuilds the
  backend and replays. Without the marker, a start after a park reaches the
  Web API and the saved-device retry, which can start the phone. The rebuild
  install refuses a new player while parked under a decoded owner
  (`App::accept_rebuilt_native_backend`). Any other transport on the parked
  device answers "Press play to resume Spotify", or "Reconnecting native
  streaming…" once its rebuild is pending.
- A decoded start claims the sink (`App::claim_decoded_sink`) before it pauses
  librespot, and `active_decoded_source()` reads the claim: the owner is
  `Decoded` from the first line of the start, through a failed start or a lost
  output device, until the source's queue runs out (the driver's teardown, the
  queue's exhausted-context resume) or an explicit Spotify start (a URI or a
  context) reaches `Network::start_playback`, which releases it. A bare resume
  never releases it, so a media key or Space during a download cannot resume
  librespot.
- Every hand-over of the sink away from librespot goes through
  `App::pause_native_playback` (the park calls it too), never a bare
  `player.pause()`: it clears the native play intent with the pause, so a
  backend rebuild under the new owner comes back idle instead of restoring
  Spotify over it. A path that loads librespot again afterwards re-arms the
  intent (`play_queued_spotify`, `resume_native_shuffle_session`), or the stall
  watchdog disarms on the false intent and a stalled load never rebuilds.
- The decoded *queue* path claims the sink as well (`release_librespot`).
  `resume_or_finish` releases that claim only where no decoded context resumes
  (nothing suspended, a Spotify context, a lost device). A resumed decoded
  context keeps it: only `start_*_queue` sets the claim, `play_index` does not.
- A native entry point asks one of two predicates before it drives librespot.
  `App::native_should_drive()` is false under a decoded owner and true under a
  Spotify queue slot, whose track librespot plays.
  `App::native_context_should_drive()` is also false under any queue slot; it
  guards the paths that restore or continue the *cached* context (the restore,
  the end-of-track continuation, the shuffle-session handlers). The recovery
  rebuild itself is never refused: every sender removes the player before it
  sends, so a refusal there loses the backend for the process.
- While the native queue slot owns the sink, `current_playback_context` names the
  *suspended* context's track. Inside `core/app/`, resolve the playing *track*
  through `App::playing_item()` (`core/app/playback_routing.rs`): it answers
  with the slot's `TrackInfo` (`uri`, `album_id`, `artist_refs`; a slot has
  no play context) and refuses a decoded owner. Outside `core/app/` read the
  `PlaybackSnapshot` (`infra/media_metadata.rs`), never the field:
  `direct_playback_context_reads` counts every other reader and may only fall.
- Radio is in `active_decoded_source` but deliberately out of
  `active_queueable_decoded_source` (repeat/shuffle) and
  `active_source_position_ms` (seek).

### Errors

- Nothing else clears `api_error`. `update_on_tick` retires it once the
  lifetime passes, handing the text to the status bar only when the error frame
  is the current screen, so a frontend with no dismissal gesture does not latch
  the first failure forever. The CLI never ticks, so its latch is intact.
- Demote a site to `set_error_status_message` only when all three hold: it
  fires from the tick or a self-refreshing retry loop (so every retry restamps
  the lifetime and the backstop can never win), it is provably unreachable as a
  CLI exit signal, and the failed operation is bookkeeping rather than the thing
  the user asked for. `flush_state_save` is the only site that qualifies today,
  and it latches its report to once per failure run - repeating it at the retry
  rate would hold `status_message_is_error` and silently drop every ordinary
  status message for the rest of the session.
