use crate::core::app::{App, RouteId};
use crate::tui::event::Key;

/// Handler for plugin custom screens. Global keybindings have already run;
/// everything that reaches here is forwarded to the owning plugin's `on_key`
/// callback as a config.yml-style key string (drained by the script engine
/// right after the key event).
pub fn handler(key: Key, app: &mut App) {
  let screen = match &app.get_current_route().id {
    RouteId::PluginScreen(name) => name.clone(),
    _ => return,
  };

  match key {
    Key::Esc => {
      app.pop_navigation_stack();
    }
    // Scroll affordance for paragraph-heavy screens; not forwarded.
    Key::PageUp => {
      app.plugin_screen_scroll = app.plugin_screen_scroll.saturating_sub(5);
    }
    Key::PageDown => {
      app.plugin_screen_scroll = app.plugin_screen_scroll.saturating_add(5);
    }
    _ => {
      let key_string = plugin_key_string(key);
      if !key_string.is_empty() {
        app.pending_plugin_screen_keys.push((screen, key_string));
      }
    }
  }
}

/// Serialize a key with the same vocabulary config.yml keybindings use
/// (mirrors `key_to_config_string` in user_config).
fn plugin_key_string(key: Key) -> String {
  match key {
    Key::Char(' ') => "space".to_string(),
    Key::Char(c) => c.to_string(),
    Key::Ctrl(c) => format!("ctrl-{}", c),
    Key::Alt(c) => format!("alt-{}", c),
    Key::Enter => "enter".to_string(),
    Key::Tab => "tab".to_string(),
    Key::Esc => "esc".to_string(),
    Key::Backspace => "backspace".to_string(),
    Key::Delete => "del".to_string(),
    Key::Left => "left".to_string(),
    Key::Right => "right".to_string(),
    Key::Up => "up".to_string(),
    Key::Down => "down".to_string(),
    Key::Home => "home".to_string(),
    Key::End => "end".to_string(),
    Key::Ins => "ins".to_string(),
    Key::PageUp => "pageup".to_string(),
    Key::PageDown => "pagedown".to_string(),
    Key::F0 => "f0".to_string(),
    Key::F1 => "f1".to_string(),
    Key::F2 => "f2".to_string(),
    Key::F3 => "f3".to_string(),
    Key::F4 => "f4".to_string(),
    Key::F5 => "f5".to_string(),
    Key::F6 => "f6".to_string(),
    Key::F7 => "f7".to_string(),
    Key::F8 => "f8".to_string(),
    Key::F9 => "f9".to_string(),
    Key::F10 => "f10".to_string(),
    Key::F11 => "f11".to_string(),
    Key::F12 => "f12".to_string(),
    Key::Unknown => String::new(),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::ActiveBlock;

  #[test]
  fn key_on_plugin_screen_lands_in_pending_queue() {
    let mut app = App::default();
    app.push_navigation_stack(
      RouteId::PluginScreen("stats".to_string()),
      ActiveBlock::PluginScreen,
    );

    handler(Key::Char('a'), &mut app);
    handler(Key::Ctrl('x'), &mut app);
    handler(Key::Enter, &mut app);

    assert_eq!(
      app.pending_plugin_screen_keys,
      vec![
        ("stats".to_string(), "a".to_string()),
        ("stats".to_string(), "ctrl-x".to_string()),
        ("stats".to_string(), "enter".to_string()),
      ]
    );
  }

  #[test]
  fn esc_pops_back_out_of_plugin_screen() {
    let mut app = App::default();
    app.push_navigation_stack(
      RouteId::PluginScreen("stats".to_string()),
      ActiveBlock::PluginScreen,
    );

    handler(Key::Esc, &mut app);

    assert!(app.pending_plugin_screen_keys.is_empty());
    assert!(!matches!(
      app.get_current_route().id,
      RouteId::PluginScreen(_)
    ));
  }
}
