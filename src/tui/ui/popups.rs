use crate::core::app::{
  ActiveBlock, AnnouncementLevel, App, DialogContext, HelpMenuModel, PlaylistPickerRow,
};
use crate::core::plugin_api::PlayableInfo;
use crate::core::plugin_api::PopupLine;
use crate::infra::network::sync::PartyStatus;
use ratatui::{
  layout::{Alignment, Constraint, Direction, Layout, Rect},
  style::{Modifier, Style},
  text::{Line, Span},
  widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Row, Table, Wrap},
  Frame,
};

use super::help::{get_filtered_help_docs, help_match_ranges};
use crate::tui::theme::{EmphasisExt, ThemeExt};

/// Rebuild [`App::help_menu_model`] if the terminal width, keybindings, or
/// filter changed since the last build. Called from the event loop (and tests)
/// before drawing, so [`draw_help_menu`] renders from immutable `App` state
/// instead of rebuilding ~80 owned Strings (plus per-cell char-count
/// truncation) on every redraw while Help is open.
pub fn ensure_help_menu_model(app: &mut App) {
  // Mirrors draw_help_menu's layout: a margin of 2 on each side of the frame.
  let total_width = (app.view.size.width as usize).saturating_sub(4);
  let stale = app.view.help_menu_model.as_ref().is_none_or(|m| {
    m.width != total_width
      || m.keys != app.user_config.keys
      || m.source != app.active_source
      || m.spotify_connected != app.spotify_connected
      || m.filter != app.view.help_filter
  });
  if !stale {
    return;
  }
  let (header, rows) = build_help_rows(app, total_width);
  // The pager counts the rows built here, so a source or session change
  // while Help is open cannot leave it paging over a stale count.
  app.view.help_docs_size = rows.len() as u32;
  let match_ranges = rows
    .iter()
    .map(|row| help_match_ranges(row, &app.view.help_filter))
    .collect();
  app.view.help_menu_model = Some(HelpMenuModel {
    width: total_width,
    keys: app.user_config.keys.clone(),
    source: app.active_source,
    spotify_connected: app.spotify_connected,
    filter: app.view.help_filter.clone(),
    header,
    rows,
    match_ranges,
  });
}

/// Split `text` into spans so every filter match renders in the highlight
/// style, making it obvious why a row survived the filter. `ranges` are
/// ascending, non-overlapping byte ranges (e.g. [`help_match_ranges`] from the
/// help model, or a settings row's fuzzy-match ranges).
pub(crate) fn highlighted_spans<'a>(
  text: &'a str,
  ranges: &[(usize, usize)],
  base: Style,
  highlight: Style,
) -> Vec<Span<'a>> {
  if ranges.is_empty() {
    return vec![Span::styled(text, base)];
  }
  let mut spans = Vec::with_capacity(ranges.len() * 2 + 1);
  let mut cursor = 0;
  for &(start, end) in ranges {
    if cursor < start {
      spans.push(Span::styled(&text[cursor..start], base));
    }
    spans.push(Span::styled(&text[start..end], highlight));
    cursor = end;
  }
  if cursor < text.len() {
    spans.push(Span::styled(&text[cursor..], base));
  }
  spans
}

fn build_help_rows(app: &App, total_width: usize) -> (String, Vec<String>) {
  // Create a one-column table to avoid flickering due to non-determinism when
  // resolving constraints on widths of table columns.
  // Calculate column widths based on available terminal width.
  let col1_width = (total_width as f32 * 0.40) as usize;
  let col2_width = (total_width as f32 * 0.30) as usize;
  let col3_width = total_width.saturating_sub(col1_width + col2_width + 2);

  let format_row = |r: Vec<String>| -> String {
    format!(
      "{}  {}  {}",
      super::help::padded_help_cell(&r[0], col1_width),
      super::help::padded_help_cell(&r[1], col2_width),
      super::help::padded_help_cell(&r[2], col3_width),
    )
  };

  let strings = &app.strings().help;
  let header = [
    strings.column_description,
    strings.column_event,
    strings.column_context,
  ];
  let header = format_row(header.iter().map(|s| s.to_string()).collect());
  // Filter before formatting/truncating so narrow terminals do not make hidden
  // parts of descriptions or contexts unsearchable.
  let rows = get_filtered_help_docs(app)
    .into_iter()
    .map(format_row)
    .collect();
  (header, rows)
}

pub fn draw_help_menu(f: &mut Frame<'_>, app: &App) {
  let strings = &app.strings().help;
  let [area] = f
    .area()
    .layout(&Layout::vertical([Constraint::Percentage(100)]).margin(2));
  let [table_area, filter_area] = area.layout(&Layout::vertical([
    Constraint::Min(0),
    Constraint::Length(1),
  ]));

  // The runner (and tests) call `ensure_help_menu_model` before drawing;
  // rendering itself never rebuilds the model, only reads it.
  let Some(model) = app.view.help_menu_model.as_ref() else {
    return;
  };

  let help_menu_style = app.user_config.theme.base_style();
  let header = &model.header;
  let start = (app.view.help_menu_offset as usize).min(model.rows.len());
  // Two border rows plus the table header leave this many data rows. This is
  // also the value used by the runner for page-size calculations.
  let visible_count = table_area.height.saturating_sub(3) as usize;
  let end = start.saturating_add(visible_count).min(model.rows.len());
  let help_docs = &model.rows[start..end];

  let rows: Vec<Row<'_>> = if model.rows.is_empty() && !app.view.help_filter.is_empty() {
    vec![Row::new([strings.no_matches(&app.view.help_filter)])
      .style(Style::default().fg(app.user_config.theme.inactive.into()))]
  } else {
    let highlight_style = Style::default()
      .fg(app.user_config.theme.active.into())
      .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD));
    help_docs
      .iter()
      .zip(&model.match_ranges[start..end])
      .map(|(item, ranges)| {
        Row::new([Line::from(highlighted_spans(
          item,
          ranges,
          help_menu_style,
          highlight_style,
        ))])
        .style(help_menu_style)
      })
      .collect()
  };

  let help_menu = Table::new(rows, &[Constraint::Percentage(100)])
    .header(Row::new([header.as_str()]))
    .block(
      Block::default()
        .borders(Borders::ALL)
        .style(help_menu_style)
        .title(Span::styled(strings.title, help_menu_style))
        .border_style(help_menu_style),
    )
    .style(help_menu_style);
  f.render_widget(help_menu, table_area);

  let theme = app.user_config.theme;
  let filter_line = if app.view.help_filter_editing {
    Line::from(vec![
      Span::styled(
        strings.filter_prompt,
        Style::default().fg(theme.active.into()),
      ),
      Span::styled(app.view.help_filter.as_str(), help_menu_style),
      Span::styled("▏", Style::default().fg(theme.active.into())),
      Span::styled(
        strings.filter_edit_hint,
        Style::default().fg(theme.inactive.into()),
      ),
    ])
  } else if !app.view.help_filter.is_empty() {
    Line::from(vec![
      Span::styled(
        strings.matches(&app.view.help_filter),
        Style::default().fg(theme.active.into()),
      ),
      Span::styled(
        strings.filter_count_hint(model.rows.len()),
        Style::default().fg(theme.inactive.into()),
      ),
    ])
  } else {
    Line::from(vec![
      Span::styled(
        app.user_config.keys.search.to_string(),
        Style::default().fg(theme.active.into()),
      ),
      Span::styled(
        strings.filter_hint,
        Style::default().fg(theme.inactive.into()),
      ),
    ])
  };
  f.render_widget(
    Paragraph::new(filter_line).style(help_menu_style),
    filter_area,
  );
}

#[cfg(test)]
mod help_menu_tests {
  use super::*;
  use ratatui::{backend::TestBackend, Terminal};

  fn rendered_help(app: &mut App) -> String {
    app.view.size = crate::core::geometry::Viewport {
      width: 100,
      height: 30,
    };
    ensure_help_menu_model(app);
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal.draw(|f| draw_help_menu(f, app)).unwrap();
    let buffer = terminal.backend().buffer();

    (0..30)
      .map(|y| {
        (0..100)
          .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol().to_string()))
          .collect::<String>()
      })
      .collect::<Vec<_>>()
      .join("\n")
  }

  #[test]
  fn help_model_is_rebuilt_when_the_source_or_the_session_changes() {
    let mut app = App::default_connected();
    ensure_help_menu_model(&mut app);
    let connected = app.view.help_menu_model.as_ref().unwrap().rows.len();

    app.active_source = crate::core::source::Source::Local;
    ensure_help_menu_model(&mut app);
    let local = app.view.help_menu_model.as_ref().unwrap().rows.len();
    assert!(
      local < connected,
      "{local} rows under Local, {connected} connected"
    );
    assert_eq!(app.view.help_docs_size as usize, local);

    app.active_source = crate::core::source::Source::Spotify;
    app.spotify_connected = false;
    ensure_help_menu_model(&mut app);
    let free = app.view.help_menu_model.as_ref().unwrap().rows.len();
    assert!(free < connected, "{free} rows without a session");
  }

  #[test]
  fn help_menu_renders_only_filtered_rows_and_live_prompt() {
    let mut app = App::default();
    app.view.help_filter = "volume".to_string();
    app.view.help_filter_editing = true;

    let rendered = rendered_help(&mut app);

    assert!(rendered.contains("Increase volume by 10%"));
    assert!(rendered.contains("Decrease volume by 10%"));
    assert!(!rendered.contains("Jump to start of playlist"));
    assert!(rendered.contains("Filter: volume▏"));
    assert!(rendered.contains("<Enter>: apply  <Esc>: cancel search"));
  }

  #[test]
  fn help_menu_shows_back_hint_when_filter_is_inactive() {
    let rendered = rendered_help(&mut App::default());

    assert!(rendered.contains("<Esc>: go back"));
    assert!(!rendered.contains("press <Esc> to go back"));
  }

  #[test]
  fn help_menu_highlights_matched_text_in_filtered_rows() {
    let mut app = App::default();
    app.view.help_filter = "volume".to_string();
    app.view.size = crate::core::geometry::Viewport {
      width: 100,
      height: 30,
    };
    ensure_help_menu_model(&mut app);

    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal.draw(|f| draw_help_menu(f, &app)).unwrap();
    let buffer = terminal.backend().buffer();

    // Locate the row and column of the first "volume" match in the buffer.
    // `find` returns a byte offset, but border symbols are multi-byte, so
    // convert to a cell column by counting the chars before the match.
    let (match_x, match_y) = (0..30)
      .find_map(|y| {
        let line: String = (0..100)
          .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol().to_string()))
          .collect();
        line.find("Increase volume").map(|byte_idx| {
          let cell_x = line[..byte_idx].chars().count() as u16;
          (cell_x + "Increase ".len() as u16, y)
        })
      })
      .expect("filtered help row should be rendered");

    let matched = buffer.cell((match_x, match_y)).unwrap();
    let unmatched = buffer
      .cell((match_x - "Increase ".len() as u16, match_y))
      .unwrap();
    assert_eq!(
      matched.style().fg,
      Some(app.user_config.theme.active.into())
    );
    assert_ne!(
      unmatched.style().fg,
      Some(app.user_config.theme.active.into())
    );
  }

  #[test]
  fn help_menu_renders_no_matches_and_clamps_a_stale_offset() {
    let mut app = App::default();
    app.view.help_filter = "not-a-real-help-row".to_string();
    app.view.help_menu_offset = u32::MAX;

    let rendered = rendered_help(&mut app);

    assert!(rendered.contains("No help rows match 'not-a-real-help-row'"));
    assert!(rendered.contains("matches for 'not-a-real-help-row' (0)"));
    assert!(rendered.contains("<Esc>: clear filter"));
  }
}

fn queue_item_line(item: &PlayableInfo, strings: &crate::core::i18n::Strings) -> String {
  match item {
    PlayableInfo::Track(t) => format!("{} - {}", t.display_name(strings), t.artists.join(", ")),
    PlayableInfo::Episode(e) => format!("{} - {}", e.name, e.show_name),
  }
}

/// Build the dimmed "up next from context" preview rows shown under the native
/// queue: what resumes once the queue drains. Returns an empty vector when
/// nothing is suspended and no queued items are pending, so `draw_queue` omits
/// the section entirely.
///
/// The source of truth is [`App::queue_suspended`](crate::core::app::App) when
/// the queue is draining over a suspended context; otherwise (queued items
/// pending over a still-playing context) it is that context's own upcoming
/// tracks. Rows read from the still-alive per-source `*_playback` state.
fn context_preview_lines(app: &App, max: usize) -> Vec<String> {
  // Format the upcoming rows of a Subsonic/YouTube `TrackInfo` context list.
  #[cfg(feature = "queue-download")]
  fn track_rows(
    tracks: &[crate::core::plugin_api::TrackInfo],
    start: usize,
    max: usize,
  ) -> Vec<String> {
    tracks
      .iter()
      .skip(start)
      .take(max)
      .map(|t| format!("{} - {}", t.name, t.artists.join(", ")))
      .collect()
  }

  // Local queues are `file://` URIs only (no API metadata), so display the
  // file name stem for each upcoming track.
  #[cfg(feature = "local-files")]
  fn local_rows(uris: &[String], start: usize, max: usize) -> Vec<String> {
    uris
      .iter()
      .skip(start)
      .take(max)
      .map(|u| {
        let trimmed = u.trim_start_matches("file://");
        std::path::Path::new(trimmed)
          .file_stem()
          .and_then(|s| s.to_str())
          .map(|s| s.to_string())
          .unwrap_or_else(|| u.clone())
      })
      .collect()
  }

  // The Spotify Web-API mirror's upcoming list (native or external context).
  let spotify_mirror = |max: usize| -> Vec<String> {
    app
      .queue
      .as_ref()
      .map(|q| {
        q.queue
          .iter()
          .take(max)
          .map(|item| queue_item_line(item, app.strings()))
          .collect()
      })
      .unwrap_or_default()
  };

  // 1. A suspended context is authoritative: the queue is draining over it.
  #[cfg(any(feature = "queue", feature = "internet-radio"))]
  if let Some(ctx) = app.queue_suspended.as_ref() {
    use crate::core::queue::SuspendedContext;
    return match ctx {
      #[cfg(feature = "streaming")]
      SuspendedContext::Spotify { .. } => spotify_mirror(max),
      // A client-side shuffle session resumes a flat track list; the Web-API
      // mirror is the best available metadata for its upcoming rows. An exhausted
      // session (`resume_index` is `None`) plays nothing on resume, so it shows no
      // preview rows.
      #[cfg(feature = "streaming")]
      SuspendedContext::SpotifyShuffled { resume_index, .. } => match resume_index {
        Some(_) => spotify_mirror(max),
        None => Vec::new(),
      },
      #[cfg(feature = "local-files")]
      SuspendedContext::Local { resume_index, .. } => {
        match (resume_index, app.local_playback.as_ref()) {
          (Some(i), Some(s)) => local_rows(&s.queue, *i, max),
          _ => Vec::new(),
        }
      }
      #[cfg(feature = "subsonic")]
      SuspendedContext::Subsonic { resume_index, .. } => {
        match (resume_index, app.subsonic_playback.as_ref()) {
          (Some(i), Some(s)) => track_rows(&s.tracks, *i, max),
          _ => Vec::new(),
        }
      }
      #[cfg(feature = "qobuz")]
      SuspendedContext::Qobuz { resume_index, .. } => {
        match (resume_index, app.qobuz_playback.as_ref()) {
          (Some(i), Some(s)) => track_rows(&s.tracks, *i, max),
          _ => Vec::new(),
        }
      }
      #[cfg(feature = "youtube")]
      SuspendedContext::YouTube { resume_index, .. } => {
        match (resume_index, app.youtube_playback.as_ref()) {
          (Some(i), Some(s)) => track_rows(&s.tracks, *i, max),
          _ => Vec::new(),
        }
      }
      #[cfg(feature = "internet-radio")]
      SuspendedContext::Radio { station } => vec![app.strings().popups.resume(&station.name)],
    };
  }

  // 2. Queued items pending over a still-playing context: preview what resumes
  //    after them (the context's upcoming tracks, from the next index on).
  if !app.native_queue.is_empty() {
    #[cfg(feature = "local-files")]
    if let Some(s) = app.local_playback.as_ref() {
      return local_rows(&s.queue, s.index + 1, max);
    }
    #[cfg(feature = "subsonic")]
    if let Some(s) = app.subsonic_playback.as_ref() {
      return track_rows(&s.tracks, s.index + 1, max);
    }
    #[cfg(feature = "qobuz")]
    if let Some(s) = app.qobuz_playback.as_ref() {
      return track_rows(&s.tracks, s.index + 1, max);
    }
    #[cfg(feature = "youtube")]
    if let Some(s) = app.youtube_playback.as_ref() {
      return track_rows(&s.tracks, s.index + 1, max);
    }
    #[cfg(feature = "internet-radio")]
    if let Some(s) = app.radio_playback.as_ref() {
      return vec![app.strings().popups.resume(&s.station.name)];
    }
    return spotify_mirror(max);
  }

  Vec::new()
}

pub fn draw_queue(f: &mut Frame<'_>, app: &App) {
  let [area] = f
    .area()
    .layout(&Layout::vertical([Constraint::Percentage(100)]).margin(2));
  draw_queue_in(f, app, area);
}

pub fn draw_queue_in(f: &mut Frame<'_>, app: &App, area: Rect) {
  let style = app.user_config.theme.base_style();
  let mut items: Vec<ListItem> = Vec::new();

  // Row 0: "Now playing" header. Prefer the native queue slot's current track;
  // fall back to the Spotify Web-API mirror (external Connect device) otherwise.
  let now_text = app
    .queue_now_display()
    .or_else(|| {
      app
        .queue
        .as_ref()
        .and_then(|q| q.currently_playing.as_ref())
        .map(|item| queue_item_line(item, app.strings()))
    })
    .unwrap_or_else(|| "—".to_string());
  items.push(
    ListItem::new(Line::from(vec![
      Span::styled(
        app.strings().popups.now_playing,
        style.add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
      ),
      Span::raw(now_text),
    ]))
    .style(style),
  );

  // The native queue is the selectable list.
  if app.native_queue.is_empty() {
    // With an empty native queue, fall back to displaying the legacy Spotify
    // mirror only when controlling an external Connect device (the queue there
    // lives Spotify-side). Otherwise show a hint.
    if app.spotify_external_device_active() {
      if let Some(q) = app.queue.as_ref() {
        for item in &q.queue {
          items.push(ListItem::new(queue_item_line(item, app.strings())).style(style));
        }
      }
    } else if !app.queue_owns_playback() {
      // While the queue owns playback the last queued track is the "Now playing"
      // row above, so an "empty" hint would contradict it — omit it there.
      items.push(
        ListItem::new(Span::raw(
          app
            .strings()
            .popups
            .queue_empty(&app.user_config.keys.add_item_to_queue.to_string()),
        ))
        .style(style),
      );
    }
  } else {
    for track in &app.native_queue {
      let source = crate::core::queue::queue_item_source(track.uri.as_deref().unwrap_or(""));
      let label = if source == crate::core::queue::QueueItemSource::LocalFile {
        app.strings().popups.source_local
      } else {
        crate::core::queue::source_label(source)
      };
      let line = format!(
        "{} - {}  [{}]",
        track.display_name(app.strings()),
        track.artists.join(", "),
        label
      );
      items.push(ListItem::new(line).style(style));
    }
  }

  // Dimmed, non-selectable preview of what resumes once the queue drains. It is
  // appended after the selectable native-queue rows; selection stays confined to
  // those rows (queue_menu.rs counts only `1 + native_queue.len()` rows), so
  // these extra rows never receive the highlight.
  let preview = context_preview_lines(app, 5);
  if !preview.is_empty() {
    let header_style = Style::default().fg(app.user_config.theme.hint.into());
    let row_style = Style::default()
      .fg(app.user_config.theme.inactive.into())
      .add_modifier(Modifier::DIM);
    items.push(
      ListItem::new(Span::styled(
        app.strings().popups.queue_up_next,
        header_style,
      ))
      .style(style),
    );
    for line in preview {
      items.push(ListItem::new(Span::styled(line, row_style)).style(style));
    }
  }

  let mut state = ListState::default();
  let len = items.len();
  let selected = if len == 0 {
    None
  } else {
    Some(app.view.queue_selected_index.min(len.saturating_sub(1)))
  };
  state.select(selected);
  let list = List::new(items)
    .block(
      Block::default()
        .borders(Borders::ALL)
        .style(style)
        .title(Span::styled(
          app
            .strings()
            .popups
            .queue_title(&app.user_config.keys.remove_from_queue.to_string()),
          style,
        ))
        .border_style(style),
    )
    .style(style)
    .highlight_style(
      Style::default()
        .fg(app.user_config.theme.active.into())
        .bg(app.user_config.theme.inactive.into())
        .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
    )
    .highlight_symbol(
      Line::from("▶ ").style(Style::default().fg(app.user_config.theme.active.into())),
    );
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
      Span::raw(app.strings().popups.api_response),
      Span::styled(
        app.api_error(),
        Style::default().fg(app.user_config.theme.error_text.into()),
      ),
    ]),
    Line::from(Span::styled(
      app.strings().popups.playback_check,
      Style::default().fg(app.user_config.theme.text.into()),
    )),
    Line::from(Span::styled(
      app.strings().popups.check_premium,
      Style::default().fg(app.user_config.theme.text.into()),
    )),
    Line::from(Span::styled(
      app.strings().popups.check_device,
      Style::default().fg(app.user_config.theme.text.into()),
    )),
    Line::from(Span::styled(
      app.strings().popups.check_spotifyd,
      Style::default().fg(app.user_config.theme.text.into()),
    )),
    Line::from(Span::styled(
      app.strings().popups.playback_hint,
      Style::default().fg(app.user_config.theme.hint.into()),
    )),
    Line::from(Span::styled(
      app.strings().popups.error_back,
      Style::default().fg(app.user_config.theme.inactive.into()),
    )),
  ];

  // The log path gets its own reserved rows instead of a place in the list
  // above: `api_error` has no length limit, and at 80x24 the margin and the
  // border leave twelve text lines, so a wrapping API error can fill the
  // frame on its own. Ordering alone would hold only for errors short enough
  // to leave room — pinning holds for all of them, which matters because the
  // user who is reading this screen is the one with something to report.
  let block = Block::default()
    .borders(Borders::ALL)
    .style(app.user_config.theme.base_style())
    .title(Span::styled(
      app.strings().popups.error_title,
      Style::default().fg(app.user_config.theme.error_border.into()),
    ))
    .border_style(Style::default().fg(app.user_config.theme.error_border.into()));
  let inner = block.inner(chunks[0]);
  f.render_widget(block, chunks[0]);

  let log_hint = app.strings().popups.log_hint;
  let log_label = app.strings().popups.log_file;
  let log_text = vec![
    Line::from(vec![
      Span::styled(
        log_label,
        Style::default().fg(app.user_config.theme.text.into()),
      ),
      Span::styled(
        app.log_path.clone(),
        Style::default().fg(app.user_config.theme.hint.into()),
      ),
    ]),
    Line::from(Span::styled(
      log_hint,
      Style::default().fg(app.user_config.theme.hint.into()),
    )),
  ];

  // Measured rather than assumed to be two rows: at 80 columns the margin and
  // the border leave 68, and a temp-directory log path is routinely longer
  // than what is left of that after the label. A reserved row count that is
  // too small would cut the filename off the one line a bug reporter needs.
  let log_rows = wrapped_rows(&format!("{log_label}{}", app.log_path), inner.width)
    + wrapped_rows(log_hint, inner.width);

  let sections = Layout::default()
    .direction(Direction::Vertical)
    .constraints([
      Constraint::Min(0),
      // Capped so a pathological path cannot leave no room for the error
      // itself, which is the other half of the report.
      Constraint::Length(log_rows.min(inner.height.saturating_sub(2).max(1))),
    ])
    .split(inner);

  f.render_widget(
    Paragraph::new(playing_text)
      .wrap(Wrap { trim: true })
      .style(app.user_config.theme.base_style()),
    sections[0],
  );
  f.render_widget(
    Paragraph::new(log_text)
      .wrap(Wrap { trim: true })
      .style(app.user_config.theme.base_style()),
    sections[1],
  );
}

/// Rows `text` occupies once ratatui has word-wrapped it to `width`: greedy,
/// and a word longer than a row is broken across rows rather than clipped.
/// Pure so the reserved height can be tested without a terminal.
///
/// Measured in terminal cells rather than characters, because that is what
/// ratatui wraps on: a path under a CJK user directory is twice as wide as it
/// is long, and counting characters would reserve too few rows and clip it.
fn wrapped_rows(text: &str, width: u16) -> u16 {
  use unicode_width::UnicodeWidthStr;

  let width = usize::from(width);
  if width == 0 {
    return 1;
  }

  let mut rows = 1usize;
  let mut used = 0usize;
  for word in text.split_whitespace() {
    let len = UnicodeWidthStr::width(word);
    if used > 0 {
      if used + 1 + len <= width {
        used += 1 + len;
        continue;
      }
      rows += 1;
    }
    // The word now starts a row of its own, and may outgrow it.
    let overflow = len.saturating_sub(1) / width;
    rows += overflow;
    used = len - overflow * width;
  }

  u16::try_from(rows).unwrap_or(u16::MAX)
}

#[cfg(test)]
mod error_screen_tests {
  use super::*;
  use ratatui::{backend::TestBackend, Terminal};

  fn rendered_error_screen_at(app: &App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|f| draw_error_screen(f, app)).unwrap();
    let buffer = terminal.backend().buffer();

    (0..height)
      .map(|y| {
        (0..width)
          .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol().to_string()))
          .collect::<String>()
      })
      .collect::<Vec<_>>()
      .join("\n")
  }

  fn rendered_error_screen(app: &App) -> String {
    rendered_error_screen_at(app, 100, 30)
  }

  #[test]
  fn error_screen_names_the_log_file_and_the_debug_flag() {
    let mut app = App::default();
    app.log_path = "/tmp/spotatui_logs/spotatuilog42".to_string();

    let rendered = rendered_error_screen(&app);

    assert!(
      rendered.contains("/tmp/spotatui_logs/spotatuilog42"),
      "the error screen must name the log file a reporter has to attach:\n{rendered}"
    );
    assert!(
      rendered.contains("--debug"),
      "the error screen must point at the debug mode:\n{rendered}"
    );
  }

  /// 80x24 is the floor a terminal is allowed to be, and a wrapping API error
  /// is the normal case on this screen rather than an exotic one.
  #[test]
  fn wrapped_rows_counts_word_wrapping_and_oversized_words() {
    assert_eq!(wrapped_rows("", 10), 1);
    assert_eq!(wrapped_rows("short", 10), 1);
    assert_eq!(wrapped_rows("one two three", 7), 2);
    // A path is one unbreakable word: 25 characters over rows of 10.
    assert_eq!(wrapped_rows(&"x".repeat(25), 10), 3);
    assert_eq!(wrapped_rows(&"x".repeat(20), 10), 2);
    // Exactly full must not spill into an extra row.
    assert_eq!(wrapped_rows(&"x".repeat(10), 10), 1);
    assert_eq!(wrapped_rows("anything", 0), 1);
    // Double-width characters take two cells each, so half as many fit.
    assert_eq!(wrapped_rows(&"中".repeat(5), 10), 1);
    assert_eq!(wrapped_rows(&"中".repeat(6), 10), 2);
  }

  /// A log path under a CJK user directory is twice as wide as it is long.
  #[test]
  fn error_screen_shows_a_double_width_log_path_in_full() {
    let path = format!("{}/spotatui.log", "中".repeat(65));
    let mut app = App::default();
    app.log_path = path.clone();

    let rendered = rendered_error_screen_at(&app, 80, 24);
    let unbroken = rendered.replace([' ', '\n', '\u{2502}'], "");

    assert!(
      unbroken.contains(&path),
      "a double-width path must not be clipped:\n{rendered}"
    );
    assert!(
      rendered.contains("--debug"),
      "reserving too few rows would push the hint out:\n{rendered}"
    );
  }

  /// A temp-directory log path is routinely longer than the columns an 80-wide
  /// terminal leaves after the label, and a cut-off filename is useless to the
  /// person filing the report.
  #[test]
  fn error_screen_shows_a_long_log_path_in_full() {
    let path = "/tmp/spotatui_logs/a_very_long_directory_name/spotatuilog_2026_09_24_181205.log";
    let mut app = App::default();
    app.log_path = path.to_string();
    app.handle_error(anyhow::anyhow!(
      "{}",
      "Player command failed: No active device found. ".repeat(40)
    ));

    let rendered = rendered_error_screen_at(&app, 80, 24);
    // The path wraps across rows, so the row padding and the frame have to go
    // before the pieces sit next to each other again.
    let unbroken = rendered.replace([' ', '\n', '\u{2502}'], "");

    assert!(
      unbroken.contains(path),
      "the full log path must survive an 80 column terminal:\n{rendered}"
    );
  }

  #[test]
  fn error_screen_still_names_the_log_file_on_a_small_terminal() {
    let mut app = App::default();
    app.log_path = "/tmp/spotatui_logs/spotatuilog42".to_string();
    app.handle_error(anyhow::anyhow!(
      "{}",
      "Player command failed: No active device found. ".repeat(40)
    ));

    let rendered = rendered_error_screen_at(&app, 80, 24);

    assert!(
      rendered.contains("/tmp/spotatui_logs/spotatuilog42"),
      "a long error must not push the log path off an 80x24 screen:\n{rendered}"
    );
    assert!(
      rendered.contains("--debug"),
      "the debug hint must survive the same squeeze:\n{rendered}"
    );
  }
}

#[cfg(test)]
mod queue_tests {
  use super::*;
  use crate::tui::event::Key;
  use ratatui::{backend::TestBackend, Terminal};

  #[test]
  fn empty_queue_hint_names_the_configured_add_key() {
    let mut app = App::default_connected();
    app.user_config.keys.add_item_to_queue = Key::Char('a');

    let mut terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();
    terminal.draw(|f| draw_queue(f, &app)).unwrap();
    let buffer = terminal.backend().buffer();
    let content: String = (0..20)
      .flat_map(|y| (0..100).map(move |x| (x, y)))
      .filter_map(|(x, y)| buffer.cell((x, y)).map(|c| c.symbol().to_string()))
      .collect();

    assert!(
      content.contains("press a on a track"),
      "empty queue hint should name the rebound key: {content}"
    );
    assert!(
      !content.contains("press z"),
      "empty queue hint must not still show the default key: {content}"
    );
  }
}

pub fn draw_dialog(f: &mut Frame<'_>, app: &App) {
  let dialog_context = match app.get_current_route().active_block {
    ActiveBlock::Dialog(context) => context,
    _ => return,
  };

  match dialog_context {
    DialogContext::PlaylistWindow
    | DialogContext::PlaylistSearch
    | DialogContext::YouTubePlaylistWindow => {
      if let Some(playlist) = app.view.dialog.as_ref() {
        let text = vec![
          Line::from(Span::raw(app.strings().popups.delete_playlist_question)),
          Line::from(Span::styled(
            playlist.as_str(),
            Style::default().add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
          )),
          Line::from(Span::raw(app.strings().popups.question_mark)),
        ];
        draw_confirmation_dialog(f, app, app.strings().popups.confirm, text, 45);
      }
    }
    DialogContext::RemoveTrackFromPlaylistConfirm => {
      if let Some(pending_remove) = app.pending_playlist_track_removal.as_ref() {
        let text = vec![
          Line::from(Span::raw(app.strings().popups.remove_track_question)),
          Line::from(Span::styled(
            app.strings().popups.track(&pending_remove.track_name),
            Style::default().add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
          )),
          Line::from(Span::styled(
            app.strings().popups.playlist(&pending_remove.playlist_name),
            Style::default().add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
          )),
        ];
        draw_confirmation_dialog(f, app, app.strings().popups.remove_track, text, 60);
      }
    }
    DialogContext::PersistKeybindingFallback => {
      if let Some(open_settings_key) = app.pending_keybinding_persist_key() {
        let text = vec![
          Line::from(Span::raw(app.strings().popups.fallback_terminal)),
          Line::from(Span::raw(app.strings().popups.fallback_question)),
          Line::from(Span::styled(
            app
              .strings()
              .popups
              .save_shortcut(&open_settings_key.to_string()),
            Style::default().add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
          )),
        ];
        draw_confirmation_dialog(
          f,
          app,
          app.strings().popups.save_shortcut_fallback,
          text,
          66,
        );
      }
    }
    DialogContext::RemovePlaylistSyncLinkConfirm => {
      if let Some(name) = app.view.dialog.as_ref() {
        let text = vec![
          Line::from(Span::raw(app.strings().popups.remove_link_question)),
          Line::from(Span::styled(
            name.as_str(),
            Style::default().add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
          )),
          Line::from(Span::raw(app.strings().popups.mirrors_stay)),
        ];
        draw_confirmation_dialog(f, app, app.strings().popups.remove_link, text, 50);
      }
    }
    DialogContext::AddTrackToPlaylistPicker => {
      draw_add_track_to_playlist_picker_dialog(f, app);
    }
    DialogContext::PlaylistSyncPicker => {
      draw_playlist_sync_picker_dialog(f, app);
    }
  }
}

/// A modal box centred horizontally and a third of the way down `bounds`, clamped
/// to fit. Shared so every overlay in the app sits in the same place.
pub(crate) fn centered_modal_rect(
  bounds: Rect,
  requested_width: u16,
  requested_height: u16,
) -> Rect {
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
        .fg(app.user_config.theme.header.into())
        .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
    ))
    .borders(Borders::ALL)
    .style(app.user_config.theme.base_style())
    .border_style(Style::default().fg(app.user_config.theme.inactive.into()));
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

  let ok = Paragraph::new(Span::raw(app.strings().popups.ok))
    .style(Style::default().fg(if app.view.confirm {
      app.user_config.theme.hovered.into()
    } else {
      app.user_config.theme.inactive.into()
    }))
    .alignment(Alignment::Center);
  f.render_widget(ok, hchunks[0]);

  let cancel = Paragraph::new(Span::raw(app.strings().popups.cancel))
    .style(Style::default().fg(if app.view.confirm {
      app.user_config.theme.inactive.into()
    } else {
      app.user_config.theme.hovered.into()
    }))
    .alignment(Alignment::Center);
  f.render_widget(cancel, hchunks[1]);
}

fn draw_add_track_to_playlist_picker_dialog(f: &mut Frame<'_>, app: &App) {
  let rect = centered_modal_rect(f.area(), 70, 20);
  f.render_widget(Clear, rect);

  let block = Block::default()
    .title(Span::styled(
      app.strings().popups.add_track_title,
      Style::default()
        .fg(app.user_config.theme.header.into())
        .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
    ))
    .borders(Borders::ALL)
    .style(app.user_config.theme.base_style())
    .border_style(Style::default().fg(app.user_config.theme.inactive.into()));
  f.render_widget(block, rect);

  let vchunks = Layout::default()
    .direction(Direction::Vertical)
    .margin(1)
    .constraints([
      Constraint::Length(2),
      Constraint::Length(1),
      Constraint::Min(3),
      Constraint::Length(1),
    ])
    .split(rect);

  let track_name = app
    .pending_playlist_track_add
    .as_ref()
    .map(|p| p.track_name.as_str())
    .unwrap_or(app.strings().popups.selected_track);

  let header = Paragraph::new(Line::from(Span::raw(
    app.strings().popups.choose_playlist(track_name),
  )))
  .wrap(Wrap { trim: true })
  .style(app.user_config.theme.base_style());
  f.render_widget(header, vchunks[0]);

  let theme = app.user_config.theme;
  let popups = &app.strings().popups;
  let filter = app.view.playlist_picker_filter.as_str();
  let search_line = if app.view.playlist_picker_filter_editing {
    Line::from(vec![
      Span::styled(
        popups.playlist_picker_search_prompt,
        Style::default().fg(theme.active.into()),
      ),
      Span::raw(filter),
      Span::styled("▏", Style::default().fg(theme.active.into())),
      Span::styled(
        popups.playlist_picker_search_edit_hint,
        Style::default().fg(theme.inactive.into()),
      ),
    ])
  } else if !filter.is_empty() {
    Line::from(vec![
      Span::styled(
        popups.playlist_picker_search_prompt,
        Style::default().fg(theme.active.into()),
      ),
      Span::raw(filter),
    ])
  } else {
    Line::from(Span::styled(
      popups.playlist_picker_search_hint(&app.user_config.keys.search.to_string()),
      Style::default().fg(theme.inactive.into()),
    ))
  };
  f.render_widget(
    Paragraph::new(search_line).style(theme.base_style()),
    vchunks[1],
  );

  let mut list_state = ListState::default();
  // Rows follow the active source: local YouTube playlists under the YouTube
  // source, editable Spotify playlists plus folder rows otherwise (must stay
  // in sync with the picker's key handler).
  let picker_rows = app.playlist_picker_items();

  if picker_rows.is_empty() {
    let empty_message = if filter.trim().is_empty() {
      popups.no_editable_playlists.to_string()
    } else {
      popups.no_matching_playlists(filter.trim())
    };
    let empty_text = Paragraph::new(empty_message)
      .style(Style::default().fg(app.user_config.theme.inactive.into()))
      .alignment(Alignment::Center);
    f.render_widget(empty_text, vchunks[2]);
  } else {
    let is_own_playlist = |playlist: &crate::core::plugin_api::PlaylistInfo| -> bool {
      // Local YouTube playlists carry no owner id — they are always the
      // user's own (no "(collab)" suffix).
      playlist.owner_id.is_none()
        || app
          .user()
          .as_ref()
          .is_some_and(|user| Some(user.id.as_str()) == playlist.owner_id.as_deref())
    };
    let items: Vec<ListItem> = picker_rows
      .iter()
      .map(|row| {
        let label = match row {
          // Same folder rendering as the sidebar (ui/library.rs): back rows
          // ("← name") as-is, other folders with the FOLDER_ICON prefix.
          PlaylistPickerRow::Folder(folder) => {
            let name = folder.display_name(app.strings());
            if name.starts_with('\u{2190}') {
              name.into_owned()
            } else {
              format!("{} {name}", super::util::FOLDER_ICON)
            }
          }
          PlaylistPickerRow::Playlist(playlist) => {
            if is_own_playlist(playlist) {
              playlist.name.clone()
            } else {
              // `owner` is the display name, falling back to the owner id.
              app
                .strings()
                .popups
                .collaborative_playlist(&playlist.name, &playlist.owner)
            }
          }
        };
        ListItem::new(Span::raw(label))
      })
      .collect();
    let selected = app
      .view
      .playlist_picker_selected_index
      .min(picker_rows.len() - 1);
    list_state.select(Some(selected));

    let list = List::new(items)
      .style(app.user_config.theme.base_style())
      .highlight_style(Style::default().fg(app.user_config.theme.hovered.into()))
      .highlight_symbol("▶ ");

    f.render_stateful_widget(list, vchunks[2], &mut list_state);
  }

  let footer = Paragraph::new(app.strings().popups.playlist_picker_controls(
    &app.user_config.keys.move_down.to_string(),
    &app.user_config.keys.move_up.to_string(),
  ))
  .style(Style::default().fg(app.user_config.theme.inactive.into()))
  .alignment(Alignment::Center);
  f.render_widget(footer, vchunks[3]);
}

fn draw_playlist_sync_picker_dialog(f: &mut Frame<'_>, app: &App) {
  let rect = centered_modal_rect(f.area(), 50, 12);
  f.render_widget(Clear, rect);

  let block = Block::default()
    .title(Span::styled(
      app.strings().popups.mirror_playlist,
      Style::default()
        .fg(app.user_config.theme.header.into())
        .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
    ))
    .borders(Borders::ALL)
    .style(app.user_config.theme.base_style())
    .border_style(Style::default().fg(app.user_config.theme.inactive.into()));
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

  let master = app
    .pending_playlist_sync_master()
    .map(|endpoint| endpoint.name.as_str())
    .unwrap_or(app.strings().popups.selected_playlist);

  let header = Paragraph::new(Line::from(Span::raw(
    app.strings().popups.mirror_onto(master),
  )))
  .wrap(Wrap { trim: true })
  .style(app.user_config.theme.base_style());
  f.render_widget(header, vchunks[0]);

  let sources = app.playlist_sync_picker_sources();
  if sources.is_empty() {
    let empty_text = Paragraph::new(app.strings().popups.no_mirror_source)
      .style(Style::default().fg(app.user_config.theme.inactive.into()))
      .alignment(Alignment::Center);
    f.render_widget(empty_text, vchunks[1]);
  } else {
    let items: Vec<ListItem> = sources
      .iter()
      .map(|source| ListItem::new(Span::raw(app.strings().sidebar.source_label(*source))))
      .collect();
    let selected = app.view.playlist_sync_picker_index.min(sources.len() - 1);
    let mut list_state = ListState::default();
    list_state.select(Some(selected));

    let list = List::new(items)
      .style(app.user_config.theme.base_style())
      .highlight_style(Style::default().fg(app.user_config.theme.hovered.into()))
      .highlight_symbol("▶ ");

    f.render_stateful_widget(list, vchunks[1], &mut list_state);
  }

  let footer = Paragraph::new(app.strings().popups.mirror_picker_controls(
    &app.user_config.keys.move_down.to_string(),
    &app.user_config.keys.move_up.to_string(),
  ))
  .style(Style::default().fg(app.user_config.theme.inactive.into()))
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
    AnnouncementLevel::Info => (
      app.strings().popups.announcement_info,
      app.user_config.theme.active,
    ),
    AnnouncementLevel::Warning => (
      app.strings().popups.announcement_warning,
      app.user_config.theme.hint,
    ),
    AnnouncementLevel::Critical => (
      app.strings().popups.announcement_critical,
      app.user_config.theme.error_text,
    ),
  };

  let mut text = vec![
    Line::from(Span::styled(
      format!("{}  {}", level_label, announcement.title),
      Style::default().add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
    )),
    Line::from(""),
  ];

  for line in announcement.body.lines() {
    text.push(Line::from(line.to_string()));
  }

  if let Some(url) = &announcement.url {
    text.push(Line::from(""));
    text.push(Line::from(Span::styled(
      app.strings().popups.announcement_more(url),
      Style::default().add_modifier(app.user_config.behavior.emphasis(Modifier::ITALIC)),
    )));
  }

  text.push(Line::from(""));
  text.push(Line::from(Span::styled(
    app.strings().popups.announcement_dismiss,
    Style::default().fg(app.user_config.theme.inactive.into()),
  )));

  let paragraph = Paragraph::new(text)
    .style(app.user_config.theme.base_style())
    .alignment(Alignment::Left)
    .wrap(Wrap { trim: false })
    .block(
      Block::default()
        .borders(Borders::ALL)
        .style(app.user_config.theme.base_style())
        .border_style(Style::default().fg(accent_color.into()))
        .title(app.strings().popups.announcement_title),
    );

  f.render_widget(paragraph, rect);
}

pub fn draw_recap_prompt(f: &mut Frame<'_>, app: &App) {
  let Some(prompt) = app.recap_prompt() else {
    return;
  };

  let width = std::cmp::min(f.area().width.saturating_sub(4), 64);
  let height = 10;
  let rect = f
    .area()
    .centered(Constraint::Length(width), Constraint::Length(height));

  f.render_widget(Clear, rect);

  let text = vec![
    Line::from(Span::styled(
      app.strings().popups.recap_ready,
      Style::default()
        .fg(app.user_config.theme.active.into())
        .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
    )),
    Line::from(""),
    Line::from(app.strings().popups.recap_listens(prompt.listens)),
    Line::from(app.strings().popups.recap_browser),
    Line::from(""),
    Line::from(Span::styled(
      app.strings().popups.recap_controls,
      Style::default().fg(app.user_config.theme.inactive.into()),
    )),
  ];

  let paragraph = Paragraph::new(text)
    .style(app.user_config.theme.base_style())
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: false })
    .block(
      Block::default()
        .borders(Borders::ALL)
        .style(app.user_config.theme.base_style())
        .border_style(Style::default().fg(app.user_config.theme.active.into()))
        .title(app.strings().popups.recap_title),
    );

  f.render_widget(paragraph, rect);
}

/// The offer to set up Soloist, after librespot failed several tracks in a row.
pub fn draw_soloist_offer(f: &mut Frame<'_>, app: &App) {
  if !app.soloist_offer_shows() {
    return;
  }
  let strings = &app.strings().popups;
  let width = std::cmp::min(f.area().width.saturating_sub(4), 70);
  let rect = f
    .area()
    .centered(Constraint::Length(width), Constraint::Length(13));
  f.render_widget(Clear, rect);
  let text = vec![
    Line::from(strings.soloist_offer_body),
    Line::from(""),
    Line::from(strings.soloist_offer_step_download),
    Line::from(strings.soloist_offer_step_key),
    Line::from(strings.soloist_offer_step_engine),
    Line::from(""),
    Line::from(Span::styled(
      strings.soloist_offer_controls,
      Style::default().fg(app.user_config.theme.inactive.into()),
    )),
  ];
  let paragraph = Paragraph::new(text)
    .style(app.user_config.theme.base_style())
    .wrap(Wrap { trim: false })
    .block(
      Block::default()
        .borders(Borders::ALL)
        .style(app.user_config.theme.base_style())
        .border_style(Style::default().fg(app.user_config.theme.active.into()))
        .title(strings.soloist_offer_title),
    );
  f.render_widget(paragraph, rect);
}

/// The Soloist pairing guide over every screen, until Soloist logs in or Esc closes it.
pub fn draw_soloist_pairing(f: &mut Frame<'_>, app: &App) {
  let Some(device) = app.soloist_pairing_device() else {
    return;
  };
  let strings = &app.strings().popups;
  let width = std::cmp::min(f.area().width.saturating_sub(4), 60);
  let rect = f
    .area()
    .centered(Constraint::Length(width), Constraint::Length(9));
  f.render_widget(Clear, rect);
  let text = vec![
    Line::from(strings.soloist_pair_open),
    Line::from(strings.soloist_pair_pick(device)),
    Line::from(""),
    Line::from(Span::styled(
      strings.soloist_pair_waiting,
      Style::default().fg(app.user_config.theme.active.into()),
    )),
    Line::from(""),
    Line::from(Span::styled(
      strings.plugin_close_hint,
      Style::default().fg(app.user_config.theme.inactive.into()),
    )),
  ];
  let paragraph = Paragraph::new(text)
    .style(app.user_config.theme.base_style())
    .wrap(Wrap { trim: false })
    .block(
      Block::default()
        .borders(Borders::ALL)
        .style(app.user_config.theme.base_style())
        .border_style(Style::default().fg(app.user_config.theme.active.into()))
        .title(strings.soloist_pair_title),
    );
  f.render_widget(paragraph, rect);
}

#[cfg(all(test, feature = "soloist"))]
mod soloist_pairing_tests {
  use super::*;
  use crate::tui::event::Key;
  use ratatui::{backend::TestBackend, Terminal};

  #[test]
  fn the_pairing_guide_names_the_device_until_esc_closes_it() {
    let mut app = App::default();
    app.soloist_notice(crate::infra::soloist::Notice::NotPaired);
    let mut terminal = Terminal::new(TestBackend::new(80, 20)).unwrap();
    terminal.draw(|f| draw_soloist_pairing(f, &app)).unwrap();
    let content: String = terminal
      .backend()
      .buffer()
      .content()
      .iter()
      .map(|cell| cell.symbol())
      .collect();
    let pick = app
      .strings()
      .popups
      .soloist_pair_pick(&app.user_config.behavior.streaming_device_name);
    assert!(content.contains(&pick), "{content}");

    crate::tui::handlers::handle_app(Key::Esc, &mut app);
    assert_eq!(app.soloist_pairing_device(), None);
  }

  #[test]
  fn the_soloist_offer_lists_the_steps_until_esc_closes_it() {
    let mut app = App::default();
    app.offer_soloist_setup();
    let mut terminal = Terminal::new(TestBackend::new(80, 20)).unwrap();
    terminal.draw(|f| draw_soloist_offer(f, &app)).unwrap();
    let content: String = terminal
      .backend()
      .buffer()
      .content()
      .iter()
      .map(|cell| cell.symbol())
      .collect();
    let strings = &app.strings().popups;
    assert!(content.contains(strings.soloist_offer_title), "{content}");
    assert!(
      content.contains(strings.soloist_offer_step_key),
      "{content}"
    );

    crate::tui::handlers::handle_app(Key::Esc, &mut app);
    assert!(!app.soloist_offer_shows());
  }
}

pub fn draw_community_pin_prompt(f: &mut Frame<'_>, app: &App) {
  let width = std::cmp::min(f.area().width.saturating_sub(4), 66);
  let height = 14;
  let rect = f
    .area()
    .centered(Constraint::Length(width), Constraint::Length(height));

  f.render_widget(Clear, rect);

  let text = vec![
    Line::from(Span::styled(
      app.strings().popups.community_pinned,
      Style::default()
        .fg(app.user_config.theme.active.into())
        .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
    )),
    Line::from(""),
    Line::from(app.strings().popups.community_details),
    Line::from(app.strings().popups.community_add),
    Line::from(""),
    Line::from(app.strings().popups.community_choice),
    Line::from(""),
    Line::from(Span::styled(
      app.strings().popups.community_controls,
      Style::default().fg(app.user_config.theme.inactive.into()),
    )),
  ];

  let paragraph = Paragraph::new(text)
    .style(app.user_config.theme.base_style())
    .alignment(Alignment::Left)
    .wrap(Wrap { trim: false })
    .block(
      Block::default()
        .borders(Borders::ALL)
        .style(app.user_config.theme.base_style())
        .border_style(Style::default().fg(app.user_config.theme.active.into()))
        .title(app.strings().popups.community_title),
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
      app.strings().popups.exit_question,
      Style::default().add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
    )),
    Line::from(""),
    Line::from(app.strings().popups.exit_yes_no),
    Line::from(Span::styled(
      app.strings().popups.exit_controls,
      Style::default().fg(app.user_config.theme.inactive.into()),
    )),
  ];

  let paragraph = Paragraph::new(text)
    .style(app.user_config.theme.base_style())
    .alignment(Alignment::Center)
    .block(
      Block::default()
        .borders(Borders::ALL)
        .style(app.user_config.theme.base_style())
        .border_style(Style::default().fg(app.user_config.theme.active.into()))
        .title(app.strings().popups.exit_title),
    );

  f.render_widget(paragraph, rect);
}

/// Draw the sort menu popup overlay
pub fn draw_sort_menu(f: &mut Frame<'_>, app: &App) {
  if !app.view.sort_menu_visible {
    return;
  }

  let context = match app.view.sort_context {
    Some(ctx) => ctx,
    None => return,
  };

  let available_fields = context.available_fields();
  let current_sort = app.sort_state(context);

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
        format!(
          " {}",
          current_sort.order.indicator_icon(
            &app.user_config.behavior.sort_ascending_icon,
            &app.user_config.behavior.sort_descending_icon,
          )
        )
      } else {
        String::new()
      };
      let text = format!(
        "{}{}{}",
        app.strings().popups.sort_field(*field),
        shortcut,
        indicator
      );

      let style = if i == app.view.sort_menu_selected {
        Style::default()
          .fg(app.user_config.theme.active.into())
          .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD))
      } else if *field == current_sort.field {
        Style::default().fg(app.user_config.theme.hovered.into())
      } else {
        Style::default().fg(app.user_config.theme.text.into())
      };

      ListItem::new(text).style(style)
    })
    .collect();

  let title = match context {
    crate::core::sort::SortContext::PlaylistTracks => app.strings().popups.sort_tracks,
    crate::core::sort::SortContext::SavedAlbums => app.strings().popups.sort_albums,
    crate::core::sort::SortContext::SavedArtists => app.strings().popups.sort_artists,
    crate::core::sort::SortContext::RecentlyPlayed => app.strings().popups.sort_title,
  };

  let list = List::new(items)
    .block(
      Block::default()
        .borders(Borders::ALL)
        .style(app.user_config.theme.base_style())
        .border_style(Style::default().fg(app.user_config.theme.active.into()))
        .title(Span::styled(
          title,
          Style::default()
            .fg(app.user_config.theme.active.into())
            .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
        )),
    )
    .highlight_style(
      Style::default()
        .fg(app.user_config.theme.active.into())
        .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
    )
    .highlight_symbol(
      Line::from("▶ ").style(Style::default().fg(app.user_config.theme.active.into())),
    );

  let mut state = ListState::default();
  state.select(Some(app.view.sort_menu_selected));

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
    .fg(app.user_config.theme.active.into())
    .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD));
  let hint_style = Style::default().fg(app.user_config.theme.hint.into());

  let mut lines: Vec<Line> = Vec::new();

  match app.party_status() {
    PartyStatus::Disconnected | PartyStatus::Connecting => {
      if !app.view.party_input.is_empty()
        || app.view.party_input_idx > 0
        || !app.view.party_join_name.is_empty()
      {
        let code_str: String = app
          .view
          .party_input
          .iter()
          .filter(|c| c.is_alphanumeric())
          .map(|c| c.to_ascii_uppercase())
          .collect();
        let name_str: String = app.view.party_join_name.iter().collect();
        let trimmed_name = name_str.trim();
        lines.push(Line::from(Span::styled(
          app.strings().popups.party_code_prompt,
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
        lines.push(Line::from(Span::styled(
          app.strings().popups.party_name_prompt,
          style,
        )));
        lines.push(Line::from(Span::styled(
          format!("  [ {} ]", name_display),
          active_style,
        )));
        lines.push(Line::from(""));
        if code_str.len() == 6 && !trimmed_name.is_empty() {
          lines.push(Line::from(Span::styled(
            app.strings().popups.party_join_enter,
            hint_style,
          )));
        } else if code_str.len() == 6 {
          lines.push(Line::from(Span::styled(
            app.strings().popups.party_name_required,
            hint_style,
          )));
        } else {
          let char_count = app.strings().popups.party_code_length(code_str.len());
          lines.push(Line::from(Span::styled(char_count, hint_style)));
        }
        lines.push(Line::from(Span::styled(
          app
            .strings()
            .popups
            .party_name_length(trimmed_name.chars().count()),
          hint_style,
        )));
        lines.push(Line::from(Span::styled(
          app.strings().popups.party_code_then_name,
          hint_style,
        )));
        lines.push(Line::from(Span::styled(
          app.strings().popups.esc_cancel,
          hint_style,
        )));
      } else {
        lines.push(Line::from(Span::styled(
          app.strings().popups.listening_party,
          active_style,
        )));
        lines.push(Line::from(""));
        if *app.party_status() == PartyStatus::Connecting {
          lines.push(Line::from(Span::styled(
            app.strings().popups.connecting,
            hint_style,
          )));
        } else {
          lines.push(Line::from(vec![
            Span::styled("1 ", active_style),
            Span::styled(app.strings().popups.host_party, style),
          ]));
          lines.push(Line::from(vec![
            Span::styled("2 ", active_style),
            Span::styled(app.strings().popups.join_party, style),
          ]));
          lines.push(Line::from(""));
          lines.push(Line::from(Span::styled(
            app.strings().popups.esc_close,
            hint_style,
          )));
        }
      }
    }
    PartyStatus::Hosting => {
      lines.push(Line::from(Span::styled(
        app.strings().popups.party_hosting,
        active_style,
      )));
      lines.push(Line::from(""));
      if let Some(session) = app.party_session() {
        let code_display = if session.code.is_empty() {
          app.strings().popups.generating.to_string()
        } else {
          session.code.clone()
        };
        lines.push(Line::from(vec![
          Span::styled(app.strings().popups.share_code, style),
          Span::styled(code_display, active_style),
        ]));
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
          Span::styled(app.strings().popups.control_label, style),
          Span::styled(
            match session.control_mode {
              crate::infra::network::sync::ControlMode::HostOnly => {
                app.strings().popups.control_host_only
              }
              crate::infra::network::sync::ControlMode::SharedControl => {
                app.strings().popups.control_shared
              }
            },
            style,
          ),
        ]));
        lines.push(Line::from(""));
        if session.guests.is_empty() {
          lines.push(Line::from(Span::styled(
            app.strings().popups.waiting_guests,
            hint_style,
          )));
        } else {
          let listener_label = if session.guests.len() == 1 {
            app.strings().popups.listener_one.to_string()
          } else {
            app.strings().popups.listeners(session.guests.len())
          };
          lines.push(Line::from(Span::styled(listener_label, style)));
          for (i, guest) in session.guests.iter().enumerate() {
            let label = format!("  {}. {}", i + 1, guest);
            lines.push(Line::from(Span::styled(label, style)));
          }
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
          app.strings().popups.party_toggle_control,
          hint_style,
        )));
        lines.push(Line::from(Span::styled(
          app.strings().popups.party_leave,
          hint_style,
        )));
        lines.push(Line::from(Span::styled(
          app.strings().popups.party_close_menu,
          hint_style,
        )));
      }
    }
    PartyStatus::Joined => {
      lines.push(Line::from(Span::styled(
        app.strings().popups.party_guest,
        active_style,
      )));
      lines.push(Line::from(""));
      if let Some(session) = app.party_session() {
        lines.push(Line::from(vec![
          Span::styled(app.strings().popups.host_label, style),
          Span::styled(&session.host_name, style),
        ]));
        lines.push(Line::from(vec![
          Span::styled(app.strings().popups.room_label, style),
          Span::styled(&session.code, active_style),
        ]));
        lines.push(Line::from(vec![
          Span::styled(app.strings().popups.mode_label, style),
          Span::styled(app.strings().popups.following_host, hint_style),
        ]));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
          app.strings().popups.party_leave,
          hint_style,
        )));
        lines.push(Line::from(Span::styled(
          app.strings().popups.party_close_menu,
          hint_style,
        )));
      }
    }
  }

  let title = match app.party_status() {
    PartyStatus::Hosting => app.strings().popups.party_hosting_title,
    PartyStatus::Joined => app.strings().popups.party_joined_title,
    _ => app.strings().popups.party_title,
  };

  let paragraph = Paragraph::new(lines)
    .block(
      Block::default()
        .borders(Borders::ALL)
        .style(style)
        .title(Span::styled(title, active_style))
        .border_style(Style::default().fg(app.user_config.theme.active.into())),
    )
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: false });

  f.render_widget(paragraph, popup_area);
}

/// Draw the plugin popup overlay, if one is active.
///
/// Called last in the terminal draw closure so it overlays every screen.
pub fn draw_plugin_popup(f: &mut Frame<'_>, app: &App) {
  let popup = match &app.plugin_popup {
    Some(p) => p,
    None => return,
  };

  // Compute width: fit to longest line/title, clamped to 70% of area.
  let area = f.area();
  let max_width = (area.width as u32 * 70 / 100).min(u16::MAX as u32) as u16;
  let content_width = popup
    .lines
    .iter()
    .map(|l| l.text.len() as u16)
    .chain(std::iter::once(popup.title.len() as u16))
    .max()
    .unwrap_or(0)
    .saturating_add(4); // 2 border + 2 padding
  let width = content_width.clamp(20, max_width);

  // Height: line count + 2 borders + 1 footer, clamped.
  let footer_lines = 1u16;
  let content_height = popup.lines.len() as u16 + 2 + footer_lines;
  let max_height = area.height.saturating_sub(2).max(3);
  let height = content_height.clamp(4, max_height);

  let rect = centered_modal_rect(area, width, height);
  f.render_widget(Clear, rect);

  // Build styled lines.
  let mut ratatui_lines: Vec<Line> = popup.lines.iter().map(|pl| build_popup_line(pl)).collect();

  // Footer hint.
  ratatui_lines.push(Line::from(Span::styled(
    app.strings().popups.plugin_close_hint,
    Style::default().fg(app.user_config.theme.hint.into()),
  )));

  let block = Block::default()
    .borders(Borders::ALL)
    .style(app.user_config.theme.base_style())
    .border_style(Style::default().fg(app.user_config.theme.active.into()))
    .title(Span::styled(
      popup.title.clone(),
      Style::default()
        .fg(app.user_config.theme.header.into())
        .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
    ));

  let paragraph = Paragraph::new(ratatui_lines)
    .block(block)
    .scroll((app.view.plugin_popup_scroll, 0));

  f.render_widget(paragraph, rect);
}

fn build_popup_line<'a>(pl: &'a PopupLine) -> Line<'a> {
  let mut style = Style::default();
  if let Some(fg) = pl.fg {
    style = style.fg(fg.into());
  }
  if pl.bold {
    style = style.add_modifier(Modifier::BOLD);
  }
  if pl.italic {
    style = style.add_modifier(Modifier::ITALIC);
  }
  Line::from(Span::styled(pl.text.clone(), style))
}

#[cfg(test)]
mod playlist_sync_picker_tests {
  use super::*;
  use crate::core::action::Action;
  use crate::core::plugin_api::PlaylistInfo;
  use crate::core::source::Source;
  use ratatui::{backend::TestBackend, Terminal};

  #[test]
  fn the_mirror_picker_lists_the_offered_sources() {
    let mut app = App::default_connected().under_source(Source::Qobuz);
    app.qobuz_playlists_mut().push(PlaylistInfo {
      generated_name: None,
      uri: "qobuz:playlist:9".to_string(),
      name: "Mine".to_string(),
      owner: "qobuz".to_string(),
      track_count: 3,
      id: Some("9".to_string()),
      owner_id: None,
      collaborative: false,
      public: None,
      image_url: None,
    });
    app.view.selected_playlist_index = Some(0);
    app.apply(Action::OpenPlaylistSyncPicker);

    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|f| draw_dialog(f, &app)).unwrap();
    let buffer = terminal.backend().buffer();
    let content: String = (0..24)
      .flat_map(|y| (0..80).map(move |x| (x, y)))
      .filter_map(|(x, y)| buffer.cell((x, y)).map(|c| c.symbol().to_string()))
      .collect();

    assert!(
      content.contains("Mirror Playlist"),
      "picker title missing: {content}"
    );
    assert!(
      content.contains("Mirror \"Mine\" onto:"),
      "picker header missing: {content}"
    );
    assert!(
      content.contains("Spotify"),
      "the connected session should be offered: {content}"
    );
    assert!(
      !content.contains("Qobuz"),
      "the master's own source must not be offered: {content}"
    );
  }
}

#[cfg(test)]
mod playlist_picker_localization_tests {
  use super::*;
  use crate::core::app::{PlaylistFolder, PlaylistFolderItem};
  use crate::core::i18n::Language;
  use ratatui::{backend::TestBackend, Terminal};

  #[test]
  fn the_playlist_picker_shows_the_search_hint_query_and_no_match_message() {
    let mut app = App::default();
    let render = |app: &App| {
      let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
      terminal
        .draw(|frame| draw_add_track_to_playlist_picker_dialog(frame, app))
        .unwrap();
      let buffer = terminal.backend().buffer();
      (0..buffer.area.height)
        .map(|y| {
          (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
    };

    assert!(render(&app).contains("/ search playlists"));

    app.view.playlist_picker_filter = "zzz".to_string();
    app.view.playlist_picker_filter_editing = true;
    let text = render(&app);
    assert!(text.contains("Search: zzz"), "{text}");
    assert!(text.contains("No playlists match \"zzz\""), "{text}");
  }

  #[test]
  fn the_playlist_picker_relabels_generated_folders_and_back_rows_without_renaming_user_folders() {
    let mut app = App::default();
    for back in [false, true] {
      app.view.playlist_picker_folder_id = if back { 2 } else { 0 };
      *app.playlist_folder_items_mut() = [true, false]
        .into_iter()
        .map(|generated_name| {
          PlaylistFolderItem::Folder(PlaylistFolder {
            name: if back {
              "← Unnamed Folder"
            } else {
              "Unnamed Folder"
            }
            .into(),
            generated_name,
            current_id: if back { 2 } else { 0 },
            target_id: 1,
          })
        })
        .collect();
      for language in [Language::En, Language::ZhHans, Language::ZhHant] {
        app.user_config.behavior.language = language;
        app.refresh_language();
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        terminal
          .draw(|frame| draw_add_track_to_playlist_picker_dialog(frame, &app))
          .unwrap();
        use unicode_width::UnicodeWidthStr;
        let buffer = terminal.backend().buffer();
        let mut text = String::new();
        for y in 0..buffer.area.height {
          let mut x = 0;
          while x < buffer.area.width {
            let symbol = buffer[(x, y)].symbol();
            text.push_str(symbol);
            x += symbol.width().max(1) as u16;
          }
          text.push('\n');
        }
        let prefix = if back {
          "← ".to_string()
        } else {
          format!("{} ", super::super::util::FOLDER_ICON)
        };
        assert!(
          text.contains(&format!(
            "{prefix}{}",
            app.strings().status.infra.unnamed_folder
          )),
          "{text}"
        );
        assert!(text.contains(&format!("{prefix}Unnamed Folder")), "{text}");
      }
    }
  }
}
