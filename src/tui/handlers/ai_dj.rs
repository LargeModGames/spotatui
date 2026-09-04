//! Key handling for the AI DJ screen.
//!
//! Two things here are easy to get wrong and both are guarded by tests:
//!
//! * The prompt is a typing surface, so `handle_app`'s global bindings must be
//!   bypassed while it has focus — otherwise `d` opens the device picker instead
//!   of typing a `d`. See the early return in `handlers::mod`.
//! * Cursor position is tracked in **display columns**, not character count, so a
//!   CJK or emoji prompt keeps its caret in the right place.

use super::common_key_events;
use crate::core::action::{Action, LibraryTarget};
use crate::core::app::App;
use crate::infra::dj::setup::{DjSetup, DjSetupStep};
use crate::tui::event::Key;
use unicode_width::UnicodeWidthChar;

pub fn handler(key: Key, app: &mut App) {
  // The picker is modal and this has to be the very first branch. Below it,
  // `Key::Char(c) => insert(app, c)` is total over every character, so a picker arm
  // placed after it would be unreachable and letters would type into the prompt
  // behind the overlay. Above it, the modifier block would let Ctrl+T or Ctrl+O
  // start minutes of background work with the very backend the user is part way
  // through changing.
  if app.dj.setup.is_some() {
    setup_key(key, app);
    return;
  }

  // The DJ's own action keys, which `handle_app` never sees here: this screen takes
  // every key so the prompt can contain 'd', ' ', and the rest. Without this the
  // toggles are only reachable from *other* screens, which is where you least want
  // them.
  //
  // Only honoured for bindings that carry a modifier. A user who rebinds one to a
  // bare character has to be able to type that character.
  if !matches!(key, Key::Char(_)) {
    let keys = &app.user_config.keys;
    if key == keys.dj_toggle_auto_queue {
      app.apply(Action::ToggleDjAutoQueue);
      return;
    }
    if key == keys.dj_vibe_shift {
      app.apply(Action::DjVibeShift);
      return;
    }
    if key == keys.dj_toggle_fresh_only {
      app.apply(Action::ToggleDjFreshOnly);
      return;
    }
    if key == keys.dj_pick_model {
      app.apply(Action::OpenDjSetup);
      return;
    }
  }

  match key {
    // Escape clears a half-typed prompt first, and only leaves the screen when
    // there is nothing to abandon. The single copy of the rule: `handle_escape`
    // delegates its `ActiveBlock::AiDj` arm back here rather than repeating it.
    Key::Esc => {
      if app.dj.input.is_empty() {
        app.pop_navigation_stack();
      } else {
        app.dj.input.clear();
        app.dj.input_idx = 0;
        app.dj.input_cursor = 0;
      }
    }
    // Non-character keys only, for the same reason the modifier block above says:
    // `keys.move_left` defaults to a bare `h`, and this arm sits above
    // `Key::Char(c)`, so without the guard a prompt could never begin with one.
    k if !matches!(k, Key::Char(_))
      && common_key_events::left_event(k, &app.user_config.keys)
      && app.dj.input.is_empty() =>
    {
      common_key_events::handle_left_event(app)
    }
    Key::Enter => submit(app),
    Key::Char(c) => insert(app, c),
    Key::Backspace | Key::Ctrl('h') => backspace(app),
    Key::Delete | Key::Ctrl('d') => delete(app),
    Key::Left | Key::Ctrl('b') => move_left(app),
    Key::Right | Key::Ctrl('f') => move_right(app),
    Key::Home | Key::Ctrl('a') => {
      app.dj.input_idx = 0;
      app.dj.input_cursor = 0;
    }
    Key::End | Key::Ctrl('e') => {
      app.dj.input_idx = app.dj.input.len();
      app.dj.input_cursor = display_width(&app.dj.input);
    }
    Key::Ctrl('u') => {
      app.dj.input.clear();
      app.dj.input_idx = 0;
      app.dj.input_cursor = 0;
    }
    // Scrolling is measured in wrapped display rows, matching what the renderer
    // windows — see `ui::ai_dj`.
    //
    // Arrow and page keys only: `k`/`j` deliberately do NOT scroll here, because
    // `Key::Char` is matched above and a text prompt must be able to type them.
    Key::Up => scroll_back(app, 1),
    Key::Down => scroll_forward(app, 1),
    Key::PageUp => scroll_back(app, 10),
    Key::PageDown => scroll_forward(app, 10),
    _ => {}
  }
}

fn display_width(chars: &[char]) -> u16 {
  chars
    .iter()
    .map(|c| c.width().unwrap_or(0) as u16)
    .sum::<u16>()
}

fn insert(app: &mut App, c: char) {
  let idx = app.dj.input_idx.min(app.dj.input.len());
  app.dj.input.insert(idx, c);
  app.dj.input_idx = idx + 1;
  app.dj.input_cursor = app
    .dj
    .input_cursor
    .saturating_add(c.width().unwrap_or(0) as u16);
}

fn backspace(app: &mut App) {
  if app.dj.input_idx == 0 {
    return;
  }
  let removed = app.dj.input.remove(app.dj.input_idx - 1);
  app.dj.input_idx -= 1;
  app.dj.input_cursor = app
    .dj
    .input_cursor
    .saturating_sub(removed.width().unwrap_or(0) as u16);
}

fn delete(app: &mut App) {
  if app.dj.input_idx < app.dj.input.len() {
    app.dj.input.remove(app.dj.input_idx);
  }
}

fn move_left(app: &mut App) {
  if app.dj.input_idx == 0 {
    return;
  }
  app.dj.input_idx -= 1;
  let width = app.dj.input[app.dj.input_idx].width().unwrap_or(0) as u16;
  app.dj.input_cursor = app.dj.input_cursor.saturating_sub(width);
}

fn move_right(app: &mut App) {
  if app.dj.input_idx >= app.dj.input.len() {
    return;
  }
  let width = app.dj.input[app.dj.input_idx].width().unwrap_or(0) as u16;
  app.dj.input_idx += 1;
  app.dj.input_cursor = app.dj.input_cursor.saturating_add(width);
}

fn scroll_back(app: &mut App, amount: u16) {
  let bound = crate::tui::ui::ai_dj::max_scroll_bound(app);
  app.dj.scroll = app.dj.scroll.saturating_add(amount).min(bound);
}

fn scroll_forward(app: &mut App, amount: u16) {
  app.dj.scroll = app.dj.scroll.saturating_sub(amount);
}

/// Send what the user typed to the DJ.
fn submit(app: &mut App) {
  let text: String = app.dj.input.iter().collect();
  app.apply(Action::AskDj(text));
}

/// Open the DJ screen, warming the library index if the filter is already on.
/// The whole consequence lives on `App` (`open_ai_dj_screen`) and is reached
/// through the shared `Action::OpenLibrary(LibraryTarget::AiDj)` vocabulary,
/// so every entry point (key, library row) fires the same sequence.
pub fn open(app: &mut App) {
  app.apply(Action::OpenLibrary(LibraryTarget::AiDj));
}

/// What a keypress means to the picker.
///
/// Resolved while `app.dj.setup` is borrowed, then acted on after that borrow drops.
/// Without this split, Enter (which has to touch `app.user_config` and `app.dj`
/// together) cannot be expressed at all.
enum SetupIntent {
  MoveUp,
  MoveDown,
  /// 1-based row, from the digit shortcuts.
  Select(usize),
  /// Enter on a list row.
  Advance,
  /// Esc from the Model or Custom step: one step back.
  Back,
  /// Esc from the Backend step: dismiss, which is itself an answer.
  Dismiss,
  /// Enter on the free-text step.
  CommitCustom,
  Push(char),
  Backspace,
  ClearCustom,
  Ignore,
}

fn setup_key(key: Key, app: &mut App) {
  // Resolve the navigation intent against the user's bindings *before* borrowing
  // `app.dj` mutably, so `j`/`k` keep working when they have been rebound.
  let goes_down = common_key_events::down_event(key, &app.user_config.keys);
  let goes_up = common_key_events::up_event(key, &app.user_config.keys);

  let intent = {
    let Some(setup) = app.dj.setup.as_ref() else {
      return;
    };
    match setup.step {
      // A typing surface: no j/k navigation here, or a model name could not contain
      // a 'j'. The same rule the DJ prompt itself follows.
      DjSetupStep::Custom => match key {
        Key::Esc => SetupIntent::Back,
        Key::Enter => SetupIntent::CommitCustom,
        Key::Backspace | Key::Ctrl('h') => SetupIntent::Backspace,
        Key::Ctrl('u') => SetupIntent::ClearCustom,
        Key::Char(c) => SetupIntent::Push(c),
        _ => SetupIntent::Ignore,
      },
      DjSetupStep::Backend => match key {
        Key::Esc => SetupIntent::Dismiss,
        Key::Enter => SetupIntent::Advance,
        Key::Up => SetupIntent::MoveUp,
        Key::Down => SetupIntent::MoveDown,
        // Matched ahead of the movement guards so a user who has rebound
        // `move_down` to a digit still gets the digit's row-select meaning here.
        Key::Char(c @ '1'..='9') => SetupIntent::Select(c as usize - '0' as usize),
        // `_ if` rather than `k if`: a bound-but-unused binding is a warning and CI
        // clippy runs with `-D warnings`.
        _ if goes_up => SetupIntent::MoveUp,
        _ if goes_down => SetupIntent::MoveDown,
        // Everything else is swallowed. That is the point of a modal: no global
        // binding, no DJ action, no navigation escapes it.
        _ => SetupIntent::Ignore,
      },
      DjSetupStep::Model => match key {
        Key::Esc => SetupIntent::Back,
        Key::Enter => SetupIntent::Advance,
        Key::Up => SetupIntent::MoveUp,
        Key::Down => SetupIntent::MoveDown,
        Key::Char(c @ '1'..='9') => SetupIntent::Select(c as usize - '0' as usize),
        _ if goes_up => SetupIntent::MoveUp,
        _ if goes_down => SetupIntent::MoveDown,
        _ => SetupIntent::Ignore,
      },
    }
  };

  apply_setup_intent(intent, app);
}

fn apply_setup_intent(intent: SetupIntent, app: &mut App) {
  match intent {
    SetupIntent::MoveUp => {
      if let Some(setup) = app.dj.setup.as_mut() {
        setup.move_up();
      }
    }
    SetupIntent::MoveDown => {
      if let Some(setup) = app.dj.setup.as_mut() {
        setup.move_down();
      }
    }
    SetupIntent::Select(row) => {
      // A digit picks its row outright rather than only moving the cursor: the rows
      // are numbered on screen, and a shortcut that still needs an Enter saves
      // nothing.
      let picked = app
        .dj
        .setup
        .as_mut()
        .is_some_and(|setup| setup.select_row(row));
      if picked {
        advance(app);
      }
    }
    SetupIntent::Advance => advance(app),
    SetupIntent::Back => {
      if let Some(setup) = app.dj.setup.as_mut() {
        setup.step = match setup.step {
          DjSetupStep::Custom => DjSetupStep::Model,
          _ => DjSetupStep::Backend,
        };
      }
    }
    SetupIntent::Dismiss => {
      app.apply(Action::DismissDjSetup);
    }
    SetupIntent::CommitCustom => {
      app.apply(Action::CommitDjSetup);
    }
    SetupIntent::Push(c) => {
      if let Some(setup) = app.dj.setup.as_mut() {
        setup.custom.push(c);
      }
    }
    SetupIntent::Backspace => {
      if let Some(setup) = app.dj.setup.as_mut() {
        setup.custom.pop();
      }
    }
    SetupIntent::ClearCustom => {
      if let Some(setup) = app.dj.setup.as_mut() {
        setup.custom.clear();
      }
    }
    SetupIntent::Ignore => {}
  }
}

/// Enter on a list row: advance a step, open the free-text step, or finish.
fn advance(app: &mut App) {
  let Some(step) = app.dj.setup.as_ref().map(|setup| setup.step) else {
    return;
  };
  match step {
    DjSetupStep::Backend => {
      if let Some(setup) = app.dj.setup.as_mut() {
        setup.enter_model_step(&app.user_config.behavior);
      }
    }
    DjSetupStep::Model => {
      let wants_free_text = app
        .dj
        .setup
        .as_ref()
        .and_then(DjSetup::selected_model)
        .is_some_and(|model| model.custom);
      if wants_free_text {
        if let Some(setup) = app.dj.setup.as_mut() {
          setup.enter_custom_step(&app.user_config.behavior);
        }
      } else {
        app.apply(Action::CommitDjSetup);
      }
    }
    DjSetupStep::Custom => {
      app.apply(Action::CommitDjSetup);
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::{ActiveBlock, RouteId};
  use crate::core::user_config::{UserConfig, UserConfigPaths};
  use crate::infra::dj::{DjLine, TurnKind};
  use crate::infra::network::IoEvent;
  use std::sync::mpsc::{channel, Receiver};
  use std::time::SystemTime;

  fn dj_app() -> (App, Receiver<IoEvent>) {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.push_navigation_stack(RouteId::AiDj, ActiveBlock::AiDj);
    (app, rx)
  }

  fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
      handler(Key::Char(c), app);
    }
  }

  #[test]
  fn typing_fills_the_prompt_and_tracks_the_cursor() {
    let (mut app, _rx) = dj_app();
    type_text(&mut app, "chill");
    assert_eq!(app.dj.input.iter().collect::<String>(), "chill");
    assert_eq!(app.dj.input_idx, 5);
    assert_eq!(app.dj.input_cursor, 5);
  }

  #[test]
  fn the_cursor_counts_display_columns_not_characters() {
    let (mut app, _rx) = dj_app();
    type_text(&mut app, "東京");
    assert_eq!(app.dj.input.len(), 2, "two characters");
    assert_eq!(app.dj.input_cursor, 4, "but four columns");
  }

  #[test]
  fn backspace_removes_the_character_before_the_cursor() {
    let (mut app, _rx) = dj_app();
    type_text(&mut app, "abc");
    handler(Key::Backspace, &mut app);
    assert_eq!(app.dj.input.iter().collect::<String>(), "ab");
    assert_eq!(app.dj.input_cursor, 2);
  }

  #[test]
  fn backspace_on_an_empty_prompt_is_a_no_op() {
    let (mut app, _rx) = dj_app();
    handler(Key::Backspace, &mut app);
    assert!(app.dj.input.is_empty());
    assert_eq!(app.dj.input_idx, 0);
  }

  #[test]
  fn arrows_move_within_the_prompt_and_clamp_at_the_ends() {
    let (mut app, _rx) = dj_app();
    type_text(&mut app, "ab");
    handler(Key::Left, &mut app);
    handler(Key::Left, &mut app);
    handler(Key::Left, &mut app);
    assert_eq!(app.dj.input_idx, 0);
    assert_eq!(app.dj.input_cursor, 0);
    for _ in 0..5 {
      handler(Key::Right, &mut app);
    }
    assert_eq!(app.dj.input_idx, 2);
    assert_eq!(app.dj.input_cursor, 2);
  }

  #[test]
  fn insert_in_the_middle_lands_at_the_cursor() {
    let (mut app, _rx) = dj_app();
    type_text(&mut app, "ac");
    handler(Key::Left, &mut app);
    handler(Key::Char('b'), &mut app);
    assert_eq!(app.dj.input.iter().collect::<String>(), "abc");
  }

  #[test]
  fn ctrl_u_clears_the_prompt() {
    let (mut app, _rx) = dj_app();
    type_text(&mut app, "scrap this");
    handler(Key::Ctrl('u'), &mut app);
    assert!(app.dj.input.is_empty());
    assert_eq!(app.dj.input_cursor, 0);
  }

  #[test]
  fn enter_dispatches_ask_dj_and_clears_the_prompt() {
    let (mut app, rx) = dj_app();
    type_text(&mut app, "something chill");
    handler(Key::Enter, &mut app);

    assert!(app.dj.input.is_empty(), "prompt should clear on submit");
    assert!(app.dj.thinking, "the turn is in flight");

    // Exactly one line, and the handler owns it. The brain reads it back out of
    // the transcript, so the request must not carry it a second time — see
    // `session::history_from_transcript`.
    assert_eq!(app.dj.transcript.len(), 1);
    assert_eq!(app.dj.transcript[0], DjLine::user("something chill"));

    let event = rx.try_recv().expect("an AskDj should be dispatched");
    match event {
      IoEvent::AskDj(request) => {
        assert!(
          request.extra_instruction.is_none(),
          "a typed prompt travels in the transcript, not alongside it"
        );
        assert_eq!(request.generation, app.dj.generation);
        assert!(
          !request.must_act,
          "the listener is watching, so the DJ may ask them something"
        );
        // The words become the standing vibe only if this turn actually queues.
        assert_eq!(request.vibe_on_success.as_deref(), Some("something chill"));
        assert!(
          app.dj.vibe.is_none(),
          "nothing has been queued yet, so there is no direction to follow"
        );
      }
      other => panic!("unexpected event: {:?}", std::mem::discriminant(&other)),
    }
  }

  #[test]
  fn enter_on_an_empty_prompt_dispatches_nothing() {
    let (mut app, rx) = dj_app();
    handler(Key::Enter, &mut app);
    assert!(rx.try_recv().is_err());
    assert!(!app.dj.thinking);
  }

  #[test]
  fn a_whitespace_only_prompt_dispatches_nothing() {
    let (mut app, rx) = dj_app();
    type_text(&mut app, "   ");
    handler(Key::Enter, &mut app);
    assert!(rx.try_recv().is_err());
  }

  #[test]
  fn submitting_while_thinking_is_refused_rather_than_stacking_turns() {
    let (mut app, rx) = dj_app();
    app.dj.thinking = true;
    type_text(&mut app, "again");
    handler(Key::Enter, &mut app);
    assert!(rx.try_recv().is_err(), "must not stack a second turn");
    assert!(app.status_message().is_some());
    // Refusing the turn *and* eating the request is the worst of both: the user
    // spent a minute typing it and has nothing to resend.
    assert_eq!(
      app.dj.input.iter().collect::<String>(),
      "again",
      "a refused turn must leave the typed prompt in the box"
    );
  }

  #[test]
  fn opening_the_dj_while_it_is_already_open_does_not_stack_a_second_route() {
    let (mut app, _rx) = dj_app();
    // Reachable in practice: Left moves focus to the sidebar, so `handle_app`'s
    // `ActiveBlock::AiDj` early return no longer fires and the open key (or the
    // sidebar's own row) calls straight back into `open`.
    app.set_current_route_state(Some(ActiveBlock::Library), None);
    open(&mut app);
    assert_eq!(
      app.get_current_route().active_block,
      ActiveBlock::AiDj,
      "focus should come back to the DJ rather than stay on the sidebar"
    );
    // The actual symptom of a stacked duplicate: Esc leaves the DJ and lands
    // straight back on it.
    app.pop_navigation_stack();
    assert_ne!(
      app.get_current_route().id,
      RouteId::AiDj,
      "one Esc must be enough to leave the DJ"
    );
  }

  #[test]
  fn opening_the_picker_from_the_sidebar_gives_it_the_keyboard() {
    // Same reachable state as the test above, and the symptom is worse: the modal
    // paints, but `handle_app` keeps routing keys to the sidebar's handler, so the
    // picker is visible and accepts nothing.
    let (mut app, _rx) = dj_app();
    app.set_current_route_state(Some(ActiveBlock::Library), None);
    app.apply(Action::OpenDjSetup);
    assert!(app.dj.setup.is_some(), "the picker should be open");
    assert_eq!(
      app.get_current_route().active_block,
      ActiveBlock::AiDj,
      "a painted modal that cannot be typed into is worse than no modal"
    );
    app.pop_navigation_stack();
    assert_ne!(
      app.get_current_route().id,
      RouteId::AiDj,
      "one Esc must be enough to leave the DJ"
    );
  }

  #[test]
  fn a_prompt_can_begin_with_the_move_left_character() {
    // `keys.move_left` is a bare `h` by default and the left arm sits above
    // `Key::Char(c)`, so without a guard the prompt could never start with one.
    let (mut app, _rx) = dj_app();
    type_text(&mut app, "hello");
    assert_eq!(app.dj.input.iter().collect::<String>(), "hello");
    assert_eq!(
      app.get_current_route().active_block,
      ActiveBlock::AiDj,
      "typing must not move focus to the sidebar"
    );
    // The real left bindings still leave an empty prompt.
    app.dj.input.clear();
    app.dj.input_idx = 0;
    app.dj.input_cursor = 0;
    handler(Key::Left, &mut app);
    assert_eq!(
      app.get_current_route().active_block,
      ActiveBlock::Empty,
      "the arrow still hands focus to the sidebar"
    );
  }

  #[test]
  fn turning_auto_queue_off_does_not_abandon_a_question_in_flight() {
    // The listener asked something and then flipped the toggle. Clearing the flag
    // would let `submit` start a second brain call beside the first, and bumping
    // the generation would make the answer they are waiting for land and be
    // discarded.
    let (mut app, rx) = dj_app();
    type_text(&mut app, "something warm");
    handler(Key::Enter, &mut app);
    assert!(matches!(rx.try_recv(), Ok(IoEvent::AskDj(_))));
    let generation = app.dj.generation;

    app.dj.auto_queue = true;
    app.apply(Action::ToggleDjAutoQueue);

    assert!(!app.dj.auto_queue);
    assert!(app.dj.thinking, "the question is still being answered");
    assert_eq!(
      app.dj.generation, generation,
      "bumping it would throw away the answer the listener is waiting for"
    );
  }

  #[test]
  fn turning_auto_queue_off_still_abandons_a_refill() {
    let (mut app, _rx) = dj_app();
    app.dj.auto_queue = true;
    app.dj.begin_turn(TurnKind::Refill);
    app.dj.step = Some((2, 4));
    let generation = app.dj.generation;

    app.apply(Action::ToggleDjAutoQueue);

    assert!(!app.dj.thinking, "nobody is waiting on a refill");
    assert!(
      app.dj.step.is_none(),
      "a counter left from an abandoned turn would read as the next one's progress"
    );
    assert_ne!(app.dj.generation, generation);
  }

  #[test]
  fn scrolling_back_is_bounded_by_the_transcript() {
    let (mut app, _rx) = dj_app();
    app
      .dj
      .push_line(DjLine::system("one short line".to_string()));
    for _ in 0..500 {
      scroll_back(&mut app, 1);
    }
    // Unbounded, this needed 500 presses of Down before anything moved.
    assert!(app.dj.scroll < 50, "scroll ran away to {}", app.dj.scroll);
  }

  #[test]
  fn submitting_bumps_the_generation_so_a_pending_refill_is_dropped() {
    let (mut app, _rx) = dj_app();
    let before = app.dj.generation;
    type_text(&mut app, "new direction");
    handler(Key::Enter, &mut app);
    assert_ne!(app.dj.generation, before);
  }

  #[test]
  fn dj_events_do_not_pin_the_global_spinner() {
    // A brain call runs for up to `dj_agent_timeout_secs`, and `App::dispatch`
    // sets `is_loading` until the service-lane task finishes — pinning the global
    // spinner for minutes. `dj.thinking` is the DJ's progress surface; the global
    // one must stay free for ordinary work.
    let (mut app, rx) = dj_app();
    type_text(&mut app, "something chill");
    handler(Key::Enter, &mut app);
    assert!(matches!(rx.try_recv(), Ok(IoEvent::AskDj(_))));
    assert!(!app.is_loading, "submit must not set the global spinner");

    app.dj.thinking = false;
    app.apply(Action::DjVibeShift);
    assert!(matches!(rx.try_recv(), Ok(IoEvent::AskDj(_))));
    assert!(!app.is_loading, "a vibe shift must not either");

    app.dj.thinking = false;
    app.apply(Action::ToggleDjAutoQueue);
    assert!(matches!(rx.try_recv(), Ok(IoEvent::DjTopUp(..))));
    assert!(!app.is_loading, "nor the auto-queue toggle's refill");
  }

  #[test]
  fn auto_queue_waits_while_an_external_spotify_device_plays() {
    // On a remote Connect device queued tracks go to the Web API queue, so the
    // native queue never fills and a length-based refill would fire after every
    // batch, forever. The toggle stays on, but no refill is dispatched.
    let (mut app, rx) = dj_app();
    app.current_playback_context = Some(external_spotify_context());
    assert!(app.spotify_external_device_active());

    app.apply(Action::ToggleDjAutoQueue);
    assert!(app.dj.auto_queue, "the preference itself still flips on");
    assert!(
      rx.try_recv().is_err(),
      "but no refill may be dispatched against an unobservable queue"
    );
    assert!(!app.dj.thinking);
    assert!(
      app
        .status_message()
        .is_some_and(|message| message.contains("another Spotify device")),
      "and the pause is said out loud, or auto-queue just looks broken"
    );
  }

  fn external_spotify_context() -> rspotify::model::context::CurrentPlaybackContext {
    use rspotify::model::{
      context::{Actions, CurrentPlaybackContext},
      CurrentlyPlayingType, Device, DeviceType, RepeatState,
    };
    CurrentPlaybackContext {
      device: Device {
        id: Some("external-device".to_string()),
        is_active: true,
        is_private_session: false,
        is_restricted: false,
        name: "Phone".to_string(),
        _type: DeviceType::Smartphone,
        volume_percent: Some(50),
      },
      repeat_state: RepeatState::Off,
      shuffle_state: false,
      context: None,
      timestamp: chrono::Utc::now(),
      progress: None,
      is_playing: true,
      item: None,
      currently_playing_type: CurrentlyPlayingType::Track,
      actions: Actions::default(),
    }
  }

  #[test]
  fn vim_keys_type_rather_than_scroll() {
    // A text prompt must be able to contain 'k' and 'j'. Scrolling is on the
    // arrow and page keys instead.
    let (mut app, _rx) = dj_app();
    type_text(&mut app, "kj");
    assert_eq!(app.dj.input.iter().collect::<String>(), "kj");
    assert_eq!(app.dj.scroll, 0, "typing must never scroll");
  }

  #[test]
  fn arrow_and_page_keys_scroll_the_transcript() {
    let (mut app, _rx) = dj_app();
    // Enough history to have somewhere to scroll back to: the offset is clamped
    // against the transcript, so an empty one correctly refuses to move.
    for i in 0..40 {
      app.dj.push_line(DjLine::system(format!("line {i}")));
    }
    handler(Key::Up, &mut app);
    assert_eq!(app.dj.scroll, 1);
    handler(Key::Down, &mut app);
    assert_eq!(app.dj.scroll, 0);

    // Still works with text in the prompt: there is no multi-line editing to
    // conflict with.
    type_text(&mut app, "text");
    handler(Key::PageUp, &mut app);
    assert_eq!(app.dj.scroll, 10);
    handler(Key::PageDown, &mut app);
    assert_eq!(app.dj.scroll, 0);
  }

  #[test]
  fn toggling_auto_queue_on_with_a_short_queue_asks_for_a_refill() {
    let (mut app, rx) = dj_app();
    app.apply(Action::ToggleDjAutoQueue);
    assert!(app.dj.auto_queue);
    match rx.try_recv().expect("a top-up should be dispatched") {
      IoEvent::DjTopUp(generation, _) => assert_eq!(generation, app.dj.generation),
      _ => panic!("expected DjTopUp"),
    }
  }

  #[test]
  fn toggling_auto_queue_off_invalidates_work_in_flight() {
    let (mut app, _rx) = dj_app();
    app.apply(Action::ToggleDjAutoQueue);
    let generation_on = app.dj.generation;
    app.dj.thinking = true;
    app.apply(Action::ToggleDjAutoQueue);
    assert!(!app.dj.auto_queue);
    assert!(!app.dj.thinking, "a pending refill must be released");
    assert_ne!(app.dj.generation, generation_on);
  }

  #[test]
  fn a_vibe_shift_bumps_the_generation_and_asks_again() {
    let (mut app, rx) = dj_app();
    let before = app.dj.generation;
    app.apply(Action::DjVibeShift);
    assert_ne!(app.dj.generation, before);
    assert!(app.dj.thinking);
    match rx.try_recv().expect("an AskDj should be dispatched") {
      // The steer rides alongside: the listener never typed it, so it is not in
      // the transcript for the brain to read.
      IoEvent::AskDj(request) => {
        assert!(request
          .extra_instruction
          .as_deref()
          .is_some_and(|steer| steer.contains("Change direction")));
        // A shift has just dropped the DJ's queued tracks, so the turn has to
        // refill rather than stopping to ask what "different" means.
        assert!(
          request.must_act,
          "a vibe shift may not come back with words"
        );
      }
      other => panic!("unexpected event: {:?}", std::mem::discriminant(&other)),
    }
    let last = app.dj.transcript.last().unwrap();
    assert!(last.text.contains("vibe"));
    assert_eq!(
      last.speaker,
      crate::infra::dj::DjSpeaker::System,
      "the shift is announced, not spoken by the listener"
    );
  }

  #[test]
  fn a_vibe_shift_while_thinking_is_refused() {
    let (mut app, rx) = dj_app();
    app.dj.thinking = true;
    app.apply(Action::DjVibeShift);
    assert!(rx.try_recv().is_err());
  }

  #[test]
  fn toggling_fresh_only_starts_the_crawl_once() {
    let (mut app, rx) = dj_app();
    assert!(!app.dj.avoid_library, "off unless configured on");

    app.apply(Action::ToggleDjFreshOnly);
    assert!(app.dj.avoid_library);
    assert!(matches!(rx.try_recv(), Ok(IoEvent::DjIndexLibrary)));

    // Off, then on again with the index already cached: no second crawl.
    app.apply(Action::ToggleDjFreshOnly);
    assert!(!app.dj.avoid_library);
    app.dj.library = Some(crate::infra::dj::DjLibrary::default());
    app.apply(Action::ToggleDjFreshOnly);
    assert!(app.dj.avoid_library);
    assert!(
      rx.try_recv().is_err(),
      "a cached index must not be rebuilt on every toggle"
    );
  }

  #[test]
  fn the_config_default_seeds_the_toggle() {
    let (tx, _rx) = channel();
    let mut config = UserConfig::new();
    config.behavior.dj_avoid_library = true;
    let app = App::new(tx, config, Some(SystemTime::now()));
    assert!(
      app.dj.avoid_library,
      "behavior.dj_avoid_library has to reach DjState or the config does nothing"
    );
  }

  #[test]
  fn opening_the_screen_with_the_filter_already_on_starts_the_crawl() {
    // The config-default path never runs `ToggleDjFreshOnly`, so opening the
    // screen is the only chance to warm the index. Without this the first turn
    // crawls inline on the serial lane, blocking every other event behind it.
    let (tx, rx) = channel();
    let mut config = UserConfig::new();
    config.behavior.dj_avoid_library = true;
    let mut app = App::new(tx, config, Some(SystemTime::now()));

    open(&mut app);
    assert_eq!(app.get_current_route().id, RouteId::AiDj);
    assert!(matches!(rx.try_recv(), Ok(IoEvent::DjIndexLibrary)));
  }

  #[test]
  fn opening_the_screen_with_the_filter_off_crawls_nothing() {
    // The crawl is the whole cost of this feature; a listener who never turns it
    // on must never pay it.
    let (mut app, rx) = dj_app();
    app.pop_navigation_stack();
    open(&mut app);
    assert!(rx.try_recv().is_err());
  }

  #[test]
  fn the_dj_action_keys_work_while_the_prompt_has_focus() {
    // `handle_app` hands this screen every key so the prompt can contain 'd' and
    // ' ', which means the DJ's own shortcuts have to be honoured here or they are
    // unreachable from the one screen they belong to.
    let (mut app, _rx) = dj_app();
    handler(app.user_config.keys.dj_toggle_fresh_only, &mut app);
    assert!(app.dj.avoid_library);
    handler(app.user_config.keys.dj_toggle_auto_queue, &mut app);
    assert!(app.dj.auto_queue);
    assert!(
      app.dj.input.is_empty(),
      "an action key must not type itself"
    );
  }

  #[test]
  fn a_bare_character_binding_still_types_instead_of_acting() {
    // A user is free to rebind the toggle to 'x'. On a typing surface the
    // character has to win, or the prompt cannot contain it.
    let (mut app, _rx) = dj_app();
    app.user_config.keys.dj_toggle_fresh_only = Key::Char('x');
    handler(Key::Char('x'), &mut app);
    assert_eq!(app.dj.input.iter().collect::<String>(), "x");
    assert!(!app.dj.avoid_library);
  }

  #[test]
  fn the_back_key_leaves_the_screen() {
    let (mut app, _rx) = dj_app();
    handler(Key::Esc, &mut app);
    assert_ne!(app.get_current_route().id, RouteId::AiDj);
  }

  #[test]
  fn the_back_key_is_prompt_text_here_rather_than_a_way_out() {
    // Nothing in this handler treats the back key specially, and that is the point:
    // it defaults to `q`, so a DJ that consumed it could never be told about Queen.
    // The other half of the rule is in `runner::dispatch_key`, which has to let the
    // key through instead of popping the route before this function is ever called.
    let (mut app, _rx) = dj_app();
    let back = app.user_config.keys.back;
    assert_eq!(back, Key::Char('q'), "the default this test is about");

    handler(back, &mut app);

    assert_eq!(app.dj.input.iter().collect::<String>(), "q");
    assert_eq!(app.get_current_route().id, RouteId::AiDj);
  }

  #[test]
  fn the_picker_swallows_letters_instead_of_typing_them() {
    // The picker branch has to sit above `Key::Char(c) => insert`, which is total
    // over every character; below it the letters would land in the prompt behind
    // the overlay.
    let (mut app, _rx) = dj_app();
    app.apply(Action::OpenDjSetup);
    handler(Key::Char('j'), &mut app);
    handler(Key::Char('q'), &mut app);
    assert!(app.dj.input.is_empty());
    assert!(app.dj.setup.is_some(), "and 'q' does not leave the screen");
  }

  #[test]
  fn the_picker_swallows_the_dj_action_keys_so_none_start_background_work() {
    // The picker branch also has to sit above the modifier block: these keys start
    // minutes of work with the very backend the user is part way through changing.
    let (mut app, rx) = dj_app();
    app.apply(Action::OpenDjSetup);
    let keys = app.user_config.keys.clone();
    handler(keys.dj_toggle_auto_queue, &mut app);
    handler(keys.dj_vibe_shift, &mut app);
    handler(keys.dj_toggle_fresh_only, &mut app);
    assert!(rx.try_recv().is_err(), "nothing may be dispatched");
    assert!(!app.dj.auto_queue);
    assert!(!app.dj.avoid_library);
    assert!(!app.dj.thinking);
    assert!(app.dj.setup.is_some());
  }

  #[test]
  fn escape_steps_back_through_the_picker_before_leaving_the_screen() {
    // The dismiss below is the one keypress in this file that reaches
    // `save_config`, so the path is redirected at a scratch file first. That is
    // what makes the seam safe to touch at all (it would otherwise rewrite the
    // developer's real ~/.config/spotatui/config.yml during `cargo test`), and it
    // is also the only way to see the *success* branch: with `UserConfig::new()`'s
    // empty path every run takes the error branch instead.
    let dir = tempfile::tempdir().expect("a writable scratch directory");
    let (mut app, _rx) = dj_app();
    app.user_config.path_to_config = Some(UserConfigPaths {
      config_file_path: dir.path().join("config.yml"),
    });

    app.apply(Action::OpenDjSetup);
    handler(Key::Enter, &mut app);
    assert_eq!(app.dj.setup.as_ref().unwrap().step, DjSetupStep::Model);

    handler(Key::Esc, &mut app);
    assert_eq!(app.dj.setup.as_ref().unwrap().step, DjSetupStep::Backend);

    // Dismissing counts as an answer, or `open` would ask again on every entry.
    handler(Key::Esc, &mut app);
    assert!(app.dj.setup.is_none());
    assert_eq!(app.user_config.behavior.dj_configured, Some(true));
    assert_eq!(app.get_current_route().id, RouteId::AiDj, "still on the DJ");
    // Assert the branch, not merely that *something* was said: both branches of
    // `persist_dj_setup` write `status_message`, and only `status_message_is_error`
    // tells them apart. `status_message.is_some()` alone would hold even with the
    // error guard deleted outright.
    assert!(!app.status_message_is_error());
    assert_eq!(app.status_message(), Some("keeping the current DJ brain"));
    assert!(
      dir.path().join("config.yml").exists(),
      "and the marker actually landed, which is the only reason to persist"
    );

    handler(Key::Esc, &mut app);
    assert_ne!(app.get_current_route().id, RouteId::AiDj);
  }

  #[test]
  fn a_dismiss_that_cannot_reach_disk_says_so_instead_of_re_prompting_forever() {
    // The in-memory marker stops the re-prompt for this session only. If the write
    // fails, the next launch asks again, so the failure has to reach the user: a
    // logged-and-swallowed error is indistinguishable from the picker being broken.
    let dir = tempfile::tempdir().expect("a writable scratch directory");
    let (mut app, _rx) = dj_app();
    app.user_config.path_to_config = Some(UserConfigPaths {
      // A missing parent rather than a read-only directory: `write_private_file`
      // fails at `open` on every platform and for every user, including the root
      // account CI containers usually run as, which ignores mode bits.
      config_file_path: dir.path().join("no-such-directory").join("config.yml"),
    });

    app.apply(Action::OpenDjSetup);
    handler(Key::Esc, &mut app);

    assert!(app.dj.setup.is_none(), "the picker still closes");
    assert!(
      app.status_message_is_error(),
      "a save that never landed must be surfaced, not logged"
    );
    assert!(app
      .status_message()
      .is_some_and(|message| message.starts_with("DJ: could not save that choice")));
  }

  #[test]
  fn a_digit_picks_its_row_outright() {
    // The rows are numbered on screen, so a digit that only moved the cursor would
    // be a shortcut that saves nothing.
    let (mut app, _rx) = dj_app();
    app.apply(Action::OpenDjSetup);
    handler(Key::Char('2'), &mut app);
    let setup = app.dj.setup.as_ref().unwrap();
    assert_eq!(setup.backend_index, 1);
    assert_eq!(setup.step, DjSetupStep::Model, "and advances with it");
  }

  #[test]
  fn a_digit_on_the_free_text_row_opens_the_typing_step_rather_than_doing_nothing() {
    // The digit path reaches `choice()`, which refuses the "Custom…" row. Without
    // the free-text branch in `advance` that refusal would read as a dead key.
    let (mut app, _rx) = dj_app();
    app.apply(Action::OpenDjSetup);
    handler(Key::Enter, &mut app);
    let custom_row = app.dj.setup.as_ref().unwrap().models.len();
    assert!(custom_row <= 9, "the claude list is short enough to reach");
    handler(Key::Char((b'0' + custom_row as u8) as char), &mut app);
    assert_eq!(app.dj.setup.as_ref().unwrap().step, DjSetupStep::Custom);
  }

  #[test]
  fn the_custom_row_opens_a_typing_step_that_takes_letters() {
    let (mut app, _rx) = dj_app();
    app.apply(Action::OpenDjSetup);
    handler(Key::Enter, &mut app);
    let last = app.dj.setup.as_ref().unwrap().models.len() - 1;
    app.dj.setup.as_mut().unwrap().model_index = last;
    handler(Key::Enter, &mut app);
    assert_eq!(app.dj.setup.as_ref().unwrap().step, DjSetupStep::Custom);

    // A model name has to be able to contain a 'j', so no navigation here.
    app.dj.setup.as_mut().unwrap().custom.clear();
    for c in "jazzy-model".chars() {
      handler(Key::Char(c), &mut app);
    }
    handler(Key::Backspace, &mut app);
    assert_eq!(app.dj.setup.as_ref().unwrap().custom, "jazzy-mode");
  }

  #[test]
  fn opening_the_dj_on_a_fresh_config_shows_the_picker() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    open(&mut app);
    assert!(
      app.dj.setup.is_some(),
      "a machine-written default config has never chosen an AI"
    );
  }

  #[test]
  fn opening_the_dj_on_a_configured_install_never_shows_the_picker() {
    let (tx, _rx) = channel();
    let mut config = UserConfig::new();
    config.behavior.dj_configured = Some(true);
    let mut app = App::new(tx, config, Some(SystemTime::now()));
    open(&mut app);
    assert!(app.dj.setup.is_none());
  }

  #[test]
  fn reopening_the_dj_rebuilds_a_picker_left_open_by_a_mouse_click() {
    // Clicking the sidebar leaves the screen without the picker's Esc ever running,
    // so the next visit must not resume rows detected in a session that moved on.
    let (tx, _rx) = channel();
    let mut config = UserConfig::new();
    config.behavior.dj_configured = Some(true);
    let mut app = App::new(tx, config, Some(SystemTime::now()));
    app.apply(Action::OpenDjSetup);
    app.dj.setup.as_mut().unwrap().step = DjSetupStep::Model;
    app.pop_navigation_stack();

    open(&mut app);
    assert_eq!(
      app.dj.setup.as_ref().map(|setup| setup.step),
      Some(DjSetupStep::Backend)
    );
  }

  #[test]
  fn the_reopen_binding_shows_the_picker_from_the_dj_screen() {
    // It carries a modifier precisely so it survives the prompt having focus.
    let (mut app, _rx) = dj_app();
    handler(app.user_config.keys.dj_pick_model, &mut app);
    assert!(app.dj.setup.is_some());
    assert!(app.dj.input.is_empty());
  }

  #[test]
  fn the_reopen_binding_does_not_push_a_second_dj_route() {
    let (mut app, _rx) = dj_app();
    app.apply(Action::OpenDjSetup);
    app.dj.close_setup();
    app.pop_navigation_stack();
    assert_ne!(
      app.get_current_route().id,
      RouteId::AiDj,
      "a second AiDj route would strand the user on the DJ screen"
    );
  }

  #[test]
  fn confirming_the_picker_writes_the_backend_and_marks_the_dj_configured() {
    let (mut app, _rx) = dj_app();
    app.apply(Action::OpenDjSetup);
    handler(Key::Enter, &mut app);

    // No config path is seeded, so the save fails into a status message and
    // never touches the filesystem.
    app.apply(Action::CommitDjSetup);
    let behavior = &app.user_config.behavior;
    assert_eq!(behavior.dj_backend, "agent_cli");
    assert_eq!(behavior.dj_agent_command, vec!["claude".to_string()]);
    assert_eq!(behavior.dj_agent_model.as_deref(), Some("haiku"));
    assert_eq!(behavior.dj_configured, Some(true));
    assert!(app.dj.setup.is_none(), "the picker closes on commit");
    assert!(app
      .dj
      .transcript
      .last()
      .is_some_and(|line| line.text.contains("claude/haiku")));
  }

  #[test]
  fn dismissing_the_picker_marks_the_dj_configured_so_it_does_not_reappear() {
    let (mut app, _rx) = dj_app();
    app.apply(Action::OpenDjSetup);
    app.apply(Action::DismissDjSetup);
    assert!(app.dj.setup.is_none());
    assert_eq!(app.user_config.behavior.dj_configured, Some(true));

    // "I do not want to be asked" changes nothing else: the brain stays whatever it
    // already was.
    let behavior = &app.user_config.behavior;
    assert_eq!(behavior.dj_backend, "agent_cli");
    assert_eq!(
      behavior.dj_agent_command,
      crate::core::user_config::default_dj_agent_command()
    );
    assert_eq!(behavior.dj_agent_model, None);
    assert_eq!(behavior.dj_model, None);
  }

  #[test]
  fn the_picker_bumps_the_generation_so_an_in_flight_batch_is_dropped() {
    let (mut app, _rx) = dj_app();
    app.apply(Action::OpenDjSetup);
    handler(Key::Enter, &mut app);
    let before = app.dj.generation;
    app.apply(Action::CommitDjSetup);
    assert_ne!(
      app.dj.generation, before,
      "a batch in flight came from the backend just replaced"
    );
  }
}
