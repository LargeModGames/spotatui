use crate::core::app::App;
use crate::core::plugin_api::{self, PluginPopup};
use crate::infra::network::IoEvent;

/// An action queued by a plugin, drained by the runner while holding `&mut App`.
///
/// Each variant routes through the same `App` methods as the equivalent keybinding,
/// so native-streaming fast paths and throttling/coalescing are automatically honoured.
/// Tests inspect effects via pattern matching (no `derive` needed).
pub(crate) enum ScriptEffect {
  Play,
  Pause,
  Next,
  Previous,
  Seek(u32),
  SetVolume(u8),
  SetShuffle(bool),
  /// Resolved at drain time (country lookup needs `App`).
  Search(String),
  /// message, ttl_secs
  Notify(String, u64),
  /// Error message, ttl_secs -- always shown; blocks normal message overwrites until it expires.
  NotifyError(String, u64),
  /// Set or clear a playbar segment for a plugin (keyed by plugin name).
  SetPlaybarSegment {
    plugin: String,
    text: Option<String>,
  },
  /// Show a plugin popup dialog.
  ShowPopup(PluginPopup),
  /// Apply theme color overrides at runtime (field name -> color).
  SetTheme(Vec<(String, ratatui::style::Color)>),
  /// A whitelisted IoEvent built by the API layer. Lua never constructs raw
  /// IoEvents; only the documented `spotatui.*` actions can produce these.
  Dispatch(IoEvent),
  /// Cycle repeat off -> context -> track via `App::repeat()`, which keeps the
  /// native-streaming fast path.
  CycleRepeat,
  /// Unfollow a playlist; the current user id is resolved at drain time.
  UnfollowPlaylist(String),
  /// Navigate to a whitelisted target (validated at the API layer against
  /// [`NAV_TARGETS`]); apply mirrors the matching keybinding exactly.
  Navigate(String),
  /// Pop the navigation stack (same as the back key).
  Back,
  /// Publish (retained) content for a registered plugin screen.
  SetScreenContent {
    name: String,
    content: crate::core::plugin_api::PluginScreenContent,
  },
  /// Navigate to a registered plugin screen.
  ShowScreen(String),
  /// Pop the named plugin screen if it is the current route.
  CloseScreen(String),
}

/// Screens reachable via `spotatui.navigate(name)`.
pub(super) const NAV_TARGETS: &[&str] = &[
  "home",
  "queue",
  "settings",
  "devices",
  "help",
  "lyrics",
  "recently_played",
  "party",
  "analysis",
  "miniplayer",
];

/// Replicate the matching keybinding for each nav target. Unknown targets are
/// rejected at the API layer, so the fallback arm is unreachable in practice.
fn apply_navigate(app: &mut App, target: &str) {
  use crate::core::app::{ActiveBlock, RouteId, SourceFocus};
  use crate::core::source::Source;

  match target {
    "home" => app.push_navigation_stack(RouteId::Home, ActiveBlock::Empty),
    "queue" => {
      app.dispatch(IoEvent::GetQueue);
      app.push_navigation_stack(RouteId::Queue, ActiveBlock::Queue);
    }
    "settings" => {
      app.load_settings_for_category();
      app.push_navigation_stack(RouteId::Settings, ActiveBlock::Settings);
    }
    "devices" => {
      // Mirrors the manage_devices keybinding: open the Source & Device picker,
      // focus per active source, and only fetch devices under Spotify.
      app.source_list_index = Source::ALL
        .iter()
        .position(|s| *s == app.active_source)
        .unwrap_or(0);
      app.source_device_focus = if app.active_source == Source::Spotify {
        SourceFocus::Devices
      } else {
        SourceFocus::Source
      };
      app.push_navigation_stack(RouteId::SelectedDevice, ActiveBlock::SelectDevice);
      if app.active_source == Source::Spotify {
        app.dispatch(IoEvent::GetDevices);
      }
    }
    "help" => app.push_navigation_stack(RouteId::HelpMenu, ActiveBlock::HelpMenu),
    "lyrics" => app.push_navigation_stack(RouteId::LyricsView, ActiveBlock::LyricsView),
    // The network handler pushes the route once the data arrives, exactly like
    // the keybinding.
    "recently_played" => app.dispatch(IoEvent::GetRecentlyPlayed),
    "party" => app.push_navigation_stack(RouteId::Party, ActiveBlock::Party),
    "analysis" => app.get_audio_analysis(),
    "miniplayer" => {
      if app.get_current_route().id == RouteId::MiniPlayer {
        app.pop_navigation_stack();
      } else {
        app.push_navigation_stack(RouteId::MiniPlayer, ActiveBlock::MiniPlayer);
      }
    }
    _ => {}
  }
}

/// Returns `true` when the current playback state indicates active playback.
pub(super) fn effective_is_playing(app: &App) -> bool {
  plugin_api::playback_state(app)
    .map(|p| p.is_playing)
    .unwrap_or(false)
}

/// Drain queued effects into the app while holding `&mut App`.
pub(super) fn apply_effects(effects: Vec<ScriptEffect>, app: &mut App) {
  for effect in effects {
    match effect {
      ScriptEffect::Play => {
        if !effective_is_playing(app) {
          app.toggle_playback();
        }
      }
      ScriptEffect::Pause => {
        if effective_is_playing(app) {
          app.toggle_playback();
        }
      }
      ScriptEffect::Next => app.next_track(),
      ScriptEffect::Previous => app.previous_track(),
      ScriptEffect::Seek(ms) => app.seek_to(ms),
      ScriptEffect::SetVolume(v) => app.set_volume_percent(v),
      ScriptEffect::SetShuffle(desired) => {
        let current = plugin_api::playback_state(app)
          .map(|p| p.shuffle)
          .unwrap_or(false);
        if current != desired {
          app.shuffle();
        }
      }
      ScriptEffect::Search(query) => {
        let country = app.get_user_country();
        app.dispatch(IoEvent::GetSearchResults(query, country));
      }
      ScriptEffect::Notify(msg, ttl) => app.set_status_message(msg, ttl),
      ScriptEffect::NotifyError(msg, ttl) => app.set_error_status_message(msg, ttl),
      ScriptEffect::SetPlaybarSegment { plugin, text } => match text {
        Some(t) => {
          app.plugin_playbar_segments.insert(plugin, t);
        }
        None => {
          app.plugin_playbar_segments.remove(&plugin);
        }
      },
      ScriptEffect::ShowPopup(popup) => {
        app.plugin_popup = Some(popup);
        app.plugin_popup_scroll = 0;
      }
      ScriptEffect::Dispatch(event) => app.dispatch(event),
      ScriptEffect::CycleRepeat => app.repeat(),
      ScriptEffect::Navigate(target) => apply_navigate(app, &target),
      ScriptEffect::Back => {
        app.pop_navigation_stack();
      }
      ScriptEffect::SetScreenContent { name, content } => {
        app.plugin_screens.insert(name, content);
      }
      ScriptEffect::ShowScreen(name) => {
        use crate::core::app::{ActiveBlock, RouteId};
        if app.get_current_route().id != RouteId::PluginScreen(name.clone()) {
          app.push_navigation_stack(RouteId::PluginScreen(name), ActiveBlock::PluginScreen);
        }
        app.plugin_screen_scroll = 0;
      }
      ScriptEffect::CloseScreen(name) => {
        use crate::core::app::RouteId;
        if app.get_current_route().id == RouteId::PluginScreen(name) {
          app.pop_navigation_stack();
        }
      }
      ScriptEffect::UnfollowPlaylist(playlist_id) => {
        let user_id = app.user.as_ref().map(|u| u.id.clone());
        if let Some(user_id) = user_id {
          app.dispatch(IoEvent::UserUnfollowPlaylist(user_id, playlist_id));
        } else {
          app.set_error_status_message(
            "plugin unfollow_playlist: user profile not loaded yet".to_string(),
            4,
          );
        }
      }
      ScriptEffect::SetTheme(pairs) => {
        for (field, color) in pairs {
          match field.as_str() {
            "active" => app.user_config.theme.active = color,
            "banner" => app.user_config.theme.banner = color,
            "error_border" => app.user_config.theme.error_border = color,
            "error_text" => app.user_config.theme.error_text = color,
            "hint" => app.user_config.theme.hint = color,
            "hovered" => app.user_config.theme.hovered = color,
            "inactive" => app.user_config.theme.inactive = color,
            "playbar_background" => app.user_config.theme.playbar_background = color,
            "playbar_progress" => app.user_config.theme.playbar_progress = color,
            "playbar_progress_text" => app.user_config.theme.playbar_progress_text = color,
            "playbar_text" => app.user_config.theme.playbar_text = color,
            "selected" => app.user_config.theme.selected = color,
            "text" => app.user_config.theme.text = color,
            "background" => app.user_config.theme.background = color,
            "header" => app.user_config.theme.header = color,
            "highlighted_lyrics" => app.user_config.theme.highlighted_lyrics = color,
            "analysis_bar" => app.user_config.theme.analysis_bar = color,
            "analysis_bar_text" => app.user_config.theme.analysis_bar_text = color,
            _ => {} // unknown fields were rejected at the API layer
          }
        }
      }
    }
  }
}
