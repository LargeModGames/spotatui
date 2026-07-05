use super::common_key_events;
use crate::core::app::App;
use crate::tui::event::Key;

/// The Queue screen is a header row ("Now playing", index 0, non-actionable)
/// followed by one selectable row per native-queue item. Selection index `k`
/// (k >= 1) maps to `native_queue[k - 1]`.
pub fn handler(key: Key, app: &mut App) {
  match key {
    k if common_key_events::down_event(k, &app.user_config.keys) => move_selection(1, app),
    k if common_key_events::up_event(k, &app.user_config.keys) => move_selection(-1, app),
    // Shift+J / Shift+K reorder the selected item (hardcoded per design, not
    // rebindable). J moves it down, K moves it up.
    Key::Char('J') => reorder(1, app),
    Key::Char('K') => reorder(-1, app),
    k if k == app.user_config.keys.remove_from_queue => remove_selected(app),
    Key::Enter => play_selected(app),
    _ => {}
  }
}

/// Jump to the selected queue row: drop every item before it, then start the
/// native queue there. If a decoded context is playing and the queue does not
/// already own playback, suspend it mid-track first (so it resumes at the same
/// track + position when the queue drains). Row 0 (now playing) is non-actionable.
fn play_selected(app: &mut App) {
  let selected = app.queue_selected_index;
  if selected == 0 {
    return;
  }
  let skip = selected - 1;
  if skip >= app.native_queue.len() {
    return;
  }
  // Drop the items before the selected one so it becomes the queue head.
  app.native_queue.drain(..skip);
  app.queue_selected_index = 1;
  if !app.queue_owns_playback() {
    app.suspend_active_decoded_context_mid_track();
  }
  app.dispatch(crate::infra::network::IoEvent::AdvanceNativeQueue);
}

/// Total selectable rows: the now-playing header plus every queue item.
fn row_count(app: &App) -> usize {
  1 + app.native_queue.len()
}

fn move_selection(delta: i32, app: &mut App) {
  let max_index = row_count(app).saturating_sub(1);
  let current = app.queue_selected_index;
  app.queue_selected_index = match delta {
    -1 => current.saturating_sub(1),
    _ => (current + 1).min(max_index),
  };
}

/// Remove the selected queue item and clamp the selection to the shortened list.
/// Row 0 (now playing) is non-actionable.
fn remove_selected(app: &mut App) {
  let selected = app.queue_selected_index;
  if selected == 0 {
    return;
  }
  let idx = selected - 1;
  if idx < app.native_queue.len() {
    app.native_queue.remove(idx);
    let max_index = row_count(app).saturating_sub(1);
    if app.queue_selected_index > max_index {
      app.queue_selected_index = max_index;
    }
  }
}

/// Swap the selected item with its neighbor and move the selection with it, so
/// the highlighted row keeps following the same item. No wrap at the ends.
fn reorder(delta: i32, app: &mut App) {
  let selected = app.queue_selected_index;
  if selected == 0 {
    return;
  }
  let idx = selected - 1;
  let len = app.native_queue.len();
  if idx >= len {
    return;
  }
  match delta {
    1 if idx + 1 < len => {
      app.native_queue.swap(idx, idx + 1);
      app.queue_selected_index = selected + 1;
    }
    -1 if idx > 0 => {
      app.native_queue.swap(idx, idx - 1);
      app.queue_selected_index = selected - 1;
    }
    _ => {}
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::plugin_api::TrackInfo;
  use crate::core::user_config::UserConfig;
  use std::sync::mpsc::channel;
  use std::time::SystemTime;

  fn track(uri: &str, name: &str) -> TrackInfo {
    TrackInfo {
      uri: Some(uri.to_string()),
      name: name.to_string(),
      artists: vec!["Artist".to_string()],
      album: "Album".to_string(),
      duration_ms: 1000,
      id: None,
      album_id: None,
      artist_refs: vec![],
      is_playable: true,
      is_local: false,
      track_number: 0,
      explicit: false,
      image_url: None,
    }
  }

  fn app_with_queue(names: &[&str]) -> App {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.native_queue = names.iter().map(|n| track("spotify:track:x", n)).collect();
    app
  }

  #[test]
  fn move_selection_clamps_to_rows() {
    let mut app = app_with_queue(&["A", "B"]);
    // Rows: [now playing, A, B] => max index 2.
    app.queue_selected_index = 0;
    move_selection(-1, &mut app);
    assert_eq!(app.queue_selected_index, 0);
    move_selection(1, &mut app);
    move_selection(1, &mut app);
    move_selection(1, &mut app);
    move_selection(1, &mut app);
    assert_eq!(app.queue_selected_index, 2);
  }

  #[test]
  fn remove_ignores_now_playing_row() {
    let mut app = app_with_queue(&["A", "B"]);
    app.queue_selected_index = 0;
    remove_selected(&mut app);
    assert_eq!(app.native_queue.len(), 2);
  }

  #[test]
  fn remove_deletes_selected_and_clamps() {
    let mut app = app_with_queue(&["A", "B"]);
    // Select last item (row 2 => native_queue[1] == "B").
    app.queue_selected_index = 2;
    remove_selected(&mut app);
    assert_eq!(app.native_queue.len(), 1);
    assert_eq!(app.native_queue[0].name, "A");
    // Selection clamped from 2 to the new max (row 1).
    assert_eq!(app.queue_selected_index, 1);
  }

  #[test]
  fn reorder_down_swaps_and_follows_item() {
    let mut app = app_with_queue(&["A", "B", "C"]);
    // Select A (row 1).
    app.queue_selected_index = 1;
    reorder(1, &mut app);
    assert_eq!(
      app
        .native_queue
        .iter()
        .map(|t| t.name.as_str())
        .collect::<Vec<_>>(),
      vec!["B", "A", "C"]
    );
    // Selection followed A down to row 2.
    assert_eq!(app.queue_selected_index, 2);
  }

  #[test]
  fn reorder_up_swaps_and_follows_item() {
    let mut app = app_with_queue(&["A", "B", "C"]);
    // Select C (row 3).
    app.queue_selected_index = 3;
    reorder(-1, &mut app);
    assert_eq!(
      app
        .native_queue
        .iter()
        .map(|t| t.name.as_str())
        .collect::<Vec<_>>(),
      vec!["A", "C", "B"]
    );
    assert_eq!(app.queue_selected_index, 2);
  }

  #[test]
  fn reorder_at_bounds_is_noop() {
    let mut app = app_with_queue(&["A", "B"]);
    // First item can't move up.
    app.queue_selected_index = 1;
    reorder(-1, &mut app);
    assert_eq!(app.native_queue[0].name, "A");
    assert_eq!(app.queue_selected_index, 1);
    // Last item can't move down.
    app.queue_selected_index = 2;
    reorder(1, &mut app);
    assert_eq!(app.native_queue[1].name, "B");
    assert_eq!(app.queue_selected_index, 2);
    // Now-playing row is non-actionable.
    app.queue_selected_index = 0;
    reorder(1, &mut app);
    assert_eq!(app.queue_selected_index, 0);
  }
}
