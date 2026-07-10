use super::common_key_events;
use crate::core::app::App;
use crate::infra::network::IoEvent;
use crate::tui::event::Key;

pub fn handler(key: Key, app: &mut App) {
  match key {
    k if common_key_events::left_event(k, &app.user_config.keys) => {
      common_key_events::handle_left_event(app)
    }
    k if common_key_events::down_event(k, &app.user_config.keys) => {
      if let Some(stats) = &app.stats_data {
        app.stats_selected_track = common_key_events::on_down_press_handler(
          &stats.top_tracks,
          Some(app.stats_selected_track),
        );
      }
    }
    k if common_key_events::up_event(k, &app.user_config.keys) => {
      if let Some(stats) = &app.stats_data {
        app.stats_selected_track =
          common_key_events::on_up_press_handler(&stats.top_tracks, Some(app.stats_selected_track));
      }
    }
    Key::Char('[') => cycle_period(app, app.stats_period.prev()),
    Key::Char(']') => cycle_period(app, app.stats_period.next()),
    Key::Enter => {
      let uri = app.stats_data.as_ref().and_then(|stats| {
        stats
          .top_tracks
          .get(app.stats_selected_track)
          .and_then(|entry| entry.uri.clone())
      });
      match uri {
        Some(uri) if uri.starts_with("spotify:track:") => {
          app.dispatch(IoEvent::StartPlayback(None, Some(vec![uri]), Some(0)));
        }
        Some(_) | None => {
          let has_tracks = app
            .stats_data
            .as_ref()
            .is_some_and(|stats| !stats.top_tracks.is_empty());
          if has_tracks {
            app.set_status_message("This entry can't be played directly".to_string(), 4);
          }
        }
      }
    }
    _ => {}
  }
}

fn cycle_period(app: &mut App, period: crate::infra::history::RecapPeriod) {
  app.stats_period = period;
  app.stats_data = None;
  app.stats_selected_track = 0;
  app.stats_loading = true;
  app.dispatch(IoEvent::LoadListeningStats(period));
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::user_config::UserConfig;
  use crate::infra::history::{RankedEntry, RecapPeriod, StatsData};
  use std::sync::mpsc::{channel, Receiver};
  use std::time::SystemTime;

  fn app_with_track(uri: Option<String>) -> (App, Receiver<IoEvent>) {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.stats_data = Some(StatsData {
      total_plays: 1,
      total_time_ms: 60_000,
      top_tracks: vec![RankedEntry {
        display: "Song - Artist".to_string(),
        detail: "1 plays · 1m".to_string(),
        value: 60_000,
        uri,
      }],
      top_artists: vec![],
      top_albums: vec![],
      days: vec![],
    });
    (app, rx)
  }

  #[test]
  fn cycling_period_dispatches_stats_load() {
    let (mut app, rx) = app_with_track(None);
    handler(Key::Char(']'), &mut app);
    assert_eq!(app.stats_period, RecapPeriod::Month);
    assert!(app.stats_loading);
    assert!(app.stats_data.is_none());
    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::LoadListeningStats(RecapPeriod::Month))
    ));
  }

  #[test]
  fn enter_plays_selected_spotify_track() {
    let (mut app, rx) = app_with_track(Some("spotify:track:abc".to_string()));
    handler(Key::Enter, &mut app);
    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::StartPlayback(None, Some(uris), Some(0))) if uris == vec!["spotify:track:abc".to_string()]
    ));
  }

  #[test]
  fn enter_on_unplayable_entry_shows_status_message() {
    let (mut app, rx) = app_with_track(None);
    handler(Key::Enter, &mut app);
    assert!(rx.try_recv().is_err());
    assert!(app.status_message.is_some());
  }

  #[test]
  fn right_from_sidebar_refocuses_stats_content() {
    use crate::core::app::{ActiveBlock, RouteId};

    let (mut app, _rx) = app_with_track(None);
    app.push_navigation_stack(RouteId::Stats, ActiveBlock::Stats);
    // Left puts focus back on the sidebar; right must return it to the screen.
    common_key_events::handle_left_event(&mut app);
    common_key_events::handle_right_event(&mut app);
    assert_eq!(app.get_current_route().active_block, ActiveBlock::Stats);
  }
}
