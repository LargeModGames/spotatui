use super::*;

impl App {
  /// Dedupe keys for everything the DJ should not queue again: what is already
  /// waiting in the native queue, plus the track playing right now.
  ///
  /// The recently-played window is added by the caller from the taste brief;
  /// this covers only what `App` itself knows.
  #[cfg(feature = "dj-core")]
  #[cfg_attr(not(any(feature = "mcp-server", feature = "ai-dj")), allow(dead_code))]
  pub fn dj_skip_keys(&self) -> std::collections::HashSet<String> {
    use crate::infra::dj::dedupe_key;
    let mut keys: std::collections::HashSet<String> = self
      .native_queue
      .iter()
      .map(|track| dedupe_key(&track.name, &track.artists.join(", ")))
      .collect();
    if let Some(snapshot) = crate::infra::media_metadata::current_playback_snapshot(self) {
      keys.insert(dedupe_key(
        &snapshot.metadata.title,
        &snapshot.primary_artist(),
      ));
    }
    keys
  }

  /// Queue a batch of DJ-chosen tracks, returning how many were accepted.
  ///
  /// Routes every track through [`Self::add_track_to_native_queue`] rather than
  /// pushing into `native_queue` directly, so the no-URI guard, the radio
  /// rejection, and the external-Connect-device fallback to the Spotify Web API
  /// queue all still apply. That fallback is also why callers cap the batch:
  /// on an external device each track costs its own API round trip.
  ///
  /// The per-track status messages the single-track path emits are harmless
  /// here: they are set and replaced within one lock, before any draw, and the
  /// caller overwrites them with a single aggregate message afterwards.
  #[cfg(feature = "dj-core")]
  #[cfg_attr(not(any(feature = "mcp-server", feature = "ai-dj")), allow(dead_code))]
  pub fn extend_native_queue_from_dj(&mut self, tracks: Vec<TrackInfo>) -> usize {
    let mut accepted = 0usize;
    for track in tracks {
      // Mirror `add_track_to_native_queue`'s rejections so the count we report
      // is honest, rather than inferring them from a length change.
      let Some(uri) = track.uri.clone() else {
        continue;
      };
      if uri.starts_with("radio:") {
        continue;
      }
      let before = self.native_queue.len();
      self.add_track_to_native_queue(track);
      accepted += 1;
      // A Spotify track on an external Connect device is dispatched to the Web
      // API queue instead of pushed locally, so only remember what a vibe shift
      // would actually be able to drop again. Remembered by URI, not position:
      // the queue can be reordered, appended to, or pruned by hand before the
      // shift happens.
      if self.native_queue.len() > before {
        self.dj.queued_uris.insert(uri);
      }
    }
    accepted
  }

  /// Drop the DJ's own contributions from the queue.
  ///
  /// Used by a vibe shift: a new direction that only takes effect after the
  /// already-queued tracks have played reads as broken, so the DJ's own picks go
  /// and anything the user queued by hand stays — wherever it sits. Matching is
  /// by URI, so a track the user queued by hand *and* the DJ also picked goes
  /// too; that is the DJ's pick as much as theirs.
  #[cfg(feature = "dj-core")]
  #[cfg_attr(not(feature = "ai-dj"), allow(dead_code))]
  pub fn drop_dj_queued_tracks(&mut self) -> usize {
    let dj_uris = std::mem::take(&mut self.dj.queued_uris);
    if dj_uris.is_empty() {
      return 0;
    }
    let before = self.native_queue.len();
    self
      .native_queue
      .retain(|track| !track.uri.as_ref().is_some_and(|uri| dj_uris.contains(uri)));
    before - self.native_queue.len()
  }

  /// Open the DJ screen, warming the library index if the filter is already
  /// on.
  ///
  /// Every entry point goes through here so the crawl is never left to the
  /// resolve step. `behavior.dj_avoid_library: true` seeds the toggle without
  /// going through `toggle_dj_fresh_only`, so without this the first turn of such
  /// a session would crawl inline on the serial IoEvent lane — head-of-line
  /// blocking every other event behind a few seconds of pagination.
  /// Moved verbatim from the terminal DJ handler; reached through
  /// `Action::OpenLibrary(LibraryTarget::AiDj)`.
  #[cfg(feature = "ai-dj")]
  pub(super) fn open_ai_dj_screen(&mut self) {
    self.focus_dj_screen();
    // Asked once, on the first visit, and only here because `open` is the single
    // funnel every entry point goes through. Not in `first_run.rs`: that runs before
    // the TUI, gated on the absence of client.yml, and would interrogate every user
    // about coding agents even if they never open the DJ.
    //
    // An already-open picker is rebuilt rather than resumed. That state means the user
    // left the screen by a route the picker's own Esc never saw (a mouse click on the
    // sidebar), so its rows were detected against a session that has moved on.
    if self.dj.setup.is_some() || !self.user_config.behavior.dj_is_configured() {
      use crate::infra::dj::setup::DjSetup;
      self.dj.setup = Some(DjSetup::new(&self.user_config.behavior));
    }
  }

  /// Make the DJ screen current and focused, then start the library crawl.
  #[cfg(feature = "ai-dj")]
  fn focus_dj_screen(&mut self) {
    // Pushed only when the DJ is not already the current route: a second
    // `RouteId::AiDj` on the stack turns the Esc that should leave the DJ into
    // one that lands back on it. Reachable with the DJ already open because
    // focus can be elsewhere — Left to the sidebar, then the open key or the
    // sidebar's own "AI DJ" row — which is also why focus is restored rather
    // than left where it was.
    if self.get_current_route().id == RouteId::AiDj {
      self.set_current_route_state(Some(ActiveBlock::AiDj), Some(ActiveBlock::AiDj));
    } else {
      self.push_navigation_stack(RouteId::AiDj, ActiveBlock::AiDj);
    }
    self.request_dj_library_index();
  }

  /// Kick off the library crawl unless it has already run or is running.
  /// Shared by the screen opener and the "only tracks I do not already have"
  /// toggle in the terminal handler.
  #[cfg(feature = "ai-dj")]
  pub(crate) fn request_dj_library_index(&mut self) {
    if self.dj.avoid_library && self.dj.library.is_none() && !self.dj.library_indexing {
      self.dispatch(IoEvent::DjIndexLibrary);
    }
  }

  /// Ask the DJ for something, in the listener's own words. The input box is
  /// cleared only when the turn starts.
  #[cfg(feature = "ai-dj")]
  pub(crate) fn ask_dj(&mut self, text: String) {
    use crate::infra::dj::{AskDjRequest, DjLine, TurnKind};
    // Checked before the trim so a blank ask during a turn still reports busy.
    if self.dj.thinking {
      self.set_status_message("The DJ is still working on the last request", 3);
      return;
    }
    let text = text.trim();
    if text.is_empty() {
      return;
    }
    let text = text.to_string();
    self.dj.clear_input();

    // Jumping back to the newest line: the user just spoke, so that is what they
    // want to see.
    self.dj.scroll = 0;
    // A new instruction supersedes any refill already in flight.
    let generation = self.dj.bump_generation();
    let turn_seq = self.dj.begin_turn(TurnKind::Ask);
    self.dj.push_line(DjLine::user(text.clone()));

    // No `extra_instruction`: the line above *is* the instruction, and the brain
    // reads it from the transcript. Passing it here as well is what made the DJ see
    // the same request two and three times over.
    //
    // `dispatch_without_spinner` throughout the DJ actions: a brain call runs for
    // up to `dj_agent_timeout_secs`, and plain `dispatch` would pin the global
    // `is_loading` spinner for all of it. `dj.thinking` is the progress surface.
    self.dispatch_without_spinner(IoEvent::AskDj(Box::new(AskDjRequest {
      extra_instruction: None,
      generation,
      // The listener is right here, so the DJ may ask them something instead of
      // guessing.
      must_act: false,
      // Only if the turn queues: a turn spent answering "what have I been
      // listening to?" must not become the standing direction for every refill.
      vibe_on_success: Some(text),
      turn_seq,
    })));
  }

  /// Toggle continuous auto-queue, refilling at once if the queue is short.
  #[cfg(feature = "ai-dj")]
  pub(crate) fn toggle_dj_auto_queue(&mut self) {
    use crate::infra::dj::TurnKind;
    self.dj.auto_queue = !self.dj.auto_queue;
    // Either direction invalidates a *refill* in flight: switching off should drop
    // a pending one, and switching on starts a fresh one.
    //
    // A question the listener typed is not this toggle's to abandon. Bumping the
    // generation would make its answer land and be discarded, and clearing
    // `thinking` would let a new ask start a second brain call alongside the
    // first. Nothing is left unguarded by skipping both: no refill can be in
    // flight while an ask is (`wants_top_up` gates on the same flag), and later
    // refills are already stopped by `auto_queue` itself.
    let asked_turn_in_flight = self.dj.asked_turn_in_flight();
    let generation = if asked_turn_in_flight {
      self.dj.generation
    } else {
      self.dj.bump_generation()
    };
    if self.dj.auto_queue {
      // Same rule the driver tick applies, so the immediate refill and the ongoing
      // ones cannot disagree — on an external Connect device the native queue never
      // fills, and refilling on its length would queue a batch per brain call
      // forever. Said out loud, or auto-queue just looks broken on that device.
      let external = self.spotify_external_device_active();
      if external {
        self.set_status_message(
          "DJ auto-queue on — waits while another Spotify device is playing",
          6,
        );
      } else {
        self.set_status_message("DJ auto-queue on", 4);
      }
      if crate::infra::network::dj::wants_top_up(self.native_queue.len(), &self.dj, external) {
        let turn_seq = self.dj.begin_turn(TurnKind::Refill);
        self.dispatch_without_spinner(IoEvent::DjTopUp(generation, turn_seq));
      }
    } else {
      if !asked_turn_in_flight {
        // `finish_turn`, not a bare flag clear: it drops the step counter with the
        // flag. Left behind, that counter belongs to a turn nobody is waiting on,
        // and the next refill would paint it as its own progress until its first
        // step lands.
        self.dj.finish_turn(self.dj.turn_seq);
      }
      self.set_status_message("DJ auto-queue off", 4);
    }
  }

  /// Toggle "only tracks I do not already have", warming the index when on.
  #[cfg(feature = "ai-dj")]
  pub(crate) fn toggle_dj_fresh_only(&mut self) {
    self.dj.avoid_library = !self.dj.avoid_library;
    if self.dj.avoid_library {
      self.set_status_message("DJ: only tracks you don't already have", 4);
      self.request_dj_library_index();
    } else {
      self.set_status_message("DJ: recommending from everything", 4);
    }
  }

  /// Re-roll the direction: drop the DJ's own queued tracks and ask again.
  #[cfg(feature = "ai-dj")]
  pub(crate) fn dj_vibe_shift(&mut self) {
    use crate::infra::dj::{AskDjRequest, DjLine, TurnKind, MAX_BATCH};
    if self.dj.thinking {
      self.set_status_message("The DJ is already working on something", 3);
      return;
    }
    let generation = self.dj.bump_generation();
    let dropped = self.drop_dj_queued_tracks();
    let turn_seq = self.dj.begin_turn(TurnKind::Ask);
    self.dj.scroll = 0;
    self.dj.push_line(DjLine::system(if dropped > 0 {
      format!("Shifting the vibe (dropped {dropped} queued track(s))")
    } else {
      "Shifting the vibe".to_string()
    }));
    // A steer rather than a transcript line: the listener never typed this, and the
    // system line above already tells them what happened.
    self.dispatch_without_spinner(IoEvent::AskDj(Box::new(AskDjRequest {
      extra_instruction: Some(format!(
        "Change direction. Queue {MAX_BATCH} tracks that go somewhere different from what has been \
         playing, while still being something this listener would enjoy."
      )),
      generation,
      // A vibe shift has just emptied the queue, so it has to refill it rather than
      // stopping to ask what "different" means.
      must_act: true,
      vibe_on_success: None,
      turn_seq,
    })));
  }

  /// Show the DJ's backend/model picker, opening the DJ screen first when it
  /// is not current. Does not consult `dj_is_configured()`: the key press is
  /// the request.
  #[cfg(feature = "ai-dj")]
  pub(crate) fn open_dj_setup(&mut self) {
    use crate::infra::dj::setup::DjSetup;
    self.focus_dj_screen();
    self.dj.setup = Some(DjSetup::new(&self.user_config.behavior));
  }
}

#[cfg(all(test, feature = "dj-core"))]
mod tests {
  use super::*;
  use crate::core::app::test_support::*;

  /// A vibe shift drops the DJ's picks by identity, not by truncating a tail
  /// count: the DJ's tracks stop being a contiguous tail the moment the user
  /// queues by hand after a batch lands (or deletes a DJ pick from the queue
  /// screen), and the old count-based truncate took the user's track instead.
  #[cfg(feature = "dj-core")]
  #[test]
  fn a_vibe_shift_drops_the_djs_picks_and_keeps_what_the_user_queued_after_them() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));

    app.extend_native_queue_from_dj(vec![
      queue_track(Some("subsonic:track:dj1"), "DJ Pick 1"),
      queue_track(Some("subsonic:track:dj2"), "DJ Pick 2"),
    ]);
    // The desync case: the user queues by hand *after* the DJ's batch, so the
    // DJ's picks are no longer the queue's tail.
    app.add_track_to_native_queue(queue_track(Some("subsonic:track:mine"), "My Track"));

    assert_eq!(app.drop_dj_queued_tracks(), 2);
    assert_eq!(app.native_queue.len(), 1);
    assert_eq!(
      app.native_queue[0].name, "My Track",
      "the user's own pick must survive the shift"
    );

    // The set was consumed: a second shift with nothing new queued drops nothing.
    assert_eq!(app.drop_dj_queued_tracks(), 0);
  }

  /// Deleting a DJ pick from the queue screen must not make a later vibe shift
  /// overreach — the count-based version truncated by the stale count.
  #[cfg(feature = "dj-core")]
  #[test]
  fn a_dj_pick_removed_by_hand_does_not_widen_the_vibe_shift() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));

    app.add_track_to_native_queue(queue_track(Some("subsonic:track:mine"), "My Track"));
    app.extend_native_queue_from_dj(vec![
      queue_track(Some("subsonic:track:dj1"), "DJ Pick 1"),
      queue_track(Some("subsonic:track:dj2"), "DJ Pick 2"),
    ]);
    // The user deletes one DJ pick by hand (queue-screen removal).
    app.native_queue.retain(|track| track.name != "DJ Pick 2");

    assert_eq!(app.drop_dj_queued_tracks(), 1, "only the remaining DJ pick");
    assert_eq!(app.native_queue.len(), 1);
    assert_eq!(app.native_queue[0].name, "My Track");
  }
}
