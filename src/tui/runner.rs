use crate::core::app::{self, ActiveBlock, App, RouteId};
use crate::core::driver::{DiscordRpcHandle, Driver, MprisHandle, TickEnv};
use crate::core::user_config::UserConfig;
use crate::infra::network::IoEvent;
use crate::tui::event::{self, Key};
use crate::tui::handlers;
use crate::tui::ui;
use anyhow::{anyhow, Result};
use crossterm::{
  event::{
    DisableMouseCapture, EnableMouseCapture, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
  },
  execute,
  terminal::{supports_keyboard_enhancement, SetTitle},
};
use log::info;
use ratatui::backend::Backend;
use std::{
  cmp::{max, min},
  io::stdout,
  sync::{atomic::AtomicU64, Arc},
  time::Instant,
};
use tokio::sync::Mutex;

/// Whether the terminal renders real pixels. False until the renderer is
/// initialized, and always false in a decode-only build (`art-decode` without
/// `cover-art`), where art is fetched solely for the adaptive theme.
#[cfg(feature = "art-decode")]
fn cover_art_full_image_support() -> bool {
  #[cfg(feature = "cover-art")]
  {
    crate::tui::cover_art::full_image_support()
  }
  #[cfg(not(feature = "cover-art"))]
  {
    false
  }
}

fn back_key_clears_playlist_filter(app: &mut App, active_block: ActiveBlock) -> bool {
  if active_block == ActiveBlock::TrackTable && app.is_playlist_track_filter_active() {
    app.clear_playlist_track_filter();
    true
  } else {
    false
  }
}

/// The runner normally handles the configurable back key before `handle_app`.
/// Help's inline filter must get first chance while it is being edited (so `q`
/// can be query text), when opening the filter, and when Esc should clear a
/// confirmed filter rather than close Help.
fn help_menu_captures_key_before_back(app: &App, key: Key) -> bool {
  app.get_current_route().active_block == ActiveBlock::HelpMenu
    && (app.view.help_filter_editing
      || key == app.user_config.keys.search
      || (key == Key::Esc && !app.view.help_filter.is_empty()))
}

/// The DJ screen is a typing surface, exactly like `ActiveBlock::Input`, and the
/// runner's back-key branch has to leave it alone for the same reason: the default
/// back key is `q`, and both the prompt ("play me some Queen") and the picker's
/// free-text model step ("qwen2.5-coder") need that character to be text. Without
/// this the key never reaches `handle_app`, so the picker's "swallows everything"
/// guarantee is void — the route pops out from under an open modal.
///
/// Leaving the screen still works: `ai_dj::handler` pops the stack on Esc once
/// there is nothing left to abandon.
#[cfg(feature = "ai-dj")]
fn dj_captures_key_before_back(app: &App) -> bool {
  app.get_current_route().active_block == ActiveBlock::AiDj
}

/// Blocks that must see the configurable back key *before* the runner acts on it
/// themselves. Ordering is the whole point, so this is one predicate rather than
/// two conditions spelled out at the call site: a screen that is missing here has
/// its route popped instead of receiving the key, however well its own handler
/// copes with it.
fn key_reaches_handlers_before_back(app: &App, key: Key) -> bool {
  #[cfg(feature = "ai-dj")]
  if dj_captures_key_before_back(app) {
    return true;
  }
  help_menu_captures_key_before_back(app, key)
}

/// One keypress, from the runner's event loop. Returns `true` when the user asked
/// to quit and the loop must break.
///
/// Extracted from the loop so the ordering of these branches is testable: the bug
/// this guards against is not "does the DJ handler cope with `q`" but "does `q`
/// ever get as far as the DJ handler". `Key::Ctrl('c')` stays at the call site,
/// because it is an unconditional quit that predates any of this.
fn dispatch_key(key: Key, app: &mut App) -> bool {
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
    handlers::input_handler(key, app);
  } else if key_reaches_handlers_before_back(app, key) {
    handlers::handle_app(key, app);
  } else if key == app.user_config.keys.back {
    if !back_key_clears_playlist_filter(app, current_active_block) {
      if current_active_block == ActiveBlock::Settings {
        handlers::handle_app(key, app);
      } else if app.get_current_route().active_block == ActiveBlock::AnnouncementPrompt {
        if let Some(dismissed_id) = app.dismiss_active_announcement() {
          app.runtime_state.mark_announcement_seen(dismissed_id);
          let patch = crate::core::state::PersistedRuntimeState::announcements(
            &app.runtime_state.seen_announcement_ids,
            &app.runtime_state.dismissed_announcements,
          );
          if let Err(error) = app.save_runtime_state(&patch) {
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
    handlers::handle_app(key, app);
  }
  false
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::TrackTableContext;
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

  // Both help tests assert through `key_reaches_handlers_before_back`, the function
  // the runner actually consults, rather than through
  // `help_menu_captures_key_before_back` directly. On the Help route the two agree,
  // but only the outer one fails when Help stops being wired into the gate at all,
  // which is the way this protection would realistically be lost.
  #[test]
  fn help_filter_captures_back_key_while_editing() {
    let mut app = app();
    app.push_navigation_stack(RouteId::HelpMenu, ActiveBlock::HelpMenu);
    app.view.help_filter_editing = true;

    assert!(key_reaches_handlers_before_back(
      &app,
      app.user_config.keys.back
    ));
  }

  #[test]
  fn confirmed_help_filter_captures_escape_but_not_normal_back_key() {
    let mut app = app();
    app.push_navigation_stack(RouteId::HelpMenu, ActiveBlock::HelpMenu);
    app.view.help_filter = "volume".to_string();

    assert!(key_reaches_handlers_before_back(&app, Key::Esc));
    assert!(!key_reaches_handlers_before_back(
      &app,
      app.user_config.keys.back
    ));
  }

  /// The default back key is `q`, and the DJ's free-text model step exists so an
  /// Ollama user can type `qwen2.5-coder`. If the runner claims the key first, the
  /// first character of that name closes the screen instead.
  #[cfg(feature = "ai-dj")]
  #[test]
  fn the_back_key_types_into_the_dj_picker_instead_of_closing_the_screen() {
    use crate::infra::dj::setup::{DjSetup, DjSetupStep};

    let mut app = app();
    app.push_navigation_stack(RouteId::AiDj, ActiveBlock::AiDj);
    let mut setup = DjSetup::new(&app.user_config.behavior);
    setup.step = DjSetupStep::Custom;
    setup.custom.clear();
    app.dj.setup = Some(setup);

    assert_eq!(
      app.user_config.keys.back,
      Key::Char('q'),
      "the default this test is about"
    );
    assert!(!dispatch_key(app.user_config.keys.back, &mut app));

    assert_eq!(app.dj.setup.as_ref().unwrap().custom, "q");
    assert_eq!(
      app.get_current_route().id,
      RouteId::AiDj,
      "and the picker is still open on the screen it belongs to"
    );
  }

  /// The other half of the same rule: exempting the DJ must not exempt everything
  /// else. `q` on an ordinary screen still goes back.
  #[test]
  fn the_back_key_still_pops_the_route_outside_the_dj() {
    let mut app = app();
    app.push_navigation_stack(RouteId::TrackTable, ActiveBlock::TrackTable);

    assert!(!dispatch_key(app.user_config.keys.back, &mut app));

    assert_ne!(app.get_current_route().id, RouteId::TrackTable);
  }

  /// `y` at the exit prompt is the one keypress that ends the process, and the
  /// runner's loop reads that from this return value now that the chain is a
  /// function.
  #[test]
  fn confirming_the_exit_prompt_asks_the_loop_to_break() {
    let mut app = app();
    app.push_navigation_stack(RouteId::ExitPrompt, ActiveBlock::ExitPrompt);

    assert!(dispatch_key(Key::Char('y'), &mut app));
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
  history_collector: crate::infra::history::HistoryCollectorHandle,
) -> Result<()> {
  info!("ui thread initialized");
  // The driver owns everything the app must do on a timer (see
  // `core::driver`); this loop's job shrinks to drawing frames, reading
  // events, and calling `driver.tick` at the configured tick rate.
  let mut driver = Driver::new(shared_position, mpris_manager, discord_rpc_manager);

  let mut terminal = ratatui::init();
  // Probe the terminal's image protocol only now that the terminal is set up;
  // `App` construction must not touch stdout.
  #[cfg(feature = "cover-art")]
  crate::tui::cover_art::init_renderer();
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
    app.view.terminal_input_caps.keyboard_enhancement_supported = keyboard_enhancement_supported;
    app.view.terminal_input_caps.keyboard_enhancement_enabled = keyboard_enhancement_enabled;
    app.view.terminal_input_caps.ctrl_punct_reliable = app::CapabilityState::Unknown;
  }

  let events = event::Events::new(user_config.behavior.tick_rate_milliseconds);

  let mut is_first_render = true;

  loop {
    let terminal_size =
      terminal
        .backend()
        .size()
        .ok()
        .map(|size| crate::core::geometry::Viewport {
          width: size.width,
          height: size.height,
        });
    let title_update = {
      let mut app = app.lock().await;

      if let Some(size) = terminal_size {
        if is_first_render || app.view.size != size {
          app.view.help_menu_offset = 0;
          app.view.help_menu_page = 0;
          app.view.size = size;

          let potential_limit = max((size.height as i32) - 13, 0) as u32;
          let max_limit = min(potential_limit, 50);
          let large_search_limit = min((f32::from(size.height) / 1.4) as u32, max_limit);
          let small_search_limit = min((f32::from(size.height) / 2.85) as u32, max_limit / 2);

          app.dispatch(IoEvent::UpdateSearchLimits(
            large_search_limit,
            small_search_limit,
          ));

          app.view.help_menu_max_lines = (size.height as u32).saturating_sub(8);
        }
      };

      // Rebuild the formatted Help rows before drawing so the render path
      // below reads immutable App state only.
      if app.get_current_route().active_block == ActiveBlock::HelpMenu {
        ui::ensure_help_menu_model(&mut app);
      }

      let current_route = app.get_current_route();
      // The banner animates whenever the Home screen is displayed, regardless
      // of which block has focus (on Home the focused block is usually Empty
      // or Library, not Home), so gate the fast tick on the route. A disabled
      // banner gradient renders statically and needs no fast tick.
      let animation_active = (current_route.id == RouteId::Home
        && app.user_config.behavior.banner_gradient)
        || current_route.active_block == ActiveBlock::Analysis
        || (current_route.id == RouteId::LyricsView
          && app.lyrics_status == crate::core::app::LyricsStatus::Found)
        || app.view.liked_song_animation_frame.is_some()
        || app.theme_fade_active();
      let current_tick_rate = if animation_active {
        app.user_config.behavior.animation_tick_rate_milliseconds
      } else {
        app.user_config.behavior.tick_rate_milliseconds
      };
      events.set_tick_rate(current_tick_rate);

      // Drop protocol caches for art the store no longer holds; rebuilds are
      // lazy, so this is a no-op whenever the art is unchanged.
      #[cfg(feature = "cover-art")]
      crate::tui::cover_art::sync(&app.cover_art);

      // The cursor must be invisible while the frame diff is written. Ratatui
      // applies the diff first and shows or hides the cursor afterwards, so a
      // cursor left visible by the previous frame travels over every freshly
      // written cell run and shows up as a strobing block inside the animated
      // banner. Hide it here; the draw closure below re-shows it at the input
      // position after the write.
      terminal.hide_cursor()?;

      terminal.draw(|f| {
        use ratatui::{prelude::Style, widgets::Block};
        f.render_widget(
          Block::default().style(Style::default().bg(app.user_config.theme.background.into())),
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
          ActiveBlock::CommunityPinPrompt => {
            ui::draw_main_layout(f, &app);
            ui::draw_community_pin_prompt(f, &app);
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

        // Cursor management lives inside the frame: ratatui shows and moves the
        // terminal cursor when a widget requests a position and hides it when
        // none does. The old post-draw hide/show pair ran on every animation
        // frame (16 ms on Home), and the alternating toggle reset the
        // terminal's blink phase each frame, so the cursor strobed.
        if current_route.active_block == ActiveBlock::Input {
          let cursor_offset = crate::tui::layout::main_layout_margin(&app) + 1;
          f.set_cursor_position((
            cursor_offset + app.view.input_cursor_position - app.view.input_scroll_offset.get(),
            cursor_offset,
          ));
        }
      })?;

      driver.next_window_title(&app)
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

        // `break` before the scripting hook below, not after: a quit must not run
        // one more round of pending plugin commands on its way out.
        if dispatch_key(key, &mut app) {
          break;
        }
        driver.run_pending_script_commands(&mut app);
      }
      event::Event::Mouse(mouse) => {
        let mut app = app.lock().await;
        if !app.user_config.behavior.disable_mouse_inputs {
          handlers::mouse_handler(mouse, &mut app);
        }
      }
      event::Event::Tick(elapsed) => {
        // The frontend owns the macOS main thread, so its run loop is pumped
        // here rather than by the driver.
        #[cfg(all(feature = "macos-media", target_os = "macos"))]
        {
          use objc2_foundation::{NSDate, NSRunLoop};
          NSRunLoop::currentRunLoop().runUntilDate(&NSDate::dateWithTimeIntervalSinceNow(0.001));
        }

        let mut app = app.lock().await;
        // The visualizer bar count is frontend geometry (terminal width and
        // layout margin), so it is resolved here and injected.
        #[cfg(any(feature = "audio-viz", feature = "audio-viz-cpal"))]
        let viz_bars = (app.get_current_route().active_block == ActiveBlock::Analysis).then(|| {
          ui::audio_analysis::visualizer_bar_count(
            app.user_config.behavior.visualizer_style,
            ui::audio_analysis::visualizer_inner_width(&app),
          )
        });
        driver.tick(
          &mut app,
          elapsed,
          TickEnv {
            now: Instant::now(),
            #[cfg(any(feature = "audio-viz", feature = "audio-viz-cpal"))]
            viz_bars,
            #[cfg(feature = "art-decode")]
            cover_art_full_image_support: cover_art_full_image_support(),
          },
        );
      }
    }

    if is_first_render {
      let mut app = app.lock().await;
      driver.dispatch_startup(&mut app);
      // The formatted Help row count is frontend presentation, so it stays
      // out of the driver's startup dispatch.
      app.view.help_docs_size = ui::help::get_help_docs(&app).len() as u32;

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

  {
    let mut app = app.lock().await;
    driver.on_quit(&mut app);
  }

  // A volume/resize/shuffle change may still be inside its debounce window;
  // persist it before the process exits.
  {
    let mut app = app.lock().await;
    app.flush_state_save(true);
  }

  #[cfg(feature = "streaming")]
  pause_native_playback_before_exit(app).await;

  // Stop the collector and all network work it owns before the final sync and
  // clear. In particular, a pause-triggered now-playing push must not race the
  // exit clear and recreate a stale public widget.
  history_collector.shutdown().await;

  // Restore the terminal before the exit network calls: there is no reason to
  // hold the alternate screen and raw mode while waiting on HTTP.
  if let Some(title) = driver.window_title_reset() {
    let _ = execute!(stdout(), SetTitle(title));
  }
  let _ = execute!(stdout(), DisableMouseCapture);
  if keyboard_enhancement_enabled {
    let _ = execute!(stdout(), PopKeyboardEnhancementFlags);
  }
  ratatui::restore();

  // Sync history to cloud on exit
  let sync_token_opt = {
    let app_guard = app.lock().await;
    app_guard.user_config.behavior.sync_token.clone()
  };

  if let Some(token) = sync_token_opt {
    info!("Synchronizing listening history to cloud before exit...");
    // Keep the clear strictly last after the collector has stopped: now-playing
    // updates are upserts, so a late push would otherwise recreate a stale
    // "paused" card on the public widget.
    //
    // Each call is bounded separately rather than sharing one budget, so a slow
    // history upload cannot starve the clear. Without these the shared client
    // allows a 10s connect plus a 30s request, stalling quit for up to a minute
    // on a half-open connection.
    let history_sync = crate::infra::history::sync_history_to_cloud(&token);
    match tokio::time::timeout(std::time::Duration::from_secs(2), history_sync).await {
      Ok(Err(e)) => log::warn!("failed to run exit history cloud sync: {}", e),
      Err(_) => log::warn!("exit history cloud sync timed out; records will sync next run"),
      Ok(Ok(())) => {}
    }

    let clear_now_playing = crate::infra::history::clear_now_playing_from_cloud(&token);
    match tokio::time::timeout(std::time::Duration::from_secs(1), clear_now_playing).await {
      Ok(Err(e)) => log::warn!("failed to clear now-playing on exit: {}", e),
      Err(_) => log::warn!("clearing now-playing on exit timed out"),
      Ok(Ok(())) => {}
    }
  }

  driver.clear_presence();

  Ok(())
}
