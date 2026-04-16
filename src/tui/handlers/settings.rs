use crate::core::app::{App, SettingValue, SettingsCategory};
use crate::handlers::common_key_events::{down_event, left_event, right_event, up_event};
use crate::tui::event::Key;

pub fn handler(key: Key, app: &mut App) {
  if app.settings_unsaved_prompt_visible {
    handle_unsaved_changes_prompt(key, app);
    return;
  }

  if app.settings_edit_mode {
    handle_edit_mode(key, app);
  } else {
    handle_navigation(key, app);
  }
}

fn handle_navigation(key: Key, app: &mut App) {
  match key {
    // Category switching with left/right (only when not in edit mode)
    key if left_event(key) => switch_category_left(app),
    key if right_event(key) => switch_category_right(app),

    // Item selection with up/down
    key if down_event(key) => select_next_item(app),
    key if up_event(key) => select_previous_item(app),

    // Enter edit mode
    Key::Enter => enter_edit_mode(app),

    // Save settings
    key if key == app.effective_save_settings_key() => {
      let _ = save_settings(app);
    }

    // Exit settings
    Key::Esc => request_exit_settings(app),
    key if key == app.user_config.keys.back => {
      request_exit_settings(app);
    }
    _ => {}
  }
}

fn handle_unsaved_changes_prompt(key: Key, app: &mut App) {
  match key {
    Key::Char('y') | Key::Char('Y') if save_settings(app) => {
      close_settings(app);
    }
    Key::Char('n') | Key::Char('N') | Key::Esc => {
      close_settings(app);
    }
    Key::Enter => {
      if app.settings_unsaved_prompt_save_selected {
        if save_settings(app) {
          close_settings(app);
        }
      } else {
        close_settings(app);
      }
    }
    key if left_event(key) || right_event(key) => {
      app.settings_unsaved_prompt_save_selected = !app.settings_unsaved_prompt_save_selected;
    }
    key if key == app.user_config.keys.back => {
      close_settings(app);
    }
    _ => {}
  }
}

fn request_exit_settings(app: &mut App) {
  if has_unsaved_settings_changes(app) {
    app.settings_unsaved_prompt_visible = true;
    app.settings_unsaved_prompt_save_selected = true;
    app.settings_edit_mode = false;
    app.settings_edit_buffer.clear();
  } else {
    close_settings(app);
  }
}

fn close_settings(app: &mut App) {
  app.settings_unsaved_prompt_visible = false;
  app.settings_unsaved_prompt_save_selected = true;
  app.settings_edit_mode = false;
  app.settings_edit_buffer.clear();
  app.pop_navigation_stack();
}

fn has_unsaved_settings_changes(app: &App) -> bool {
  app.settings_items != app.settings_saved_items
}

fn handle_edit_mode(key: Key, app: &mut App) {
  if let Some(setting) = app.settings_items.get(app.settings_selected_index) {
    match &setting.value {
      SettingValue::Bool(_) => handle_bool_edit(key, app),
      SettingValue::Number(_) => handle_number_edit(key, app),
      SettingValue::Preset(_) => handle_preset_edit(key, app),
      SettingValue::Cycle(_, _) => {} // enter_edit_mode handles Cycle without entering edit mode
      SettingValue::Key(_) => handle_key_edit(key, app),
      SettingValue::String(_) | SettingValue::Color(_) => handle_string_edit(key, app),
    }
  }
}

fn handle_bool_edit(key: Key, app: &mut App) {
  match key {
    Key::Enter | Key::Char(' ') => {
      // Toggle the boolean value
      if let Some(setting) = app.settings_items.get_mut(app.settings_selected_index) {
        if let SettingValue::Bool(v) = setting.value {
          setting.value = SettingValue::Bool(!v);
        }
      }
      app.settings_edit_mode = false;
    }
    Key::Esc => {
      app.settings_edit_mode = false;
    }
    key if left_event(key) || right_event(key) => {
      // Toggle on left/right as well for better UX
      if let Some(setting) = app.settings_items.get_mut(app.settings_selected_index) {
        if let SettingValue::Bool(v) = setting.value {
          setting.value = SettingValue::Bool(!v);
        }
      }
    }
    _ => {}
  }
}

fn handle_number_edit(key: Key, app: &mut App) {
  match key {
    Key::Enter => {
      // Parse and apply the edited number
      if let Ok(num) = app.settings_edit_buffer.parse::<i64>() {
        if let Some(setting) = app.settings_items.get_mut(app.settings_selected_index) {
          setting.value = SettingValue::Number(num);
        }
      }
      app.settings_edit_mode = false;
      app.settings_edit_buffer.clear();
    }
    Key::Esc => {
      app.settings_edit_mode = false;
      app.settings_edit_buffer.clear();
    }
    Key::Char(c) if c.is_ascii_digit() || c == '-' => {
      app.settings_edit_buffer.push(c);
    }
    Key::Backspace => {
      app.settings_edit_buffer.pop();
    }
    key if up_event(key) => {
      // Increment value
      if let Some(setting) = app.settings_items.get_mut(app.settings_selected_index) {
        if let SettingValue::Number(v) = setting.value {
          let new_val = v + 1;
          setting.value = SettingValue::Number(new_val);
          app.settings_edit_buffer = new_val.to_string();
        }
      }
    }
    key if down_event(key) => {
      // Decrement value
      if let Some(setting) = app.settings_items.get_mut(app.settings_selected_index) {
        if let SettingValue::Number(v) = setting.value {
          let new_val = v - 1;
          setting.value = SettingValue::Number(new_val);
          app.settings_edit_buffer = new_val.to_string();
        }
      }
    }
    _ => {}
  }
}

fn handle_string_edit(key: Key, app: &mut App) {
  match key {
    Key::Enter => {
      // Apply the edited string
      if let Some(setting) = app.settings_items.get_mut(app.settings_selected_index) {
        let new_value = app.settings_edit_buffer.clone();

        if setting.id == "behavior.auto_update_delay" {
          if let Err(e) = crate::cli::parse_delay_secs(&new_value) {
            app.handle_error(anyhow::anyhow!("Invalid delay format: {}", e));
            return;
          }
        }

        match &setting.value {
          SettingValue::String(_) => {
            setting.value = SettingValue::String(new_value);
          }
          SettingValue::Color(_) => {
            setting.value = SettingValue::Color(new_value);
          }
          _ => {}
        }
      }
      app.settings_edit_mode = false;
      app.settings_edit_buffer.clear();
    }
    Key::Esc => {
      app.settings_edit_mode = false;
      app.settings_edit_buffer.clear();
    }
    Key::Char(c) => {
      app.settings_edit_buffer.push(c);
    }
    Key::Backspace => {
      app.settings_edit_buffer.pop();
    }
    _ => {}
  }
}

/// Check if a keybinding conflicts with another action
/// Returns Some(action_name) if conflict found, None otherwise
fn check_keybinding_conflict(app: &App, new_key: Key, current_setting_id: &str) -> Option<String> {
  // Iterate through all settings items
  for setting in &app.settings_items {
    // Skip if it's the same setting we're editing
    if setting.id == current_setting_id {
      continue;
    }

    // Only check keybinding settings
    if !setting.id.starts_with("keys.") {
      continue;
    }

    // Get the key value from this setting
    if let SettingValue::Key(key_string) = &setting.value {
      // Parse the key string to compare
      if let Ok(existing_key) = crate::core::user_config::parse_key_public(key_string.clone()) {
        // Check if keys match (case-sensitive comparison)
        if existing_key == new_key {
          // Return the friendly name of the conflicting action
          return Some(setting.name.clone());
        }
      }
    }
  }

  None
}

fn handle_key_edit(key: Key, app: &mut App) {
  match key {
    // Escape cancels the key binding edit
    Key::Esc => {
      app.settings_edit_mode = false;
      app.settings_edit_buffer.clear();
    }
    // Ignore standalone modifier key presses (e.g. pressing Shift alone)
    Key::Unknown => {}
    // Any other key press is captured as the new keybinding
    _ => {
      // Check if this is a reserved key
      if let Err(e) = crate::core::user_config::check_reserved_keys_public(key) {
        // Show error but don't apply the reserved key
        app.handle_error(anyhow::anyhow!("{}", e));
        app.settings_edit_mode = false;
        app.settings_edit_buffer.clear();
        return;
      }

      // Check for keybinding conflicts
      if let Some(setting) = app.settings_items.get(app.settings_selected_index) {
        if let Some(conflict_name) = check_keybinding_conflict(app, key, &setting.id) {
          // Show error and don't apply the conflicting key
          let key_display = key_to_config_string(&key);
          app.handle_error(anyhow::anyhow!(
            "Key {} is already assigned to {}",
            key_display,
            conflict_name
          ));
          app.settings_edit_mode = false;
          app.settings_edit_buffer.clear();
          return;
        }
      }

      // Convert the key to string representation
      let key_string = key_to_config_string(&key);

      // Apply the new keybinding
      if let Some(setting) = app.settings_items.get_mut(app.settings_selected_index) {
        setting.value = SettingValue::Key(key_string);
      }

      app.settings_edit_mode = false;
      app.settings_edit_buffer.clear();
    }
  }
}

/// Convert a Key to its config file string representation
fn key_to_config_string(key: &Key) -> String {
  match key {
    Key::Char(c) if *c == ' ' => "space".to_string(),
    Key::Char(c) => c.to_string(),
    Key::Ctrl(c) => format!("ctrl-{}", c),
    Key::Alt(c) => format!("alt-{}", c),
    Key::Enter => "enter".to_string(),
    Key::Esc => "esc".to_string(),
    Key::Backspace => "backspace".to_string(),
    Key::Delete => "del".to_string(),
    Key::Left => "left".to_string(),
    Key::Right => "right".to_string(),
    Key::Up => "up".to_string(),
    Key::Down => "down".to_string(),
    Key::PageUp => "pageup".to_string(),
    Key::PageDown => "pagedown".to_string(),
    Key::Home => "home".to_string(),
    Key::End => "end".to_string(),
    Key::Tab => "tab".to_string(),
    Key::Ins => "ins".to_string(),
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
    Key::Unknown => "unknown".to_string(),
  }
}

fn switch_category_left(app: &mut App) {
  let current_index = app.settings_category.index();
  let new_index = if current_index == 0 {
    SettingsCategory::all().len() - 1
  } else {
    current_index - 1
  };
  app.settings_category = SettingsCategory::from_index(new_index);
  app.settings_selected_index = 0;
  app.load_settings_for_category();
}

fn switch_category_right(app: &mut App) {
  let current_index = app.settings_category.index();
  let new_index = (current_index + 1) % SettingsCategory::all().len();
  app.settings_category = SettingsCategory::from_index(new_index);
  app.settings_selected_index = 0;
  app.load_settings_for_category();
}

fn select_next_item(app: &mut App) {
  if !app.settings_items.is_empty() {
    app.settings_selected_index = (app.settings_selected_index + 1) % app.settings_items.len();
  }
}

fn select_previous_item(app: &mut App) {
  if !app.settings_items.is_empty() {
    if app.settings_selected_index == 0 {
      app.settings_selected_index = app.settings_items.len() - 1;
    } else {
      app.settings_selected_index -= 1;
    }
  }
}

fn enter_edit_mode(app: &mut App) {
  if let Some(setting) = app.settings_items.get(app.settings_selected_index) {
    // For booleans, toggle directly without entering edit mode
    if let SettingValue::Bool(v) = setting.value {
      // Need to get mutable reference
      if let Some(setting_mut) = app.settings_items.get_mut(app.settings_selected_index) {
        setting_mut.value = SettingValue::Bool(!v);
      }
      return;
    }

    // For presets, cycle to next preset directly
    if let SettingValue::Preset(ref preset_name) = setting.value {
      use crate::core::user_config::ThemePreset;
      let current = ThemePreset::from_name(preset_name);
      let next = current.next();
      if let Some(setting_mut) = app.settings_items.get_mut(app.settings_selected_index) {
        setting_mut.value = SettingValue::Preset(next.name().to_string());
      }
      return;
    }

    // For cycle values, advance to next option directly
    if let SettingValue::Cycle(ref current, options) = setting.value {
      let next = cycle_next(current, options);
      if let Some(setting_mut) = app.settings_items.get_mut(app.settings_selected_index) {
        if let SettingValue::Cycle(ref mut cur, _) = setting_mut.value {
          *cur = next;
        }
      }
      return;
    }

    // For other types, enter edit mode
    app.settings_edit_mode = true;
    // Pre-populate the edit buffer with current value
    app.settings_edit_buffer = match &setting.value {
      SettingValue::Number(v) => v.to_string(),
      SettingValue::String(v) => v.clone(),
      SettingValue::Key(v) => v.clone(),
      SettingValue::Color(v) => v.clone(),
      _ => String::new(),
    };
  }
}

/// Returns the next option after `current` in `options`, wrapping around.
fn cycle_next(current: &str, options: &[&str]) -> String {
  let idx = options.iter().position(|o| *o == current).unwrap_or(0);
  options[(idx + 1) % options.len()].to_string()
}

fn handle_preset_edit(key: Key, app: &mut App) {
  use crate::core::user_config::ThemePreset;

  match key {
    Key::Enter | Key::Char(' ') => {
      // Cycle to next preset
      if let Some(setting) = app.settings_items.get(app.settings_selected_index) {
        if let SettingValue::Preset(ref preset_name) = setting.value {
          let current = ThemePreset::from_name(preset_name);
          let next = current.next();
          if let Some(setting_mut) = app.settings_items.get_mut(app.settings_selected_index) {
            setting_mut.value = SettingValue::Preset(next.name().to_string());
          }
        }
      }
      app.settings_edit_mode = false;
    }
    Key::Esc => {
      app.settings_edit_mode = false;
    }
    key if right_event(key) => {
      // Next preset
      if let Some(setting) = app.settings_items.get(app.settings_selected_index) {
        if let SettingValue::Preset(ref preset_name) = setting.value {
          let current = ThemePreset::from_name(preset_name);
          let next = current.next();
          if let Some(setting_mut) = app.settings_items.get_mut(app.settings_selected_index) {
            setting_mut.value = SettingValue::Preset(next.name().to_string());
          }
        }
      }
    }
    key if left_event(key) => {
      // Previous preset
      if let Some(setting) = app.settings_items.get(app.settings_selected_index) {
        if let SettingValue::Preset(ref preset_name) = setting.value {
          let current = ThemePreset::from_name(preset_name);
          let prev = current.prev();
          if let Some(setting_mut) = app.settings_items.get_mut(app.settings_selected_index) {
            setting_mut.value = SettingValue::Preset(prev.name().to_string());
          }
        }
      }
    }
    _ => {}
  }
}

fn save_settings(app: &mut App) -> bool {
  // Apply settings to user_config and save to file
  app.apply_settings_changes();
  #[cfg(target_os = "macos")]
  if app.user_config.keys.open_settings != Key::Ctrl(',') {
    app.keybinding_runtime.effective_open_settings = None;
    app.keybinding_runtime.fallback_reason = None;
  }
  if let Err(e) = app.user_config.save_config() {
    app.handle_error(anyhow::anyhow!("Failed to save settings: {}", e));
    return false;
  }

  app.settings_saved_items = app.settings_items.clone();
  true
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::{ActiveBlock, RouteId};

  fn open_settings(app: &mut App) -> RouteId {
    let previous_route = app.get_current_route().id.clone();
    app.load_settings_for_category();
    app.push_navigation_stack(RouteId::Settings, ActiveBlock::Settings);
    previous_route
  }

  fn first_bool_setting_index(app: &App) -> usize {
    app
      .settings_items
      .iter()
      .position(|setting| matches!(setting.value, SettingValue::Bool(_)))
      .expect("expected a boolean setting")
  }

  #[test]
  fn esc_without_changes_exits_settings_without_prompt() {
    let mut app = App::default();
    let previous_route = open_settings(&mut app);

    handler(Key::Esc, &mut app);

    assert_eq!(app.get_current_route().id, previous_route);
    assert!(!app.settings_unsaved_prompt_visible);
  }

  #[test]
  fn esc_with_changes_opens_unsaved_prompt() {
    let mut app = App::default();
    open_settings(&mut app);

    app.settings_selected_index = first_bool_setting_index(&app);
    handler(Key::Enter, &mut app);
    handler(Key::Esc, &mut app);

    assert!(app.settings_unsaved_prompt_visible);
    assert_eq!(app.get_current_route().active_block, ActiveBlock::Settings);
  }

  #[test]
  fn n_dismisses_unsaved_prompt_and_discards_changes() {
    let mut app = App::default();
    let previous_route = open_settings(&mut app);

    app.settings_selected_index = first_bool_setting_index(&app);
    handler(Key::Enter, &mut app);
    handler(Key::Esc, &mut app);
    assert!(app.settings_unsaved_prompt_visible);

    handler(Key::Char('n'), &mut app);

    assert!(!app.settings_unsaved_prompt_visible);
    assert_eq!(app.get_current_route().id, previous_route);
  }
}
