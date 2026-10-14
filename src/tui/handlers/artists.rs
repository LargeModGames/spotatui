use super::common_key_events;
use crate::core::action::{Action, OpenTarget};
use crate::core::app::App;
use crate::core::plugin_api::ArtistInfo;
use crate::tui::event::Key;

pub fn handler(key: Key, app: &mut App) {
  app.forget_pending_artist_selection();

  match key {
    k if common_key_events::toward_sidebar_event(k, &app.user_config) => {
      common_key_events::focus_sidebar(app)
    }
    k if common_key_events::down_event(k, &app.user_config.keys) => {
      let current_index = app.view.artists_list_index;
      let artists_len = app.artists.len();

      if artists_len == 0 {
        return;
      }

      // At the end of the loaded list?
      if current_index == artists_len - 1 {
        if app.current_saved_artists_has_more() {
          app.select_artist_when_next_page_lands(artists_len);
          app.get_current_user_saved_artists_next();
          return;
        }
        // At the very end of all pages: wrap back to top
        app.view.artists_list_index = 0;
        return;
      }

      let next_index =
        common_key_events::on_down_press_handler(&app.artists, Some(app.view.artists_list_index));
      app.view.artists_list_index = next_index;
    }
    k if common_key_events::up_event(k, &app.user_config.keys) => {
      if !app.artists.is_empty() {
        let next_index =
          common_key_events::on_up_press_handler(&app.artists, Some(app.view.artists_list_index));
        app.view.artists_list_index = next_index;
      }
    }
    k if common_key_events::high_event(k) => {
      if !app.artists.is_empty() {
        let next_index = common_key_events::on_high_press_handler();
        app.view.artists_list_index = next_index;
      }
    }
    k if common_key_events::middle_event(k) => {
      if !app.artists.is_empty() {
        let next_index = common_key_events::on_middle_press_handler(&app.artists);
        app.view.artists_list_index = next_index;
      }
    }
    k if common_key_events::low_event(k) => {
      if !app.artists.is_empty() {
        let next_index = common_key_events::on_low_press_handler(&app.artists);
        app.view.artists_list_index = next_index;
      }
    }
    Key::Enter => {
      if let Some((id, name)) = selected_saved_artist(app) {
        app.apply(Action::Open(OpenTarget::Artist { id, name }));
      }
    }
    Key::Char('D') => {
      if let Some((id, _)) = selected_saved_artist(app) {
        app.apply(Action::UnfollowArtist(id));
      }
    }
    Key::Char('e') => {
      if let Some(uri) = selected_saved_artist_uri(app) {
        app.apply(Action::PlayContext { uri, offset: None });
      }
    }
    Key::Char('r') => {
      if let Some((id, name)) = selected_saved_artist(app) {
        app.apply(Action::RecommendFromArtist { id, name });
      }
    }
    k if k == app.user_config.keys.next_page => {
      let len = app.artists.len();
      if len == 0 {
        return;
      }
      let target = app.view.artists_list_index.saturating_add(page_step(app));
      if target >= len && app.current_saved_artists_has_more() {
        app.view.artists_list_index = len - 1;
        app.select_artist_when_next_page_lands(target);
        app.get_current_user_saved_artists_next();
        return;
      }
      app.view.artists_list_index = target.min(len - 1);
    }
    k if k == app.user_config.keys.previous_page => {
      app.view.artists_list_index = app.view.artists_list_index.saturating_sub(page_step(app));
    }
    // Open sort menu
    Key::Char(',') => {
      super::sort_menu::open_sort_menu(app, crate::core::sort::SortContext::SavedArtists);
    }
    _ => {}
  }
}

/// The row under the cursor; `None` on a missing page.
fn selected_saved_artist_row(app: &App) -> Option<&ArtistInfo> {
  app.artists.get(app.view.artists_list_index)
}

/// The (id, name) of the row under the cursor; `None` on an id-less row.
fn selected_saved_artist(app: &App) -> Option<(String, String)> {
  let artist = selected_saved_artist_row(app)?;
  Some((artist.id.clone()?, artist.name.clone()))
}

/// The `spotify:artist:` context URI of the row under the cursor.
fn selected_saved_artist_uri(app: &App) -> Option<String> {
  selected_saved_artist_row(app)?.uri.clone()
}

fn page_step(app: &App) -> usize {
  usize::from(app.view.size.height / 2).max(1)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::pagination::CursorPaged;
  use crate::core::plugin_api::ArtistInfo;
  use crate::core::user_config::UserConfig;
  use crate::infra::network::IoEvent;
  use std::sync::mpsc::{channel, Receiver};
  use std::time::SystemTime;

  type Ev = IoEvent;

  fn app_with_saved_artists() -> (App, Receiver<IoEvent>) {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.library_mut().saved_artists.add_pages(CursorPaged {
      items: vec![
        ArtistInfo {
          id: Some("artist1".to_string()),
          uri: Some("spotify:artist:artist1".to_string()),
          name: "First".to_string(),
          image_url: None,
        },
        ArtistInfo {
          id: Some("artist2".to_string()),
          uri: Some("spotify:artist:artist2".to_string()),
          name: "Second".to_string(),
          image_url: None,
        },
      ],
      ..CursorPaged::default()
    });
    app.set_saved_artists_to_table_continuous();
    app.view.artists_list_index = 1;
    (app, rx)
  }

  #[test]
  fn enter_opens_the_selected_artist_by_id() {
    let (mut app, rx) = app_with_saved_artists();

    handler(Key::Enter, &mut app);

    match rx.try_recv() {
      Ok(IoEvent::GetArtist(id, name, _)) => {
        assert_eq!(id, "artist2");
        assert_eq!(name, "Second");
      }
      _ => panic!("expected GetArtist"),
    }
  }

  #[test]
  fn d_unfollows_the_selected_artist() {
    let (mut app, rx) = app_with_saved_artists();

    handler(Key::Char('D'), &mut app);

    match rx.try_recv() {
      Ok(IoEvent::UserUnfollowArtists(ids)) => assert_eq!(ids, vec!["artist2".to_string()]),
      _ => panic!("expected UserUnfollowArtists"),
    }
  }

  #[test]
  fn e_starts_the_selected_artists_context() {
    let (mut app, rx) = app_with_saved_artists();

    handler(Key::Char('e'), &mut app);

    match rx.try_recv() {
      Ok(IoEvent::StartPlayback(context, uris, offset)) => {
        assert_eq!(context.as_deref(), Some("spotify:artist:artist2"));
        assert!(uris.is_none());
        assert!(offset.is_none());
      }
      _ => panic!("expected StartPlayback"),
    }
  }

  #[test]
  fn r_seeds_recommendations_from_the_selected_artist() {
    let (mut app, rx) = app_with_saved_artists();

    handler(Key::Char('r'), &mut app);

    assert_eq!(app.recommendations_seed, "Second");
    assert!(
      matches!(rx.try_recv(), Ok(IoEvent::GetRecommendationsForSeed(..))),
      "the seeded fetch was dispatched"
    );
  }

  #[test]
  fn down_moves_to_next_artist() {
    let (mut app, _rx) = app_with_saved_artists();
    app.view.artists_list_index = 0;

    handler(Key::Down, &mut app);

    assert_eq!(app.view.artists_list_index, 1);
  }

  #[test]
  fn down_at_end_wraps_to_top_when_no_more_pages() {
    let (mut app, rx) = app_with_saved_artists();
    app.view.artists_list_index = 1; // Last item in fixture

    handler(Key::Down, &mut app);

    assert_eq!(app.view.artists_list_index, 0);
    // Ensure no network request was triggered
    assert!(rx.try_recv().is_err());
  }

  #[test]
  fn down_at_end_dispatches_next_page_when_more_exist() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));

    let page = CursorPaged {
      items: vec![
        ArtistInfo {
          id: Some("artist1".to_string()),
          name: "First".to_string(),
          ..Default::default()
        },
        ArtistInfo {
          id: Some("artist2".to_string()),
          name: "Second".to_string(),
          ..Default::default()
        },
      ],
      next: Some("https://api.spotify.com/v1/me/following?after=artist2".to_string()),
      cursor_after: Some("artist2".to_string()),
      ..CursorPaged::default()
    };
    app.library_mut().saved_artists.add_pages(page);
    app.set_saved_artists_to_table_continuous();
    app.view.artists_list_index = 1; // Last item

    handler(Key::Down, &mut app);

    // Check if correct GetFollowedArtists event was dispatched
    match rx.try_recv() {
      Ok(Ev::GetFollowedArtists(Some(cursor))) => {
        assert_eq!(cursor, "artist2");
      }
      _ => panic!("expected GetFollowedArtists with cursor"),
    }

    // Simulate the page landing: verify the cursor lands on the new item (index 2)!
    let page2 = CursorPaged {
      items: vec![ArtistInfo {
        id: Some("artist3".to_string()),
        name: "Third".to_string(),
        ..Default::default()
      }],
      ..CursorPaged::default()
    };
    app.library_mut().saved_artists.add_pages(page2);
    app.set_saved_artists_to_table_continuous();

    assert_eq!(app.view.artists_list_index, 2);
  }

  #[test]
  fn up_moves_to_previous_artist() {
    let (mut app, _rx) = app_with_saved_artists();
    app.view.artists_list_index = 1;

    handler(Key::Up, &mut app);

    assert_eq!(app.view.artists_list_index, 0);
  }

  #[test]
  fn high_middle_low_jump_within_artists() {
    let (mut app, _rx) = app_with_saved_artists();
    app.view.artists_list_index = 1;

    handler(Key::Char('H'), &mut app);
    assert_eq!(app.view.artists_list_index, 0);

    handler(Key::Char('L'), &mut app);
    assert_eq!(app.view.artists_list_index, 1);

    handler(Key::Char('M'), &mut app);
    assert!(app.view.artists_list_index <= 1);
  }

  #[test]
  fn next_and_previous_page_step_viewport() {
    let (mut app, _rx) = app_with_saved_artists();
    app.view.artists_list_index = 0;
    app.view.size.height = 10; // page_step will be (10 / 2) = 5

    let next_page_key = app.user_config.keys.next_page;
    let prev_page_key = app.user_config.keys.previous_page;

    // When no more pages exist, next_page clamps to len - 1
    handler(next_page_key, &mut app);
    assert_eq!(app.view.artists_list_index, 1);

    // previous_page steps backward
    handler(prev_page_key, &mut app);
    assert_eq!(app.view.artists_list_index, 0);
  }
}
