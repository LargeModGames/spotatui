use crate::core::app::{self, ActiveBlock, App, RouteId};
use crate::core::auth;
use crate::core::user_config::UserConfig;
#[cfg(any(feature = "audio-viz", feature = "audio-viz-cpal"))]
use crate::infra::audio;
#[cfg(feature = "discord-rpc")]
use crate::infra::discord_rpc;
#[cfg(all(feature = "mpris", target_os = "linux"))]
use crate::infra::mpris;
use crate::infra::network::IoEvent;
#[cfg(feature = "scripting")]
use crate::infra::scripting::ScriptEngine;
use crate::tui::event::{self, Key};
use crate::tui::handlers;
use crate::tui::ui;
use anyhow::{anyhow, Result};
use crossterm::{
  cursor::MoveTo,
  event::{
    DisableMouseCapture, EnableMouseCapture, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
  },
  execute,
  terminal::{supports_keyboard_enhancement, SetTitle},
  ExecutableCommand,
};
use log::info;
use ratatui::backend::Backend;
use std::{
  cmp::{max, min},
  io::stdout,
  sync::{atomic::AtomicU64, Arc},
  time::{Duration, Instant, SystemTime},
};
use tokio::sync::Mutex;

const DEFAULT_WINDOW_TITLE: &str = "spt - spotatui";

#[derive(Default)]
struct WindowTitleState {
  last_title: Option<String>,
}

#[cfg(feature = "discord-rpc")]
pub type DiscordRpcHandle = Option<discord_rpc::DiscordRpcManager>;
#[cfg(not(feature = "discord-rpc"))]
pub type DiscordRpcHandle = Option<()>;

#[cfg(all(feature = "mpris", target_os = "linux"))]
pub type MprisHandle = Option<Arc<mpris::MprisManager>>;
#[cfg(not(all(feature = "mpris", target_os = "linux")))]
pub type MprisHandle = Option<()>;

#[cfg(feature = "discord-rpc")]
#[derive(Clone, Debug, PartialEq)]
struct DiscordTrackInfo {
  title: String,
  artist: String,
  album: String,
  image_url: Option<String>,
  duration_ms: u32,
}

#[cfg(feature = "discord-rpc")]
#[derive(Default)]
struct DiscordPresenceState {
  last_track: Option<DiscordTrackInfo>,
  last_is_playing: Option<bool>,
  last_progress_ms: u128,
}

#[cfg(all(feature = "mpris", target_os = "linux"))]
#[derive(Default, PartialEq)]
struct MprisMetadata {
  title: String,
  artists: Vec<String>,
  album: String,
  duration_ms: u32,
  art_url: Option<String>,
}

#[cfg(all(feature = "mpris", target_os = "linux"))]
#[derive(Default)]
struct MprisState {
  last_metadata: Option<MprisMetadata>,
  last_is_playing: Option<bool>,
  last_shuffle: Option<bool>,
  last_loop: Option<mpris::LoopStatusEvent>,
}

#[cfg(feature = "discord-rpc")]
fn build_discord_playback(app: &App) -> Option<discord_rpc::DiscordPlayback> {
  let snapshot = crate::infra::media_metadata::current_playback_snapshot(app)?;
  let artist = snapshot.primary_artist();
  let track_info = DiscordTrackInfo {
    title: snapshot.metadata.title,
    artist,
    album: snapshot.metadata.album,
    image_url: snapshot.metadata.image_url,
    duration_ms: snapshot.metadata.duration_ms,
  };

  let base_state = if track_info.album.is_empty() {
    track_info.artist.clone()
  } else {
    format!("{} - {}", track_info.artist, track_info.album)
  };
  let state = if snapshot.is_playing {
    base_state
  } else if base_state.is_empty() {
    "Paused".to_string()
  } else {
    format!("Paused: {}", base_state)
  };

  Some(discord_rpc::DiscordPlayback {
    title: track_info.title,
    artist: track_info.artist,
    album: track_info.album,
    state,
    image_url: track_info.image_url,
    duration_ms: track_info.duration_ms,
    progress_ms: snapshot.progress_ms,
    is_playing: snapshot.is_playing,
  })
}

#[cfg(feature = "discord-rpc")]
fn update_discord_presence(
  manager: &discord_rpc::DiscordRpcManager,
  state: &mut DiscordPresenceState,
  app: &App,
) {
  let playback = build_discord_playback(app);

  match playback {
    Some(playback) => {
      let track_info = DiscordTrackInfo {
        title: playback.title.clone(),
        artist: playback.artist.clone(),
        album: playback.album.clone(),
        image_url: playback.image_url.clone(),
        duration_ms: playback.duration_ms,
      };

      let track_changed = state.last_track.as_ref() != Some(&track_info);
      let playing_changed = state.last_is_playing != Some(playback.is_playing);
      let progress_delta = playback.progress_ms.abs_diff(state.last_progress_ms);
      let progress_changed = progress_delta > 5000;

      if track_changed || playing_changed || progress_changed {
        manager.set_activity(&playback);
        state.last_track = Some(track_info);
        state.last_is_playing = Some(playback.is_playing);
        state.last_progress_ms = playback.progress_ms;
      }
    }
    None => {
      if state.last_track.is_some() {
        manager.clear();
        state.last_track = None;
        state.last_is_playing = None;
        state.last_progress_ms = 0;
      }
    }
  }
}

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
#[cfg(feature = "cover-art")]
fn cover_art_request_for(
  app: &App,
  snapshot: &crate::infra::media_metadata::PlaybackSnapshot,
) -> Option<crate::tui::cover_art::CoverArtRequest> {
  use crate::tui::cover_art::CoverArtRequest;

  #[cfg(feature = "local-files")]
  if let Some(local) = app.local_playback.as_ref() {
    let uri = local.queue.get(local.index)?;
    let path = crate::infra::local::file_uri_to_path(uri).ok()?;
    return Some(CoverArtRequest::LocalFile {
      key: uri.clone(),
      path,
    });
  }

  snapshot
    .metadata
    .image_url
    .clone()
    .map(CoverArtRequest::Url)
}

fn playback_window_title(app: &App) -> String {
  let Some(snapshot) = crate::infra::media_metadata::current_playback_snapshot(app) else {
    return DEFAULT_WINDOW_TITLE.to_string();
  };

  let title = sanitize_window_title_component(&snapshot.metadata.title);
  let artist_raw = sanitize_window_title_component(&snapshot.primary_artist());
  // Compose the artist segment with the em-dash separator, matching today's
  // `"{title} — {artist}"` output; omitted when there's no artist.
  let artist = if artist_raw.trim().is_empty() {
    String::new()
  } else {
    format!(" — {}", artist_raw)
  };
  app
    .user_config
    .format
    .window_title
    .render(&[&title, &artist])
}

fn sanitize_window_title_component(value: &str) -> String {
  value.chars().filter(|c| !c.is_control()).collect()
}

fn next_window_title(state: &mut WindowTitleState, app: &App) -> Option<String> {
  if !app.user_config.behavior.set_window_title {
    return state
      .last_title
      .take()
      .map(|_| DEFAULT_WINDOW_TITLE.to_string());
  }

  let title = playback_window_title(app);
  if state.last_title.as_ref() == Some(&title) {
    None
  } else {
    state.last_title = Some(title.clone());
    Some(title)
  }
}

fn reset_window_title(state: &mut WindowTitleState) -> Result<()> {
  if state
    .last_title
    .as_deref()
    .is_some_and(|title| title != DEFAULT_WINDOW_TITLE)
  {
    execute!(stdout(), SetTitle(DEFAULT_WINDOW_TITLE))?;
    state.last_title = None;
  }
  Ok(())
}

fn back_key_clears_playlist_filter(app: &mut App, active_block: ActiveBlock) -> bool {
  if active_block == ActiveBlock::TrackTable && app.is_playlist_track_filter_active() {
    app.clear_playlist_track_filter();
    true
  } else {
    false
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::{NativeTrackInfo, TrackTableContext};
  use rspotify::model::idtypes::PlaylistId;
  use std::{sync::mpsc::channel, time::SystemTime};

  fn app() -> App {
    let (tx, _rx) = channel();
    App::new(
      tx,
      crate::core::user_config::UserConfig::new(),
      Some(SystemTime::now()),
    )
  }

  #[test]
  fn playback_window_title_uses_current_native_track() {
    let mut app = app();
    app.is_streaming_active = true;
    app.native_track_info = Some(NativeTrackInfo {
      name: "The Track".to_string(),
      artists_display: "The Artist".to_string(),
      album: "The Album".to_string(),
      duration_ms: 180_000,
      kind: crate::core::app::NativeTrackKind::Track,
    });

    assert_eq!(playback_window_title(&app), "The Track — The Artist");
  }

  #[test]
  fn playback_window_title_strips_control_characters() {
    let mut app = app();
    app.is_streaming_active = true;
    app.native_track_info = Some(NativeTrackInfo {
      name: "The\x1b]2;Bad\x07 Track".to_string(),
      artists_display: "The\nArtist".to_string(),
      album: "The Album".to_string(),
      duration_ms: 180_000,
      kind: crate::core::app::NativeTrackKind::Track,
    });

    assert_eq!(playback_window_title(&app), "The]2;Bad Track — TheArtist");
  }

  #[test]
  fn playback_window_title_falls_back_without_playback() {
    let app = app();

    assert_eq!(playback_window_title(&app), DEFAULT_WINDOW_TITLE);
  }

  #[test]
  fn disabling_window_title_restores_default_once() {
    let mut app = app();
    let mut state = WindowTitleState {
      last_title: Some("The Track — The Artist".to_string()),
    };
    app.user_config.behavior.set_window_title = false;

    assert_eq!(
      next_window_title(&mut state, &app).as_deref(),
      Some(DEFAULT_WINDOW_TITLE)
    );
    assert_eq!(next_window_title(&mut state, &app), None);
  }

  #[test]
  fn back_key_clears_playlist_filter_before_navigation_pop() {
    let mut app = app();
    app.track_table.context = Some(TrackTableContext::MyPlaylists);
    app.playlist_track_table_id = Some(
      PlaylistId::from_id("37i9dQZF1DX4WYpdgoIcn6")
        .unwrap()
        .into_static(),
    );
    app.active_playlist_track_filter = Some("query".to_string());
    app.push_navigation_stack(RouteId::TrackTable, ActiveBlock::TrackTable);

    assert!(back_key_clears_playlist_filter(
      &mut app,
      ActiveBlock::TrackTable
    ));

    assert!(app.active_playlist_track_filter.is_none());
    assert_eq!(app.get_current_route().id, RouteId::TrackTable);
  }
}

#[cfg(all(feature = "mpris", target_os = "linux"))]
fn update_mpris_state(manager: &mpris::MprisManager, state: &mut MprisState, app: &App) {
  use rspotify::model::enums::RepeatState;

  // Local-file playback owns its own state and never populates the Spotify
  // playback context, so it takes a dedicated path that reads metadata, play
  // state, and position straight from the live local player. Skipped while the
  // native queue owns the sink: `local_playback` is then a *suspended* context,
  // so fall through to the snapshot path, which renders the queue slot and
  // clears its shuffle/repeat instead of publishing this context's stale modes.
  #[cfg(feature = "local-files")]
  if let Some(local) = app
    .local_playback
    .as_ref()
    .filter(|_| !app.queue_owns_playback())
  {
    use crate::infra::media_metadata::{select_media_metadata, LocalMediaMetadata};

    let is_playing = !local.player.is_paused();
    let position_ms = local.player.position().as_millis() as u64;

    // `select_media_metadata` is the single, unit-tested decision for which
    // source the OS integration follows; local always wins while it is active.
    let metadata = select_media_metadata(
      Some(LocalMediaMetadata {
        title: local.name.clone(),
        artists: vec![local.artists.clone()],
        album: local.album.clone(),
        duration_ms: local.duration_ms as u32,
      }),
      None,
    )
    .expect("local metadata is present");

    let new_metadata = MprisMetadata {
      title: metadata.title.clone(),
      artists: metadata.artists.clone(),
      album: metadata.album.clone(),
      duration_ms: metadata.duration_ms,
      art_url: metadata.image_url.clone(),
    };
    if state.last_metadata.as_ref() != Some(&new_metadata) {
      manager.set_metadata(
        &metadata.title,
        &metadata.artists,
        &metadata.album,
        metadata.duration_ms,
        metadata.image_url,
      );
      state.last_metadata = Some(new_metadata);
    }

    if state.last_is_playing != Some(is_playing) {
      manager.set_playback_status(is_playing);
      state.last_is_playing = Some(is_playing);
    }

    manager.set_position(position_ms);

    // Local playback carries the decoded shuffle/repeat modes; push them like
    // the snapshot branch below so keyboard and MPRIS toggles reach clients
    // (this dedicated branch returns before the snapshot path runs).
    if state.last_shuffle != Some(app.decoded_shuffle) {
      manager.set_shuffle(app.decoded_shuffle);
      state.last_shuffle = Some(app.decoded_shuffle);
    }
    let loop_status = match app.decoded_repeat {
      crate::infra::queue::RepeatMode::Off => mpris::LoopStatusEvent::None,
      crate::infra::queue::RepeatMode::Track => mpris::LoopStatusEvent::Track,
      crate::infra::queue::RepeatMode::Context => mpris::LoopStatusEvent::Playlist,
    };
    if state.last_loop != Some(loop_status) {
      manager.set_loop_status(loop_status);
      state.last_loop = Some(loop_status);
    }
    return;
  }

  if let Some(snapshot) = crate::infra::media_metadata::current_playback_snapshot(app) {
    let new_metadata = MprisMetadata {
      title: snapshot.metadata.title.clone(),
      artists: snapshot.metadata.artists.clone(),
      album: snapshot.metadata.album.clone(),
      duration_ms: snapshot.metadata.duration_ms,
      art_url: snapshot.metadata.image_url.clone(),
    };
    if state.last_metadata.as_ref() != Some(&new_metadata) {
      manager.set_metadata(
        &snapshot.metadata.title,
        &snapshot.metadata.artists,
        &snapshot.metadata.album,
        snapshot.metadata.duration_ms,
        snapshot.metadata.image_url.clone(),
      );
      state.last_metadata = Some(new_metadata);
    }

    if state.last_is_playing != Some(snapshot.is_playing) {
      manager.set_playback_status(snapshot.is_playing);
      state.last_is_playing = Some(snapshot.is_playing);
    }

    manager.set_position(snapshot.progress_ms as u64);

    if state.last_shuffle != Some(snapshot.shuffle) {
      manager.set_shuffle(snapshot.shuffle);
      state.last_shuffle = Some(snapshot.shuffle);
    }

    // A `None` repeat means the source has no repeat control (native queue,
    // radio); reset to `None` rather than leaving a stale Track/Playlist that a
    // prior decoded context pushed.
    let loop_status = match snapshot.repeat {
      Some(RepeatState::Track) => mpris::LoopStatusEvent::Track,
      Some(RepeatState::Context) => mpris::LoopStatusEvent::Playlist,
      Some(RepeatState::Off) | None => mpris::LoopStatusEvent::None,
    };
    if state.last_loop != Some(loop_status) {
      manager.set_loop_status(loop_status);
      state.last_loop = Some(loop_status);
    }
  } else if state.last_metadata.is_some() {
    manager.set_stopped();
    state.last_metadata = None;
    state.last_is_playing = None;
  }
}

#[cfg(feature = "streaming")]
async fn pause_native_playback_before_exit(app: &Arc<Mutex<App>>) {
  let player = {
    let mut app = app.lock().await;
    if !app.is_streaming_active {
      return;
    }

    let Some(player) = app.streaming_player.clone() else {
      return;
    };

    let is_playing = app.native_is_playing.unwrap_or_else(|| {
      app
        .current_playback_context
        .as_ref()
        .map(|context| context.is_playing)
        .unwrap_or(false)
    });

    if !is_playing {
      return;
    }

    app.native_is_playing = Some(false);
    if let Some(context) = app.current_playback_context.as_mut() {
      context.is_playing = false;
    }

    player
  };

  player.pause();
  tokio::time::sleep(std::time::Duration::from_millis(150)).await;
}

pub async fn start_ui(
  user_config: UserConfig,
  app: &Arc<Mutex<App>>,
  shared_position: Option<Arc<AtomicU64>>,
  mpris_manager: MprisHandle,
  discord_rpc_manager: DiscordRpcHandle,
) -> Result<()> {
  info!("ui thread initialized");
  #[cfg(not(feature = "discord-rpc"))]
  let _ = discord_rpc_manager;
  #[cfg(not(feature = "streaming"))]
  let _ = &shared_position;
  #[cfg(not(all(feature = "mpris", target_os = "linux")))]
  let _ = &mpris_manager;

  let mut terminal = ratatui::init();
  execute!(stdout(), EnableMouseCapture)?;
  let keyboard_enhancement_supported = supports_keyboard_enhancement().unwrap_or(false);
  let keyboard_enhancement_enabled = keyboard_enhancement_supported
    && execute!(
      stdout(),
      PushKeyboardEnhancementFlags(
        KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
          | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
          | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
      )
    )
    .is_ok();
  if keyboard_enhancement_enabled {
    info!("enabled keyboard enhancement flags");
  }
  {
    let mut app = app.lock().await;
    app.terminal_input_caps.keyboard_enhancement_supported = keyboard_enhancement_supported;
    app.terminal_input_caps.keyboard_enhancement_enabled = keyboard_enhancement_enabled;
    app.terminal_input_caps.ctrl_punct_reliable = app::CapabilityState::Unknown;
  }

  let events = event::Events::new(user_config.behavior.tick_rate_milliseconds);

  #[cfg(all(feature = "mpris", target_os = "linux"))]
  let mut prev_is_streaming_active = false;

  #[cfg(any(feature = "audio-viz", feature = "audio-viz-cpal"))]
  let mut audio_capture: Option<audio::AudioCaptureManager> = None;

  #[cfg(feature = "discord-rpc")]
  let mut discord_presence_state = DiscordPresenceState::default();

  #[cfg(all(feature = "mpris", target_os = "linux"))]
  let mut mpris_state = MprisState::default();

  let mut window_title_state = WindowTitleState::default();
  // Last track the shared detector fired lyrics for, so the lookup re-fires
  // only on an actual track change rather than every tick.
  let mut last_track_identity: Option<TrackIdentity> = None;
  // Cache key (URL / file URI) of the cover art last requested, so the per-tick
  // cover-art evaluation dispatches a fetch only when the resolved art changes.
  #[cfg(feature = "cover-art")]
  let mut last_cover_art_key: Option<String> = None;
  let mut is_first_render = true;

  // Throttled persistence of the active non-Spotify playback session, so it can
  // resume the exact song/position on the next launch (see
  // `core::persisted_playback`). `last_session_save` throttles the periodic
  // writes; `session_was_present` lets a Some -> None transition (queue ended,
  // switched to Spotify) clear the file so a stale session is never resurrected.
  let mut last_session_save: Option<Instant> = None;
  let mut session_was_present = false;
  const SESSION_SAVE_INTERVAL: Duration = Duration::from_secs(3);

  // Tracks whether the active internet-radio stream has produced audio yet, so
  // the tick can tell "stream just started, sink not filled" apart from "stream
  // died / drained" — both of which report `is_finished()` (empty sink).
  #[cfg(feature = "internet-radio")]
  let mut radio_stream_started = false;

  // The Lua VM (plus its HTTP client) is only constructed when the user
  // actually has script files; a zero-plugin install skips the engine — and
  // its per-tick `on_tick` dispatch — entirely.
  #[cfg(feature = "scripting")]
  let mut script_engine: Option<ScriptEngine> = {
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

  loop {
    let terminal_size = terminal.backend().size().ok();
    let title_update = {
      let mut app = app.lock().await;

      #[cfg(all(feature = "mpris", target_os = "linux"))]
      {
        let current_is_streaming_active = app.is_streaming_active;
        if prev_is_streaming_active && !current_is_streaming_active {
          if let Some(ref mpris) = mpris_manager {
            mpris.set_stopped();
          }
        }
        prev_is_streaming_active = current_is_streaming_active;
      }

      if let Some(size) = terminal_size {
        if is_first_render || app.size != size {
          app.help_menu_max_lines = 0;
          app.help_menu_offset = 0;
          app.help_menu_page = 0;
          app.size = size;

          let potential_limit = max((app.size.height as i32) - 13, 0) as u32;
          let max_limit = min(potential_limit, 50);
          let large_search_limit = min((f32::from(size.height) / 1.4) as u32, max_limit);
          let small_search_limit = min((f32::from(size.height) / 2.85) as u32, max_limit / 2);

          app.dispatch(IoEvent::UpdateSearchLimits(
            large_search_limit,
            small_search_limit,
          ));

          app.help_menu_max_lines = if app.size.height > 8 {
            (app.size.height as u32) - 8
          } else {
            0
          };
        }
      };

      let current_route = app.get_current_route();
      // The banner animates whenever the Home screen is displayed, regardless
      // of which block has focus (on Home the focused block is usually Empty
      // or Library, not Home), so gate the fast tick on the route. A disabled
      // banner gradient renders statically and needs no fast tick.
      let animation_active = (current_route.id == RouteId::Home
        && app.user_config.behavior.banner_gradient)
        || current_route.active_block == ActiveBlock::Analysis
        || app.liked_song_animation_frame.is_some();
      let current_tick_rate = if animation_active {
        app.user_config.behavior.animation_tick_rate_milliseconds
      } else {
        app.user_config.behavior.tick_rate_milliseconds
      };
      events.set_tick_rate(current_tick_rate);

      terminal.draw(|f| {
        use ratatui::{prelude::Style, widgets::Block};
        f.render_widget(
          Block::default().style(Style::default().bg(app.user_config.theme.background)),
          f.area(),
        );

        match current_route.active_block {
          ActiveBlock::HelpMenu => ui::draw_help_menu(f, &app),
          ActiveBlock::Queue => ui::draw_queue(f, &app),
          ActiveBlock::Party => {
            ui::draw_main_layout(f, &app);
            ui::draw_party(f, &app);
          }
          ActiveBlock::Error => ui::draw_error_screen(f, &app),
          ActiveBlock::SelectDevice => ui::draw_device_list(f, &app),
          ActiveBlock::Analysis => ui::audio_analysis::draw(f, &app),
          ActiveBlock::LyricsView => ui::draw_lyrics_view(f, &app),
          ActiveBlock::MiniPlayer => ui::draw_miniplayer(f, &app),
          #[cfg(feature = "cover-art")]
          ActiveBlock::CoverArtView => ui::draw_cover_art_view(f, &app),
          ActiveBlock::AnnouncementPrompt => ui::draw_announcement_prompt(f, &app),
          ActiveBlock::RecapPrompt => {
            ui::draw_main_layout(f, &app);
            ui::draw_recap_prompt(f, &app);
          }
          ActiveBlock::ExitPrompt => ui::draw_exit_prompt(f, &app),
          ActiveBlock::Settings => ui::settings::draw_settings(f, &app),
          ActiveBlock::PluginScreen => ui::draw_plugin_screen(f, &app),
          ActiveBlock::CreatePlaylistForm => {
            ui::draw_main_layout(f, &app);
            ui::draw_create_playlist_form(f, &app);
          }
          _ => ui::draw_main_layout(f, &app),
        }

        // Plugin popup overlays every screen.
        ui::draw_plugin_popup(f, &app);
      })?;

      if current_route.active_block == ActiveBlock::Input {
        terminal.show_cursor()?;
      } else {
        terminal.hide_cursor()?;
      }

      let cursor_offset = if app.size.height
        > crate::core::layout::small_terminal_height(&app.user_config.behavior)
      {
        2
      } else {
        1
      };

      terminal.backend_mut().execute(MoveTo(
        cursor_offset + app.input_cursor_position - app.input_scroll_offset.get(),
        cursor_offset,
      ))?;

      // Only refresh when a Spotify session exists; a free-source launch has no
      // token expiry and must not schedule refreshes.
      if let Some(expiry) = app.spotify_token_expiry {
        if auth::should_refresh_token_at(expiry, SystemTime::now()) && !app.auth_refresh_in_progress
        {
          app.auth_refresh_in_progress = true;
          app.dispatch(IoEvent::RefreshAuthentication);
        }
      }
      next_window_title(&mut window_title_state, &app)
    };
    if let Some(title) = title_update {
      execute!(stdout(), SetTitle(title.as_str()))?;
    }

    match events.next()? {
      event::Event::Input(key) => {
        let mut app = app.lock().await;
        if key == Key::Ctrl('c') {
          app.close_io_channel();
          break;
        }

        let current_active_block = app.get_current_route().active_block;

        if current_active_block == ActiveBlock::ExitPrompt {
          match key {
            Key::Enter | Key::Char('y') | Key::Char('Y') => {
              app.close_io_channel();
              break;
            }
            Key::Esc | Key::Char('n') | Key::Char('N') => {
              app.pop_navigation_stack();
            }
            _ if key == app.user_config.keys.back => {
              app.pop_navigation_stack();
            }
            _ => {}
          }
        } else if current_active_block == ActiveBlock::Input {
          handlers::input_handler(key, &mut app);
        } else if key == app.user_config.keys.back {
          if !back_key_clears_playlist_filter(&mut app, current_active_block) {
            if current_active_block == ActiveBlock::Settings {
              handlers::handle_app(key, &mut app);
            } else if app.get_current_route().active_block == ActiveBlock::AnnouncementPrompt {
              if let Some(dismissed_id) = app.dismiss_active_announcement() {
                app.user_config.mark_announcement_seen(dismissed_id);
                if let Err(error) = app.user_config.save_config() {
                  app.handle_error(anyhow!(
                    "Failed to persist dismissed announcement: {}",
                    error
                  ));
                }
              }

              if app.active_announcement.is_none() {
                app.pop_navigation_stack();
              }
            } else if app.get_current_route().active_block != ActiveBlock::Input {
              let pop_result = match app.pop_navigation_stack() {
                Some(ref x) if x.id == RouteId::Search => app.pop_navigation_stack(),
                Some(x) => Some(x),
                None => None,
              };
              if pop_result.is_none() {
                app.push_navigation_stack(RouteId::ExitPrompt, ActiveBlock::ExitPrompt);
              }
            }
          }
        } else {
          handlers::handle_app(key, &mut app);
        }
        #[cfg(feature = "scripting")]
        if let Some(engine) = script_engine.as_mut() {
          engine.run_pending_commands(&mut app);
        }
      }
      event::Event::Mouse(mouse) => {
        let mut app = app.lock().await;
        if !app.user_config.behavior.disable_mouse_inputs {
          handlers::mouse_handler(mouse, &mut app);
        }
      }
      event::Event::Tick(elapsed) => {
        #[cfg(all(feature = "macos-media", target_os = "macos"))]
        {
          use objc2_foundation::{NSDate, NSRunLoop};
          NSRunLoop::currentRunLoop().runUntilDate(&NSDate::dateWithTimeIntervalSinceNow(0.001));
        }

        let mut app = app.lock().await;
        app.update_on_tick(elapsed);

        #[cfg(feature = "streaming")]
        app.flush_pending_native_seek();
        app.flush_pending_api_seek();
        app.flush_pending_source_seek();
        app.flush_pending_volume();
        app.flush_config_save(false);

        #[cfg(feature = "scripting")]
        if let Some(engine) = script_engine.as_mut() {
          engine.on_tick(&mut app);
        }

        #[cfg(feature = "discord-rpc")]
        if let Some(ref manager) = discord_rpc_manager {
          update_discord_presence(manager, &mut discord_presence_state, &app);
        }

        #[cfg(all(feature = "mpris", target_os = "linux"))]
        if let Some(ref mpris) = mpris_manager {
          update_mpris_state(mpris, &mut mpris_state, &app);
        }

        // Shared track-change detector. One place decides "the playing track
        // changed" off the source-agnostic snapshot, then drives BOTH lyrics
        // (every source) and cover art (cover-art feature) — so both light up
        // for Spotify, local files, Subsonic, radio and YouTube through a single
        // path.
        {
          let snapshot = crate::infra::media_metadata::current_playback_snapshot(&app);

          // Lyrics fire once per track (identity latch): their inputs — title,
          // artist, duration — ARE the identity, so they are correct at the
          // instant the identity changes.
          let identity = snapshot.as_ref().map(track_identity);
          if identity != last_track_identity {
            last_track_identity = identity;
            match snapshot.as_ref() {
              Some(snapshot) => {
                use crate::infra::media_metadata::PlaybackItemKind;
                // LRCLIB lookup by title + artist + duration. Source agnostic;
                // radio (duration 0) simply resolves to "not found". Podcast
                // episodes have no lyrics, so skip the lookup and show the
                // not-found message rather than stale lyrics.
                if snapshot.item_kind == PlaybackItemKind::Track {
                  let title = snapshot.metadata.title.clone();
                  let artist = snapshot.primary_artist();
                  app.desired_lyrics_identity = Some((title.clone(), artist.clone()));
                  app.dispatch(IoEvent::GetLyrics(
                    title,
                    artist,
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
          #[cfg(feature = "cover-art")]
          {
            use crate::core::app::CoverArtStatus;
            let enabled = app
              .user_config
              .do_draw_cover_art(app.cover_art.full_image_support());
            let desired = if enabled {
              snapshot
                .as_ref()
                .and_then(|snapshot| cover_art_request_for(&app, snapshot))
            } else {
              None
            };
            match desired {
              Some(request) => {
                app.desired_cover_art_key = Some(request.key().to_string());
                if last_cover_art_key.as_deref() != Some(request.key()) {
                  last_cover_art_key = Some(request.key().to_string());
                  // Keep the previous image on screen until the new one
                  // resolves (smooth swap); the fetch runs off-lock.
                  app.cover_art_status = CoverArtStatus::Loading;
                  app.dispatch(IoEvent::FetchCoverArt(request));
                }
              }
              None => {
                app.desired_cover_art_key = None;
                // No art to show (radio, art disabled, nothing playing): drop
                // any stale image once, so the pane shows the placeholder.
                if last_cover_art_key.take().is_some() || app.cover_art.available() {
                  app.cover_art.clear();
                }
                app.cover_art_status = if enabled && snapshot.is_some() {
                  CoverArtStatus::Unavailable
                } else {
                  CoverArtStatus::NotStarted
                };
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
            Some(QueueNowPlaying::Decoded(d)) if d.player.is_finished() && !d.advancing => {
              d.advancing = true; // atomic check-and-set: one dispatch only
              true
            }
            _ => false,
          };
          if advance {
            app.dispatch(crate::infra::network::IoEvent::AdvanceNativeQueue);
          }
        }

        // Decoded-source auto-advance, one macro invocation per source (the
        // blocks are identical except for which `*_playback` session and queue
        // field they read). Each session reads its progress live from the
        // player at render time; the only state self-managed here is
        // end-of-track. When the sink drains and no track change is already in
        // flight (`!advancing`), `advance_decision` picks the move: advance /
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
              use crate::infra::queue::{advance_decision, next_index, Decision};
              let queue_len = $app.native_queue.len();
              let repeat = $app.decoded_repeat;
              let decision = $app.$playback.as_ref().map(|s| {
                advance_decision(
                  s.player.is_finished(),
                  s.advancing,
                  next_index(s.index, s.$queue.len()).is_some(),
                  queue_len,
                  repeat,
                )
              });
              match decision {
                Some(d @ (Decision::AdvanceContext | Decision::RepeatTrack)) => {
                  if let Some(s) = $app.$playback.as_mut() {
                    s.advancing = true; // atomic check-and-set: one dispatch only
                  }
                  $app.dispatch(if d == Decision::RepeatTrack {
                    crate::infra::network::IoEvent::ReplayCurrentTrack
                  } else {
                    crate::infra::network::IoEvent::NextTrack
                  });
                }
                Some(Decision::SuspendToQueue) => {
                  // End-of-track handoff: under Repeat One the context resumes
                  // the same track, so a queued song can't consume the repeat.
                  $app.suspend_active_decoded_context_for_skip(
                    crate::infra::queue::SuspendCause::AutoAdvance,
                  );
                  $app.dispatch(crate::infra::network::IoEvent::AdvanceNativeQueue);
                }
                Some(Decision::Teardown) => $app.$playback = None,
                Some(Decision::None) | None => {}
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
              radio_stream_started = true;
            } else if radio_stream_started {
              app.radio_playback = None;
              radio_stream_started = false;
              app.set_status_message("Radio stream ended", 4);
            }
          }
          None => radio_stream_started = false,
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
        if app.has_persistable_session() {
          let due = last_session_save
            .map(|t| t.elapsed() >= SESSION_SAVE_INTERVAL)
            .unwrap_or(true);
          if due {
            last_session_save = Some(Instant::now());
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
          session_was_present = true;
        } else {
          if session_was_present {
            last_session_save = None;
            tokio::task::spawn_blocking(|| {
              if let Ok(path) = crate::core::persisted_playback::default_session_path() {
                if let Err(e) = crate::core::persisted_playback::clear(&path) {
                  log::warn!("[session] failed to clear playback session: {e}");
                }
              }
            });
          }
          session_was_present = false;
        }

        #[cfg(feature = "streaming")]
        if !source_owns_playback {
          if let Some(ref pos) = shared_position {
            if app.is_streaming_active {
              let recently_seeked = app
                .last_native_seek
                .is_some_and(|t| t.elapsed().as_millis() < app::SEEK_POSITION_IGNORE_MS);

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
          if let Some(ref pos) = shared_position {
            if app.is_streaming_active {
              let position_ms = pos.load(std::sync::atomic::Ordering::Relaxed);
              if position_ms > 0 {
                app.song_progress_ms = position_ms as u128;
              }
            }
          }
        }

        #[cfg(any(feature = "audio-viz", feature = "audio-viz-cpal"))]
        {
          let in_analysis_view = app.get_current_route().active_block == ActiveBlock::Analysis;

          if in_analysis_view {
            if audio_capture.is_none() {
              audio_capture = audio::AudioCaptureManager::new();
              app.audio_capture_active = audio_capture.is_some();
            }

            if let Some(ref capture) = audio_capture {
              if let Some(spectrum) = capture.get_spectrum() {
                app.spectrum_data = Some(app::SpectrumData {
                  bands: spectrum.bands,
                  peak: spectrum.peak,
                });
                app.audio_capture_active = capture.is_active();
              }
            }
          } else if audio_capture.is_some() {
            audio_capture = None;
            app.audio_capture_active = false;
            app.spectrum_data = None;
          }
        }
      }
    }

    if is_first_render {
      let mut app = app.lock().await;
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
        RouteId::RecentlyPlayed if app.spotify_connected => {
          app.dispatch(IoEvent::GetRecentlyPlayed)
        }
        RouteId::AlbumList if app.spotify_connected => {
          app.dispatch(IoEvent::GetCurrentUserSavedAlbums(None))
        }
        RouteId::Artists if app.spotify_connected => {
          app.dispatch(IoEvent::GetFollowedArtists(None))
        }
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
      app.help_docs_size = ui::help::get_help_docs(&app).len() as u32;

      // `--play-file`: kick off local playback now that dispatch is wired.
      if let Some(uri) = app.pending_play_file.take() {
        app.dispatch(IoEvent::StartPlayback(Some(uri), None, None));
      }

      #[cfg(feature = "scripting")]
      if let Some(engine) = script_engine.as_mut() {
        engine.on_start(&mut app);
      }

      is_first_render = false;
    }
  }

  // Capture the exact final position of a non-Spotify session on a graceful
  // quit (the throttled in-loop save is up to a few seconds stale). Done
  // synchronously before teardown so the player is still alive to read from.
  {
    let session = app.lock().await.current_persisted_session();
    if let Some(session) = session {
      if let Ok(path) = crate::core::persisted_playback::default_session_path() {
        if let Err(e) = crate::core::persisted_playback::save(&path, &session) {
          log::warn!("[session] failed to persist playback session on exit: {e}");
        }
      }
    }
  }

  #[cfg(feature = "scripting")]
  if let Some(engine) = script_engine.as_mut() {
    let mut app = app.lock().await;
    engine.on_quit(&mut app);
  }

  // A volume/resize/shuffle change may still be inside its debounce window;
  // persist it before the process exits.
  {
    let mut app = app.lock().await;
    app.flush_config_save(true);
  }

  #[cfg(feature = "streaming")]
  pause_native_playback_before_exit(app).await;

  // Sync history to cloud on exit
  let sync_token_opt = {
    let app_guard = app.lock().await;
    app_guard.user_config.behavior.sync_token.clone()
  };

  if let Some(token) = sync_token_opt {
    info!("Synchronizing listening history to cloud before exit...");
    if let Err(e) = crate::infra::history::sync_history_to_cloud(&token).await {
      log::warn!("failed to run exit history cloud sync: {}", e);
    }
    if let Err(e) = crate::infra::history::clear_now_playing_from_cloud(&token).await {
      log::warn!("failed to clear now-playing on exit: {}", e);
    }
  }

  reset_window_title(&mut window_title_state)?;
  execute!(stdout(), DisableMouseCapture)?;
  if keyboard_enhancement_enabled {
    let _ = execute!(stdout(), PopKeyboardEnhancementFlags);
  }
  ratatui::restore();

  #[cfg(feature = "discord-rpc")]
  if let Some(ref manager) = discord_rpc_manager {
    manager.clear();
  }

  Ok(())
}
