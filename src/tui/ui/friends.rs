use crate::core::app::{ActiveBlock, App, FriendAddMode, FriendEntry, FriendFilter};
use ratatui::{
  layout::{Constraint, Layout, Rect},
  style::{Modifier, Style},
  text::{Line, Span},
  widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph},
  Frame,
};

use super::util::{get_color, truncate_text};

pub fn draw_friends(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let current_route = app.get_current_route();
  let highlight_state = (
    current_route.active_block == ActiveBlock::Friends,
    current_route.hovered_block == ActiveBlock::Friends,
  );

  let outer_block = Block::default()
    .borders(Borders::ALL)
    .border_type(BorderType::Rounded)
    .border_style(get_color(highlight_state, app.user_config.theme))
    .title(Span::styled(
      " Friends ",
      get_color(highlight_state, app.user_config.theme)
        .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
    ));

  let inner = outer_block.inner(layout_chunk);
  f.render_widget(outer_block, layout_chunk);

  // No sync token → show account-required prompt
  if app.user_config.behavior.sync_token.is_none() {
    draw_no_token_prompt(f, app, inner);
    return;
  }

  // Split inner area: friend-code card | tabs+list | help bar
  let [top_area, list_area, help_area] = inner.layout(&Layout::vertical([
    Constraint::Length(3),
    Constraint::Min(1),
    Constraint::Length(1),
  ]));

  draw_friend_code_card(f, app, top_area);
  draw_friends_body(f, app, list_area);
  draw_help_bar(f, app, help_area);

  // Add-friend overlay rendered on top
  if app.friend_add_dialog_visible {
    draw_add_friend_dialog(f, app, layout_chunk);
  }
}

// ── No-token prompt ───────────────────────────────────────────────────────────

fn draw_no_token_prompt(f: &mut Frame<'_>, app: &App, area: Rect) {
  let theme = app.user_config.theme;

  let lines = vec![
    Line::default(),
    Line::default(),
    Line::from(Span::styled(
      "Account Required",
      Style::default()
        .fg(theme.banner)
        .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
    )),
    Line::default(),
    Line::from(Span::styled(
      "Friends require a spotatui web account to sync your",
      Style::default().fg(theme.text),
    )),
    Line::from(Span::styled(
      "listening data and connect with other users.",
      Style::default().fg(theme.text),
    )),
    Line::default(),
    Line::from(vec![
      Span::styled("  Sign up at: ", Style::default().fg(theme.inactive)),
      Span::styled(
        "https://spotatui.com",
        Style::default()
          .fg(theme.hint)
          .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
      ),
    ]),
    Line::default(),
    Line::from(Span::styled(
      "After signing up, copy your sync token from the dashboard",
      Style::default().fg(theme.inactive),
    )),
    Line::from(Span::styled(
      "and paste it into Settings → Behavior → sync_token",
      Style::default().fg(theme.inactive),
    )),
    Line::default(),
    Line::from(Span::styled(
      "Press Esc to go back",
      Style::default().fg(theme.hint),
    )),
  ];

  let para = Paragraph::new(lines).alignment(ratatui::layout::Alignment::Center);
  f.render_widget(para, area);
}

// ── Friend code card ──────────────────────────────────────────────────────────

fn draw_friend_code_card(f: &mut Frame<'_>, app: &App, area: Rect) {
  let theme = app.user_config.theme;

  let code_text = app.friend_code.as_deref().unwrap_or("Loading...");

  let hint = "  c — copy";

  let line = Line::from(vec![
    Span::styled(
      " YOUR CODE  ",
      Style::default()
        .fg(theme.inactive)
        .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
    ),
    Span::styled(
      code_text,
      Style::default()
        .fg(theme.hint)
        .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
    ),
    Span::raw("  "),
    Span::styled(hint, Style::default().fg(theme.inactive)),
  ]);

  let card = Paragraph::new(line).block(
    Block::default()
      .borders(Borders::ALL)
      .border_type(BorderType::Rounded)
      .border_style(Style::default().fg(theme.active)),
  );

  f.render_widget(card, area);
}

// ── Main body: filter tabs + friend list ──────────────────────────────────────

fn draw_friends_body(f: &mut Frame<'_>, app: &App, area: Rect) {
  let theme = app.user_config.theme;

  // Split into a thin tab/control row and the list below
  let [controls_area, list_area] = area.layout(&Layout::vertical([
    Constraint::Length(1),
    Constraint::Min(1),
  ]));

  // Controls row: [All (n)] [Online (n)]  ... search hint ... [+ Add Friend]
  draw_filter_tabs(f, app, controls_area);

  // The actual friends list
  let filtered = filtered_friends(app);

  if filtered.is_empty() {
    let msg = if !app.friend_search_input.is_empty() {
      "No friends match your search"
    } else if app.friends_loading {
      "Loading friends..."
    } else if app.friends.is_empty() {
      "No friends yet — press 'a' to add one!"
    } else {
      "No friends online right now"
    };
    let para = Paragraph::new(Span::styled(msg, Style::default().fg(theme.inactive)))
      .alignment(ratatui::layout::Alignment::Center);
    // Center vertically a bit
    let [_, center, _] = list_area.layout(&Layout::vertical([
      Constraint::Percentage(30),
      Constraint::Length(1),
      Constraint::Min(1),
    ]));
    f.render_widget(para, center);
    return;
  }

  draw_friend_list(f, app, list_area, &filtered);
}

fn draw_filter_tabs(f: &mut Frame<'_>, app: &App, area: Rect) {
  let theme = app.user_config.theme;
  let online_count = app.friends.iter().filter(|f| f.is_online).count();
  let all_count = app.friends.len();

  let all_style = if app.friend_filter == FriendFilter::All {
    Style::default()
      .fg(theme.selected)
      .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD))
  } else {
    Style::default().fg(theme.inactive)
  };
  let online_style = if app.friend_filter == FriendFilter::Online {
    Style::default()
      .fg(theme.selected)
      .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD))
  } else {
    Style::default().fg(theme.inactive)
  };

  // Build search preview (inline: any unbound key types into the filter)
  let search_str: String = app.friend_search_input.iter().collect();
  let search_hint = if search_str.is_empty() {
    "search…".to_string()
  } else {
    format!("/{}/", search_str)
  };

  let line = Line::from(vec![
    Span::styled(format!(" All ({}) ", all_count), all_style),
    Span::styled(" │ ", Style::default().fg(theme.inactive)),
    Span::styled(format!(" Online ({}) ", online_count), online_style),
    Span::styled("    ", Style::default()),
    Span::styled(search_hint, Style::default().fg(theme.inactive)),
    Span::styled("    ", Style::default()),
    Span::styled("+ Add Friend", Style::default().fg(theme.active)),
    Span::raw(" "),
  ]);

  let para = Paragraph::new(line);
  f.render_widget(para, area);
}

fn draw_friend_list(f: &mut Frame<'_>, app: &App, area: Rect, friends: &[&FriendEntry]) {
  let theme = app.user_config.theme;

  let items: Vec<ListItem> = friends
    .iter()
    .enumerate()
    .map(|(i, friend)| {
      let is_selected = i == app.friend_selected_index;

      // Online indicator
      let online_span = if friend.is_online {
        Span::styled("● ", Style::default().fg(theme.active))
      } else {
        Span::styled("○ ", Style::default().fg(theme.inactive))
      };

      // Name
      let name_style = if is_selected {
        Style::default()
          .fg(theme.selected)
          .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD))
      } else {
        Style::default().fg(theme.text)
      };
      let name_span = Span::styled(
        format!("{:<20}", truncate_text(&friend.name, 20)),
        name_style,
      );

      // Now-playing or status
      let np_spans = if let Some(np) = &friend.now_playing {
        vec![
          Span::styled("▶ ", Style::default().fg(theme.active)),
          Span::styled(
            truncate_text(&np.title, 28),
            Style::default().fg(theme.hint),
          ),
          Span::styled(
            format!(" — {}", truncate_text(&np.artists, 20)),
            Style::default().fg(theme.inactive),
          ),
        ]
      } else if friend.is_online {
        vec![Span::styled("idle", Style::default().fg(theme.inactive))]
      } else {
        vec![Span::styled("offline", Style::default().fg(theme.inactive))]
      };

      let mut spans = vec![online_span, name_span, Span::raw("  ")];
      spans.extend(np_spans);

      ListItem::new(Line::from(spans))
    })
    .collect();

  let mut state = ListState::default();
  state.select(Some(
    app
      .friend_selected_index
      .min(friends.len().saturating_sub(1)),
  ));

  let list = List::new(items)
    .highlight_style(
      Style::default()
        .fg(theme.selected)
        .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
    )
    .highlight_symbol("▶ ");

  f.render_stateful_widget(list, area, &mut state);
}

// ── Help bar ──────────────────────────────────────────────────────────────────

fn draw_help_bar(f: &mut Frame<'_>, app: &App, area: Rect) {
  let theme = app.user_config.theme;

  let line = Line::from(vec![
    hint_span("↑/↓", theme),
    Span::styled(" Navigate  ", Style::default().fg(theme.inactive)),
    hint_span("c", theme),
    Span::styled(" Copy code  ", Style::default().fg(theme.inactive)),
    hint_span("a", theme),
    Span::styled(" Add friend  ", Style::default().fg(theme.inactive)),
    hint_span("u", theme),
    Span::styled(" Unfollow  ", Style::default().fg(theme.inactive)),
    hint_span("Tab", theme),
    Span::styled(" Filter  ", Style::default().fg(theme.inactive)),
    Span::styled("type to search  ", Style::default().fg(theme.inactive)),
    hint_span("Esc", theme),
    Span::styled(" Back", Style::default().fg(theme.inactive)),
  ]);

  f.render_widget(Paragraph::new(line), area);
}

fn hint_span(key: &'static str, theme: crate::core::user_config::Theme) -> Span<'static> {
  Span::styled(
    key,
    Style::default().fg(theme.hint).add_modifier(Modifier::BOLD),
  )
}

// ── Add-Friend dialog overlay ─────────────────────────────────────────────────

fn draw_add_friend_dialog(f: &mut Frame<'_>, app: &App, parent: Rect) {
  let theme = app.user_config.theme;

  // Center a 50×18 dialog box
  let dialog_width = 52u16.min(parent.width.saturating_sub(4));
  let dialog_height = 14u16.min(parent.height.saturating_sub(4));
  let x = parent.x + (parent.width.saturating_sub(dialog_width)) / 2;
  let y = parent.y + (parent.height.saturating_sub(dialog_height)) / 2;
  let dialog_area = Rect::new(x, y, dialog_width, dialog_height);

  f.render_widget(Clear, dialog_area);

  let block = Block::default()
    .borders(Borders::ALL)
    .border_type(BorderType::Rounded)
    .border_style(Style::default().fg(theme.active))
    .title(Span::styled(
      " Add Friend ",
      Style::default()
        .fg(theme.active)
        .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
    ));

  let inner = block.inner(dialog_area);
  f.render_widget(block, dialog_area);

  // Mode tabs
  let [tabs_area, content_area, help_area] = inner.layout(&Layout::vertical([
    Constraint::Length(1),
    Constraint::Min(1),
    Constraint::Length(1),
  ]));

  // Tab row
  let code_style = if app.friend_add_mode == FriendAddMode::Code {
    Style::default().fg(theme.selected).add_modifier(
      app
        .user_config
        .behavior
        .emphasis(Modifier::BOLD | Modifier::UNDERLINED),
    )
  } else {
    Style::default().fg(theme.inactive)
  };
  let search_style = if app.friend_add_mode == FriendAddMode::Search {
    Style::default().fg(theme.selected).add_modifier(
      app
        .user_config
        .behavior
        .emphasis(Modifier::BOLD | Modifier::UNDERLINED),
    )
  } else {
    Style::default().fg(theme.inactive)
  };
  let tabs_line = Line::from(vec![
    Span::styled(" By Friend Code ", code_style),
    Span::styled(" │ ", Style::default().fg(theme.inactive)),
    Span::styled(" Search by Name ", search_style),
  ]);
  f.render_widget(Paragraph::new(tabs_line), tabs_area);

  match app.friend_add_mode {
    FriendAddMode::Code => draw_add_by_code(f, app, content_area),
    FriendAddMode::Search => draw_add_by_search(f, app, content_area),
  }

  // Footer hints
  let footer_line = Line::from(vec![
    hint_span("Tab", theme),
    Span::styled(" Switch  ", Style::default().fg(theme.inactive)),
    hint_span("Enter", theme),
    Span::styled(" Add  ", Style::default().fg(theme.inactive)),
    hint_span("Esc", theme),
    Span::styled(" Cancel", Style::default().fg(theme.inactive)),
  ]);
  f.render_widget(Paragraph::new(footer_line), help_area);
}

fn draw_add_by_code(f: &mut Frame<'_>, app: &App, area: Rect) {
  let theme = app.user_config.theme;

  let input: String = app.friend_add_input.iter().collect();
  let display = if input.is_empty() {
    "Enter friend code...".to_string()
  } else {
    input.clone()
  };
  let style = if input.is_empty() {
    Style::default().fg(theme.inactive)
  } else {
    Style::default()
      .fg(theme.hint)
      .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD))
  };

  let [_, input_row, hint_row, _] = area.layout(&Layout::vertical([
    Constraint::Length(1),
    Constraint::Length(3),
    Constraint::Length(1),
    Constraint::Min(0),
  ]));

  let input_widget = Paragraph::new(Span::styled(display, style))
    .alignment(ratatui::layout::Alignment::Center)
    .block(
      Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.active)),
    );
  f.render_widget(input_widget, input_row);

  let hint = Paragraph::new(Span::styled(
    "Type code then press Enter",
    Style::default().fg(theme.inactive),
  ))
  .alignment(ratatui::layout::Alignment::Center);
  f.render_widget(hint, hint_row);
}

fn draw_add_by_search(f: &mut Frame<'_>, app: &App, area: Rect) {
  let theme = app.user_config.theme;

  let [input_row, results_area] = area.layout(&Layout::vertical([
    Constraint::Length(3),
    Constraint::Min(1),
  ]));

  // Search input field
  let search_str: String = app.friend_user_search_input.iter().collect();
  let search_display = if search_str.is_empty() {
    "Type a username or code...".to_string()
  } else {
    search_str
  };
  let search_style = if app.friend_user_search_input.is_empty() {
    Style::default().fg(theme.inactive)
  } else {
    Style::default().fg(theme.text)
  };
  let search_widget = Paragraph::new(Span::styled(search_display, search_style)).block(
    Block::default()
      .borders(Borders::ALL)
      .border_type(BorderType::Rounded)
      .border_style(Style::default().fg(theme.inactive)),
  );
  f.render_widget(search_widget, input_row);

  // Results list
  if app.friend_user_search_results.is_empty() {
    let msg = if app.friend_user_search_input.is_empty() {
      ""
    } else {
      "No users found"
    };
    f.render_widget(
      Paragraph::new(Span::styled(msg, Style::default().fg(theme.inactive)))
        .alignment(ratatui::layout::Alignment::Center),
      results_area,
    );
    return;
  }

  let items: Vec<ListItem> = app
    .friend_user_search_results
    .iter()
    .enumerate()
    .map(|(i, r)| {
      let is_sel = i == app.friend_user_search_selected;
      let prefix = if is_sel { "▶ " } else { "  " };
      let style = if is_sel {
        Style::default()
          .fg(theme.selected)
          .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD))
      } else {
        Style::default().fg(theme.text)
      };
      let following_tag = if r.is_following { " [following]" } else { "" };
      ListItem::new(Line::from(vec![
        Span::styled(prefix, Style::default().fg(theme.selected)),
        Span::styled(format!("{}{}", r.name, following_tag), style),
      ]))
    })
    .collect();

  let mut state = ListState::default();
  state.select(Some(app.friend_user_search_selected));
  let list = List::new(items).highlight_style(
    Style::default()
      .fg(theme.selected)
      .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
  );
  f.render_stateful_widget(list, results_area, &mut state);
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Return the subset of friends that pass the active filter and search query.
pub fn filtered_friends(app: &App) -> Vec<&FriendEntry> {
  let search_str: String = app.friend_search_input.iter().collect();
  let q = search_str.to_lowercase();

  app
    .friends
    .iter()
    .filter(|f| {
      let passes_filter = match app.friend_filter {
        FriendFilter::All => true,
        FriendFilter::Online => f.is_online,
      };
      let passes_search = q.is_empty() || f.name.to_lowercase().contains(&q);
      passes_filter && passes_search
    })
    .collect()
}
