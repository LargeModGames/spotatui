//! Shared terminal runtime for the interactive TUI.
//!
//! This module owns terminal lifecycle and the main UI event loop. Startup
//! orchestration, manager creation, and task spawning stay in `main.rs`.

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
use crate::tui::{handlers, ui};
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
use ratatui::{backend::Backend, prelude::Style, widgets::Block, DefaultTerminal, Frame};
use std::{
  cmp::{max, min},
  io::stdout,
  sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
  },
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

pub async fn start_ui(
  user_config: UserConfig,
  app: &Arc<Mutex<App>>,
  shared_position: Option<Arc<AtomicU64>>,
  mpris_manager: MprisHandle,
  discord_rpc_manager: DiscordRpcHandle,
) -> Result<()> {
  info!("ui thread initialized");
  #[cfg(not(feature = "discord-rpc"))]
  let _ = &discord_rpc_manager;
  #[cfg(not(all(feature = "mpris", target_os = "linux")))]
  let _ = &mpris_manager;

  let (mut terminal, keyboard_enhancement_enabled) = setup_terminal(&user_config, app).await?;
  let run_result = run_event_loop(
    &mut terminal,
    user_config,
    app,
    shared_position,
    &mpris_manager,
    &discord_rpc_manager,
  )
  .await;
  let restore_result = restore_terminal(keyboard_enhancement_enabled);
  clear_discord_presence(&discord_rpc_manager);

  if let Err(error) = run_result {
    restore_result?;
    return Err(error);
  }

  restore_result
}

async fn setup_terminal(
  user_config: &UserConfig,
  app: &Arc<Mutex<App>>,
) -> Result<(DefaultTerminal, bool)> {
  let terminal = ratatui::init();
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

  Ok((terminal, keyboard_enhancement_enabled))
}

fn restore_terminal(keyboard_enhancement_enabled: bool) -> Result<()> {
  execute!(stdout(), DisableMouseCapture)?;
  if keyboard_enhancement_enabled {
    let _ = execute!(stdout(), PopKeyboardEnhancementFlags);
  }
  ratatui::restore();
  Ok(())
}

fn clear_discord_presence(discord_rpc_manager: &DiscordRpcHandle) {
  #[cfg(not(feature = "discord-rpc"))]
  let _ = discord_rpc_manager;
  #[cfg(feature = "discord-rpc")]
  if let Some(ref manager) = *discord_rpc_manager {
    manager.clear();
  }
}

async fn run_event_loop(
  terminal: &mut DefaultTerminal,
  user_config: UserConfig,
  app: &Arc<Mutex<App>>,
  shared_position: Option<Arc<AtomicU64>>,
  mpris_manager: &MprisHandle,
  discord_rpc_manager: &DiscordRpcHandle,
) -> Result<()> {
  #[cfg(not(feature = "streaming"))]
  let _ = &shared_position;
  let _ = (mpris_manager, discord_rpc_manager);

  let events = event::Events::new(user_config.behavior.tick_rate_milliseconds);
  let mut is_first_render = true;
  #[cfg(all(feature = "mpris", target_os = "linux"))]
  let mut prev_is_streaming_active = false;
  #[cfg(any(feature = "audio-viz", feature = "audio-viz-cpal"))]
  let mut audio_capture: Option<audio::AudioCaptureManager> = None;
  #[cfg(feature = "discord-rpc")]
  let mut discord_presence_state = discord_rpc::DiscordPresenceState::default();
  #[cfg(all(feature = "mpris", target_os = "linux"))]
  let mut mpris_state = mpris::MprisState::default();

  loop {
    render_dispatch(
      terminal,
      app,
      is_first_render,
      #[cfg(all(feature = "mpris", target_os = "linux"))]
      &mpris_manager,
      #[cfg(all(feature = "mpris", target_os = "linux"))]
      &mut prev_is_streaming_active,
    )
    .await?;

    match events.next()? {
      event::Event::Input(key) => {
        if handle_input(key, app).await {
          break;
        }
      }
      event::Event::Mouse(mouse) => {
        handle_mouse(mouse, app).await;
      }
      event::Event::Tick => {
        tick_macos_runloop();
        handle_tick(
          app,
          &shared_position,
          #[cfg(feature = "discord-rpc")]
          discord_rpc_manager,
          #[cfg(feature = "discord-rpc")]
          &mut discord_presence_state,
          #[cfg(all(feature = "mpris", target_os = "linux"))]
          mpris_manager,
          #[cfg(all(feature = "mpris", target_os = "linux"))]
          &mut mpris_state,
          #[cfg(any(feature = "audio-viz", feature = "audio-viz-cpal"))]
          &mut audio_capture,
        )
        .await;
      }
    }

    bootstrap_first_render(app, &mut is_first_render).await;
  }

  Ok(())
}

async fn render_dispatch(
  terminal: &mut DefaultTerminal,
  app: &Arc<Mutex<App>>,
  is_first_render: bool,
  #[cfg(all(feature = "mpris", target_os = "linux"))] mpris_manager: &MprisHandle,
  #[cfg(all(feature = "mpris", target_os = "linux"))] prev_is_streaming_active: &mut bool,
) -> Result<()> {
  let terminal_size = terminal.backend().size().ok();
  {
    let mut app = app.lock().await;

    #[cfg(all(feature = "mpris", target_os = "linux"))]
    if let Some(ref manager) = *mpris_manager {
      let current_is_streaming_active = app.is_streaming_active;
      if *prev_is_streaming_active && !current_is_streaming_active {
        manager.set_stopped();
      }
      *prev_is_streaming_active = current_is_streaming_active;
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

        if app.size.height > 8 {
          app.help_menu_max_lines = (app.size.height as u32) - 8;
        } else {
          app.help_menu_max_lines = 0;
        }
      }
    }

    let active_block = app.get_current_route().active_block;
    terminal.draw(|frame| render_frame(frame, &app, active_block))?;

    if active_block == ActiveBlock::Input {
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
  }

  Ok(())
}

fn render_frame(frame: &mut Frame<'_>, app: &App, active_block: ActiveBlock) {
  frame.render_widget(
    Block::default().style(Style::default().bg(app.user_config.theme.background)),
    frame.area(),
  );

  match active_block {
    ActiveBlock::HelpMenu => ui::draw_help_menu(frame, app),
    ActiveBlock::Queue => ui::draw_queue(frame, app),
    ActiveBlock::Party => {
      ui::draw_main_layout(frame, app);
      ui::draw_party(frame, app);
    }
    ActiveBlock::Error => ui::draw_error_screen(frame, app),
    ActiveBlock::SelectDevice => ui::draw_device_list(frame, app),
    ActiveBlock::Analysis => ui::audio_analysis::draw(frame, app),
    ActiveBlock::LyricsView => ui::draw_lyrics_view(frame, app),
    #[cfg(feature = "cover-art")]
    ActiveBlock::CoverArtView => ui::draw_cover_art_view(frame, app),
    ActiveBlock::AnnouncementPrompt => ui::draw_announcement_prompt(frame, app),
    ActiveBlock::ExitPrompt => ui::draw_exit_prompt(frame, app),
    ActiveBlock::Settings => ui::settings::draw_settings(frame, app),
    ActiveBlock::CreatePlaylistForm => {
      ui::draw_main_layout(frame, app);
      ui::draw_create_playlist_form(frame, app);
    }
    _ => ui::draw_main_layout(frame, app),
  }
}

async fn handle_input(key: Key, app: &Arc<Mutex<App>>) -> bool {
  let mut app = app.lock().await;
  if key == Key::Ctrl('c') {
    app.close_io_channel();
    return true;
  }

  let current_active_block = app.get_current_route().active_block;

  if current_active_block == ActiveBlock::ExitPrompt {
    match key {
      Key::Enter | Key::Char('y') | Key::Char('Y') => {
        app.close_io_channel();
        return true;
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
        Some(ref route) if route.id == RouteId::Search => app.pop_navigation_stack(),
        Some(route) => Some(route),
        None => None,
      };
      if pop_result.is_none() {
        app.push_navigation_stack(RouteId::ExitPrompt, ActiveBlock::ExitPrompt);
      }
    }
  } else {
    handlers::handle_app(key, &mut app);
  }

  false
}

async fn handle_mouse(mouse: crossterm::event::MouseEvent, app: &Arc<Mutex<App>>) {
  let mut app = app.lock().await;
  if !app.user_config.behavior.disable_mouse_inputs {
    handlers::mouse_handler(mouse, &mut app);
  }
}

#[allow(clippy::too_many_arguments)]
async fn handle_tick(
  app: &Arc<Mutex<App>>,
  shared_position: &Option<Arc<AtomicU64>>,
  #[cfg(feature = "discord-rpc")] discord_rpc_manager: &DiscordRpcHandle,
  #[cfg(feature = "discord-rpc")] discord_presence_state: &mut discord_rpc::DiscordPresenceState,
  #[cfg(all(feature = "mpris", target_os = "linux"))] mpris_manager: &MprisHandle,
  #[cfg(all(feature = "mpris", target_os = "linux"))] mpris_state: &mut mpris::MprisState,
  #[cfg(any(feature = "audio-viz", feature = "audio-viz-cpal"))] audio_capture: &mut Option<
    audio::AudioCaptureManager,
  >,
) {
  let mut app = app.lock().await;
  app.update_on_tick();

  #[cfg(feature = "streaming")]
  app.flush_pending_native_seek();
  app.flush_pending_api_seek();
  app.flush_pending_volume();

  #[cfg(feature = "discord-rpc")]
  if let Some(ref manager) = *discord_rpc_manager {
    discord_rpc::update_presence(manager, discord_presence_state, &app);
  }

  #[cfg(all(feature = "mpris", target_os = "linux"))]
  if let Some(ref manager) = *mpris_manager {
    mpris::update_state(manager, mpris_state, &app);
  }

  update_song_progress_from_shared_position(&mut app, shared_position);

  if SystemTime::now() > app.spotify_token_expiry {
    app.dispatch(IoEvent::RefreshAuthentication);
  }

  #[cfg(any(feature = "audio-viz", feature = "audio-viz-cpal"))]
  update_audio_capture(&mut app, audio_capture);
}

fn update_song_progress_from_shared_position(
  app: &mut App,
  shared_position: &Option<Arc<AtomicU64>>,
) {
  #[cfg(feature = "streaming")]
  if let Some(ref position) = *shared_position {
    if app.is_streaming_active {
      let recently_seeked = app
        .last_native_seek
        .is_some_and(|instant| instant.elapsed().as_millis() < app::SEEK_POSITION_IGNORE_MS);

      if !recently_seeked {
        let position_ms = position.load(Ordering::Relaxed);
        if position_ms > 0 {
          app.song_progress_ms = position_ms as u128;
        }
      }
    }
  }

  #[cfg(not(feature = "streaming"))]
  if let Some(ref position) = *shared_position {
    if app.is_streaming_active {
      let position_ms = position.load(Ordering::Relaxed);
      if position_ms > 0 {
        app.song_progress_ms = position_ms as u128;
      }
    }
  }
}

#[cfg(any(feature = "audio-viz", feature = "audio-viz-cpal"))]
fn update_audio_capture(app: &mut App, audio_capture: &mut Option<audio::AudioCaptureManager>) {
  let in_analysis_view = app.get_current_route().active_block == ActiveBlock::Analysis;

  if in_analysis_view {
    if audio_capture.is_none() {
      *audio_capture = audio::AudioCaptureManager::new();
      app.audio_capture_active = audio_capture.is_some();
    }

    if let Some(ref capture) = *audio_capture {
      if let Some(spectrum) = capture.get_spectrum() {
        app.spectrum_data = Some(app::SpectrumData {
          bands: spectrum.bands,
          peak: spectrum.peak,
        });
        app.audio_capture_active = capture.is_active();
      }
    }
  } else if audio_capture.is_some() {
    *audio_capture = None;
    app.audio_capture_active = false;
    app.spectrum_data = None;
  }
}

async fn bootstrap_first_render(app: &Arc<Mutex<App>>, is_first_render: &mut bool) {
  if !*is_first_render {
    return;
  }

  let mut app = app.lock().await;
  app.dispatch(IoEvent::GetPlaylists);
  app.dispatch(IoEvent::GetUser);
  app.dispatch(IoEvent::GetCurrentPlayback);
  if app.user_config.behavior.enable_global_song_count {
    app.dispatch(IoEvent::FetchGlobalSongCount);
  }
  app.dispatch(IoEvent::FetchAnnouncements);
  app.help_docs_size = ui::help::get_help_docs(&app).len() as u32;
  *is_first_render = false;
}

fn tick_macos_runloop() {
  #[cfg(all(feature = "macos-media", target_os = "macos"))]
  {
    use objc2_foundation::{NSDate, NSRunLoop};
    NSRunLoop::currentRunLoop().runUntilDate(&NSDate::dateWithTimeIntervalSinceNow(0.001));
  }
}
