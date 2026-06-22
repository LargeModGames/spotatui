use super::common_key_events;
use crate::core::app::ActiveBlock;
use crate::core::app::{App, EpisodeTableContext};
use crate::infra::network::IoEvent;
use crate::tui::event::Key;
use rspotify::model::{EpisodeId, PlayableId};

pub fn handler(key: Key, app: &mut App) {
  match key {
    k if common_key_events::left_event(k, &app.user_config.keys) => {
      common_key_events::handle_left_event(app)
    }
    k if common_key_events::down_event(k, &app.user_config.keys) => {
      if let Some(episodes) = &mut app.library.show_episodes.get_results(None) {
        let next_index =
          common_key_events::on_down_press_handler(&episodes.items, Some(app.episode_list_index));
        app.episode_list_index = next_index;
      }
    }
    k if common_key_events::up_event(k, &app.user_config.keys) => {
      if let Some(episodes) = &mut app.library.show_episodes.get_results(None) {
        let next_index =
          common_key_events::on_up_press_handler(&episodes.items, Some(app.episode_list_index));
        app.episode_list_index = next_index;
      }
    }
    k if common_key_events::high_event(k) => {
      if let Some(_episodes) = app.library.show_episodes.get_results(None) {
        let next_index = common_key_events::on_high_press_handler();
        app.episode_list_index = next_index;
      }
    }
    k if common_key_events::middle_event(k) => {
      if let Some(episodes) = app.library.show_episodes.get_results(None) {
        let next_index = common_key_events::on_middle_press_handler(&episodes.items);
        app.episode_list_index = next_index;
      }
    }
    k if common_key_events::low_event(k) => {
      if let Some(episodes) = app.library.show_episodes.get_results(None) {
        let next_index = common_key_events::on_low_press_handler(&episodes.items);
        app.episode_list_index = next_index;
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
    Key::Ctrl('e') => jump_to_end(app),
    Key::Ctrl('a') => jump_to_start(app),
    _ => {}
  }
}

fn jump_to_end(app: &mut App) {
  if let Some(episodes) = app.library.show_episodes.get_results(None) {
    let last_idx = episodes.items.len() - 1;
    app.episode_list_index = last_idx;
  }
}

fn on_enter(app: &mut App) {
  if let Some(episodes) = app.library.show_episodes.get_results(None) {
    // Episodes without a parseable id are skipped, so the playback offset must
    // count only the kept rows up to the selected index (mirrors the saved-track
    // playback path); otherwise dropping an earlier row would shift the offset.
    let mut episode_ids: Vec<PlayableId<'static>> = Vec::with_capacity(episodes.items.len());
    let mut selected_offset = None;
    for (row_index, episode) in episodes.items.iter().enumerate() {
      if let Some(id) = episode
        .id
        .as_deref()
        .and_then(|id| EpisodeId::from_id(id).ok())
      {
        if row_index == app.episode_list_index {
          selected_offset = Some(episode_ids.len());
        }
        episode_ids.push(PlayableId::Episode(id.into_static()));
      }
    }
    app.dispatch(IoEvent::StartPlayback(
      None,
      Some(episode_ids),
      selected_offset,
    ));
  }
}

fn handle_prev_event(app: &mut App) {
  app.get_episode_table_previous();
}

fn handle_next_event(app: &mut App) {
  let show_id = match app.episode_table_context {
    EpisodeTableContext::Full => app
      .selected_show_full
      .as_ref()
      .and_then(|s| s.show.id.clone()),
    EpisodeTableContext::Simplified => app
      .selected_show_simplified
      .as_ref()
      .and_then(|s| s.show.id.clone()),
  };
  if let Some(show_id) = show_id {
    app.get_episode_table_next(show_id)
  }
}

fn handle_follow_event(app: &mut App) {
  app.user_follow_show(ActiveBlock::EpisodeTable);
}

fn handle_unfollow_event(app: &mut App) {
  app.user_unfollow_show(ActiveBlock::EpisodeTable);
}

fn jump_to_start(app: &mut App) {
  app.episode_list_index = 0;
}

fn toggle_sort_by_date(app: &mut App) {
  //TODO: reverse whole list and not just currently visible episodes
  let selected_id = match app.library.show_episodes.get_results(None) {
    Some(episodes) => episodes
      .items
      .get(app.episode_list_index)
      .map(|e| e.id.clone()),
    None => None,
  };

  if let Some(episodes) = app.library.show_episodes.get_mut_results(None) {
    episodes.items.reverse();
  }

  if let Some(id) = selected_id {
    if let Some(episodes) = app.library.show_episodes.get_results(None) {
      app.episode_list_index = episodes.items.iter().position(|e| e.id == id).unwrap_or(0);
    }
  } else {
    app.episode_list_index = 0;
  }
}
