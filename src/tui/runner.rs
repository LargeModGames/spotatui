use crate::core::app::{self, ActiveBlock, App, RouteId};
use crate::core::user_config::UserConfig;
#[cfg(any(feature = "audio-viz", feature = "audio-viz-cpal"))]
use crate::infra::audio;
#[cfg(feature = "discord-rpc")]
use crate::infra::discord_rpc;
#[cfg(all(feature = "mpris", target_os = "linux"))]
use crate::infra::mpris;
use crate::infra::network::IoEvent;
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
  time::SystemTime,
};
use tokio::sync::Mutex;

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

#[cfg(all(feature = "mpris", target_os = "linux"))]
fn update_mpris_state(manager: &mpris::MprisManager, state: &mut MprisState, app: &App) {
  use rspotify::model::enums::RepeatState;

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

    if let Some(repeat_state) = snapshot.repeat {
      let loop_status = match repeat_state {
        RepeatState::Off => mpris::LoopStatusEvent::None,
        RepeatState::Track => mpris::LoopStatusEvent::Track,
        RepeatState::Context => mpris::LoopStatusEvent::Playlist,
      };
      if state.last_loop != Some(loop_status) {
        manager.set_loop_status(loop_status);
        state.last_loop = Some(loop_status);
      }
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

  if user_config.behavior.set_window_title {
    execute!(stdout(), SetTitle("spt - spotatui"))?;
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

  let mut is_first_render = true;

  loop {
    let terminal_size = terminal.backend().size().ok();
    {
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
          #[cfg(feature = "cover-art")]
          ActiveBlock::CoverArtView => ui::draw_cover_art_view(f, &app),
          ActiveBlock::AnnouncementPrompt => ui::draw_announcement_prompt(f, &app),
          ActiveBlock::ExitPrompt => ui::draw_exit_prompt(f, &app),
          ActiveBlock::Settings => ui::settings::draw_settings(f, &app),
          ActiveBlock::CreatePlaylistForm => {
            ui::draw_main_layout(f, &app);
            ui::draw_create_playlist_form(f, &app);
          }
          _ => ui::draw_main_layout(f, &app),
        }
      })?;

      if current_route.active_block == ActiveBlock::Input {
        terminal.show_cursor()?;
      } else {
        terminal.hide_cursor()?;
      }

      let cursor_offset = if app.size.height > ui::util::SMALL_TERMINAL_HEIGHT {
        2
      } else {
        1
      };

      terminal.backend_mut().execute(MoveTo(
        cursor_offset + app.input_cursor_position - app.input_scroll_offset.get(),
        cursor_offset,
      ))?;

      if SystemTime::now() > app.spotify_token_expiry {
        app.dispatch(IoEvent::RefreshAuthentication);
      }
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
        } else {
          handlers::handle_app(key, &mut app);
        }
      }
      event::Event::Mouse(mouse) => {
        let mut app = app.lock().await;
        if !app.user_config.behavior.disable_mouse_inputs {
          handlers::mouse_handler(mouse, &mut app);
        }
      }
      event::Event::Tick => {
        #[cfg(all(feature = "macos-media", target_os = "macos"))]
        {
          use objc2_foundation::{NSDate, NSRunLoop};
          NSRunLoop::currentRunLoop().runUntilDate(&NSDate::dateWithTimeIntervalSinceNow(0.001));
        }

        let mut app = app.lock().await;
        app.update_on_tick();

        #[cfg(feature = "streaming")]
        app.flush_pending_native_seek();
        app.flush_pending_api_seek();
        app.flush_pending_volume();

        #[cfg(feature = "discord-rpc")]
        if let Some(ref manager) = discord_rpc_manager {
          update_discord_presence(manager, &mut discord_presence_state, &app);
        }

        #[cfg(all(feature = "mpris", target_os = "linux"))]
        if let Some(ref mpris) = mpris_manager {
          update_mpris_state(mpris, &mut mpris_state, &app);
        }

        #[cfg(feature = "streaming")]
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
        #[cfg(not(feature = "streaming"))]
        if let Some(ref pos) = shared_position {
          if app.is_streaming_active {
            let position_ms = pos.load(std::sync::atomic::Ordering::Relaxed);
            if position_ms > 0 {
              app.song_progress_ms = position_ms as u128;
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
      app.dispatch(IoEvent::GetPlaylists);
      app.dispatch(IoEvent::GetUser);
      app.dispatch(IoEvent::GetCurrentPlayback);
      if app.user_config.behavior.enable_global_song_count {
        app.dispatch(IoEvent::FetchGlobalSongCount);
      }
      app.dispatch(IoEvent::FetchAnnouncements);
      app.help_docs_size = ui::help::get_help_docs(&app).len() as u32;
      is_first_render = false;
    }
  }

  #[cfg(feature = "streaming")]
  pause_native_playback_before_exit(app).await;

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
