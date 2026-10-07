use super::common_key_events;
use crate::core::action::{Action, ListTarget};
use crate::core::app::App;
use crate::tui::event::Key;

pub fn handler(key: Key, app: &mut App) {
  match key {
    k if common_key_events::left_event(k, &app.user_config.keys) => {
      common_key_events::handle_left_event(app)
    }
    k if common_key_events::down_event(k, &app.user_config.keys) => {
      if let Some(episodes) = app.library().show_episodes.get_results(None) {
        let next_index = common_key_events::on_down_press_handler(
          &episodes.items,
          Some(app.view.episode_list_index),
        );
        app.view.episode_list_index = next_index;
      }
    }
    k if common_key_events::up_event(k, &app.user_config.keys) => {
      if let Some(episodes) = app.library().show_episodes.get_results(None) {
        let next_index = common_key_events::on_up_press_handler(
          &episodes.items,
          Some(app.view.episode_list_index),
        );
        app.view.episode_list_index = next_index;
      }
    }
    k if common_key_events::high_event(k) => {
      if let Some(_episodes) = app.library().show_episodes.get_results(None) {
        let next_index = common_key_events::on_high_press_handler();
        app.view.episode_list_index = next_index;
      }
    }
    k if common_key_events::middle_event(k) => {
      if let Some(episodes) = app.library().show_episodes.get_results(None) {
        let next_index = common_key_events::on_middle_press_handler(&episodes.items);
        app.view.episode_list_index = next_index;
      }
    }
    k if common_key_events::low_event(k) => {
      if let Some(episodes) = app.library().show_episodes.get_results(None) {
        let next_index = common_key_events::on_low_press_handler(&episodes.items);
        app.view.episode_list_index = next_index;
      }
    }
    Key::Enter => {
      on_enter(app);
    }
    // Scroll down
    k if k == app.user_config.keys.next_page => handle_next_event(app),
    // Scroll up
    k if k == app.user_config.keys.previous_page => handle_prev_event(app),
    Key::Char('S') => toggle_sort_by_date(app),
    Key::Char('s') => handle_follow_event(app),
    Key::Char('D') => handle_unfollow_event(app),
    k if k == app.user_config.keys.jump_to_end => jump_to_end(app),
    k if k == app.user_config.keys.jump_to_start => jump_to_start(app),
    _ => {}
  }
}

fn jump_to_end(app: &mut App) {
  if let Some(episodes) = app.library().show_episodes.get_results(None) {
    if let Some(last_idx) = episodes.items.len().checked_sub(1) {
      app.view.episode_list_index = last_idx;
    }
  }
}

fn on_enter(app: &mut App) {
  let selected_index = app.view.episode_list_index;
  let request = app
    .library()
    .show_episodes
    .get_results(None)
    .map(|episodes| {
      common_key_events::uri_playback_request(
        episodes.items.iter().map(|episode| episode.uri.clone()),
        selected_index,
      )
    });
  if let Some((uris, offset)) = request {
    app.apply(Action::PlayUris { uris, offset });
  }
}

fn handle_prev_event(app: &mut App) {
  app.get_episode_table_previous();
}

fn handle_next_event(app: &mut App) {
  app.apply(Action::LoadMore(ListTarget::ShowEpisodes));
}

fn handle_follow_event(app: &mut App) {
  if let Some(show_id) = app.selected_episode_show_id() {
    app.apply(Action::SaveShow(show_id));
  }
}

fn handle_unfollow_event(app: &mut App) {
  if let Some(show_id) = app.selected_episode_show_id() {
    app.apply(Action::UnsaveShow(show_id));
  }
}

fn jump_to_start(app: &mut App) {
  app.view.episode_list_index = 0;
}

fn toggle_sort_by_date(app: &mut App) {
  //TODO: reverse whole list and not just currently visible episodes
  let selected_id = match app.library().show_episodes.get_results(None) {
    Some(episodes) => episodes
      .items
      .get(app.view.episode_list_index)
      .map(|e| e.id.clone()),
    None => None,
  };

  if let Some(episodes) = app.library_mut().show_episodes.get_mut_results(None) {
    episodes.items.reverse();
  }

  if let Some(id) = selected_id {
    if let Some(episodes) = app.library().show_episodes.get_results(None) {
      app.view.episode_list_index = episodes.items.iter().position(|e| e.id == id).unwrap_or(0);
    }
  } else {
    app.view.episode_list_index = 0;
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::pagination::Paged;
  use crate::core::plugin_api::EpisodeInfo;
  use crate::core::user_config::UserConfig;
  use crate::infra::network::IoEvent;
  use std::sync::mpsc::{channel, Receiver};
  use std::time::SystemTime;

  fn episode(uri: Option<&str>, name: &str) -> EpisodeInfo {
    EpisodeInfo {
      id: Some(name.to_string()),
      uri: uri.map(|u| u.to_string()),
      name: name.to_string(),
      duration_ms: 1_000,
      show_name: String::new(),
      description: String::new(),
      release_date: String::new(),
      is_playable: true,
      resume_point: None,
      image_url: None,
    }
  }

  fn app_with_episodes(episodes: Vec<EpisodeInfo>) -> (App, Receiver<IoEvent>) {
    app_with_episodes_and_config(episodes, UserConfig::new())
  }

  fn app_with_episodes_and_config(
    episodes: Vec<EpisodeInfo>,
    config: UserConfig,
  ) -> (App, Receiver<IoEvent>) {
    let (tx, rx) = channel();
    let mut app = App::new(tx, config, Some(SystemTime::now()));
    let limit = episodes.len() as u32;
    app.library_mut().show_episodes.add_pages(Paged {
      items: episodes,
      offset: 0,
      limit,
      total: limit,
      next: None,
      previous: None,
    });
    (app, rx)
  }

  #[test]
  fn rebound_jump_to_end_key_selects_the_last_episode() {
    let mut config = UserConfig::new();
    config.keys.jump_to_end = Key::End;
    let (mut app, _rx) = app_with_episodes_and_config(
      vec![
        episode(Some("spotify:episode:one"), "One"),
        episode(Some("spotify:episode:two"), "Two"),
        episode(Some("spotify:episode:three"), "Three"),
      ],
      config,
    );

    handler(Key::End, &mut app);

    assert_eq!(app.view.episode_list_index, 2);
  }

  #[test]
  fn jump_to_end_on_an_empty_episode_page_keeps_the_first_row() {
    let (mut app, _rx) = app_with_episodes(vec![]);

    handler(Key::Ctrl('e'), &mut app);

    assert_eq!(app.view.episode_list_index, 0);
  }

  #[test]
  fn enter_starts_the_page() {
    let (mut app, rx) = app_with_episodes(vec![episode(Some("spotify:episode:one"), "One")]);

    handler(Key::Enter, &mut app);

    assert!(
      matches!(
        rx.try_recv(),
        Ok(IoEvent::StartPlayback(None, Some(uris), _)) if uris == vec!["spotify:episode:one".to_string()]
      ),
      "Enter started the episode list"
    );
  }

  #[test]
  fn shift_d_without_an_open_show_dispatches_nothing() {
    let (mut app, rx) = app_with_episodes(vec![episode(Some("spotify:episode:one"), "One")]);

    handler(Key::Char('D'), &mut app);

    assert!(rx.try_recv().is_err(), "no show is open to unfollow");
  }

  #[test]
  fn shift_s_reverses_the_page_and_keeps_the_selected_episode() {
    let (mut app, _rx) = app_with_episodes(vec![
      episode(Some("spotify:episode:one"), "One"),
      episode(Some("spotify:episode:two"), "Two"),
      episode(Some("spotify:episode:three"), "Three"),
    ]);
    app.view.episode_list_index = 0;

    handler(Key::Char('S'), &mut app);

    let names: Vec<&str> = app
      .library()
      .show_episodes
      .get_results(None)
      .unwrap()
      .items
      .iter()
      .map(|e| e.name.as_str())
      .collect();
    assert_eq!(names, vec!["Three", "Two", "One"]);
    assert_eq!(
      app.view.episode_list_index, 2,
      "the cursor follows the episode it was on"
    );
  }
}
