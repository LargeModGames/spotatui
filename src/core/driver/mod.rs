//! The frontend-neutral scheduler ("the driver"): everything the app must do
//! on a timer, extracted from the terminal event loop so any frontend can run
//! playback correctly by calling [`Driver::tick`] at its tick rate.
//!
//! One tick advances playback state (`App::update_on_tick` plus the debounced
//! seek/volume/state flushes), refreshes the OAuth token, syncs OS presence
//! (Discord, MPRIS, window title), detects track changes (lyrics + cover
//! art), auto-advances the native queue and the decoded sources, persists the
//! non-Spotify session, and feeds the audio visualizer.
//!
//! What stays with the frontend, injected through [`TickEnv`] or kept at the
//! call site: the visualizer bar count (a function of frontend geometry), the
//! terminal image-protocol probe behind `TickEnv::cover_art_full_image_support`,
//! and the macOS `NSRunLoop` pump (the frontend owns the main thread).
//!
//! A frontend that stops ticking is a bug this module makes loud:
//! `App::playback_position_ms` reports stale once no tick has run for two
//! seconds, instead of leaving a silently frozen playbar.

mod plan;
mod presence;

use crate::core::app::{App, RouteId};
use crate::core::auth;
#[cfg(any(feature = "audio-viz", feature = "audio-viz-cpal"))]
use crate::infra::audio;
#[cfg(feature = "discord-rpc")]
use crate::infra::discord_rpc;
#[cfg(all(feature = "mpris", target_os = "linux"))]
use crate::infra::mpris;
use crate::infra::network::IoEvent;
#[cfg(feature = "scripting")]
use crate::infra::scripting::ScriptEngine;
#[cfg(feature = "scripting")]
use log::info;
use std::sync::{atomic::AtomicU64, Arc};
use std::time::{Duration, Instant, SystemTime};

#[cfg(feature = "discord-rpc")]
pub type DiscordRpcHandle = Option<discord_rpc::DiscordRpcManager>;
#[cfg(not(feature = "discord-rpc"))]
pub type DiscordRpcHandle = Option<()>;

#[cfg(all(feature = "mpris", target_os = "linux"))]
pub type MprisHandle = Option<Arc<mpris::MprisManager>>;
#[cfg(not(all(feature = "mpris", target_os = "linux")))]
pub type MprisHandle = Option<()>;

/// Identity of the currently-playing track, used by the shared track-change
/// detector to fire lyrics + cover-art fetches exactly once per track (rather
/// than every tick). Title + artists + album + duration distinguishes tracks
/// across every source without depending on a source-specific id.
type TrackIdentity = (String, Vec<String>, String, u32);

fn track_identity(snapshot: &crate::infra::media_metadata::PlaybackSnapshot) -> TrackIdentity {
  (
    snapshot.metadata.title.clone(),
    snapshot.metadata.artists.clone(),
    snapshot.metadata.album.clone(),
    snapshot.metadata.duration_ms,
  )
}

/// Resolve what cover art to fetch for the track described by `snapshot`.
///
/// Local files carry embedded artwork read straight from the file (no URL), so
/// they take a dedicated `LocalFile` request. Every other source that can supply
/// art (Spotify album art, YouTube thumbnail, Subsonic getCoverArt) surfaces it
/// as `snapshot.metadata.image_url`. `None` means the current track has no art
/// to show (e.g. internet radio, or a Spotify item without images).
#[cfg(feature = "art-decode")]
fn cover_art_request_for(
  app: &App,
  snapshot: &crate::infra::media_metadata::PlaybackSnapshot,
) -> Option<crate::core::art::CoverArtRequest> {
  use crate::core::art::CoverArtRequest;

  #[cfg(feature = "local-files")]
  if let Some(local) = app.local_playback.as_ref() {
    let uri = local.queue.get(local.index)?;
    let path = crate::infra::local::file_uri_to_path(uri).ok()?;
    return Some(CoverArtRequest::LocalFile {
      key: uri.clone(),
      path,
    });
  }
  #[cfg(not(feature = "local-files"))]
  let _ = app;

  snapshot
    .metadata
    .image_url
    .clone()
    .map(CoverArtRequest::Url)
}

/// Per-tick inputs only the frontend can supply.
pub struct TickEnv {
  /// Wall-clock instant of this tick; injected so the driver's throttling
  /// decisions stay deterministic under test.
  pub now: Instant,
  /// Visualizer bar count resolved from the frontend's geometry, or `None`
  /// while the analysis view is not showing (which releases the capture).
  #[cfg(any(feature = "audio-viz", feature = "audio-viz-cpal"))]
  pub viz_bars: Option<usize>,
  /// Whether the frontend renders real pixels for cover art (the terminal
  /// image-protocol probe); false in a decode-only build, where art is
  /// fetched solely for the adaptive theme.
  #[cfg(feature = "art-decode")]
  pub cover_art_full_image_support: bool,
}

/// The scheduler's own state: everything that used to live as locals of the
/// terminal event loop, one instance per frontend session.
pub struct Driver {
  /// Real-time position updates from the native player's event handler,
  /// read lock-free instead of waiting on the polled Spotify context.
  shared_position: Option<Arc<AtomicU64>>,
  #[cfg(all(feature = "mpris", target_os = "linux"))]
  mpris_manager: MprisHandle,
  #[cfg(feature = "discord-rpc")]
  discord_rpc_manager: DiscordRpcHandle,
  #[cfg(feature = "scripting")]
  script_engine: Option<ScriptEngine>,
  #[cfg(any(feature = "audio-viz", feature = "audio-viz-cpal"))]
  audio_capture: Option<audio::AudioCaptureManager>,
  /// Previous tick's `is_streaming_active`, so a native session that ends can
  /// push a final stopped state to MPRIS clients.
  #[cfg(all(feature = "mpris", target_os = "linux"))]
  prev_is_streaming_active: bool,
  #[cfg(feature = "discord-rpc")]
  discord_presence: presence::DiscordPresenceState,
  #[cfg(all(feature = "mpris", target_os = "linux"))]
  mpris_state: presence::MprisState,
  window_title: presence::WindowTitleState,
  /// Last track the shared detector fired lyrics for, so the lookup re-fires
  /// only on an actual track change rather than every tick.
  last_track_identity: Option<TrackIdentity>,
  /// Cache key (URL / file URI) of the cover art last requested, so the
  /// per-tick cover-art evaluation dispatches a fetch only when the resolved
  /// art changes.
  #[cfg(feature = "art-decode")]
  last_cover_art_key: Option<String>,
  /// Throttle state for `last_session.yml` (see `plan::session_persist_action`):
  /// `last_session_save` spaces the periodic writes; `session_was_present`
  /// lets a Some -> None transition (queue ended, switched to Spotify) clear
  /// the file so a stale session is never resurrected.
  last_session_save: Option<Instant>,
  session_was_present: bool,
  /// Whether the active internet-radio stream has produced audio yet, so the
  /// tick can tell "stream just started, sink not filled" apart from "stream
  /// died / drained" — both of which report `is_finished()` (empty sink).
  #[cfg(feature = "internet-radio")]
  radio_stream_started: bool,
}

impl Driver {
  pub fn new(
    shared_position: Option<Arc<AtomicU64>>,
    mpris_manager: MprisHandle,
    discord_rpc_manager: DiscordRpcHandle,
  ) -> Self {
    #[cfg(not(all(feature = "mpris", target_os = "linux")))]
    let _ = mpris_manager;
    #[cfg(not(feature = "discord-rpc"))]
    let _ = discord_rpc_manager;

    // The Lua VM (plus its HTTP client) is only constructed when the user
    // actually has script files; a zero-plugin install skips the engine — and
    // its per-tick `on_tick` dispatch — entirely.
    #[cfg(feature = "scripting")]
    let script_engine: Option<ScriptEngine> = {
      let config_dir = crate::core::user_config::default_app_config_dir();
      match config_dir {
        Some(config_dir) if ScriptEngine::has_user_scripts(&config_dir) => {
          match ScriptEngine::new() {
            Ok(mut engine) => {
              let loaded = engine.load_user_scripts(&config_dir);
              info!("loaded {loaded} lua plugin file(s)");
              Some(engine)
            }
            Err(e) => {
              log::error!("failed to initialize lua scripting engine: {e}");
              None
            }
          }
        }
        _ => {
          info!("no lua plugin files found; scripting engine not started");
          None
        }
      }
    };

    Driver {
      shared_position,
      #[cfg(all(feature = "mpris", target_os = "linux"))]
      mpris_manager,
      #[cfg(feature = "discord-rpc")]
      discord_rpc_manager,
      #[cfg(feature = "scripting")]
      script_engine,
      #[cfg(any(feature = "audio-viz", feature = "audio-viz-cpal"))]
      audio_capture: None,
      #[cfg(all(feature = "mpris", target_os = "linux"))]
      prev_is_streaming_active: false,
      #[cfg(feature = "discord-rpc")]
      discord_presence: presence::DiscordPresenceState::default(),
      #[cfg(all(feature = "mpris", target_os = "linux"))]
      mpris_state: presence::MprisState::default(),
      window_title: presence::WindowTitleState::default(),
      last_track_identity: None,
      #[cfg(feature = "art-decode")]
      last_cover_art_key: None,
      last_session_save: None,
      session_was_present: false,
      #[cfg(feature = "internet-radio")]
      radio_stream_started: false,
    }
  }

  /// One tick. The frontend calls this at its tick rate with the `App` lock
  /// held; everything slow (file I/O, network) is dispatched off-lock. Must
  /// run inside a Tokio runtime: session persistence goes through
  /// `tokio::task::spawn_blocking`, which panics without one.
  pub fn tick(&mut self, app: &mut App, elapsed: Duration, env: TickEnv) {
    #[cfg(feature = "streaming")]
    if let Some(player) = app.streaming_player.clone() {
      if let Some(error) = player.take_audio_backend_error() {
        app.pause_native_playback();
        app.set_status_message(format!("Audio backend failed: {error}"), 15);
      }
    }

    #[cfg(all(feature = "mpris", target_os = "linux"))]
    {
      let current_is_streaming_active = app.is_streaming_active;
      if self.prev_is_streaming_active && !current_is_streaming_active {
        if let Some(ref mpris) = self.mpris_manager {
          mpris.set_stopped();
        }
      }
      self.prev_is_streaming_active = current_is_streaming_active;
    }

    // Only refresh when a Spotify session exists; a free-source launch has no
    // token expiry and must not schedule refreshes. This lives on the tick so
    // every frontend gets it: one that skipped it would work for exactly one
    // token lifetime and then 401 forever.
    if let Some(expiry) = app.spotify_token_expiry {
      if auth::should_refresh_token_at(expiry, SystemTime::now()) && !app.auth_refresh_in_progress {
        app.auth_refresh_in_progress = true;
        app.dispatch(IoEvent::RefreshAuthentication);
      }
    }

    app.update_on_tick(elapsed);

    #[cfg(feature = "streaming")]
    app.flush_pending_native_seek();
    app.flush_pending_api_seek();
    app.flush_pending_source_seek();
    app.flush_pending_volume();
    app.flush_state_save(false);

    #[cfg(feature = "scripting")]
    if let Some(engine) = self.script_engine.as_mut() {
      engine.on_tick(app);
    }

    #[cfg(feature = "discord-rpc")]
    if let Some(ref manager) = self.discord_rpc_manager {
      presence::update_discord_presence(manager, &mut self.discord_presence, app);
    }

    #[cfg(all(feature = "mpris", target_os = "linux"))]
    if let Some(ref mpris) = self.mpris_manager {
      presence::update_mpris_state(mpris, &mut self.mpris_state, app);
    }

    // Shared track-change detector. One place decides "the playing track
    // changed" off the source-agnostic snapshot, then drives BOTH lyrics
    // (every source) and cover art (cover-art feature) — so both light up
    // for Spotify, local files, Subsonic, radio and YouTube through a single
    // path.
    {
      let snapshot = crate::infra::media_metadata::current_playback_snapshot(app);

      // Lyrics fire once per track (identity latch): their inputs — title,
      // artist, duration — ARE the identity, so they are correct at the
      // instant the identity changes.
      let identity = snapshot.as_ref().map(track_identity);
      if identity != self.last_track_identity {
        self.last_track_identity = identity;
        app.view.lyrics_view.reset();
        match snapshot.as_ref() {
          Some(snapshot) => {
            use crate::infra::media_metadata::PlaybackItemKind;
            // LRCLIB lookup by title + artist + duration. Source agnostic;
            // radio (duration 0) simply resolves to "not found". Podcast
            // episodes have no lyrics, so skip the lookup and show the
            // not-found message rather than stale lyrics.
            if snapshot.item_kind == PlaybackItemKind::Track {
              let title = snapshot.metadata.title.clone();
              // The identity latch keys on the joined display credit, but the
              // LRCLIB lookup needs the structured artist list so it can fall
              // back to the primary artist alone for collaborations (#410).
              let artists = snapshot.metadata.artists.clone();
              app.desired_lyrics_identity = Some((title.clone(), artists.join(", ")));
              app.dispatch(IoEvent::GetLyrics(
                title,
                artists,
                snapshot.metadata.duration_ms as f64 / 1000.0,
              ));
            } else {
              app.desired_lyrics_identity = None;
              app.lyrics = None;
              app.lyrics_status = crate::core::app::LyricsStatus::NotFound;
              app
                .plugin_data_generations
                .bump(crate::core::app::PluginDataKind::Lyrics);
            }
          }
          None => {
            app.desired_lyrics_identity = None;
            // Nothing is playing: reset so no stale lyrics linger.
            app.lyrics = None;
            app.lyrics_status = crate::core::app::LyricsStatus::NotStarted;
            app
              .plugin_data_generations
              .bump(crate::core::app::PluginDataKind::Lyrics);
          }
        }
      }

      // Cover art is re-evaluated EVERY tick against the desired image key,
      // NOT latched to the identity change. With native streaming the
      // snapshot's `image_url` comes from the polled Spotify context, which
      // catches up seconds *after* `native_track_info` flips the identity —
      // an identity-latched fetch would fire once with the previous track's
      // URL (or none at startup) and never see the real one, leaving the art
      // stuck or missing until restart. Comparing against
      // `last_cover_art_key` keeps this a no-op on quiet ticks and fires
      // exactly once whenever the resolved art actually changes.
      #[cfg(feature = "art-decode")]
      {
        use crate::core::art::CoverArtStatus;
        let enabled = app
          .user_config
          .needs_cover_art(env.cover_art_full_image_support);
        let desired = if enabled {
          snapshot
            .as_ref()
            .and_then(|snapshot| cover_art_request_for(app, snapshot))
        } else {
          None
        };
        match plan::cover_art_action(
          desired.as_ref().map(|request| request.key()),
          self.last_cover_art_key.as_deref(),
          app.cover_art.available(),
          enabled,
          snapshot.is_some(),
        ) {
          plan::CoverArtAction::Fetch => {
            let request = desired.expect("Fetch is only planned with a desired request");
            app.desired_cover_art_key = Some(request.key().to_string());
            self.last_cover_art_key = Some(request.key().to_string());
            // Keep the previous image on screen until the new one
            // resolves (smooth swap); the fetch runs off-lock.
            app.cover_art.status = CoverArtStatus::Loading;
            app.dispatch(IoEvent::FetchCoverArt(request));
          }
          plan::CoverArtAction::Keep => {
            app.desired_cover_art_key = desired.map(|request| request.key().to_string());
          }
          plan::CoverArtAction::Drop { clear, status } => {
            app.desired_cover_art_key = None;
            self.last_cover_art_key = None;
            // No art to show (radio, art disabled, nothing playing): drop
            // any stale image once, so the pane shows the placeholder.
            if clear {
              app.clear_cover_art();
            }
            app.cover_art.status = status;
          }
        }
      }
    }

    // Native queue slot: when the queued track finishes, advance the queue
    // (play the next queued item, or resume the suspended context). Runs
    // before the per-source blocks so it takes precedence over them.
    #[cfg(any(feature = "local-files", feature = "subsonic", feature = "youtube"))]
    {
      use crate::infra::queue::QueueNowPlaying;
      let advance = match app.queue_now.as_mut() {
        Some(QueueNowPlaying::Decoded(d))
          if plan::native_queue_advance_due(d.player.is_finished(), d.advancing) =>
        {
          d.advancing = true; // atomic check-and-set: one dispatch only
          true
        }
        _ => false,
      };
      if advance {
        app.dispatch(IoEvent::AdvanceNativeQueue);
      }
    }

    // Auto-queue refill. Dispatch only: this path holds `&mut App`, so the
    // brain call itself must happen on the detached service lane. The
    // generation goes along for the ride so a refill the user has since
    // abandoned (DJ off, vibe shift, source change) can be dropped when it
    // lands instead of queueing tracks for a session that is gone.
    #[cfg(feature = "ai-dj")]
    {
      if crate::infra::network::dj::wants_top_up(
        app.native_queue.len(),
        &app.dj,
        app.spotify_external_device_active(),
      ) {
        let turn_seq = app.dj.begin_turn(crate::infra::dj::TurnKind::Refill);
        let generation = app.dj.generation;
        // Not `dispatch`: that pins the global `is_loading` spinner until the
        // service-lane task finishes, which for a brain call is minutes. The
        // DJ's own `thinking` flag is the progress surface here.
        app.dispatch_without_spinner(IoEvent::DjTopUp(generation, turn_seq));
      }
    }

    // Decoded-source auto-advance, one macro invocation per source (the
    // blocks are identical except for which `*_playback` session and queue
    // field they read). Each session reads its progress live from the
    // player at render time; the only state self-managed here is
    // end-of-track. When the sink drains and no track change is already in
    // flight (`!advancing`), `plan::decoded_advance` picks the move: advance /
    // replay (repeat-one) / suspend to the native queue / tear down.
    //
    // Decide under one borrow, then act after the borrow ends. `advancing`
    // is set *synchronously here* (atomic check-and-set, before
    // dispatching) because the sink stays empty for the whole decode — or,
    // for Subsonic/YouTube, multi-second download — window; without it the
    // next tick would re-dispatch and skip several tracks per advance.
    #[cfg(any(feature = "local-files", feature = "subsonic", feature = "youtube"))]
    macro_rules! decoded_auto_advance {
      ($app:ident, $playback:ident, $queue:ident) => {
        if !$app.queue_owns_playback() {
          use crate::infra::queue::next_index;
          let queue_len = $app.native_queue.len();
          let repeat = $app.decoded_repeat;
          let advance = $app.$playback.as_ref().map(|s| {
            plan::decoded_advance(
              s.player.is_finished(),
              s.advancing,
              next_index(s.index, s.$queue.len()).is_some(),
              queue_len,
              repeat,
            )
          });
          match advance {
            Some(plan::DecodedAdvance::Dispatch { replay }) => {
              if let Some(s) = $app.$playback.as_mut() {
                s.advancing = true; // atomic check-and-set: one dispatch only
              }
              $app.dispatch(if replay {
                IoEvent::ReplayCurrentTrack
              } else {
                IoEvent::NextTrack
              });
            }
            Some(plan::DecodedAdvance::SuspendToQueue) => {
              // End-of-track handoff: under Repeat One the context resumes
              // the same track, so a queued song can't consume the repeat.
              $app.suspend_active_decoded_context_for_skip(
                crate::infra::queue::SuspendCause::AutoAdvance,
              );
              $app.dispatch(IoEvent::AdvanceNativeQueue);
            }
            Some(plan::DecodedAdvance::Teardown) => $app.$playback = None,
            Some(plan::DecodedAdvance::None) | None => {}
          }
        }
      };
    }
    #[cfg(feature = "local-files")]
    decoded_auto_advance!(app, local_playback, queue);
    #[cfg(feature = "subsonic")]
    decoded_auto_advance!(app, subsonic_playback, tracks);
    #[cfg(feature = "youtube")]
    decoded_auto_advance!(app, youtube_playback, tracks);

    // Apply any shuffle toggle that was deferred while a track change was in
    // flight, now that the advance may have committed (a cheap no-op when the
    // queue order already matches the decoded shuffle state).
    #[cfg(any(feature = "local-files", feature = "subsonic", feature = "youtube"))]
    app.reconcile_decoded_shuffle();

    // Internet radio has no queue to auto-advance; instead the tick watches
    // for a live stream that dies (server disconnect or the ring buffer
    // draining to EOF), which leaves `is_finished()` (empty sink) true while
    // the session was never paused. `is_finished()` is also true during the
    // brief pre-playback window before the first bytes arrive, so only tear
    // down once the stream has actually started producing audio.
    #[cfg(feature = "internet-radio")]
    match app.radio_playback.as_ref() {
      Some(radio) => {
        if !radio.player.is_finished() {
          self.radio_stream_started = true;
        } else if self.radio_stream_started {
          app.radio_playback = None;
          self.radio_stream_started = false;
          app.set_status_message("Radio stream ended", 4);
        }
      }
      None => self.radio_stream_started = false,
    }

    // A decoded non-Spotify source owns the sink while its `*_playback` is
    // `Some`. Drive `song_progress_ms` from its live player, and do NOT let
    // the (paused) librespot position below clobber it.
    #[allow(unused_mut)]
    let mut source_owns_playback = false;
    // The native queue slot owns the sink when playing a decoded track; read
    // progress from its player first (it may share the suspended context's
    // player, in which case a per-source block below reads the same value).
    #[cfg(any(feature = "local-files", feature = "subsonic", feature = "youtube"))]
    if let Some(crate::infra::queue::QueueNowPlaying::Decoded(d)) = app.queue_now.as_ref() {
      source_owns_playback = true;
      app.song_progress_ms = d.player.position().as_millis();
    }
    #[allow(unused_variables)]
    let spotify_queue_slot = app.queue_now_is_spotify();
    #[cfg(feature = "local-files")]
    if !spotify_queue_slot {
      if let Some(local) = app.local_playback.as_ref() {
        source_owns_playback = true;
        let position_ms = local.player.position().as_millis();
        app.song_progress_ms = position_ms;
      }
    }
    #[cfg(feature = "subsonic")]
    if !spotify_queue_slot {
      if let Some(subsonic) = app.subsonic_playback.as_ref() {
        source_owns_playback = true;
        let position_ms = subsonic.player.position().as_millis();
        app.song_progress_ms = position_ms;
      }
    }
    #[cfg(feature = "internet-radio")]
    if !spotify_queue_slot {
      if let Some(radio) = app.radio_playback.as_ref() {
        source_owns_playback = true;
        let position_ms = radio.player.position().as_millis();
        app.song_progress_ms = position_ms;
      }
    }
    #[cfg(feature = "youtube")]
    if !spotify_queue_slot {
      if let Some(youtube) = app.youtube_playback.as_ref() {
        source_owns_playback = true;
        let position_ms = youtube.player.position().as_millis();
        app.song_progress_ms = position_ms;
      }
    }

    // Persist the active non-Spotify session so it resumes on next launch.
    // Throttled to avoid churning the file every tick; a Some -> None
    // transition (queue ended, or switched to Spotify) clears it instead.
    let has_session = app.has_persistable_session();
    match plan::session_persist_action(
      has_session,
      self.session_was_present,
      self.last_session_save,
      env.now,
    ) {
      plan::SessionPersist::Save => {
        self.last_session_save = Some(env.now);
        // Snapshot (clones the queue) only when a save is actually due,
        // not on every tick.
        if let Some(session) = app.current_persisted_session() {
          // Fire-and-forget on the blocking pool: file I/O never blocks the
          // UI tick, and a dropped handle still runs to completion.
          tokio::task::spawn_blocking(move || {
            if let Ok(path) = crate::core::persisted_playback::default_session_path() {
              if let Err(e) = crate::core::persisted_playback::save(&path, &session) {
                log::warn!("[session] failed to persist playback session: {e}");
              }
            }
          });
        }
      }
      plan::SessionPersist::Clear => {
        self.last_session_save = None;
        tokio::task::spawn_blocking(|| {
          if let Ok(path) = crate::core::persisted_playback::default_session_path() {
            if let Err(e) = crate::core::persisted_playback::clear(&path) {
              log::warn!("[session] failed to clear playback session: {e}");
            }
          }
        });
      }
      plan::SessionPersist::None => {}
    }
    self.session_was_present = has_session;

    #[cfg(feature = "streaming")]
    if !source_owns_playback {
      if let Some(ref pos) = self.shared_position {
        if app.is_streaming_active {
          let recently_seeked = app
            .last_native_seek
            .is_some_and(|t| t.elapsed().as_millis() < crate::core::app::SEEK_POSITION_IGNORE_MS);

          if !recently_seeked {
            let position_ms = pos.load(std::sync::atomic::Ordering::Relaxed);
            if position_ms > 0 {
              app.song_progress_ms = position_ms as u128;
            }
          }
        }
      }
    }
    #[cfg(not(feature = "streaming"))]
    if !source_owns_playback {
      if let Some(ref pos) = self.shared_position {
        if app.is_streaming_active {
          let position_ms = pos.load(std::sync::atomic::Ordering::Relaxed);
          if position_ms > 0 {
            app.song_progress_ms = position_ms as u128;
          }
        }
      }
    }

    #[cfg(any(feature = "audio-viz", feature = "audio-viz-cpal"))]
    match env.viz_bars {
      Some(desired_bars) => {
        if self.audio_capture.is_none() {
          // Built at the count we are about to ask for, so the first frame
          // does not immediately throw the fresh cavacore plan away.
          self.audio_capture = audio::AudioCaptureManager::new(desired_bars);
          app.audio_capture_active = self.audio_capture.is_some();
        }

        if let Some(ref capture) = self.audio_capture {
          if let Some(spectrum) = capture.get_spectrum(desired_bars) {
            app.spectrum_data = Some(spectrum);
          }
          // Kept outside the spectrum arm: a dead stream must drop the
          // "Capturing audio" status instead of freezing it on.
          app.audio_capture_active = capture.is_active();
        }
      }
      None => {
        if self.audio_capture.is_some() {
          self.audio_capture = None;
          app.audio_capture_active = false;
          app.spectrum_data = None;
        }
      }
    }
  }

  /// The one-shot startup dispatch, fired by the frontend right after its
  /// first frame: initial Spotify fetches, the startup route's data, the
  /// persisted active source's sidebar data, and `--play-file`.
  pub fn dispatch_startup(&mut self, app: &mut App) {
    // Spotify-only startup fetches: skip them entirely when launched against a
    // free source with no Spotify session, so the network layer doesn't reject
    // three events with "connect Spotify" status flashes on every launch.
    if app.spotify_connected {
      app.dispatch(IoEvent::GetCurrentPlayback);
      app.dispatch(IoEvent::GetPlaylists);
      app.dispatch(IoEvent::GetUser);
    }
    // startup_route seeds the nav stack directly (App::new), bypassing the
    // handlers that normally fetch a screen's data on navigation — kick
    // off that fetch here or the screen renders empty until re-entered.
    // (Home needs nothing extra; Discover fetches from within its menu.)
    // Spotify-backed screens are gated on a connected session; Stats reads
    // local history so it always fetches.
    match app.get_current_route().id {
      RouteId::RecentlyPlayed if app.spotify_connected => app.dispatch(IoEvent::GetRecentlyPlayed),
      RouteId::AlbumList if app.spotify_connected => {
        app.dispatch(IoEvent::GetCurrentUserSavedAlbums(None))
      }
      RouteId::Artists if app.spotify_connected => app.dispatch(IoEvent::GetFollowedArtists(None)),
      RouteId::Podcasts if app.spotify_connected => {
        app.dispatch(IoEvent::GetCurrentUserSavedShows(None))
      }
      RouteId::Stats => {
        app.stats_loading = true;
        let period = app.stats_period;
        app.dispatch(IoEvent::LoadListeningStats(period));
      }
      _ => {}
    }
    // A persisted non-Spotify active source needs its sidebar data loaded
    // too (all of these are inert no-ops when the feature is off).
    match app.active_source {
      crate::core::source::Source::Local => app.dispatch(IoEvent::GetLocalPlaylists),
      crate::core::source::Source::Subsonic => app.dispatch(IoEvent::GetSubsonicPlaylists),
      crate::core::source::Source::Radio => app.dispatch(IoEvent::GetRadioStations),
      crate::core::source::Source::YouTube => app.dispatch(IoEvent::GetYouTubePlaylists),
      crate::core::source::Source::Spotify => {}
    }
    if app.user_config.behavior.enable_global_song_count {
      app.dispatch(IoEvent::FetchGlobalSongCount);
    }
    app.dispatch(IoEvent::FetchAnnouncements);

    // `--play-file`: kick off local playback now that dispatch is wired.
    if let Some(uri) = app.pending_play_file.take() {
      app.dispatch(IoEvent::StartPlayback(Some(uri), None, None));
    }

    #[cfg(feature = "scripting")]
    if let Some(engine) = self.script_engine.as_mut() {
      engine.on_start(app);
    }
  }

  /// Run any plugin commands a keypress queued; the frontend calls this after
  /// each key event. No-op without the `scripting` feature.
  pub fn run_pending_script_commands(&mut self, app: &mut App) {
    #[cfg(feature = "scripting")]
    if let Some(engine) = self.script_engine.as_mut() {
      engine.run_pending_commands(app);
    }
    #[cfg(not(feature = "scripting"))]
    let _ = app;
  }

  /// The window title the frontend should apply now, or `None` when it is
  /// unchanged. Called once per frame (cheaper than a tick, and a title
  /// change should not wait for one).
  pub fn next_window_title(&mut self, app: &App) -> Option<String> {
    presence::next_window_title(&mut self.window_title, app)
  }

  /// On teardown: the default title to restore if this session changed it.
  pub fn window_title_reset(&mut self) -> Option<&'static str> {
    presence::window_title_reset(&mut self.window_title)
  }

  /// The scripting quit hook. No-op without the `scripting` feature.
  pub fn on_quit(&mut self, app: &mut App) {
    #[cfg(feature = "scripting")]
    if let Some(engine) = self.script_engine.as_mut() {
      engine.on_quit(app);
    }
    #[cfg(not(feature = "scripting"))]
    let _ = app;
  }

  /// Clear any published rich presence on the way out.
  pub fn clear_presence(&self) {
    #[cfg(feature = "discord-rpc")]
    if let Some(ref manager) = self.discord_rpc_manager {
      manager.clear();
    }
  }
}
