use crate::core::app::{ActiveBlock, App};
use crate::tui::banner::BANNER;
use colorgrad::{self, Gradient};
use ratatui::{
  layout::{Constraint, Layout, Rect},
  style::{Color, Modifier, Style},
  text::{Line, Span, Text},
  widgets::{Block, BorderType, Borders, Paragraph, Wrap},
  Frame,
};
use std::sync::{Mutex, OnceLock};
use unicode_width::UnicodeWidthStr;

use super::util::get_color;

#[derive(Clone, PartialEq)]
struct ChangelogCacheKey {
  text: Color,
  hint: Color,
  inactive: Color,
  banner: Color,
  active: Color,
  changelog_width: u16,
}

impl ChangelogCacheKey {
  fn from_theme(theme: &crate::core::user_config::Theme, changelog_width: u16) -> Self {
    Self {
      text: theme.text,
      hint: theme.hint,
      inactive: theme.inactive,
      banner: theme.banner,
      active: theme.active,
      changelog_width,
    }
  }
}

struct ChangelogCache {
  key: ChangelogCacheKey,
  changelog_lines: Vec<Line<'static>>,
}

static CHANGELOG_CACHE: OnceLock<Mutex<ChangelogCache>> = OnceLock::new();
static CLEAN_CHANGELOG: OnceLock<String> = OnceLock::new();

pub fn draw_home(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let [banner_area, changelog_area] = layout_chunk
    .layout(&Layout::vertical([Constraint::Length(7), Constraint::Length(93)]).margin(2));

  let current_route = app.get_current_route();
  let highlight_state = (
    current_route.active_block == ActiveBlock::Home,
    current_route.hovered_block == ActiveBlock::Home,
  );

  let welcome = Block::default()
    .title(Span::styled(
      "Welcome!",
      get_color(highlight_state, app.user_config.theme),
    ))
    .style(app.user_config.theme.base_style())
    .borders(Borders::ALL)
    .border_type(BorderType::Rounded)
    .border_style(get_color(highlight_state, app.user_config.theme));
  f.render_widget(welcome, layout_chunk);

  // Banner gradient is recomputed each frame for animation
  let gradient_lines = build_banner_gradient_lines(&app.user_config.theme, app.animation_tick);
  let base_changelog_lines = get_changelog_cache(&app.user_config.theme, changelog_area.width);

  // Contains the banner
  let top_text = Paragraph::new(Text::from(gradient_lines))
    .style(app.user_config.theme.base_style())
    .block(Block::default());
  f.render_widget(top_text, banner_area);

  // Prepend global counter status to the changelog view
  let mut changelog_lines = Vec::with_capacity(base_changelog_lines.len() + 4);

  // Listening streak strip (cache read only; populated by the history collector)
  if let Some(streaks) = &app.listening_streaks {
    if streaks.current_days > 0 {
      let mut spans = vec![Span::styled(
        format!("{}-day listening streak", streaks.current_days),
        Style::default().fg(app.user_config.theme.active),
      )];
      if streaks.today_ms > 0 {
        spans.push(Span::styled(
          format!(
            " · {} today",
            crate::infra::history::format_duration(streaks.today_ms)
          ),
          Style::default().fg(app.user_config.theme.hint),
        ));
      } else {
        spans.push(Span::styled(
          " · listen today to keep it!",
          Style::default().fg(app.user_config.theme.hint),
        ));
      }
      changelog_lines.push(Line::from(spans));
    }
  }
  let counter_message = if cfg!(feature = "telemetry") {
    if app.user_config.behavior.enable_global_song_count {
      match app.global_song_count {
        Some(count) => format!("Global songs played with spotatui: {}", count),
        None if app.global_song_count_failed => {
          "Global song counter unavailable right now.".to_string()
        }
        None => "Loading global song count...".to_string(),
      }
    } else {
      "Global song counter disabled (Settings -> Behavior).".to_string()
    }
  } else {
    "Global song counter unavailable (telemetry disabled in this build).".to_string()
  };

  let counter_style = Style::default().fg(app.user_config.theme.hint);
  changelog_lines.push(Line::from(vec![Span::styled(
    counter_message,
    counter_style,
  )]));
  changelog_lines.push(Line::from(""));
  changelog_lines.extend(base_changelog_lines);

  // CHANGELOG
  let bottom_text = Paragraph::new(Text::from(changelog_lines))
    .block(Block::default())
    .style(app.user_config.theme.base_style())
    .wrap(Wrap { trim: false })
    .scroll((app.home_scroll, 0));
  f.render_widget(bottom_text, changelog_area);
}

fn get_clean_changelog() -> &'static str {
  CLEAN_CHANGELOG
    .get_or_init(|| {
      let changelog = include_str!("../../../CHANGELOG.md");
      if cfg!(debug_assertions) {
        changelog.to_string()
      } else {
        changelog.replace("\n## [Unreleased]\n", "")
      }
    })
    .as_str()
}

fn get_changelog_cache(
  theme: &crate::core::user_config::Theme,
  changelog_width: u16,
) -> Vec<Line<'static>> {
  let cache = CHANGELOG_CACHE.get_or_init(|| {
    let changelog = get_clean_changelog();
    let key = ChangelogCacheKey::from_theme(theme, changelog_width);
    Mutex::new(ChangelogCache {
      changelog_lines: build_changelog_lines(changelog, theme, changelog_width),
      key,
    })
  });
  let mut cache = cache.lock().expect("changelog cache lock failed");
  let key = ChangelogCacheKey::from_theme(theme, changelog_width);
  if cache.key != key {
    let changelog = get_clean_changelog();
    cache.changelog_lines = build_changelog_lines(changelog, theme, changelog_width);
    cache.key = key;
  }
  cache.changelog_lines.clone()
}

fn build_banner_gradient_lines(
  theme: &crate::core::user_config::Theme,
  animation_tick: u64,
) -> Vec<Line<'static>> {
  fn to_rgba(color: ratatui::style::Color) -> (u8, u8, u8, u8) {
    match color {
      ratatui::style::Color::Rgb(r, g, b) => (r, g, b, 255),
      ratatui::style::Color::Black => (0, 0, 0, 255),
      ratatui::style::Color::Red => (255, 0, 0, 255),
      ratatui::style::Color::Green => (0, 255, 0, 255),
      ratatui::style::Color::Yellow => (255, 255, 0, 255),
      ratatui::style::Color::Blue => (0, 0, 255, 255),
      ratatui::style::Color::Magenta => (255, 0, 255, 255),
      ratatui::style::Color::Cyan => (0, 255, 255, 255),
      ratatui::style::Color::Gray => (128, 128, 128, 255),
      ratatui::style::Color::DarkGray => (64, 64, 64, 255),
      ratatui::style::Color::LightRed => (255, 128, 128, 255),
      ratatui::style::Color::LightGreen => (128, 255, 128, 255),
      ratatui::style::Color::LightYellow => (255, 255, 128, 255),
      ratatui::style::Color::LightBlue => (128, 128, 255, 255),
      ratatui::style::Color::LightMagenta => (255, 128, 255, 255),
      ratatui::style::Color::LightCyan => (128, 255, 255, 255),
      ratatui::style::Color::White => (255, 255, 255, 255),
      _ => (255, 255, 255, 255),
    }
  }

  let c1 = to_rgba(theme.banner);
  let c2 = to_rgba(theme.hovered);
  let c3 = to_rgba(theme.active);

  // Build a looping gradient: banner → hovered → active → banner
  // This ensures a smooth wrap-around for continuous animation
  let grad = colorgrad::GradientBuilder::new()
    .colors(&[
      colorgrad::Color::from_rgba8(c1.0, c1.1, c1.2, c1.3),
      colorgrad::Color::from_rgba8(c2.0, c2.1, c2.2, c2.3),
      colorgrad::Color::from_rgba8(c3.0, c3.1, c3.2, c3.3),
      colorgrad::Color::from_rgba8(c1.0, c1.1, c1.2, c1.3),
    ])
    .build::<colorgrad::LinearGradient>()
    .unwrap();

  // Phase offset scrolls the gradient over time (~4 seconds per full cycle at 62 FPS)
  let phase = animation_tick as f64 * 0.004;

  BANNER
    .lines()
    .enumerate()
    .map(|(row, line)| {
      let chars: Vec<char> = line.chars().collect();
      let line_len = chars.len().max(1);
      let spans: Vec<Span<'static>> = chars
        .into_iter()
        .enumerate()
        .map(|(col, ch)| {
          // Diagonal gradient: combine column position and row offset
          let t = ((col as f64 / line_len as f64) + (row as f64 * 0.08) + phase) % 1.0;
          let [r, g, b, _] = grad.at(t as f32).to_rgba8();
          Span::styled(
            ch.to_string(),
            Style::default().fg(ratatui::style::Color::Rgb(r, g, b)),
          )
        })
        .collect();
      Line::from(spans)
    })
    .collect()
}

#[derive(Clone)]
struct StyledSegment {
  text: String,
  style: Style,
}

fn parse_markdown_inline(text: &str, base_style: Style) -> Vec<StyledSegment> {
  let mut segments: Vec<StyledSegment> = Vec::new();
  let mut buffer = String::new();
  let mut chars = text.chars().peekable();
  let mut is_bold = false;

  while let Some(ch) = chars.next() {
    if ch == '*' && chars.peek() == Some(&'*') {
      if !buffer.is_empty() {
        let style = if is_bold {
          base_style.add_modifier(Modifier::BOLD)
        } else {
          base_style
        };
        segments.push(StyledSegment {
          text: std::mem::take(&mut buffer),
          style,
        });
      }
      chars.next();
      is_bold = !is_bold;
    } else {
      buffer.push(ch);
    }
  }

  if !buffer.is_empty() {
    let style = if is_bold {
      base_style.add_modifier(Modifier::BOLD)
    } else {
      base_style
    };
    segments.push(StyledSegment {
      text: buffer,
      style,
    });
  }

  segments
}

fn segments_to_spans(segments: Vec<StyledSegment>) -> Vec<Span<'static>> {
  segments
    .into_iter()
    .map(|segment| Span::styled(segment.text, segment.style))
    .collect()
}

fn split_segments_by_whitespace(segments: &[StyledSegment]) -> Vec<StyledSegment> {
  let mut tokens = Vec::new();

  for segment in segments {
    let mut buffer = String::new();
    let mut buffer_is_whitespace: Option<bool> = None;

    for ch in segment.text.chars() {
      let is_whitespace = ch.is_whitespace();
      match buffer_is_whitespace {
        Some(current_state) if current_state == is_whitespace => buffer.push(ch),
        Some(_) => {
          tokens.push(StyledSegment {
            text: std::mem::take(&mut buffer),
            style: segment.style,
            // Assuming this is fine for now; might need adjustment if logic changes
          });
          buffer.push(ch);
          buffer_is_whitespace = Some(is_whitespace);
        }
        None => {
          buffer.push(ch);
          buffer_is_whitespace = Some(is_whitespace);
        }
      }
    }

    if !buffer.is_empty() {
      tokens.push(StyledSegment {
        text: buffer,
        style: segment.style,
      });
    }
  }

  tokens
}

fn wrap_segments_with_indent(
  segments: &[StyledSegment],
  max_width: usize,
  prefix: &str,
  prefix_style: Style,
  indent: &str,
  indent_style: Style,
) -> Vec<Line<'static>> {
  let prefix_width = UnicodeWidthStr::width(prefix);
  let indent_width = UnicodeWidthStr::width(indent);
  let mut lines: Vec<Line<'static>> = Vec::new();
  let tokens = split_segments_by_whitespace(segments);
  let mut current: Vec<StyledSegment> = Vec::new();
  let mut current_width = 0;
  let mut is_first_line = true;

  for token in tokens {
    let token_width = UnicodeWidthStr::width(token.text.as_str());
    let is_whitespace = token.text.chars().all(char::is_whitespace);
    let available_width = if is_first_line {
      max_width.saturating_sub(prefix_width)
    } else {
      max_width.saturating_sub(indent_width)
    };

    if current_width == 0 && is_whitespace {
      continue;
    }

    if current_width + token_width > available_width && current_width > 0 {
      let prefix_to_use = if is_first_line { prefix } else { indent };
      let style_to_use = if is_first_line {
        prefix_style
      } else {
        indent_style
      };
      let mut spans = Vec::with_capacity(current.len() + 1);
      spans.push(Span::styled(prefix_to_use.to_string(), style_to_use));
      spans.extend(segments_to_spans(current));
      lines.push(Line::from(spans));

      current = Vec::new();
      current_width = 0;
      is_first_line = false;

      if is_whitespace {
        continue;
      }
    }

    current_width += token_width;
    current.push(token);
  }

  if !current.is_empty() || lines.is_empty() {
    let prefix_to_use = if is_first_line { prefix } else { indent };
    let style_to_use = if is_first_line {
      prefix_style
    } else {
      indent_style
    };
    let mut spans = Vec::with_capacity(current.len() + 1);
    spans.push(Span::styled(prefix_to_use.to_string(), style_to_use));
    spans.extend(segments_to_spans(current));
    lines.push(Line::from(spans));
  }

  lines
}

fn build_changelog_lines(
  changelog: &str,
  theme: &crate::core::user_config::Theme,
  max_width: u16,
) -> Vec<Line<'static>> {
  let mut lines: Vec<Line<'static>> = vec![];
  let max_width = usize::from(max_width);

  lines.push(Line::from(Span::styled(
    format!(
      "Log located in /tmp/spotatui_logs/spotatuilog{}",
      std::process::id()
    ),
    Style::default().fg(theme.hint),
  )));

  lines.push(Line::from(Span::styled(
    "Please report any bugs or missing features to https://github.com/LargeModGames/spotatui",
    Style::default().fg(theme.hint),
  )));
  lines.push(Line::from(""));

  for line in changelog.lines() {
    if line.starts_with("- ") {
      let content = line.trim_start_matches("- ");
      let segments = parse_markdown_inline(content, Style::default().fg(theme.text));
      let bullet_prefix = "  • ";
      let indent = " ".repeat(UnicodeWidthStr::width(bullet_prefix));
      lines.extend(wrap_segments_with_indent(
        &segments,
        max_width,
        bullet_prefix,
        Style::default().fg(theme.inactive),
        &indent,
        Style::default().fg(theme.text),
      ));
      continue;
    }

    let styled_line = if line.starts_with("# ") {
      Line::from(Span::styled(
        line.trim_start_matches("# ").to_string(),
        Style::default()
          .fg(theme.banner)
          .add_modifier(Modifier::BOLD),
      ))
    } else if line.starts_with("## [") {
      Line::from(Span::styled(
        format!("═══ {} ═══", line.trim_start_matches("## ")),
        Style::default()
          .fg(theme.active)
          .add_modifier(Modifier::BOLD),
      ))
    } else {
      let segments = parse_markdown_inline(line, Style::default().fg(theme.text));
      let line_spans = segments_to_spans(segments);
      Line::from(line_spans)
    };

    lines.push(styled_line);
  }

  lines
}
