use super::common_key_events;
use crate::core::action::{Action, ListTarget};
use crate::core::app::App;
use crate::core::plugin_api::ShowInfo;
use crate::tui::event::Key;

pub fn handler(key: Key, app: &mut App) {
  match key {
    k if common_key_events::left_event(k, &app.user_config.keys) => {
      common_key_events::handle_left_event(app)
    }
    k if common_key_events::down_event(k, &app.user_config.keys) => {
      if let Some(shows) = &mut app.library.saved_shows.get_results(None) {
        let next_index =
          common_key_events::on_down_press_handler(&shows.items, Some(app.view.shows_list_index));
        app.view.shows_list_index = next_index;
      }
    }
    k if common_key_events::up_event(k, &app.user_config.keys) => {
      if let Some(shows) = &mut app.library.saved_shows.get_results(None) {
        let next_index =
          common_key_events::on_up_press_handler(&shows.items, Some(app.view.shows_list_index));
        app.view.shows_list_index = next_index;
      }
    }
    k if common_key_events::high_event(k) => {
      if let Some(_shows) = app.library.saved_shows.get_results(None) {
        let next_index = common_key_events::on_high_press_handler();
        app.view.shows_list_index = next_index;
      }
    }
    k if common_key_events::middle_event(k) => {
      if let Some(shows) = app.library.saved_shows.get_results(None) {
        let next_index = common_key_events::on_middle_press_handler(&shows.items);
        app.view.shows_list_index = next_index;
      }
    }
    k if common_key_events::low_event(k) => {
      if let Some(shows) = app.library.saved_shows.get_results(None) {
        let next_index = common_key_events::on_low_press_handler(&shows.items);
        app.view.shows_list_index = next_index;
      }
    }
    Key::Enter => {
      if let Some(selected_show) = selected_saved_show(app).cloned() {
        app.apply(Action::OpenShowEpisodes(selected_show));
      }
    }
    k if k == app.user_config.keys.next_page => {
      app.apply(Action::LoadMore(ListTarget::SavedShows));
    }
    k if k == app.user_config.keys.previous_page => app.get_current_user_saved_shows_previous(),
    Key::Char('D') => {
      if let Some(id) = selected_saved_show(app).and_then(|show| show.id.clone()) {
        app.apply(Action::UnsaveShow(id));
      }
    }
    _ => {}
  }
}

/// The row under the cursor; `None` on a missing page.
fn selected_saved_show(app: &App) -> Option<&ShowInfo> {
  app
    .library
    .saved_shows
    .get_results(None)?
    .items
    .get(app.view.shows_list_index)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::pagination::Paged;
  use crate::core::user_config::UserConfig;
  use crate::infra::network::IoEvent;
  use std::sync::mpsc::{channel, Receiver};
  use std::time::SystemTime;

  fn app_with_shows(ids: &[Option<&str>]) -> (App, Receiver<IoEvent>) {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.library.saved_shows.add_pages(Paged {
      items: ids
        .iter()
        .enumerate()
        .map(|(index, &id)| ShowInfo {
          id: id.map(|id| id.to_string()),
          name: format!("Show {index}"),
          ..Default::default()
        })
        .collect(),
      offset: 0,
      limit: ids.len() as u32,
      total: ids.len() as u32,
      next: None,
      previous: None,
    });
    (app, rx)
  }

  #[test]
  fn enter_opens_the_selected_shows_episodes() {
    let (mut app, rx) = app_with_shows(&[Some("show-one"), Some("show-two")]);
    app.view.shows_list_index = 1;

    handler(Key::Enter, &mut app);

    assert!(
      matches!(
        rx.try_recv(),
        Ok(IoEvent::GetShowEpisodes(show)) if show.id.as_deref() == Some("show-two")
      ),
      "Enter fetched the selected show's episodes"
    );
  }

  #[test]
  fn shift_d_unfollows_the_selected_show() {
    let (mut app, rx) = app_with_shows(&[Some("show-one")]);

    handler(Key::Char('D'), &mut app);

    assert!(
      matches!(
        rx.try_recv(),
        Ok(IoEvent::CurrentUserSavedShowDelete(id)) if id == "show-one"
      ),
      "the unfollow was dispatched for the selected show"
    );
  }

  #[test]
  fn shift_d_on_a_show_without_an_id_dispatches_nothing() {
    let (mut app, rx) = app_with_shows(&[None]);

    handler(Key::Char('D'), &mut app);

    assert!(rx.try_recv().is_err(), "an id-less row unfollows nothing");
  }

  #[test]
  fn down_wraps_at_the_last_show() {
    let (mut app, _rx) = app_with_shows(&[Some("show-one"), Some("show-two")]);
    app.view.shows_list_index = 1;

    handler(Key::Down, &mut app);

    assert_eq!(app.view.shows_list_index, 0);
  }
}
