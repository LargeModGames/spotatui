use crate::core::app::{ActiveBlock, AnnouncementLevel, App, DialogContext};
use crate::infra::network::sync::PartyStatus;
use ratatui::{
  layout::{Alignment, Constraint, Direction, Layout, Rect},
  style::{Modifier, Style},
  text::{Line, Span},
  widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Row, Table, Wrap},
  Frame,
};
use rspotify::model::PlayableItem;
use rspotify::prelude::Id;

use super::help::get_help_docs;
use super::util::create_artist_string;

pub fn draw_help_menu(f: &mut Frame<'_>, app: &App) {
  let [area] = f
    .area()
    .layout(&Layout::vertical([Constraint::Percentage(100)]).margin(2));

  // Create a one-column table to avoid flickering due to non-determinism when
  // resolving constraints on widths of table columns.
  // Calculate column widths based on available terminal width
  let total_width = area.width as usize;
  let col1_width = (total_width as f32 * 0.40) as usize;
  let col2_width = (total_width as f32 * 0.30) as usize;
  let col3_width = total_width.saturating_sub(col1_width + col2_width + 2);

  let truncate = |s: &str, max: usize| -> String {
    if max == 0 {
      return String::new();
    }
    if s.chars().count() > max {
      let truncated: String = s.chars().take(max.saturating_sub(1)).collect();
      format!("{}…", truncated)
    } else {
      s.to_string()
    }
  };

  let format_row = |r: Vec<String>| -> Vec<String> {
    vec![format!(
      "{:<w1$}  {:<w2$}  {:<w3$}",
      truncate(&r[0], col1_width),
      truncate(&r[1], col2_width),
      truncate(&r[2], col3_width),
      w1 = col1_width,
      w2 = col2_width,
      w3 = col3_width,
    )]
  };

  let help_menu_style = app.user_config.theme.base_style();
  let header = ["Description", "Event", "Context"];
  let header = format_row(header.iter().map(|s| s.to_string()).collect());

  let help_docs = get_help_docs(app);
  let help_docs = help_docs
    .into_iter()
    .map(format_row)
    .collect::<Vec<Vec<String>>>();
  let help_docs = &help_docs[app.help_menu_offset as usize..];

  let rows = help_docs
    .iter()
    .map(|item| Row::new(item.clone()).style(help_menu_style));

  let help_menu = Table::new(rows, &[Constraint::Percentage(100)])
    .header(Row::new(header))
    .block(
      Block::default()
        .borders(Borders::ALL)
        .style(help_menu_style)
        .title(Span::styled(
          "Help (press <Esc> to go back)",
          help_menu_style,
        ))
        .border_style(help_menu_style),
    )
    .style(help_menu_style);
  f.render_widget(help_menu, area);
}

fn queue_item_line(item: &PlayableItem) -> String {
  match item {
    PlayableItem::Track(track) => {
      format!("{} - {}", track.name, create_artist_string(&track.artists))
    }
    PlayableItem::Episode(episode) => {
      format!("{} - {}", episode.name, episode.show.name)
    }
  }
}

pub fn draw_queue(f: &mut Frame<'_>, app: &App) {
  let [area] = f
    .area()
    .layout(&Layout::vertical([Constraint::Percentage(100)]).margin(2));

  let style = app.user_config.theme.base_style();
  let items: Vec<ListItem> = match &app.queue {
    None => vec![ListItem::new(Span::raw("Loading...")).style(style)],
    Some(q) => {
      let mut rows = Vec::new();
      if let Some(ref now) = q.currently_playing {
        rows.push(
          ListItem::new(Line::from(vec![
            Span::styled("Now playing: ", style.add_modifier(Modifier::BOLD)),
            Span::raw(queue_item_line(now)),
          ]))
          .style(style),
        );
      }
      for item in &q.queue {
        rows.push(ListItem::new(queue_item_line(item)).style(style));
      }
      if rows.is_empty() {
        rows.push(ListItem::new(Span::raw("No queue (no active device?)")).style(style));
      }
      rows
    }
  };

  let mut state = ListState::default();
  let len = items.len();
  let selected = if len == 0 {
    None
  } else {
    Some(app.queue_selected_index.min(len.saturating_sub(1)))
  };
  state.select(selected);
  let list = List::new(items)
    .block(
      Block::default()
        .borders(Borders::ALL)
        .style(style)
        .title(Span::styled("Queue (press Esc to go back)", style))
        .border_style(style),
    )
    .style(style)
    .highlight_style(
      Style::default()
        .fg(app.user_config.theme.active)
        .bg(app.user_config.theme.inactive)
        .add_modifier(Modifier::BOLD),
    )
    .highlight_symbol(Line::from("▶ ").style(Style::default().fg(app.user_config.theme.active)));
  f.render_stateful_widget(list, area, &mut state);
}

pub fn draw_error_screen(f: &mut Frame<'_>, app: &App) {
  let chunks = Layout::default()
    .direction(Direction::Vertical)
    .constraints([Constraint::Percentage(100)])
    .margin(5)
    .split(f.area());

  let playing_text = vec![
    Line::from(vec![
      Span::raw("Api response: "),
      Span::styled(
        &app.api_error,
        Style::default().fg(app.user_config.theme.error_text),
      ),
    ]),
    Line::from(Span::styled(
      "If you are trying to play a track, please check that",
      Style::default().fg(app.user_config.theme.text),
    )),
    Line::from(Span::styled(
      " 1. You have a Spotify Premium Account",
      Style::default().fg(app.user_config.theme.text),
    )),
    Line::from(Span::styled(
      " 2. Your playback device is active and selected - press `d` to go to device selection menu",
      Style::default().fg(app.user_config.theme.text),
    )),
    Line::from(Span::styled(
      " 3. If you're using spotifyd as a playback device, your device name must not contain spaces",
      Style::default().fg(app.user_config.theme.text),
    )),
    Line::from(Span::styled("Hint: a playback device must be either an official spotify client or a light weight alternative such as spotifyd",
        Style::default().fg(app.user_config.theme.hint)
        ),
    ),
    Line::from(
      Span::styled(
          "\nPress <Esc> to return",
          Style::default().fg(app.user_config.theme.inactive),
      ),
    )
  ];

  let playing_paragraph = Paragraph::new(playing_text)
    .wrap(Wrap { trim: true })
    .style(app.user_config.theme.base_style())
    .block(
      Block::default()
        .borders(Borders::ALL)
        .style(app.user_config.theme.base_style())
        .title(Span::styled(
          "Error",
          Style::default().fg(app.user_config.theme.error_border),
        ))
        .border_style(Style::default().fg(app.user_config.theme.error_border)),
    );
  f.render_widget(playing_paragraph, chunks[0]);
}

pub fn draw_dialog(f: &mut Frame<'_>, app: &App) {
  let dialog_context = match app.get_current_route().active_block {
    ActiveBlock::Dialog(context) => context,
    _ => return,
  };

  match dialog_context {
    DialogContext::PlaylistWindow | DialogContext::PlaylistSearch => {
      if let Some(playlist) = app.dialog.as_ref() {
        let text = vec![
          Line::from(Span::raw("Are you sure you want to delete the playlist: ")),
          Line::from(Span::styled(
            playlist.as_str(),
            Style::default().add_modifier(Modifier::BOLD),
          )),
          Line::from(Span::raw("?")),
        ];
        draw_confirmation_dialog(f, app, "Confirm", text, 45);
      }
    }
    DialogContext::RemoveTrackFromPlaylistConfirm => {
      if let Some(pending_remove) = app.pending_playlist_track_removal.as_ref() {
        let text = vec![
          Line::from(Span::raw("Remove this track from playlist?")),
          Line::from(Span::styled(
            format!("Track: {}", pending_remove.track_name),
            Style::default().add_modifier(Modifier::BOLD),
          )),
          Line::from(Span::styled(
            format!("Playlist: {}", pending_remove.playlist_name),
            Style::default().add_modifier(Modifier::BOLD),
          )),
        ];
        draw_confirmation_dialog(f, app, "Remove Track", text, 60);
      }
    }
    DialogContext::PersistKeybindingFallback => {
      if let Some(persist) = app.pending_keybinding_persist.as_ref() {
        let text = vec![
          Line::from(Span::raw("Ctrl+, is not reported by this terminal stack.")),
          Line::from(Span::raw("Use fallback shortcut for Open Settings?")),
          Line::from(Span::styled(
            format!("Save as: {}", persist.open_settings_key),
            Style::default().add_modifier(Modifier::BOLD),
          )),
        ];
        draw_confirmation_dialog(f, app, "Save Shortcut Fallback", text, 66);
      }
    }
    DialogContext::AddTrackToPlaylistPicker => {
      draw_add_track_to_playlist_picker_dialog(f, app);
    }
  }
}

fn centered_modal_rect(bounds: Rect, requested_width: u16, requested_height: u16) -> Rect {
  let width = requested_width.min(bounds.width.saturating_sub(2).max(1));
  let height = requested_height.min(bounds.height.saturating_sub(2).max(1));
  let left = bounds.x + bounds.width.saturating_sub(width) / 2;
  let top = bounds.y + bounds.height.saturating_sub(height) / 3;
  Rect::new(left, top, width, height)
}

fn draw_confirmation_dialog(
  f: &mut Frame<'_>,
  app: &App,
  title: &str,
  text: Vec<Line<'_>>,
  requested_width: u16,
) {
  let rect = centered_modal_rect(f.area(), requested_width, 10);
  f.render_widget(Clear, rect);

  let block = Block::default()
    .title(Span::styled(
      title,
      Style::default()
        .fg(app.user_config.theme.header)
        .add_modifier(Modifier::BOLD),
    ))
    .borders(Borders::ALL)
    .style(app.user_config.theme.base_style())
    .border_style(Style::default().fg(app.user_config.theme.inactive));
  f.render_widget(block, rect);

  let vchunks = Layout::default()
    .direction(Direction::Vertical)
    .margin(1)
    .constraints([Constraint::Min(3), Constraint::Length(3)])
    .split(rect);

  let text = Paragraph::new(text)
    .wrap(Wrap { trim: true })
    .style(app.user_config.theme.base_style())
    .alignment(Alignment::Center);
  f.render_widget(text, vchunks[0]);

  let hchunks = Layout::default()
    .direction(Direction::Horizontal)
    .horizontal_margin(3)
    .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)])
    .split(vchunks[1]);

  let ok = Paragraph::new(Span::raw("Ok"))
    .style(Style::default().fg(if app.confirm {
      app.user_config.theme.hovered
    } else {
      app.user_config.theme.inactive
    }))
    .alignment(Alignment::Center);
  f.render_widget(ok, hchunks[0]);

  let cancel = Paragraph::new(Span::raw("Cancel"))
    .style(Style::default().fg(if app.confirm {
      app.user_config.theme.inactive
    } else {
      app.user_config.theme.hovered
    }))
    .alignment(Alignment::Center);
  f.render_widget(cancel, hchunks[1]);
}

fn draw_add_track_to_playlist_picker_dialog(f: &mut Frame<'_>, app: &App) {
  let rect = centered_modal_rect(f.area(), 70, 20);
  f.render_widget(Clear, rect);

  let block = Block::default()
    .title(Span::styled(
      "Add Track To Playlist",
      Style::default()
        .fg(app.user_config.theme.header)
        .add_modifier(Modifier::BOLD),
    ))
    .borders(Borders::ALL)
    .style(app.user_config.theme.base_style())
    .border_style(Style::default().fg(app.user_config.theme.inactive));
  f.render_widget(block, rect);

  let vchunks = Layout::default()
    .direction(Direction::Vertical)
    .margin(1)
    .constraints([
      Constraint::Length(2),
      Constraint::Min(3),
      Constraint::Length(1),
    ])
    .split(rect);

  let track_name = app
    .pending_playlist_track_add
    .as_ref()
    .map(|p| p.track_name.as_str())
    .unwrap_or("Selected track");

  let header = Paragraph::new(Line::from(Span::raw(format!(
    "Choose a playlist for: {}",
    track_name
  ))))
  .wrap(Wrap { trim: true })
  .style(app.user_config.theme.base_style());
  f.render_widget(header, vchunks[0]);

  let mut list_state = ListState::default();
  let editable_playlists = app.editable_playlists();

  if editable_playlists.is_empty() {
    let empty_text = Paragraph::new("No editable playlists available")
      .style(Style::default().fg(app.user_config.theme.inactive))
      .alignment(Alignment::Center);
    f.render_widget(empty_text, vchunks[1]);
  } else {
    let is_own_playlist = |playlist: &rspotify::model::SimplifiedPlaylist| -> bool {
      app
        .user
        .as_ref()
        .is_some_and(|user| user.id.id() == playlist.owner.id.id())
    };
    let items: Vec<ListItem> = editable_playlists
      .iter()
      .map(|playlist| {
        let label = if is_own_playlist(playlist) {
          playlist.name.clone()
        } else {
          let owner = playlist
            .owner
            .display_name
            .as_deref()
            .unwrap_or_else(|| playlist.owner.id.id());
          format!("{} - {} (collab)", playlist.name, owner)
        };
        ListItem::new(Span::raw(label))
      })
      .collect();
    let selected = app
      .playlist_picker_selected_index
      .min(editable_playlists.len() - 1);
    list_state.select(Some(selected));

    let list = List::new(items)
      .style(app.user_config.theme.base_style())
      .highlight_style(Style::default().fg(app.user_config.theme.hovered))
      .highlight_symbol("▶ ");

    f.render_stateful_widget(list, vchunks[1], &mut list_state);
  }

  let footer = Paragraph::new("Enter add | q cancel | j/k or arrows move | H/M/L jump")
    .style(Style::default().fg(app.user_config.theme.inactive))
    .alignment(Alignment::Center);
  f.render_widget(footer, vchunks[2]);
}

pub fn draw_announcement_prompt(f: &mut Frame<'_>, app: &App) {
  let Some(announcement) = &app.active_announcement else {
    return;
  };

  let width = std::cmp::min(f.area().width.saturating_sub(4), 74);
  let height = std::cmp::min(f.area().height.saturating_sub(4), 16);
  let rect = f
    .area()
    .centered(Constraint::Length(width), Constraint::Length(height));

  f.render_widget(Clear, rect);

  let (level_label, accent_color) = match announcement.level {
    AnnouncementLevel::Info => ("INFO", app.user_config.theme.active),
    AnnouncementLevel::Warning => ("WARNING", app.user_config.theme.hint),
    AnnouncementLevel::Critical => ("CRITICAL", app.user_config.theme.error_text),
  };

  let mut text = vec![
    Line::from(Span::styled(
      format!("{}  {}", level_label, announcement.title),
      Style::default().add_modifier(Modifier::BOLD),
    )),
    Line::from(""),
  ];

  for line in announcement.body.lines() {
    text.push(Line::from(line.to_string()));
  }

  if let Some(url) = &announcement.url {
    text.push(Line::from(""));
    text.push(Line::from(Span::styled(
      format!("More: {}", url),
      Style::default().add_modifier(Modifier::ITALIC),
    )));
  }

  text.push(Line::from(""));
  text.push(Line::from(Span::styled(
    "[Press ENTER or ESC to dismiss]",
    Style::default().fg(app.user_config.theme.inactive),
  )));

  let paragraph = Paragraph::new(text)
    .style(app.user_config.theme.base_style())
    .alignment(Alignment::Left)
    .wrap(Wrap { trim: false })
    .block(
      Block::default()
        .borders(Borders::ALL)
        .style(app.user_config.theme.base_style())
        .border_style(Style::default().fg(accent_color))
        .title(" Announcement "),
    );

  f.render_widget(paragraph, rect);
}

pub fn draw_exit_prompt(f: &mut Frame<'_>, app: &App) {
  let width = std::cmp::min(f.area().width.saturating_sub(4), 56);
  let height = 8;
  let rect = f
    .area()
    .centered(Constraint::Length(width), Constraint::Length(height));

  f.render_widget(Clear, rect);

  let text = vec![
    Line::from(Span::styled(
      "Exit spotatui?",
      Style::default().add_modifier(Modifier::BOLD),
    )),
    Line::from(""),
    Line::from("Press Y for Yes or N for No"),
    Line::from(Span::styled(
      "[ENTER = Yes, ESC = No]",
      Style::default().fg(app.user_config.theme.inactive),
    )),
  ];

  let paragraph = Paragraph::new(text)
    .style(app.user_config.theme.base_style())
    .alignment(Alignment::Center)
    .block(
      Block::default()
        .borders(Borders::ALL)
        .style(app.user_config.theme.base_style())
        .border_style(Style::default().fg(app.user_config.theme.active))
        .title(" Confirm Exit "),
    );

  f.render_widget(paragraph, rect);
}

/// Draw the sort menu popup overlay
pub fn draw_sort_menu(f: &mut Frame<'_>, app: &App) {
  if !app.sort_menu_visible {
    return;
  }

  let context = match app.sort_context {
    Some(ctx) => ctx,
    None => return,
  };

  let available_fields = context.available_fields();
  let current_sort = match context {
    crate::core::sort::SortContext::PlaylistTracks => &app.playlist_sort,
    crate::core::sort::SortContext::SavedAlbums => &app.album_sort,
    crate::core::sort::SortContext::SavedArtists => &app.artist_sort,
    crate::core::sort::SortContext::RecentlyPlayed => &app.playlist_sort,
  };

  let width = std::cmp::min(f.area().width.saturating_sub(4), 35);
  let height = (available_fields.len() + 4) as u16; // +4 for borders/padding
  let rect = f
    .area()
    .centered(Constraint::Length(width), Constraint::Length(height));

  f.render_widget(Clear, rect);

  // Build list items
  let items: Vec<ListItem> = available_fields
    .iter()
    .enumerate()
    .map(|(i, field)| {
      let shortcut = field
        .shortcut()
        .map(|c| format!(" ({})", c))
        .unwrap_or_default();
      let indicator = if *field == current_sort.field {
        format!(" {}", current_sort.order.indicator())
      } else {
        String::new()
      };
      let text = format!("{}{}{}", field.display_name(), shortcut, indicator);

      let style = if i == app.sort_menu_selected {
        Style::default()
          .fg(app.user_config.theme.active)
          .add_modifier(Modifier::BOLD)
      } else if *field == current_sort.field {
        Style::default().fg(app.user_config.theme.hovered)
      } else {
        Style::default().fg(app.user_config.theme.text)
      };

      ListItem::new(text).style(style)
    })
    .collect();

  let title = match context {
    crate::core::sort::SortContext::PlaylistTracks => "Sort Tracks",
    crate::core::sort::SortContext::SavedAlbums => "Sort Albums",
    crate::core::sort::SortContext::SavedArtists => "Sort Artists",
    crate::core::sort::SortContext::RecentlyPlayed => "Sort",
  };

  let list = List::new(items)
    .block(
      Block::default()
        .borders(Borders::ALL)
        .style(app.user_config.theme.base_style())
        .border_style(Style::default().fg(app.user_config.theme.active))
        .title(Span::styled(
          title,
          Style::default()
            .fg(app.user_config.theme.active)
            .add_modifier(Modifier::BOLD),
        )),
    )
    .highlight_style(
      Style::default()
        .fg(app.user_config.theme.active)
        .add_modifier(Modifier::BOLD),
    )
    .highlight_symbol(Line::from("▶ ").style(Style::default().fg(app.user_config.theme.active)));

  let mut state = ListState::default();
  state.select(Some(app.sort_menu_selected));

  f.render_stateful_widget(list, rect, &mut state);
}

pub fn draw_party(f: &mut Frame<'_>, app: &App) {
  let [area] = f
    .area()
    .layout(&Layout::vertical([Constraint::Percentage(100)]).margin(2));

  let popup_width = 50u16.min(area.width);
  let popup_height = 16u16.min(area.height);
  let popup_x = (area.width.saturating_sub(popup_width)) / 2 + area.x;
  let popup_y = (area.height.saturating_sub(popup_height)) / 2 + area.y;
  let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

  f.render_widget(Clear, popup_area);

  let style = app.user_config.theme.base_style();
  let active_style = Style::default()
    .fg(app.user_config.theme.active)
    .add_modifier(Modifier::BOLD);
  let hint_style = Style::default().fg(app.user_config.theme.hint);

  let mut lines: Vec<Line> = Vec::new();

  match &app.party_status {
    PartyStatus::Disconnected | PartyStatus::Connecting => {
      if !app.party_input.is_empty() || app.party_input_idx > 0 || !app.party_join_name.is_empty() {
        let code_str: String = app
          .party_input
          .iter()
          .filter(|c| c.is_alphanumeric())
          .map(|c| c.to_ascii_uppercase())
          .collect();
        let name_str: String = app.party_join_name.iter().collect();
        let trimmed_name = name_str.trim();
        lines.push(Line::from(Span::styled(
          "Enter 6-character party code:",
          style,
        )));
        lines.push(Line::from(""));
        let display = format!(
          "  [ {} ]",
          if code_str.is_empty() {
            "______".to_string()
          } else {
            let mut padded = code_str.clone();
            while padded.len() < 6 {
              padded.push('_');
            }
            padded
          }
        );
        lines.push(Line::from(Span::styled(display, active_style)));
        lines.push(Line::from(""));

        let name_display = if name_str.is_empty() {
          "________________".to_string()
        } else {
          name_str.clone()
        };
        lines.push(Line::from(Span::styled("Enter your name:", style)));
        lines.push(Line::from(Span::styled(
          format!("  [ {} ]", name_display),
          active_style,
        )));
        lines.push(Line::from(""));
        if code_str.len() == 6 && !trimmed_name.is_empty() {
          lines.push(Line::from(Span::styled("Press Enter to join", hint_style)));
        } else if code_str.len() == 6 {
          lines.push(Line::from(Span::styled(
            "Type a display name to continue",
            hint_style,
          )));
        } else {
          let char_count = format!("{}/6 characters", code_str.len());
          lines.push(Line::from(Span::styled(char_count, hint_style)));
        }
        lines.push(Line::from(Span::styled(
          format!("Name length: {}/32", trimmed_name.chars().count()),
          hint_style,
        )));
        lines.push(Line::from(Span::styled(
          "Code fills first, then name input",
          hint_style,
        )));
        lines.push(Line::from(Span::styled("Esc to cancel", hint_style)));
      } else {
        lines.push(Line::from(Span::styled("Listening Party", active_style)));
        lines.push(Line::from(""));
        if app.party_status == PartyStatus::Connecting {
          lines.push(Line::from(Span::styled("Connecting...", hint_style)));
        } else {
          lines.push(Line::from(vec![
            Span::styled("1 ", active_style),
            Span::styled("Host a Party", style),
          ]));
          lines.push(Line::from(vec![
            Span::styled("2 ", active_style),
            Span::styled("Join a Party", style),
          ]));
          lines.push(Line::from(""));
          lines.push(Line::from(Span::styled("Esc to close", hint_style)));
        }
      }
    }
    PartyStatus::Hosting => {
      lines.push(Line::from(Span::styled(
        "Hosting Listening Party",
        active_style,
      )));
      lines.push(Line::from(""));
      if let Some(session) = &app.party_session {
        let code_display = if session.code.is_empty() {
          "Generating...".to_string()
        } else {
          session.code.clone()
        };
        lines.push(Line::from(vec![
          Span::styled("Share this code: ", style),
          Span::styled(code_display, active_style),
        ]));
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
          Span::styled("Control: ", style),
          Span::styled(session.control_mode.to_string(), style),
        ]));
        lines.push(Line::from(""));
        if session.guests.is_empty() {
          lines.push(Line::from(Span::styled(
            "Waiting for guests...",
            hint_style,
          )));
        } else {
          let listener_label = if session.guests.len() == 1 {
            "1 listener:".to_string()
          } else {
            format!("{} listeners:", session.guests.len())
          };
          lines.push(Line::from(Span::styled(listener_label, style)));
          for (i, guest) in session.guests.iter().enumerate() {
            let label = format!("  {}. {}", i + 1, guest);
            lines.push(Line::from(Span::styled(label, style)));
          }
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
          "c - toggle control mode",
          hint_style,
        )));
        lines.push(Line::from(Span::styled("l - leave party", hint_style)));
        lines.push(Line::from(Span::styled("Esc to close menu", hint_style)));
      }
    }
    PartyStatus::Joined => {
      lines.push(Line::from(Span::styled(
        "Listening Party (Guest)",
        active_style,
      )));
      lines.push(Line::from(""));
      if let Some(session) = &app.party_session {
        lines.push(Line::from(vec![
          Span::styled("Host: ", style),
          Span::styled(&session.host_name, style),
        ]));
        lines.push(Line::from(vec![
          Span::styled("Room: ", style),
          Span::styled(&session.code, active_style),
        ]));
        lines.push(Line::from(vec![
          Span::styled("Mode: ", style),
          Span::styled("Following host playback", hint_style),
        ]));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("l - leave party", hint_style)));
        lines.push(Line::from(Span::styled("Esc to close menu", hint_style)));
      }
    }
  }

  let title = match &app.party_status {
    PartyStatus::Hosting => "Party (Hosting)",
    PartyStatus::Joined => "Party (Joined)",
    _ => "Party",
  };

  let paragraph = Paragraph::new(lines)
    .block(
      Block::default()
        .borders(Borders::ALL)
        .style(style)
        .title(Span::styled(title, active_style))
        .border_style(Style::default().fg(app.user_config.theme.active)),
    )
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: false });

  f.render_widget(paragraph, popup_area);
}
