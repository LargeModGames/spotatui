use crate::app::{App, SettingValue, SettingsCategory};
use ratatui::{
  layout::{Constraint, Layout, Rect},
  style::{Modifier, Style},
  text::{Line, Span},
  widgets::{Block, Borders, List, ListItem, Paragraph, Tabs},
  Frame,
};

pub fn draw_settings(f: &mut Frame<'_>, app: &App) {
  let [tabs_area, list_area, help_area] = f.area().layout(
    &Layout::vertical([
      Constraint::Length(3), // Category tabs
      Constraint::Min(1),    // Settings list
      Constraint::Length(3), // Help bar
    ])
    .margin(2),
  );

  draw_category_tabs(f, app, tabs_area);
  draw_settings_list(f, app, list_area);
  draw_settings_help(f, app, help_area);
}

fn draw_category_tabs(f: &mut Frame<'_>, app: &App, area: Rect) {
  let titles: Vec<Line> = SettingsCategory::all()
    .iter()
    .map(|cat| Line::from(cat.name()))
    .collect();

  let selected = app.settings_category.index();

  let tabs = Tabs::new(titles)
    .select(selected)
    .block(
      Block::default()
        .borders(Borders::ALL)
        .title("Settings (←/→ to switch tabs)"),
    )
    .highlight_style(
      Style::default()
        .fg(app.user_config.theme.selected)
        .add_modifier(Modifier::BOLD),
    )
    .style(app.user_config.theme.base_style());

  f.render_widget(tabs, area);
}

fn draw_settings_list(f: &mut Frame<'_>, app: &App, area: Rect) {
  let items: Vec<ListItem> = app
    .settings_items
    .iter()
    .enumerate()
    .map(|(i, setting)| {
      let is_selected = i == app.settings_selected_index;
      let is_editing = is_selected && app.settings_edit_mode;

      // Format the value display
      let value_str = if is_editing {
        match &setting.value {
          SettingValue::Bool(v) => {
            // Show toggle state (shouldn't reach here with new logic, but just in case)
            if *v {
              "[●] On  [ ] Off"
            } else {
              "[ ] On  [●] Off"
            }
            .to_string()
          }
          _ => {
            // Show edit buffer with cursor
            format!("{}▏", app.settings_edit_buffer)
          }
        }
      } else {
        match &setting.value {
          SettingValue::Bool(v) => {
            // Show toggle indicator - pressing Enter will toggle
            if *v { "[●] On" } else { "[○] Off" }.to_string()
          }
          SettingValue::Number(v) => v.to_string(),
          SettingValue::String(v) => format!("\"{}\"", v),
          SettingValue::Key(v) => format!("[{}]", v),
          SettingValue::Color(v) => format!("■ {}", v),
          SettingValue::Preset(v) => format!("◆ {} ◆", v), // Show preset name with arrows hint
        }
      };

      // Build the line with name and value
      let name_style = if is_selected {
        Style::default()
          .fg(app.user_config.theme.selected)
          .add_modifier(Modifier::BOLD)
      } else {
        Style::default().fg(app.user_config.theme.text)
      };

      let value_style = if is_editing {
        Style::default()
          .fg(app.user_config.theme.hint)
          .add_modifier(Modifier::BOLD)
      } else if is_selected {
        Style::default().fg(app.user_config.theme.selected)
      } else {
        Style::default().fg(app.user_config.theme.inactive)
      };

      let line = Line::from(vec![
        Span::styled(format!("{}: ", setting.name), name_style),
        Span::styled(value_str, value_style),
      ]);

      ListItem::new(line)
    })
    .collect();

  let title = format!(
    "{} Settings ({} items)",
    app.settings_category.name(),
    app.settings_items.len()
  );

  let list = List::new(items)
    .block(
      Block::default()
        .borders(Borders::ALL)
        .title(title)
        .style(app.user_config.theme.base_style())
        .border_style(Style::default().fg(app.user_config.theme.inactive)),
    )
    .highlight_style(
      Style::default()
        .fg(app.user_config.theme.selected)
        .add_modifier(Modifier::BOLD),
    )
    .highlight_symbol(
      Line::from("▶ ").style(
        Style::default()
          .fg(app.user_config.theme.selected)
          .add_modifier(Modifier::BOLD),
      ),
    );

  f.render_widget(list, area);
}

fn draw_settings_help(f: &mut Frame<'_>, app: &App, area: Rect) {
  let help_text = if app.settings_edit_mode {
    match app.settings_items.get(app.settings_selected_index) {
      Some(setting) => match &setting.value {
        SettingValue::Bool(_) => "Space/Enter: Toggle | ←/→: Toggle | Esc: Cancel",
        SettingValue::Number(_) => {
          "↑/↓: Increment/Decrement | Type numbers | Enter: Confirm | Esc: Cancel"
        }
        _ => "Type to edit | Enter: Confirm | Esc: Cancel",
      },
      None => "",
    }
  } else {
    "↑/↓: Select | ←/→: Switch Tab | Enter: Toggle/Edit | Alt-S: Save | Esc/q: Exit"
  };

  let help = Paragraph::new(help_text)
    .style(
      Style::default()
        .fg(app.user_config.theme.hint)
        .bg(app.user_config.theme.background),
    )
    .block(
      Block::default()
        .borders(Borders::ALL)
        .title("Controls")
        .style(app.user_config.theme.base_style())
        .border_style(Style::default().fg(app.user_config.theme.inactive)),
    );

  f.render_widget(help, area);
}
