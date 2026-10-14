use super::common_key_events;
use crate::core::action::{Action, OpenTarget};
use crate::core::app::App;
use crate::core::plugin_api::ArtistInfo;
use crate::tui::event::Key;

pub fn handler(key: Key, app: &mut App) {
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

      let next_index = common_key_events::on_down_press_handler(
        &app.artists,
        Some(app.view.artists_list_index),
      );
      app.view.artists_list_index = next_index;
    }
    k if common_key_events::up_event(k, &app.user_config.keys) => {
      if !app.artists.is_empty() {
        let next_index = common_key_events::on_up_press_handler(
          &app.artists,
          Some(app.view.artists_list_index),
        );
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
      if target >= len {
        if app.current_saved_artists_has_more() {
          app.view.artists_list_index = len - 1;
          app.select_artist_when_next_page_lands(target);
          app.get_current_user_saved_artists_next();
          return;
        }
      }
      app.view.artists_list_index = target.min(len - 1);
    }
    k if k == app.user_config.keys.previous_page => {
      app.view.artists_list_index = app
        .view
        .artists_list_index
        .saturating_sub(page_step(app));
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
}
