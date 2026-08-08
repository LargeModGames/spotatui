//! The AI DJ screen: a conversation transcript above a prompt.
//!
//! ## Why the transcript is hand-wrapped
//!
//! `Paragraph::scroll` offsets **rendered** lines, so an offset clamped against
//! the *message* count drifts the moment any message wraps to more than one row —
//! you can scroll past the end, or fail to reach it. (That bug is latent in
//! `plugin_popup_scroll`.) Nothing in the repo combines wrapping with scrolling,
//! so this does what the help menu does: wrap to display rows ourselves, then
//! window those rows. Offset and rendered rows are then the same unit.
//!
//! The caret is a painted glyph rather than the terminal cursor, because
//! `runner.rs` only shows the hardware cursor for `ActiveBlock::Input` and then
//! unconditionally moves it to the search box.

use crate::core::app::{ActiveBlock, App};
use crate::infra::dj::setup::DjSetupStep;
use crate::infra::dj::DjSpeaker;
use crate::tui::event::Key;
use crate::tui::ui::popups::centered_modal_rect;
use crate::tui::ui::util::get_color;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;
use unicode_width::UnicodeWidthChar;

/// Rows reserved for the prompt box (one line of text plus its border).
const PROMPT_HEIGHT: u16 = 3;

/// Column the picker's notes start in, so they line up down the list.
const NOTE_COLUMN: usize = 25;

/// Indent for a note that has moved to a line of its own, clear of the number.
const NOTE_INDENT: usize = 5;

/// The width the picker modal *asks* for. What it gets is
/// [`centered_modal_rect`]'s clamp of this against the panel it is drawn in, which
/// is why nothing downstream may assume 70: a 30% sidebar on an 80-column terminal
/// leaves the DJ panel 56 columns and the modal 54.
const MODAL_WIDTH: u16 = 70;

pub fn draw_ai_dj(f: &mut Frame, app: &App, layout_chunk: Rect) {
  // No legend under the picker: the modal swallows every key it names.
  let mut hints = if app.dj.setup.is_some() {
    Vec::new()
  } else {
    hint_lines(app, layout_chunk.width as usize)
  };
  // The transcript's Min(3) wins the fight for rows on a short panel.
  hints.truncate(layout_chunk.height.saturating_sub(PROMPT_HEIGHT + 3) as usize);
  let chunks = Layout::default()
    .direction(Direction::Vertical)
    // Passed by value, not `.as_ref()`: with `palette` in the build (via the
    // audio-viz colour gradients) there are two applicable `AsRef` impls and the
    // call stops resolving. Matches how every other screen calls this.
    .constraints([
      Constraint::Min(3),
      Constraint::Length(hints.len() as u16),
      Constraint::Length(PROMPT_HEIGHT),
    ])
    .split(layout_chunk);

  draw_transcript(f, app, chunks[0]);
  if !hints.is_empty() {
    f.render_widget(Paragraph::new(hints), chunks[1]);
  }
  draw_prompt(f, app, chunks[2]);
  // Painted last so it covers both. Sized against `layout_chunk`, not `f.area()`:
  // AiDj is drawn inside `draw_main_layout`, so the sidebar and playbar stay on
  // screen around the overlay.
  if app.dj.setup.is_some() {
    draw_setup(f, app, layout_chunk);
  }
}

/// The panel title, which is where the DJ's modes are surfaced.
///
/// All of them matter enough to show: they change what the DJ does with every
/// request, and a mode you cannot see is a mode you forget you left on.
pub(crate) fn title(app: &App) -> String {
  let mut title = String::from("AI DJ");
  let mut modes: Vec<String> = Vec::new();
  // The brain is a mode like the others, and a more expensive one to leave set
  // wrong: `claude -p` spends a subscription quota per turn.
  if let Some(label) = crate::infra::dj::setup::active_label(&app.user_config.behavior) {
    modes.push(label);
  }
  if app.dj.auto_queue {
    modes.push("auto-queue on".to_string());
  }
  if app.dj.avoid_library {
    modes.push(
      if app.dj.library_indexing {
        "fresh only (indexing…)"
      } else {
        "fresh only"
      }
      .to_string(),
    );
  }
  if !modes.is_empty() {
    title.push_str(" — ");
    title.push_str(&modes.join(", "));
  }
  title
}

/// The keybind legend above the prompt.
///
/// The DJ's action keys are Ctrl combinations nobody can guess, and the panel
/// title only names a mode once it is already on. Built from the user's bindings,
/// so a rebind never leaves the legend naming a key that does nothing — and an
/// action rebound to a bare character is left out entirely, because the handler
/// only honours these keys while they carry a modifier: on a typing surface the
/// character types itself.
fn hint_lines(app: &App, width: usize) -> Vec<Line<'static>> {
  let keys = &app.user_config.keys;
  let mut parts: Vec<String> = Vec::new();
  for (key, action) in [
    (keys.dj_toggle_auto_queue, "auto-queue"),
    (keys.dj_vibe_shift, "vibe shift"),
    (keys.dj_toggle_fresh_only, "fresh only"),
    (keys.dj_pick_model, "AI/model"),
  ] {
    if !matches!(key, Key::Char(_)) {
      parts.push(format!("{key} {action}"));
    }
  }
  parts.push("↑↓ scroll".to_string());
  parts.push("Enter send".to_string());
  parts.push("Esc clear/back".to_string());

  // Wrapped rather than truncated, the same way the picker treats its footer: on
  // a narrow panel the tail is the half that would be lost. Packed by whole
  // parts, not words — "<Ctrl+g>" on one row and its "AI/model" on the next is a
  // legend that reads as two broken entries.
  let width = width.max(1);
  let mut rows: Vec<String> = Vec::new();
  let mut current = String::new();
  let mut current_width = 0usize;
  for part in parts {
    let part_width: usize = part.chars().map(|c| c.width().unwrap_or(0)).sum();
    if part_width > width {
      // A part wider than the panel cannot stay atomic; break it like a long word.
      if !current.is_empty() {
        rows.push(std::mem::take(&mut current));
        current_width = 0;
      }
      rows.extend(wrap_text(&part, width));
      continue;
    }
    let sep_width = if current.is_empty() { 0 } else { 3 };
    if current_width + sep_width + part_width > width && !current.is_empty() {
      rows.push(std::mem::take(&mut current));
      current_width = 0;
    }
    if !current.is_empty() {
      current.push_str(" · ");
      current_width += 3;
    }
    current.push_str(&part);
    current_width += part_width;
  }
  if !current.is_empty() {
    rows.push(current);
  }

  let style = Style::default().fg(app.user_config.theme.hint);
  rows
    .into_iter()
    .map(|line| Line::from(Span::styled(line, style)))
    .collect()
}

fn draw_transcript(f: &mut Frame, app: &App, area: Rect) {
  let highlight_state = (
    app.get_current_route().active_block == ActiveBlock::AiDj,
    app.get_current_route().hovered_block == ActiveBlock::AiDj,
  );
  let block = Block::default()
    .title(Span::styled(
      title(app),
      get_color(highlight_state, app.user_config.theme),
    ))
    .borders(Borders::ALL)
    .border_style(get_color(highlight_state, app.user_config.theme));

  let inner = block.inner(area);
  f.render_widget(block, area);
  if inner.width == 0 || inner.height == 0 {
    return;
  }

  let rows = wrap_transcript(app, inner.width as usize);
  if rows.is_empty() {
    f.render_widget(Paragraph::new(empty_hint(app)), inner);
    return;
  }

  // Window the wrapped rows. `scroll` counts from the bottom so the newest lines
  // stay visible as the conversation grows, which is what a chat should do.
  let visible = inner.height as usize;
  let max_offset = rows.len().saturating_sub(visible);
  let offset = max_offset.saturating_sub(app.dj.scroll as usize);
  let end = (offset + visible).min(rows.len());
  let window = rows[offset..end].to_vec();

  f.render_widget(Paragraph::new(window), inner);
}

fn empty_hint(app: &App) -> Vec<Line<'static>> {
  let theme = app.user_config.theme;
  let mut lines = vec![
    Line::from(Span::styled(
      "Ask the DJ for something, or turn auto-queue on.",
      Style::default().fg(theme.text),
    )),
    Line::from(""),
    Line::from(Span::styled(
      "\"something chill for focusing\"",
      Style::default().fg(theme.inactive),
    )),
    Line::from(Span::styled(
      "\"more like this but slower\"",
      Style::default().fg(theme.inactive),
    )),
    Line::from(""),
  ];

  // Which brain is in use is the setting that costs money or quota per turn, so
  // both the answer and the way to change it belong on the screen that spends it.
  // Two lines rather than one sentence: together they overflow a narrow panel, and
  // these are rendered unwrapped.
  if let Some(label) = crate::infra::dj::setup::active_label(&app.user_config.behavior) {
    lines.push(Line::from(Span::styled(
      format!("Using {label}."),
      Style::default().fg(theme.inactive),
    )));
  }
  lines.push(Line::from(Span::styled(
    format!(
      "{} to change which AI and model the DJ uses.",
      app.user_config.keys.dj_pick_model
    ),
    Style::default().fg(theme.inactive),
  )));
  lines
}

/// An upper bound on how far back the transcript can usefully be scrolled.
///
/// The exact bound needs the rendered width, which the handler does not have, so
/// this wraps at a deliberately narrow one: narrower produces *more* rows, so the
/// answer is never below the true maximum and `draw_transcript`'s own clamp still
/// picks the final offset. What it buys is a bound proportional to the
/// conversation rather than none at all — unbounded, holding the scroll key on a
/// three-line transcript takes hundreds of presses before anything moves back.
pub(crate) fn max_scroll_bound(app: &App) -> u16 {
  const NARROWEST_PLAUSIBLE_WIDTH: usize = 24;
  u16::try_from(wrap_transcript(app, NARROWEST_PLAUSIBLE_WIDTH).len()).unwrap_or(u16::MAX)
}

/// Turn the transcript into display rows, hard-wrapped to `width`.
///
/// Public for the tests, which assert that scrolling and wrapping stay in the
/// same unit.
pub(crate) fn wrap_transcript(app: &App, width: usize) -> Vec<Line<'static>> {
  let theme = app.user_config.theme;
  let mut rows: Vec<Line<'static>> = Vec::new();

  for line in &app.dj.transcript {
    let (prefix, style) = match line.speaker {
      DjSpeaker::User => ("you  ", Style::default().fg(theme.active)),
      DjSpeaker::Dj => ("dj   ", Style::default().fg(theme.text)),
      DjSpeaker::System => ("     ", Style::default().fg(theme.inactive)),
    };
    let body_width = width.saturating_sub(prefix.len()).max(1);
    for (index, chunk) in wrap_text(&line.text, body_width).into_iter().enumerate() {
      // Only the first row of a message carries the speaker label; continuation
      // rows are indented to line up under it.
      let label = if index == 0 {
        prefix.to_string()
      } else {
        " ".repeat(prefix.len())
      };
      rows.push(Line::from(vec![
        Span::styled(label, Style::default().fg(theme.inactive)),
        Span::styled(chunk, style),
      ]));
    }
  }

  if app.dj.thinking {
    // The step counter is what tells a minutes-long agent-CLI turn apart from a
    // hang: each step there is a fresh subprocess.
    let label = match app.dj.step {
      Some((step, of)) => format!("dj   …thinking ({step}/{of})"),
      None => "dj   …thinking".to_string(),
    };
    rows.push(Line::from(Span::styled(
      label,
      Style::default()
        .fg(theme.inactive)
        .add_modifier(Modifier::ITALIC),
    )));
  }
  rows
}

/// Wrap on word boundaries, falling back to a hard break for a word longer than
/// the line. Uses display width, so CJK and emoji do not overflow.
fn wrap_text(text: &str, width: usize) -> Vec<String> {
  if width == 0 {
    return vec![text.to_string()];
  }
  let mut lines = Vec::new();
  let mut current = String::new();
  let mut current_width = 0usize;

  for word in text.split_whitespace() {
    let word_width: usize = word.chars().map(|c| c.width().unwrap_or(0)).sum();
    let space = if current.is_empty() { 0 } else { 1 };
    if current_width + space + word_width > width && !current.is_empty() {
      lines.push(std::mem::take(&mut current));
      current_width = 0;
    }
    if word_width > width {
      // A single word longer than the line: break it by display width.
      if !current.is_empty() {
        lines.push(std::mem::take(&mut current));
        current_width = 0;
      }
      let mut piece = String::new();
      let mut piece_width = 0usize;
      for ch in word.chars() {
        let ch_width = ch.width().unwrap_or(0);
        if piece_width + ch_width > width && !piece.is_empty() {
          lines.push(std::mem::take(&mut piece));
          piece_width = 0;
        }
        piece.push(ch);
        piece_width += ch_width;
      }
      if !piece.is_empty() {
        current = piece;
        current_width = piece_width;
      }
      continue;
    }
    if !current.is_empty() {
      current.push(' ');
      current_width += 1;
    }
    current.push_str(word);
    current_width += word_width;
  }
  if !current.is_empty() {
    lines.push(current);
  }
  if lines.is_empty() {
    lines.push(String::new());
  }
  lines
}

/// One picker row: `"› 1. claude (no API key)  uses your Claude Pro/Max plan"`.
///
/// Only the first nine rows are numbered, because only those have a digit shortcut —
/// a "10." that does nothing would be a promise the keymap does not keep.
///
/// `inner_width` is the modal's real inside width, measured from the rect
/// [`centered_modal_rect`] returned rather than from the width it was asked for:
/// the two differ on any panel narrower than 72 columns, and the whole decision
/// below is "does this fit". The note says what the row costs — "uses your Claude
/// Pro/Max plan", "$5/$25 per MTok" — so it is never the half that gets dropped:
/// it moves to a line of its own, wrapped, rather than being truncated.
fn option_lines(
  app: &App,
  index: usize,
  cursor: usize,
  label: &str,
  note: &str,
  ready: bool,
  inner_width: usize,
) -> Vec<Line<'static>> {
  let theme = app.user_config.theme;
  let selected = index == cursor;
  let number = if index < 9 {
    format!("{}. ", index + 1)
  } else {
    "   ".to_string()
  };
  let style = if selected {
    Style::default()
      .fg(theme.active)
      .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD))
  } else if ready {
    Style::default().fg(theme.text)
  } else {
    // Dimmed but still selectable: "I am about to install it" is a real answer.
    Style::default().fg(theme.inactive)
  };
  let note_style = Style::default().fg(theme.inactive);
  // Pad the label out to the note column so the notes line up down the list, but
  // never past the modal's own edge: on a narrow modal the padding is what would
  // push the label itself into truncation.
  let gutter = 2 + number.chars().count();
  let pad = NOTE_COLUMN.min(inner_width.saturating_sub(gutter + 1));
  let head = format!(
    "{}{number}{label:<pad$} ",
    if selected { "› " } else { "  " }
  );

  if note.is_empty() {
    return vec![Line::from(Span::styled(head, style))];
  }
  if head.chars().count() + note.chars().count() <= inner_width {
    return vec![Line::from(vec![
      Span::styled(head, style),
      Span::styled(note.to_string(), note_style),
    ])];
  }
  let mut lines = vec![Line::from(Span::styled(head, style))];
  // Wrapped, not just moved: `openai_compat`'s note is 55 columns, which is longer
  // than the inside of the modal a narrow panel leaves.
  for chunk in wrap_text(note, inner_width.saturating_sub(NOTE_INDENT).max(1)) {
    lines.push(Line::from(Span::styled(
      format!("{}{chunk}", " ".repeat(NOTE_INDENT)),
      note_style,
    )));
  }
  lines
}

/// The backend/model picker.
///
/// Drawn from inside [`draw_ai_dj`] rather than at the `runner.rs` popup level so the
/// existing `TestBackend` harness, which calls `draw_ai_dj` directly, can assert on
/// it.
fn draw_setup(f: &mut Frame, app: &App, layout_chunk: Rect) {
  let Some(setup) = app.dj.setup.as_ref() else {
    return;
  };
  let theme = app.user_config.theme;
  let hint_style = Style::default().fg(theme.hint);

  // Two passes over `centered_modal_rect`, because each axis depends on the other:
  // the width decides how many lines a row takes (a note that does not fit moves to
  // a line of its own) and the line count decides the height. The helper clamps the
  // two axes independently, so the width this first call reports is already the
  // width the final rect has; only the vertical placement is still open.
  let inner_width = centered_modal_rect(layout_chunk, MODAL_WIDTH, layout_chunk.height)
    .width
    .saturating_sub(2) as usize;

  let (title, entries, cursor) = match setup.step {
    DjSetupStep::Backend => (
      "Set up the AI DJ",
      setup
        .backends
        .iter()
        .map(|row| (row.label.as_str(), row.note.as_str(), row.ready))
        .collect::<Vec<_>>(),
      setup.backend_index,
    ),
    DjSetupStep::Model => (
      "Choose a model",
      setup
        .models
        .iter()
        .map(|row| (row.label.as_str(), row.note.as_str(), true))
        .collect(),
      setup.model_index,
    ),
    // A typing surface, not a list: one row, and nothing to scroll through.
    DjSetupStep::Custom => ("Model name", Vec::new(), 0),
  };

  // Rows, plus where each one starts and ends. The viewport below scrolls by whole
  // rows, which it can only do if it knows which lines belong to which row.
  let mut rows: Vec<Line<'static>> = Vec::new();
  let mut row_spans: Vec<(usize, usize)> = Vec::new();
  for (index, (label, note, ready)) in entries.iter().enumerate() {
    let start = rows.len();
    rows.extend(option_lines(
      app,
      index,
      cursor,
      label,
      note,
      *ready,
      inner_width,
    ));
    row_spans.push((start, rows.len()));
  }
  if setup.step == DjSetupStep::Custom {
    rows.push(Line::from(vec![
      Span::styled(
        format!("  {}", setup.custom),
        Style::default().fg(theme.text),
      ),
      // A painted caret, the same idiom `draw_prompt` uses: the hardware cursor is
      // owned by the search box.
      Span::styled("▏", Style::default().fg(theme.active)),
    ]));
    row_spans.push((0, rows.len()));
  }

  let mut hints = vec![match setup.step {
    // Esc here keeps whatever is already configured, and says so: a modal whose
    // escape hatch silently rewrote config would be worse than no modal.
    DjSetupStep::Backend => {
      "↑↓ select · 1-9 jump · Enter choose · Esc keep the current brain".to_string()
    }
    // Built from the rows rather than fixed, because the model step is not one
    // shape. A command spotatui does not own gets a single row and deliberately no
    // free-text row (`models_for`), so a hardcoded "last row types a name" would
    // point at a row that is not there, and "↑↓ select" at a list of one.
    DjSetupStep::Model => {
      let mut parts = Vec::new();
      if setup.models.len() > 1 {
        parts.push("↑↓ select");
      }
      parts.push("Enter choose");
      parts.push("Esc back");
      if setup.models.last().is_some_and(|model| model.custom) {
        parts.push("last row types a name");
      }
      parts.join(" · ")
    }
    DjSetupStep::Custom => "Enter to use it · Esc back".to_string(),
  }];
  if setup.step == DjSetupStep::Custom {
    if let Some(row) = setup.selected_backend() {
      // The bare name, not the row's label: "for claude (no API key)" reads as if the
      // key were the subject when the subject is the model being typed.
      let name = row.agent.unwrap_or(row.backend);
      hints.push(match (row.backend, row.agent) {
        // The picker deliberately does not edit the URL, so the one thing a
        // self-hosted user still has to do belongs on this screen.
        ("openai_compat", _) => format!("for {name} · set behavior.dj_base_url for the endpoint"),
        // The Custom… row carries this too, but that row is gone by the time
        // anyone is typing here, which is when the format actually matters.
        (_, Some("opencode")) => format!("for {name} · as provider/model"),
        _ => format!("for {name}"),
      });
    }
  }
  // Wrapped for the same reason a note is: the hint is the modal's only statement
  // of what Esc does, and "Esc keep the c" is not that statement.
  let mut footer: Vec<Line<'static>> = hints
    .iter()
    .flat_map(|hint| wrap_text(hint, inner_width.max(1)))
    .map(|line| Line::from(Span::styled(line, hint_style)))
    .collect();

  // Rows, a blank separator, the footer, and the two borders. The separator is
  // asked for rather than emitted, so it is the first thing a short panel drops.
  let requested_height = u16::try_from(rows.len() + footer.len() + 3).unwrap_or(u16::MAX);
  let rect = centered_modal_rect(layout_chunk, MODAL_WIDTH, requested_height);
  f.render_widget(Clear, rect);
  let block = Block::default()
    .title(Span::styled(
      title,
      Style::default()
        .fg(theme.header)
        .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
    ))
    .borders(Borders::ALL)
    .style(theme.base_style())
    .border_style(Style::default().fg(theme.active));
  let inner = block.inner(rect);
  f.render_widget(block, rect);
  if inner.width == 0 || inner.height == 0 {
    return;
  }

  // `centered_modal_rect` clamps the height it was asked for, so the list does not
  // always fit. The footer is pinned to the bottom rather than scrolling with the
  // rows: "Esc keeps the current brain" is exactly what a user who cannot see the
  // whole list needs to read, and a footer that scrolls away is gone when it
  // matters. The rows still win a fight for the last line, because a modal with no
  // visible row is no modal at all.
  let inner_height = inner.height as usize;
  let mut footer_height = footer.len().min(inner_height.saturating_sub(1));
  footer.truncate(footer_height);
  let mut list_height = inner_height - footer_height;
  let clipped = rows.len() > list_height && list_height > 1;
  if clipped {
    // Spend one of the list's own rows saying there is more of it, so a user whose
    // cursor walks off the bottom knows it is scrolling rather than stuck.
    list_height -= 1;
    footer_height += 1;
  }

  // Scroll just far enough to bring the selected row's last line into view, and
  // never past its first: on a row taller than the window, the head is the half
  // that carries the label and the highlight.
  let (cursor_start, cursor_end) = row_spans.get(cursor).copied().unwrap_or((0, 0));
  let offset = cursor_end.saturating_sub(list_height).min(cursor_start);
  let end = (offset + list_height).min(rows.len());
  if clipped {
    let above = row_spans
      .iter()
      .filter(|(start, _)| *start < offset)
      .count();
    let below = row_spans.iter().filter(|(_, stop)| *stop > end).count();
    footer.insert(
      0,
      Line::from(Span::styled(
        match (above, below) {
          (0, below) => format!("… {below} more below"),
          (above, 0) => format!("… {above} more above"),
          (above, below) => format!("… {above} above, {below} below"),
        },
        hint_style,
      )),
    );
  }

  // Windowed rather than `Paragraph::scroll`, for the reason this module's header
  // gives: offset and rendered rows stay the same unit.
  f.render_widget(
    Paragraph::new(rows[offset..end].to_vec()),
    Rect {
      height: list_height as u16,
      ..inner
    },
  );
  if footer_height > 0 {
    f.render_widget(
      Paragraph::new(footer),
      Rect {
        y: inner.y + list_height as u16,
        height: footer_height as u16,
        ..inner
      },
    );
  }
}

fn draw_prompt(f: &mut Frame, app: &App, area: Rect) {
  let focused = app.get_current_route().active_block == ActiveBlock::AiDj;
  let theme = app.user_config.theme;
  let block = Block::default()
    .title(Span::styled(
      "Ask the DJ",
      get_color((focused, false), theme),
    ))
    .borders(Borders::ALL)
    .border_style(get_color((focused, false), theme));

  let inner = block.inner(area);
  f.render_widget(block, area);
  if inner.width == 0 {
    return;
  }

  let typed: String = app.dj.input.iter().collect();
  // Keep the caret in view on a prompt longer than the box, the same way the
  // search input does.
  let cursor = app.dj.input_cursor as usize;
  let scroll = cursor.saturating_sub(inner.width.saturating_sub(1) as usize);
  let visible: String = typed.chars().skip(scroll).collect();

  let mut spans = vec![Span::styled(visible, Style::default().fg(theme.text))];
  if focused {
    // A painted caret: the hardware cursor is owned by the search box.
    spans.push(Span::styled("▏", Style::default().fg(theme.active)));
  }
  f.render_widget(Paragraph::new(Line::from(spans)), inner);
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::RouteId;
  use crate::infra::dj::DjLine;
  use ratatui::backend::TestBackend;
  use ratatui::layout::Size;
  use ratatui::Terminal;

  fn app_with(lines: Vec<DjLine>, width: u16, height: u16) -> App {
    let mut app = App::default();
    app.size = Size { width, height };
    app.dj.transcript = lines;
    app.push_navigation_stack(RouteId::AiDj, ActiveBlock::AiDj);
    app
  }

  fn render(app: &App, width: u16, height: u16) -> Vec<String> {
    render_in(app, width, height, Rect::new(0, 0, width, height))
  }

  /// Render into a `chunk` that is smaller than the terminal, the way the real
  /// layout does: the sidebar takes columns off the left and the prompt plus the
  /// playbar take rows off the bottom. The picker is sized against that chunk, so
  /// a modal that only fits "the terminal" is not a modal that fits.
  fn render_in(app: &App, width: u16, height: u16, chunk: Rect) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|f| draw_ai_dj(f, app, chunk)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..height)
      .map(|y| {
        (0..width)
          .map(|x| buffer.cell((x, y)).map_or(" ", |c| c.symbol()).to_string())
          .collect::<String>()
      })
      .collect()
  }

  #[test]
  fn wrapping_produces_more_rows_than_messages_for_long_text() {
    // The core reason we wrap ourselves: rows != messages, so a scroll offset
    // clamped against message count would be wrong.
    let long = "word ".repeat(40);
    let app = app_with(vec![DjLine::dj(long)], 40, 20);
    let rows = wrap_transcript(&app, 30);
    assert!(rows.len() > 1, "a long message must occupy several rows");
  }

  #[test]
  fn only_the_first_row_of_a_message_is_labelled() {
    let app = app_with(vec![DjLine::user("alpha beta gamma delta epsilon")], 40, 20);
    let rows = wrap_transcript(&app, 20);
    assert!(rows.len() >= 2);
    let first: String = rows[0].spans.iter().map(|s| s.content.clone()).collect();
    let second: String = rows[1].spans.iter().map(|s| s.content.clone()).collect();
    assert!(first.starts_with("you"));
    assert!(
      second.starts_with("    "),
      "continuation rows indent under the label, got {second:?}"
    );
  }

  #[test]
  fn a_word_longer_than_the_line_is_broken_rather_than_overflowing() {
    let rows = wrap_text(&"x".repeat(50), 10);
    assert!(rows.len() >= 5);
    assert!(rows.iter().all(|row| row.chars().count() <= 10));
  }

  #[test]
  fn wide_characters_are_measured_by_display_width() {
    // Each CJK glyph is two columns wide, so eight of them cannot share a
    // ten-column line with much else.
    let rows = wrap_text("東京東京東京東京 tail", 10);
    assert!(rows.len() >= 2, "got {rows:?}");
  }

  #[test]
  fn the_newest_lines_are_visible_by_default() {
    let lines: Vec<_> = (0..40).map(|i| DjLine::dj(format!("line {i}"))).collect();
    let app = app_with(lines, 40, 12);
    let rendered = render(&app, 40, 12).join("\n");
    assert!(rendered.contains("line 39"), "{rendered}");
    assert!(!rendered.contains("line 0 "), "{rendered}");
  }

  #[test]
  fn scrolling_back_reveals_older_lines() {
    let lines: Vec<_> = (0..40).map(|i| DjLine::dj(format!("line {i}"))).collect();
    let mut app = app_with(lines, 40, 12);
    app.dj.scroll = 20;
    let rendered = render(&app, 40, 12).join("\n");
    assert!(rendered.contains("line 1"), "{rendered}");
    assert!(!rendered.contains("line 39"), "{rendered}");
  }

  #[test]
  fn the_title_shows_both_modes_and_the_indexing_wait() {
    let mut app = app_with(vec![], 40, 12);

    app.dj.avoid_library = true;
    app.dj.library_indexing = true;
    assert!(
      title(&app).contains("indexing"),
      "the first crawl takes seconds; silence looks like a hang"
    );

    app.dj.library_indexing = false;
    assert!(title(&app).contains("fresh only"));

    app.dj.auto_queue = true;
    let both = title(&app);
    assert!(both.contains("auto-queue on") && both.contains("fresh only"));
  }

  #[test]
  fn the_title_names_the_active_backend() {
    // The brain is the mode that costs money or quota per turn, so it is the one
    // that must never be invisible. `UserConfig::new()` ships `["claude", "-p"]`.
    let mut app = app_with(vec![], 40, 12);
    assert_eq!(title(&app), "AI DJ — claude");

    app.user_config.behavior.dj_agent_model = Some("haiku".to_string());
    assert_eq!(title(&app), "AI DJ — claude/haiku");

    app.dj.avoid_library = true;
    assert_eq!(title(&app), "AI DJ — claude/haiku, fresh only");
  }

  #[test]
  fn an_empty_transcript_shows_a_hint() {
    // 14 rather than 12 rows: the legend above the prompt takes two at this width,
    // and this test is about the transcript hint underneath it.
    let app = app_with(vec![], 70, 14);
    let rendered = render(&app, 70, 14).join("\n");
    assert!(rendered.contains("Ask the DJ"), "{rendered}");
    // "which AI is it using" has to be answerable without pressing anything.
    assert!(rendered.contains("Using claude"), "{rendered}");
    // Built from the binding rather than hardcoded, so a rebind does not silently
    // leave the hint naming a key that does nothing.
    assert!(
      rendered.contains(&format!(
        "{} to change which AI",
        app.user_config.keys.dj_pick_model
      )),
      "{rendered}"
    );
  }

  #[test]
  fn the_keybind_legend_sits_above_the_prompt_and_follows_rebinds() {
    let mut app = app_with(vec![DjLine::dj("hello")], 80, 24);
    let rendered = render(&app, 80, 24).join("\n");
    let keys = app.user_config.keys.clone();
    for (key, action) in [
      (keys.dj_toggle_auto_queue, "auto-queue"),
      (keys.dj_vibe_shift, "vibe shift"),
      (keys.dj_toggle_fresh_only, "fresh only"),
      (keys.dj_pick_model, "AI/model"),
    ] {
      assert!(rendered.contains(&format!("{key} {action}")), "{rendered}");
    }
    assert!(rendered.contains("Enter send"), "{rendered}");

    app.user_config.keys.dj_vibe_shift = Key::Ctrl('r');
    let rendered = render(&app, 80, 24).join("\n");
    assert!(rendered.contains("<Ctrl+r> vibe shift"), "{rendered}");
    assert!(!rendered.contains("<Ctrl+y> vibe shift"), "{rendered}");
  }

  #[test]
  fn an_action_rebound_to_a_bare_character_is_left_out_of_the_legend() {
    // The handler only honours these keys while they carry a modifier; a bare 'x'
    // types itself. A legend entry for it would name a key that does nothing.
    let mut app = app_with(vec![DjLine::dj("hello")], 80, 24);
    app.user_config.keys.dj_toggle_fresh_only = Key::Char('x');
    let rendered = render(&app, 80, 24).join("\n");
    assert!(!rendered.contains("fresh only"), "{rendered}");
    assert!(
      rendered.contains("auto-queue"),
      "the others stay: {rendered}"
    );
  }

  #[test]
  fn the_legend_hides_while_the_picker_is_open() {
    // The modal swallows every key the legend names, so showing it underneath
    // would be a row of promises none of which hold.
    let mut app = app_with_picker(80, 24);
    let rendered = render(&app, 80, 24).join("\n");
    assert!(!rendered.contains("auto-queue"), "{rendered}");

    app.dj.close_setup();
    let rendered = render(&app, 80, 24).join("\n");
    assert!(
      rendered.contains("auto-queue"),
      "and returns after: {rendered}"
    );
  }

  #[test]
  fn a_narrow_panel_wraps_the_legend_rather_than_truncating_it() {
    let app = app_with(vec![DjLine::dj("hi")], 46, 20);
    let rendered = render(&app, 46, 20).join("\n");
    // The tail is the half a truncating panel loses.
    assert!(rendered.contains("Esc clear/back"), "{rendered}");
    // And each key stays on the same row as its label: the legend packs whole
    // entries, so a wrap never leaves "<Ctrl+g>" pointing at nothing.
    assert!(
      rendered.contains(&format!(
        "{} fresh only",
        app.user_config.keys.dj_toggle_fresh_only
      )),
      "{rendered}"
    );
  }

  /// The DJ screen with the picker open on its first step.
  fn app_with_picker(width: u16, height: u16) -> App {
    let mut app = app_with(vec![DjLine::dj("older conversation")], width, height);
    app.dj.setup = Some(crate::infra::dj::setup::DjSetup::new(
      &app.user_config.behavior,
    ));
    app
  }

  #[test]
  fn the_picker_paints_over_the_transcript() {
    // Enough transcript to occupy every row of the panel, so "the modal covers what
    // is behind it" is actually observable rather than accidentally true.
    let lines: Vec<_> = (0..40).map(|_| DjLine::dj("haystack")).collect();
    let mut app = app_with(lines, 80, 24);
    let bare = render(&app, 80, 24)
      .iter()
      .filter(|row| row.contains("haystack"))
      .count();

    app.dj.setup = Some(crate::infra::dj::setup::DjSetup::new(
      &app.user_config.behavior,
    ));
    let rows = render(&app, 80, 24);
    let rendered = rows.join("\n");
    assert!(rendered.contains("Set up the AI DJ"), "{rendered}");
    // The row for the shipped default, and what it bills against — the whole reason
    // the picker exists rather than a bare model field.
    assert!(rendered.contains("claude"), "{rendered}");
    assert!(rendered.contains("Pro/Max"), "{rendered}");
    let covered = rows.iter().filter(|row| row.contains("haystack")).count();
    assert!(
      covered < bare,
      "the modal must clear the rows it covers: {rendered}"
    );
  }

  #[test]
  fn the_model_step_shows_the_price_next_to_each_model() {
    let mut app = app_with_picker(80, 24);
    {
      let setup = app.dj.setup.as_mut().unwrap();
      setup.backend_index = setup
        .backends
        .iter()
        .position(|row| row.backend == "anthropic")
        .unwrap();
    }
    let behavior = app.user_config.behavior.clone();
    app.dj.setup.as_mut().unwrap().enter_model_step(&behavior);

    let rendered = render(&app, 80, 24).join("\n");
    assert!(rendered.contains("Choose a model"), "{rendered}");
    assert!(rendered.contains("claude-haiku-4-5"), "{rendered}");
    // A model list without prices is as opaque as the config field it replaces.
    assert!(rendered.contains("$1/$5"), "{rendered}");
    assert!(rendered.contains("Custom"), "{rendered}");
  }

  #[test]
  fn the_model_step_hint_does_not_promise_a_row_that_is_not_there() {
    // The one model step that is not a list: a command spotatui does not own gets a
    // single row and deliberately no free-text row, because a model named there
    // would never reach the CLI. A fixed hint would send the user arrowing down to
    // a "Custom…" row that does not exist, on the one screen whose entire job is to
    // stop the picker misreporting what is configured.
    let mut app = app_with_picker(80, 24);
    app.user_config.behavior.dj_agent_command =
      vec!["my-agent".to_string(), "--headless".to_string()];
    let behavior = app.user_config.behavior.clone();
    {
      let setup = app.dj.setup.as_mut().unwrap();
      *setup = crate::infra::dj::setup::DjSetup::new(&behavior);
      setup.enter_model_step(&behavior);
    }

    let rendered = render(&app, 80, 24).join("\n");
    assert!(rendered.contains("Keep this command"), "{rendered}");
    assert!(!rendered.contains("types a name"), "{rendered}");
    // Nor "↑↓ select" over a list of one. Esc must still be offered: it is the only
    // way back to the backend step.
    assert!(!rendered.contains("↑↓"), "{rendered}");
    assert!(rendered.contains("Esc back"), "{rendered}");

    // The ordinary case still gets the full hint, so this is a branch rather than a
    // hint that quietly went missing for everybody.
    let listed = app_with_picker(80, 24);
    let behavior = listed.user_config.behavior.clone();
    let mut listed = listed;
    listed
      .dj
      .setup
      .as_mut()
      .unwrap()
      .enter_model_step(&behavior);
    let rendered = render(&listed, 80, 24).join("\n");
    assert!(rendered.contains("types a name"), "{rendered}");
  }

  #[test]
  fn the_custom_step_shows_the_prefilled_value() {
    let mut app = app_with_picker(80, 24);
    app.user_config.behavior.dj_agent_model = Some("sonnet".to_string());
    let behavior = app.user_config.behavior.clone();
    {
      let setup = app.dj.setup.as_mut().unwrap();
      setup.enter_model_step(&behavior);
      setup.enter_custom_step(&behavior);
    }
    let rendered = render(&app, 80, 24).join("\n");
    assert!(rendered.contains("Model name"), "{rendered}");
    assert!(rendered.contains("sonnet"), "{rendered}");
    // The caret is painted here too: the hardware cursor belongs to the search box.
    assert!(rendered.contains('▏'), "{rendered}");
  }

  #[test]
  fn a_narrow_panel_keeps_the_whole_cost_note() {
    // A geometry the app really produces: an 80-column terminal with
    // `behavior.sidebar_width_percentage: 30` (supported, clamped 0-50) leaves the
    // content panel 56 columns, which `centered_modal_rect` turns into a 54-column
    // modal with 52 columns inside — sixteen fewer than the 70 the picker asks for.
    let mut app = app_with_picker(80, 22);
    {
      let setup = app.dj.setup.as_mut().unwrap();
      setup.backends.retain(|row| row.agent == Some("claude"));
      let row = setup
        .backends
        .first_mut()
        .expect("claude is a recommended agent and is always offered");
      // Pinned rather than detected: `detect_backends` appends " · not on PATH" on a
      // machine without the CLI installed, and a note whose length depends on the
      // machine cannot pin a wrapping decision. This is the note the row carries
      // wherever `claude` is present.
      row.note = "uses your Claude Pro/Max plan".to_string();
      row.ready = true;
      setup.backend_index = 0;
    }
    let rendered = render_in(&app, 80, 22, Rect::new(24, 0, 56, 22)).join("\n");
    assert!(rendered.contains("Set up the AI DJ"), "{rendered}");
    // The note is the half of the row that says what it costs. On one line it is
    // 60 columns starting at column 0 of a 52-column inside, so it only survives
    // if the wrap decision is made against the rect that was actually returned.
    assert!(
      rendered.contains("uses your Claude Pro/Max plan"),
      "the cost note must survive a narrow panel: {rendered}"
    );
    // The hint is the modal's only statement of what Esc does, so it wraps onto a
    // second line here rather than stopping at "Esc keep the c". Asserted on the
    // tail, which is the half a truncating modal loses.
    assert!(
      rendered.contains("current brain"),
      "the hint must survive a narrow panel: {rendered}"
    );
  }

  #[test]
  fn a_row_never_drops_its_note_however_narrow_the_modal_is() {
    // The note is what the row costs, so it is the one part that may not be lost to
    // truncation. Asserted against widths passed in rather than against whatever the
    // terminal happens to be: the bug this replaces was a constant that disagreed
    // with the rect.
    let app = App::default();
    let label = "openai_compat (local or self-hosted)";
    let note = "any OpenAI-compatible endpoint, e.g. Ollama on localhost";
    for inner_width in [68usize, 52, 40, 30] {
      let lines = option_lines(&app, 0, 0, label, note, true, inner_width);
      let flat: String = lines
        .iter()
        .flat_map(|line| line.spans.iter())
        .map(|span| span.content.to_string())
        .collect::<Vec<_>>()
        .join(" ");
      for word in note.split_whitespace() {
        assert!(
          flat.contains(word),
          "{word:?} was dropped at inner width {inner_width}: {lines:?}"
        );
      }
      // The head can be wider than a very narrow modal — a 36-column label is 36
      // columns whatever the padding does — but the lines the note was moved to
      // exist precisely so it fits, so those must.
      for line in lines.iter().skip(1) {
        let width: usize = line
          .spans
          .iter()
          .flat_map(|span| span.content.chars())
          .map(|c| c.width().unwrap_or(0))
          .sum();
        assert!(
          width <= inner_width,
          "a note line of {width} columns in a {inner_width}-column modal: {line:?}"
        );
      }
    }
  }

  #[test]
  fn the_note_column_never_pads_a_row_past_the_modal_edge() {
    // The padding that lines the notes up is itself capable of pushing a short label
    // off the right edge of a narrow modal, so it yields before the label does.
    let app = App::default();
    let lines = option_lines(&app, 0, 0, "agy", "cheapest", true, 20);
    let head: String = lines[0].spans.iter().map(|s| s.content.clone()).collect();
    assert!(
      head.chars().count() <= 20,
      "the head is padded past a 20-column modal: {head:?}"
    );

    // And it yields only where it must: on a modal with room for the note column,
    // the notes still line up at it. Without this the clamp above could be
    // satisfied by dropping the alignment altogether.
    let wide = option_lines(&app, 0, 0, "agy", "cheapest", true, 68);
    let head: String = wide[0].spans[0].content.to_string();
    assert_eq!(
      head.chars().count(),
      // Two gutter columns, "1. ", the padded label, one space.
      2 + 3 + NOTE_COLUMN + 1,
      "notes must still line up on a modal that fits them: {head:?}"
    );
  }

  #[test]
  fn the_selected_row_stays_on_screen_when_the_list_is_taller_than_the_modal() {
    let mut app = app_with_picker(80, 22);
    let behavior = app.user_config.behavior.clone();
    {
      let setup = app.dj.setup.as_mut().unwrap();
      // `agy` has the longest model list, and it is offered whether or not the
      // binary is installed, so the row count here does not depend on the machine.
      setup.backend_index = setup
        .backends
        .iter()
        .position(|row| row.agent == Some("agy"))
        .expect("agy is a recommended agent and is always offered");
      setup.enter_model_step(&behavior);
      // The last row, read off the list rather than counted: rows get added to
      // `models_for` and a hard-coded index would quietly stop being the last one.
      setup.model_index = setup.models.len() - 1;
    }
    let expected = app
      .dj
      .setup
      .as_ref()
      .unwrap()
      .models
      .last()
      .unwrap()
      .label
      .clone();

    // 22 rows, less the 3-row prompt and the 6-row playbar, leaves the DJ panel 13.
    let rows = render_in(&app, 80, 22, Rect::new(0, 0, 80, 13));
    let cursor_row = rows
      .iter()
      .find(|row| row.contains('›'))
      .unwrap_or_else(|| panic!("the highlighted row was never drawn:\n{}", rows.join("\n")));
    assert!(
      cursor_row.contains(&expected),
      "the cursor is on {expected:?} but the visible highlight is {cursor_row:?}:\n{}",
      rows.join("\n")
    );

    let rendered = rows.join("\n");
    // Scrolled past rows are still accounted for, and the hint is pinned rather than
    // scrolled off: a list the user cannot see the whole of is exactly when "Esc
    // back" needs to be readable.
    assert!(
      rendered.contains("more above"),
      "hidden rows must be announced: {rendered}"
    );
    assert!(
      rendered.contains("Esc back"),
      "the hint must survive scrolling: {rendered}"
    );
  }

  #[test]
  fn thinking_is_shown_while_a_turn_is_in_flight() {
    let mut app = app_with(vec![DjLine::user("chill")], 40, 12);
    app.dj.thinking = true;
    let rows = wrap_transcript(&app, 30);
    let last: String = rows
      .last()
      .unwrap()
      .spans
      .iter()
      .map(|s| s.content.clone())
      .collect();
    assert!(last.contains("thinking"), "{last}");
  }

  #[test]
  fn the_step_counter_shows_progress_through_a_multi_step_turn() {
    // Each agent-CLI step is a fresh subprocess, so a four-step turn is minutes
    // of silence; without the counter that is indistinguishable from a hang.
    let mut app = app_with(vec![DjLine::user("chill")], 40, 12);
    app.dj.thinking = true;
    app.dj.step = Some((2, 4));
    let rendered = render(&app, 40, 12).join("\n");
    assert!(rendered.contains("(2/4)"), "{rendered}");
  }

  #[test]
  fn the_prompt_shows_typed_text_and_a_painted_caret() {
    let mut app = app_with(vec![], 40, 12);
    app.dj.input = "mellow".chars().collect();
    app.dj.input_cursor = 6;
    let rendered = render(&app, 40, 12).join("\n");
    assert!(rendered.contains("mellow"), "{rendered}");
    // The caret is drawn, not delegated to the terminal cursor.
    assert!(rendered.contains('▏'), "{rendered}");
  }

  #[test]
  fn a_long_prompt_scrolls_to_keep_the_caret_visible() {
    let mut app = app_with(vec![], 20, 12);
    let typed = "abcdefghijklmnopqrstuvwxyz";
    app.dj.input = typed.chars().collect();
    app.dj.input_cursor = typed.len() as u16;
    let rendered = render(&app, 20, 12).join("\n");
    // The tail is visible; the head has scrolled off.
    assert!(rendered.contains("xyz"), "{rendered}");
    assert!(!rendered.contains("abcdef"), "{rendered}");
  }

  #[test]
  fn a_zero_width_area_does_not_panic() {
    let app = app_with(vec![DjLine::dj("anything")], 1, 3);
    let _ = render(&app, 1, 3);
  }

  #[test]
  fn a_panel_far_too_small_for_the_picker_does_not_panic() {
    // The viewport maths slices the row buffer, so every degenerate size has to be
    // a size that produces an empty window rather than an out-of-range range.
    for (width, height) in [(20u16, 6u16), (10, 4), (4, 3), (1, 1)] {
      let mut app = app_with_picker(width.max(1), height.max(1));
      // Deepest cursor there is, so the scroll offset is at its largest where there
      // is the least room for it.
      let behavior = app.user_config.behavior.clone();
      {
        let setup = app.dj.setup.as_mut().unwrap();
        setup.backend_index = setup.backends.len() - 1;
        setup.enter_model_step(&behavior);
        setup.model_index = setup.models.len() - 1;
      }
      let _ = render(&app, width, height);
    }
  }

  #[test]
  fn the_title_reflects_auto_queue() {
    let mut app = app_with(vec![], 40, 12);
    app.dj.auto_queue = true;
    let rendered = render(&app, 40, 12).join("\n");
    assert!(rendered.contains("auto-queue on"), "{rendered}");
  }
}
