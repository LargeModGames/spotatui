use crate::core::app::{App, SettingItem, SettingValue, SettingsCategory};
use crate::tui::fuzzy::fuzzy_match;
use crate::tui::theme::{EmphasisExt, ThemeExt};
use crate::tui::ui::popups::highlighted_spans;
use ratatui::{
  layout::{Alignment, Constraint, Layout, Rect},
  style::{Modifier, Style},
  text::{Line, Span},
  widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs},
  Frame,
};

const UNSAVED_PROMPT_WIDTH: u16 = 58;
const UNSAVED_PROMPT_HEIGHT: u16 = 9;

/// A name match beats an id match, so a query that reads like the label on
/// screen ranks that row above one reached through its hidden config key.
const NAME_FIELD_BONUS: i32 = 100;

/// The settings screen's vertical split: category tabs, the row list, and the
/// controls bar. `handlers::mouse` resolves clicks through this same function,
/// so the drawn rows and the clickable ones cannot drift apart.
pub fn settings_layout(area: Rect) -> [Rect; 3] {
  area.layout(
    &Layout::vertical([
      Constraint::Length(3),
      Constraint::Min(1),
      Constraint::Length(4),
    ])
    .margin(2),
  )
}

/// One row the filter kept: its index into `settings_items` and the byte
/// ranges of its name the query covered (empty when the row only survived on
/// its config id).
pub type FilteredSetting = (usize, Vec<(usize, usize)>);

/// Rows that survive the active filter, best match first. With no filter this
/// is every row in schema order.
pub fn filtered_settings(app: &App) -> Vec<FilteredSetting> {
  let query = app.view.settings_filter.trim();
  if query.is_empty() {
    return (0..app.settings_items.len())
      .map(|index| (index, Vec::new()))
      .collect();
  }

  let mut scored: Vec<(i32, FilteredSetting)> = app
    .settings_items
    .iter()
    .enumerate()
    .filter_map(|(index, setting)| {
      setting_score(setting, query).map(|(score, ranges)| (score, (index, ranges)))
    })
    .collect();
  // Equal scores keep schema order, so the list does not shuffle arbitrarily.
  scored.sort_by(|left, right| right.0.cmp(&left.0).then(left.1 .0.cmp(&right.1 .0)));
  scored.into_iter().map(|(_, row)| row).collect()
}

/// Indices into `app.settings_items` that survive the active filter, in the
/// order they are drawn.
pub fn filtered_setting_indices(app: &App) -> Vec<usize> {
  filtered_settings(app)
    .into_iter()
    .map(|(index, _)| index)
    .collect()
}

/// Where the selected row sits in the drawn (filtered) list, or `None` when
/// the filter hides it - the case a filter that matches nothing leaves behind.
pub fn selected_setting_position(app: &App) -> Option<usize> {
  filtered_setting_indices(app)
    .iter()
    .position(|index| *index == app.view.settings_selected_index)
}

/// Best score for one row plus the name ranges it matched, or `None` when the
/// query does not reach it.
///
/// Only the name and the config id are searched. Descriptions were tried and
/// removed: they are prose, so a three-letter query substring-matches most of a
/// tab at once (`for` reaches 10 of the 11 Icons rows), and because the screen
/// never draws a description, a row that survived on one is a row the user
/// cannot see a reason for. Every row the filter leaves now carries highlighted
/// characters that explain it.
fn setting_score(setting: &SettingItem, query: &str) -> Option<(i32, Vec<(usize, usize)>)> {
  let name = fuzzy_match(&setting.name, query);
  let id = fuzzy_match(searchable_id(&setting.id, query), query);
  let score = [
    name.as_ref().map(|m| m.score + NAME_FIELD_BONUS),
    id.map(|m| m.score),
  ]
  .into_iter()
  .flatten()
  .max()?;
  Some((score, name.map(|m| m.ranges).unwrap_or_default()))
}

/// The part of a config id worth searching.
///
/// Every id in a category shares one prefix (`behavior.`, `keys.`, `theme.`),
/// which carries no information and is long enough to satisfy most of a short
/// query on its own: matched whole, `vol` reaches 28 of the 34 Behavior rows
/// through `beha[v]i[o]r.` alone. The prefix is only searched when the query
/// spells out a dotted id itself.
fn searchable_id<'a>(id: &'a str, query: &str) -> &'a str {
  if query.contains('.') {
    return id;
  }
  id.split_once('.').map_or(id, |(_, suffix)| suffix)
}

pub fn draw_settings(f: &mut Frame<'_>, app: &App) {
  let [tabs_area, list_area, help_area] = settings_layout(f.area());

  draw_category_tabs(f, app, tabs_area);
  draw_settings_list(f, app, list_area);
  draw_settings_help(f, app, help_area);

  if app.view.settings_unsaved_prompt_visible {
    draw_unsaved_changes_prompt(f, app);
  }
}

fn draw_category_tabs(f: &mut Frame<'_>, app: &App, area: Rect) {
  let titles: Vec<Line> = SettingsCategory::all()
    .iter()
    .map(|cat| Line::from(cat.name()))
    .collect();

  let selected = app.view.settings_category.index();

  let tabs = Tabs::new(titles)
    .select(selected)
    .block(
      Block::default()
        .borders(Borders::ALL)
        .title("Settings (←/→ to switch tabs)"),
    )
    .highlight_style(
      Style::default()
        .fg(app.user_config.theme.selected.into())
        .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
    )
    .style(app.user_config.theme.base_style());

  f.render_widget(tabs, area);
}

fn draw_settings_list(f: &mut Frame<'_>, app: &App, area: Rect) {
  let visible = filtered_settings(app);
  let items: Vec<ListItem> = visible
    .iter()
    .map(|(i, name_ranges)| {
      let setting = &app.settings_items[*i];
      let is_selected = *i == app.view.settings_selected_index;
      let is_editing = is_selected && app.view.settings_edit_mode;

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
            format!("{}▏", app.view.settings_edit_buffer)
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
          SettingValue::Key(v) => {
            if setting.id == "keys.open_settings" {
              let effective = app.effective_open_settings_key();
              let configured = app.user_config.keys.open_settings;
              if effective != configured {
                format!("[{}] (effective: {})", v, effective)
              } else {
                format!("[{}]", v)
              }
            } else {
              format!("[{}]", v)
            }
          }
          SettingValue::Color(v) => format!("■ {}", v),
          SettingValue::Preset(v) => format!("◆ {} ◆", v), // Show preset name with arrows hint
          SettingValue::Cycle(v, _) => format!("◆ {} ◆", v),
        }
      };

      // Build the line with name and value
      let name_style = if is_selected {
        Style::default()
          .fg(app.user_config.theme.selected.into())
          .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD))
      } else {
        Style::default().fg(app.user_config.theme.text.into())
      };

      let value_style = if is_editing {
        Style::default()
          .fg(app.user_config.theme.hint.into())
          .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD))
      } else if is_selected {
        Style::default().fg(app.user_config.theme.selected.into())
      } else {
        Style::default().fg(app.user_config.theme.inactive.into())
      };

      let match_style = Style::default()
        .fg(app.user_config.theme.hint.into())
        .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD));

      let mut spans = highlighted_spans(&setting.name, name_ranges, name_style, match_style);
      spans.push(Span::styled(": ", name_style));
      spans.push(Span::styled(value_str, value_style));

      ListItem::new(Line::from(spans))
    })
    .collect();

  let filtering = !app.view.settings_filter.trim().is_empty();
  let count = if filtering {
    format!("{} of {}", visible.len(), app.settings_items.len())
  } else {
    format!("{} items", app.settings_items.len())
  };
  let title = format!("{} Settings ({count})", app.view.settings_category.name());

  // Only the filter can empty a loaded category, so the placeholder stays
  // scoped to it rather than claiming a filter that is not there.
  let items = if items.is_empty() && filtering {
    // The filter is scoped to this tab and carries over to the next, so name
    // the tab: a query typed on Behavior that only matches Theme rows would
    // otherwise read as "no such setting".
    vec![ListItem::new(Line::styled(
      format!(
        "No {} settings match the filter (←/→ to try another tab)",
        app.view.settings_category.name()
      ),
      Style::default().fg(app.user_config.theme.inactive.into()),
    ))]
  } else {
    items
  };

  let list = List::new(items)
    .block(
      Block::default()
        .borders(Borders::ALL)
        .title(title)
        .style(app.user_config.theme.base_style())
        .border_style(Style::default().fg(app.user_config.theme.inactive.into())),
    )
    .highlight_style(
      Style::default()
        .fg(app.user_config.theme.selected.into())
        .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
    )
    .highlight_symbol(
      Line::from("▶ ").style(
        Style::default()
          .fg(app.user_config.theme.selected.into())
          .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
      ),
    );

  let mut state = ListState::default();
  // The list renders the filtered view, so the highlight is a position in it,
  // not the index into `settings_items` the rest of the screen works with.
  state.select(
    visible
      .iter()
      .position(|(index, _)| *index == app.view.settings_selected_index),
  );

  f.render_stateful_widget(list, area, &mut state);
}

fn format_terminal_input_caps(app: &App) -> String {
  let enhancement = if app.view.terminal_input_caps.keyboard_enhancement_enabled {
    "enh:on"
  } else if app.view.terminal_input_caps.keyboard_enhancement_supported {
    "enh:available"
  } else {
    "enh:off"
  };

  let ctrl_comma = match app.view.terminal_input_caps.ctrl_punct_reliable {
    crate::core::app::CapabilityState::Yes => "ctrl+,=ok",
    crate::core::app::CapabilityState::No => "ctrl+,=degraded",
    crate::core::app::CapabilityState::Unknown => "ctrl+,=unknown",
  };

  format!("Terminal Input: {} | {}", enhancement, ctrl_comma)
}

fn draw_settings_help(f: &mut Frame<'_>, app: &App, area: Rect) {
  let controls_line = if app.view.settings_filter_editing {
    "Type to filter | Ctrl+W: Delete word | Ctrl+U: Clear | Enter: Apply | Esc: Cancel"
  } else if app.view.settings_edit_mode {
    match app.settings_items.get(app.view.settings_selected_index) {
      Some(setting) => match &setting.value {
        SettingValue::Bool(_) => "Space/Enter: Toggle | ←/→: Toggle | Esc: Cancel",
        SettingValue::Number(_) => {
          "↑/↓: Increment/Decrement | Type numbers | Enter: Confirm | Esc: Cancel"
        }
        SettingValue::Key(_) => "Press any key to set binding | Esc: Cancel",
        SettingValue::Preset(_) | SettingValue::Cycle(_, _) => {
          "Enter/→: Next | ←: Previous | Esc: Cancel"
        }
        _ => "Type to edit | Enter: Confirm | Esc: Cancel",
      },
      None => "",
    }
  } else {
    &format!(
      // Kept inside ~93 columns: the row is not wrapped, so anything past the
      // block's inner width is simply cut off.
      "↑/↓: Select | ←/→: Tab | Enter: Edit | Mouse: Click | {}: Filter | {}: Save | Esc/q: Exit",
      app.user_config.keys.search,
      app.effective_save_settings_key()
    )
  };
  // The filter query takes the second row while it exists, like the Help
  // menu's filter line; the terminal-caps line comes back once it is cleared.
  let second_line = if app.view.settings_filter_editing {
    format!("Filter: {}▏", app.view.settings_filter)
  } else if app.view.settings_filter.is_empty() {
    format_terminal_input_caps(app)
  } else {
    format!("Filter: {} (Esc: clear)", app.view.settings_filter)
  };
  let help_text = format!("{}\n{}", controls_line, second_line);

  let help = Paragraph::new(help_text)
    .style(
      Style::default()
        .fg(app.user_config.theme.hint.into())
        .bg(app.user_config.theme.background.into()),
    )
    .block(
      Block::default()
        .borders(Borders::ALL)
        .title("Controls")
        .style(app.user_config.theme.base_style())
        .border_style(Style::default().fg(app.user_config.theme.inactive.into())),
    );

  f.render_widget(help, area);
}

fn draw_unsaved_changes_prompt(f: &mut Frame<'_>, app: &App) {
  let bounds = f.area();
  let width = std::cmp::min(bounds.width.saturating_sub(4), UNSAVED_PROMPT_WIDTH);
  if width == 0 {
    return;
  }

  let height = UNSAVED_PROMPT_HEIGHT.min(bounds.height.saturating_sub(2).max(1));
  let left = bounds.x + bounds.width.saturating_sub(width) / 2;
  let top = bounds.y + bounds.height.saturating_sub(height) / 2;
  let rect = Rect::new(left, top, width, height);

  f.render_widget(Clear, rect);

  let block = Block::default()
    .title(" Unsaved Settings ")
    .borders(Borders::ALL)
    .style(app.user_config.theme.base_style())
    .border_style(Style::default().fg(app.user_config.theme.active.into()));
  f.render_widget(block, rect);

  let [message_area, buttons_area, hint_area] = rect.layout(
    &Layout::vertical([
      Constraint::Min(2),
      Constraint::Length(3),
      Constraint::Length(1),
    ])
    .margin(1),
  );

  let message = Paragraph::new("You have unsaved changes. Save before leaving settings?")
    .style(app.user_config.theme.base_style())
    .alignment(Alignment::Center);
  f.render_widget(message, message_area);

  let [yes_area, no_area] = buttons_area.layout(
    &Layout::horizontal([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)]).horizontal_margin(3),
  );

  let yes_selected = app.view.settings_unsaved_prompt_save_selected;
  let yes = Paragraph::new("[ Yes ]")
    .alignment(Alignment::Center)
    .style(Style::default().fg(if yes_selected {
      app.user_config.theme.hovered.into()
    } else {
      app.user_config.theme.inactive.into()
    }));
  f.render_widget(yes, yes_area);

  let no = Paragraph::new("[ No ]")
    .alignment(Alignment::Center)
    .style(Style::default().fg(if yes_selected {
      app.user_config.theme.inactive.into()
    } else {
      app.user_config.theme.hovered.into()
    }));
  f.render_widget(no, no_area);

  let hint = Paragraph::new("Y: Yes | N: No | Enter: Select | Esc: Cancel")
    .alignment(Alignment::Center)
    .style(Style::default().fg(app.user_config.theme.inactive.into()));
  f.render_widget(hint, hint_area);
}

#[cfg(test)]
mod tests {
  use super::*;
  use ratatui::{backend::TestBackend, Terminal};

  const WIDTH: u16 = 100;
  const HEIGHT: u16 = 30;

  fn filtered_app(filter: &str, editing: bool) -> App {
    let mut app = App::default();
    app.view.size = crate::core::geometry::Viewport {
      width: WIDTH,
      height: HEIGHT,
    };
    app.load_settings_for_category();
    app.view.settings_filter = filter.to_string();
    app.view.settings_filter_editing = editing;
    // The handler snaps the highlight onto the best match after every
    // keystroke; without it these renders would show a state the user cannot
    // actually reach.
    app.view.settings_selected_index = filtered_setting_indices(&app).first().copied().unwrap_or(0);
    app
  }

  fn rendered(app: &App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).unwrap();
    terminal.draw(|f| draw_settings(f, app)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..HEIGHT)
      .map(|y| {
        (0..WIDTH)
          .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol().to_string()))
          .collect::<String>()
      })
      .collect::<Vec<_>>()
      .join("\n")
  }

  #[test]
  fn a_filtered_screen_draws_only_matching_rows_and_the_live_query() {
    let rendered = rendered(&filtered_app("volinc", true));

    assert!(rendered.contains("Volume Increment"));
    assert!(!rendered.contains("Seek Duration"));
    assert!(rendered.contains("Filter: volinc▏"));
    assert!(!rendered.contains("Terminal Input:"));
  }

  #[test]
  fn a_filter_that_matches_nothing_says_so_instead_of_showing_rows() {
    let rendered = rendered(&filtered_app("zzzzzzzz", false));

    assert!(rendered.contains("No Behavior settings match the filter"));
    assert!(rendered.contains("←/→ to try another tab"));
    assert!(rendered.contains("Behavior Settings (0 of "));
    assert!(!rendered.contains("Volume Increment"));
  }

  #[test]
  fn a_row_is_reachable_through_its_name_or_its_config_id_only() {
    let seek = SettingItem {
      id: "behavior.seek_milliseconds".to_string(),
      name: "Seek Duration (ms)".to_string(),
      description: "Milliseconds to skip when seeking".to_string(),
      value: SettingValue::Number(35_000),
    };

    assert!(setting_score(&seek, "skms").is_some());
    assert!(setting_score(&seek, "millisec").is_some());
    // Only the description says "skip", and a row the filter cannot explain on
    // screen is a row the filter does not keep.
    assert!(setting_score(&seek, "skip").is_none());
    assert!(setting_score(&seek, "zqx").is_none());
  }

  #[test]
  fn the_shared_id_prefix_only_counts_when_the_query_is_dotted() {
    // `beha[v]i[o]r.` alone answers `vol`, so matching the prefix used to leave
    // 28 of the 34 Behavior rows on screen.
    let ids = |query: &str| {
      let app = filtered_app(query, true);
      filtered_setting_indices(&app)
        .iter()
        .map(|index| app.settings_items[*index].id.clone())
        .collect::<Vec<_>>()
    };
    assert_eq!(ids("vol"), vec!["behavior.volume_increment"]);
    assert_eq!(
      ids("behavior.volume").first().map(String::as_str),
      Some("behavior.volume_increment")
    );
  }

  #[test]
  fn a_name_match_outranks_a_row_reached_only_through_its_id() {
    let named = SettingItem {
      id: "behavior.unrelated".to_string(),
      name: "Volume Increment".to_string(),
      description: String::new(),
      value: SettingValue::Number(5),
    };
    let by_id = SettingItem {
      id: "behavior.volume_increment".to_string(),
      name: "Unrelated".to_string(),
      description: String::new(),
      value: SettingValue::Number(5),
    };

    let score = |setting| setting_score(setting, "volinc").map(|(score, _)| score);
    assert!(score(&named) > score(&by_id));
  }
}
