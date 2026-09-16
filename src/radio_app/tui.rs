use super::config::{self, Station, ThemeSettings};
use super::directory;
use super::mpris;
use super::{spawn_tune, LocalPlayer, PreparedTune, Session, TuneResult};
use anyhow::{Context, Result};
use crossterm::event::{
  self, DisableMouseCapture, EnableMouseCapture, Event as TerminalEvent, KeyCode, KeyEvent,
  KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
  disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Terminal;
use std::io::{self, Stdout};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

enum UiEvent {
  Key(KeyEvent),
  Mouse(MouseEvent),
  Search(Result<Vec<Station>, String>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Panel {
  Saved,
  Results,
  Settings,
}

pub(super) struct State {
  stations: Vec<Station>,
  saved_stations: Vec<Station>,
  saved_selected: usize,
  result_selected: usize,
  active_panel: Panel,
  theme: ThemeSettings,
  settings_selected: usize,
  volume: u8,
  status: String,
  search_input: Option<String>,
  searching: bool,
  session: Option<Session>,
  generation: u64,
  last_title: Option<String>,
  search_area: Rect,
  settings_area: Rect,
  saved_area: Rect,
  results_area: Rect,
}

impl State {
  pub(super) fn new(stations: Vec<Station>, volume: u8, theme: ThemeSettings) -> Self {
    let status = if stations.is_empty() {
      "No saved stations. Press S to search for a station.".to_owned()
    } else {
      "S Search stations  Enter Play  F Favorite  Space Pause  Q Quit".to_owned()
    };
    let settings_selected = theme_preset_index(&theme.preset);
    Self {
      stations: Vec::new(),
      saved_stations: stations,
      saved_selected: 0,
      result_selected: 0,
      active_panel: Panel::Saved,
      theme,
      settings_selected,
      volume,
      status,
      search_input: None,
      searching: false,
      session: None,
      generation: 0,
      last_title: None,
      search_area: Rect::default(),
      settings_area: Rect::default(),
      saved_area: Rect::default(),
      results_area: Rect::default(),
    }
  }

  fn selected_station(&self) -> Option<Station> {
    match self.active_panel {
      Panel::Saved => self.saved_stations.get(self.saved_selected),
      Panel::Results => self.stations.get(self.result_selected),
      Panel::Settings => None,
    }
    .cloned()
  }

  fn selected_is_saved(&self) -> bool {
    self.selected_station().is_some_and(|selected| {
      self
        .saved_stations
        .iter()
        .any(|station| station.url == selected.url)
    })
  }

  fn show_saved_stations(&mut self) {
    self.active_panel = Panel::Saved;
    self.status = "Saved stations. Press S to search for a station.".to_owned();
  }

  fn now_playing(&self) -> Option<String> {
    self.session.as_ref()?.now_playing.lock().ok()?.clone()
  }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run(
  mut state: State,
  player: Arc<LocalPlayer>,
  tune_tx: mpsc::UnboundedSender<TuneResult>,
  mut tune_rx: mpsc::UnboundedReceiver<TuneResult>,
  mpris: Arc<mpris::Manager>,
  mut mpris_rx: mpsc::UnboundedReceiver<mpris::Event>,
) -> Result<()> {
  let mut terminal = open_terminal()?;
  let running = Arc::new(AtomicBool::new(true));
  let (ui_tx, mut ui_rx) = mpsc::unbounded_channel();
  spawn_input_thread(Arc::clone(&running), ui_tx.clone());
  let mut tick = tokio::time::interval(Duration::from_millis(200));

  mpris.set_volume(state.volume);

  let result = async {
    loop {
      terminal.draw(|frame| draw(frame, &mut state))?;
      tokio::select! {
        _ = tick.tick() => {
          let title = state.now_playing();
          if title != state.last_title {
            state.last_title = title.clone();
            if let Some(session) = &state.session {
              mpris.set_metadata(title.unwrap_or_else(|| session.station.name.clone()), session.station.name.clone());
            }
          }
          if state.session.is_some() && player.is_finished() {
            stop(&mut state, &player);
            state.status = "The radio stream ended.".to_owned();
            mpris.set_stopped();
          }
        }
        Some(event) = ui_rx.recv() => match event {
          UiEvent::Key(key) => {
            if handle_key(key, &mut state, &player, &tune_tx, &ui_tx,
              &mpris).await? {
              break;
            }
          }
          UiEvent::Mouse(mouse) => handle_mouse(mouse, &mut state),
          UiEvent::Search(result) => {
            state.searching = false;
            state.search_input = None;
            match result {
              Ok(stations) if stations.is_empty() => state.status = "No stations found.".to_owned(),
              Ok(stations) => {
                let count = stations.len();
                state.stations = stations;
                state.result_selected = 0;
                state.active_panel = Panel::Results;
                state.status = format!("Found {count} stations. Enter plays; F adds a favorite.");
              }
              Err(message) => state.status = message,
            }
          }
        },
        Some(result) = tune_rx.recv() => finish_tune(result, &mut state, &player,
          &mpris),
        Some(event) = mpris_rx.recv() => handle_mpris(event, &mut state, &player, &tune_tx, &mpris),
      }
    }
    Ok::<(), anyhow::Error>(())
  }.await;

  running.store(false, Ordering::Relaxed);
  stop(&mut state, &player);
  close_terminal(&mut terminal)?;
  result
}

fn open_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
  enable_raw_mode().context("enabling terminal raw mode")?;
  let mut stdout = io::stdout();
  execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
    .context("entering alternate screen")?;
  Terminal::new(CrosstermBackend::new(stdout)).context("creating terminal")
}

fn close_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
  disable_raw_mode().context("disabling terminal raw mode")?;
  execute!(
    terminal.backend_mut(),
    DisableMouseCapture,
    LeaveAlternateScreen
  )
  .context("leaving alternate screen")?;
  terminal.show_cursor().context("showing cursor")
}

fn spawn_input_thread(running: Arc<AtomicBool>, tx: mpsc::UnboundedSender<UiEvent>) {
  std::thread::spawn(move || {
    while running.load(Ordering::Relaxed) {
      if event::poll(Duration::from_millis(100)).unwrap_or(false) {
        match event::read() {
          Ok(TerminalEvent::Key(key)) => {
            let _ = tx.send(UiEvent::Key(key));
          }
          Ok(TerminalEvent::Mouse(mouse)) => {
            let _ = tx.send(UiEvent::Mouse(mouse));
          }
          _ => {}
        }
      }
    }
  });
}

fn draw(frame: &mut ratatui::Frame<'_>, state: &mut State) {
  let area = frame.area();
  let chunks = Layout::default()
    .direction(Direction::Vertical)
    .constraints([
      Constraint::Length(3),
      Constraint::Length(3),
      Constraint::Min(7),
      Constraint::Length(3),
    ])
    .split(area);

  let palette = theme_palette(&state.theme.preset);
  let focus_color = palette.focused_border;
  let playing_color = palette.now_playing;
  let selection_color = palette.selection;
  let favorite_color = palette.favorite;

  let title = state
    .session
    .as_ref()
    .map(|session| {
      state
        .now_playing()
        .unwrap_or_else(|| format!("LIVE — {}", session.station.name))
    })
    .unwrap_or_else(|| "Degen Radio".to_owned());
  frame.render_widget(
    Paragraph::new(Line::styled(
      title,
      Style::default()
        .fg(playing_color)
        .add_modifier(Modifier::BOLD),
    ))
    .block(Block::default().borders(Borders::ALL).title("Now Playing")),
    chunks[0],
  );

  let controls = Layout::default()
    .direction(Direction::Horizontal)
    .constraints([Constraint::Percentage(82), Constraint::Percentage(18)])
    .split(chunks[1]);
  state.search_area = controls[0];
  state.settings_area = controls[1];
  let search_text = match &state.search_input {
    Some(input) => format!(
      "{input}{}",
      if state.searching {
        "  Searching…"
      } else {
        "_"
      }
    ),
    None => "Click here or press S to search by station name".to_owned(),
  };
  let search_style = if state.search_input.is_some() {
    Style::default()
      .fg(focus_color)
      .add_modifier(Modifier::BOLD)
  } else {
    Style::default()
  };
  frame.render_widget(
    Paragraph::new(search_text).style(search_style).block(
      Block::default()
        .borders(Borders::ALL)
        .border_style(search_style)
        .title("Search Radio Directory"),
    ),
    state.search_area,
  );

  let settings_focused = state.search_input.is_none() && state.active_panel == Panel::Settings;
  frame.render_widget(
    Paragraph::new("Click or press x").block(
      Block::default()
        .borders(Borders::ALL)
        .border_style(if settings_focused {
          Style::default().fg(focus_color)
        } else {
          Style::default()
        })
        .title("Settings"),
    ),
    state.settings_area,
  );

  let body = Layout::default()
    .direction(Direction::Horizontal)
    .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
    .split(chunks[2]);
  state.saved_area = body[0];
  state.results_area = body[1];

  let saved_items = state
    .saved_stations
    .iter()
    .map(|station| {
      ListItem::new(Line::styled(
        format!("♥ {}", station.name),
        Style::default().fg(favorite_color),
      ))
    })
    .collect::<Vec<_>>();
  let saved_border = if state.search_input.is_none() && state.active_panel == Panel::Saved {
    Style::default().fg(focus_color)
  } else {
    Style::default()
  };
  let saved_list = List::new(saved_items)
    .block(
      Block::default()
        .borders(Borders::ALL)
        .border_style(saved_border)
        .title("Saved Radio Stations — D: unfavorite  →: results"),
    )
    .highlight_symbol("▶ ")
    .highlight_style(
      Style::default()
        .fg(selection_color)
        .add_modifier(Modifier::BOLD),
    );
  let mut saved_state = ListState::default().with_selected(
    (state.active_panel == Panel::Saved && !state.saved_stations.is_empty())
      .then_some(state.saved_selected),
  );
  frame.render_stateful_widget(saved_list, state.saved_area, &mut saved_state);

  let results_border = if state.search_input.is_none() && state.active_panel == Panel::Results {
    Style::default().fg(focus_color)
  } else {
    Style::default()
  };
  if state.active_panel == Panel::Settings {
    let settings = THEME_PRESETS
      .iter()
      .map(|preset| {
        let marker = if preset.name.eq_ignore_ascii_case(&state.theme.preset) {
          "● "
        } else {
          "  "
        };
        ListItem::new(Line::styled(
          format!("{marker}{}", preset.name),
          Style::default().fg(preset.palette.focused_border),
        ))
      })
      .collect::<Vec<_>>();
    let mut settings_state = ListState::default().with_selected(Some(state.settings_selected));
    frame.render_stateful_widget(
      List::new(settings)
        .block(
          Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(focus_color))
            .title("Settings › Theme Presets — ↑/↓ choose  Enter apply  Esc close"),
        )
        .highlight_symbol("▶ ")
        .highlight_style(
          Style::default()
            .fg(selection_color)
            .add_modifier(Modifier::BOLD),
        ),
      state.results_area,
      &mut settings_state,
    );
  } else if state.stations.is_empty() {
    frame.render_widget(
      Paragraph::new(
        "Find stations from the Radio Browser directory.\n\n\
         1. Click the search box or press S\n\
         2. Type a station name and press Enter\n\
         3. Select a result with ↑/↓ or the mouse\n\
         4. Press Enter to play, F to favorite, or D to unfavorite",
      )
      .block(
        Block::default()
          .borders(Borders::ALL)
          .border_style(results_border)
          .title("Directory Results — ←: saved stations"),
      ),
      state.results_area,
    );
  } else {
    let result_items = state
      .stations
      .iter()
      .map(|station| {
        let favorite = state
          .saved_stations
          .iter()
          .any(|saved| saved.url == station.url);
        let (marker, style) = if favorite {
          ("♥ ", Style::default().fg(favorite_color))
        } else {
          ("📻 ", Style::default())
        };
        ListItem::new(Line::styled(format!("{marker}{}", station.name), style))
      })
      .collect::<Vec<_>>();
    let results = List::new(result_items)
      .block(
        Block::default()
          .borders(Borders::ALL)
          .border_style(results_border)
          .title("Directory Results — Enter: play  F: favorite  D: unfavorite  ←: saved"),
      )
      .highlight_symbol("▶ ")
      .highlight_style(
        Style::default()
          .fg(selection_color)
          .add_modifier(Modifier::BOLD),
      );
    let mut result_state = ListState::default().with_selected(
      (state.active_panel == Panel::Results && !state.stations.is_empty())
        .then_some(state.result_selected),
    );
    frame.render_stateful_widget(results, state.results_area, &mut result_state);
  }

  let footer = state
    .now_playing()
    .or_else(|| {
      state
        .session
        .as_ref()
        .map(|session| session.station.name.clone())
    })
    .map(|track| format!("{track}  |  {}  |  Volume {}%", state.status, state.volume))
    .unwrap_or_else(|| format!("{}  |  Volume {}%", state.status, state.volume));
  frame.render_widget(
    Paragraph::new(footer).block(
      Block::default()
        .borders(Borders::ALL)
        .title("S Search  x Settings  ←/→ Focus  Enter Play  F Favorite  D Unfavorite  Q Quit"),
    ),
    chunks[3],
  );
}

async fn handle_key(
  key: KeyEvent,
  state: &mut State,
  player: &Arc<LocalPlayer>,
  tune_tx: &mpsc::UnboundedSender<TuneResult>,
  ui_tx: &mpsc::UnboundedSender<UiEvent>,
  mpris: &mpris::Manager,
) -> Result<bool> {
  if let Some(input) = state.search_input.as_mut() {
    match key.code {
      KeyCode::Esc => state.search_input = None,
      KeyCode::Backspace => {
        input.pop();
      }
      KeyCode::Enter if !state.searching => {
        let query = input.trim().to_owned();
        if !query.is_empty() {
          state.searching = true;
          let tx = ui_tx.clone();
          tokio::spawn(async move {
            let result = directory::search(&query)
              .await
              .map_err(|error| format!("Radio search failed: {error:#}"));
            let _ = tx.send(UiEvent::Search(result));
          });
        }
      }
      KeyCode::Char(character)
        if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
      {
        input.push(character)
      }
      _ => {}
    }
    return Ok(false);
  }

  match key.code {
    KeyCode::Char('q') => return Ok(true),
    KeyCode::Esc if state.active_panel != Panel::Saved => state.show_saved_stations(),
    KeyCode::Esc => return Ok(true),
    KeyCode::Left | KeyCode::Char('h') if state.active_panel == Panel::Settings => {
      state.show_saved_stations()
    }
    KeyCode::Left | KeyCode::Char('h') => state.active_panel = Panel::Saved,
    KeyCode::Right | KeyCode::Char('l') if !state.stations.is_empty() => {
      state.active_panel = Panel::Results
    }
    KeyCode::Up | KeyCode::Char('k') => select_previous(state),
    KeyCode::Down | KeyCode::Char('j') => select_next(state),
    KeyCode::Enter if state.active_panel == Panel::Settings => apply_selected_theme(state),
    KeyCode::Enter => play_selected(state, player, tune_tx),
    KeyCode::Char('/') | KeyCode::Char('s') => state.search_input = Some(String::new()),
    KeyCode::Char('x') if state.active_panel == Panel::Settings => state.show_saved_stations(),
    KeyCode::Char('x') => open_settings(state),
    KeyCode::Char('f') | KeyCode::Char('F') => favorite_selected(state),
    KeyCode::Char('d') | KeyCode::Char('D') => unfavorite_selected(state),
    KeyCode::Char('r') => state.show_saved_stations(),
    KeyCode::Char(' ') => toggle(state, player, mpris),
    KeyCode::Char('+') | KeyCode::Char('=') => {
      set_volume(state, player, state.volume.saturating_add(5), mpris)
    }
    KeyCode::Char('-') => set_volume(state, player, state.volume.saturating_sub(5), mpris),
    KeyCode::Char('X') => {
      stop(state, player);
      mpris.set_stopped();
    }
    _ => {}
  }
  Ok(false)
}

fn favorite_selected(state: &mut State) {
  let Some(station) = state.selected_station() else {
    return;
  };
  if state.selected_is_saved() {
    state.status = format!("{} is already a favorite.", station.name);
    return;
  }

  let mut favorites = state.saved_stations.clone();
  favorites.push(station.clone());
  match config::save_preferences(&favorites, state.volume, &state.theme) {
    Ok(()) => {
      state.saved_stations = favorites;
      state.status = format!("Added {} to favorites.", station.name);
    }
    Err(error) => state.status = format!("Could not save favorite: {error:#}"),
  }
}

fn unfavorite_selected(state: &mut State) {
  let Some(station) = state.selected_station() else {
    return;
  };
  let Some(index) = state
    .saved_stations
    .iter()
    .position(|saved| saved.url == station.url)
  else {
    state.status = format!("{} is not a favorite.", station.name);
    return;
  };

  let mut favorites = state.saved_stations.clone();
  favorites.remove(index);
  match config::save_preferences(&favorites, state.volume, &state.theme) {
    Ok(()) => {
      state.saved_stations = favorites;
      state.saved_selected = state
        .saved_selected
        .min(state.saved_stations.len().saturating_sub(1));
      state.status = format!("Removed {} from favorites.", station.name);
    }
    Err(error) => state.status = format!("Could not remove favorite: {error:#}"),
  }
}

fn handle_mouse(mouse: MouseEvent, state: &mut State) {
  match mouse.kind {
    MouseEventKind::Down(MouseButton::Left) => {
      if contains(state.search_area, mouse.column, mouse.row) {
        state.search_input = Some(String::new());
      } else if contains(state.settings_area, mouse.column, mouse.row) {
        open_settings(state);
      } else if contains(state.saved_area, mouse.column, mouse.row) {
        state.active_panel = Panel::Saved;
        if let Some(index) = clicked_row(state.saved_area, mouse.row, state.saved_stations.len()) {
          state.saved_selected = index;
        }
      } else if contains(state.results_area, mouse.column, mouse.row) {
        if state.active_panel == Panel::Settings {
          if let Some(index) = clicked_row(state.results_area, mouse.row, THEME_PRESETS.len()) {
            state.settings_selected = index;
            apply_selected_theme(state);
          }
        } else {
          state.active_panel = Panel::Results;
          if let Some(index) = clicked_row(state.results_area, mouse.row, state.stations.len()) {
            state.result_selected = index;
          }
        }
      }
    }
    MouseEventKind::ScrollDown => {
      focus_mouse_panel(mouse, state);
      select_next(state);
    }
    MouseEventKind::ScrollUp => {
      focus_mouse_panel(mouse, state);
      select_previous(state);
    }
    _ => {}
  }
}

fn focus_mouse_panel(mouse: MouseEvent, state: &mut State) {
  if contains(state.saved_area, mouse.column, mouse.row) {
    state.active_panel = Panel::Saved;
  } else if contains(state.results_area, mouse.column, mouse.row)
    && state.active_panel != Panel::Settings
  {
    state.active_panel = Panel::Results;
  }
}

fn contains(area: Rect, column: u16, row: u16) -> bool {
  column >= area.x
    && column < area.x.saturating_add(area.width)
    && row >= area.y
    && row < area.y.saturating_add(area.height)
}

fn clicked_row(area: Rect, row: u16, item_count: usize) -> Option<usize> {
  let index = row.checked_sub(area.y.saturating_add(1))? as usize;
  (index < item_count).then_some(index)
}

#[derive(Clone, Copy)]
struct ThemePalette {
  focused_border: Color,
  now_playing: Color,
  selection: Color,
  favorite: Color,
}

#[derive(Clone, Copy)]
struct ThemePreset {
  name: &'static str,
  palette: ThemePalette,
}

const THEME_PRESETS: [ThemePreset; 6] = [
  ThemePreset {
    name: "Tokyo Night",
    palette: ThemePalette {
      focused_border: Color::Rgb(0x7a, 0xa2, 0xf7),
      now_playing: Color::Rgb(0x9e, 0xce, 0x6a),
      selection: Color::Rgb(0x7d, 0xa6, 0xff),
      favorite: Color::Rgb(0xbb, 0x9a, 0xf7),
    },
  },
  ThemePreset {
    name: "Catppuccin",
    palette: ThemePalette {
      focused_border: Color::Rgb(0x89, 0xb4, 0xfa),
      now_playing: Color::Rgb(0xa6, 0xe3, 0xa1),
      selection: Color::Rgb(0x94, 0xe2, 0xd5),
      favorite: Color::Rgb(0xf5, 0xc2, 0xe7),
    },
  },
  ThemePreset {
    name: "Osaka Jade",
    palette: ThemePalette {
      focused_border: Color::Rgb(0x50, 0x94, 0x75),
      now_playing: Color::Rgb(0x63, 0xb0, 0x7a),
      selection: Color::Rgb(0x2d, 0xd5, 0xb7),
      favorite: Color::Rgb(0xd2, 0x68, 0x9c),
    },
  },
  ThemePreset {
    name: "Gruvbox",
    palette: ThemePalette {
      focused_border: Color::Rgb(0x7d, 0xae, 0xa3),
      now_playing: Color::Rgb(0xa9, 0xb6, 0x65),
      selection: Color::Rgb(0xd8, 0xa6, 0x57),
      favorite: Color::Rgb(0xd3, 0x86, 0x9b),
    },
  },
  ThemePreset {
    name: "Nord",
    palette: ThemePalette {
      focused_border: Color::Rgb(0x81, 0xa1, 0xc1),
      now_playing: Color::Rgb(0xa3, 0xbe, 0x8c),
      selection: Color::Rgb(0x88, 0xc0, 0xd0),
      favorite: Color::Rgb(0xb4, 0x8e, 0xad),
    },
  },
  ThemePreset {
    name: "Rose Pine",
    palette: ThemePalette {
      focused_border: Color::Rgb(0x56, 0x94, 0x9f),
      now_playing: Color::Rgb(0x28, 0x69, 0x83),
      selection: Color::Rgb(0xd7, 0x82, 0x7e),
      favorite: Color::Rgb(0x90, 0x7a, 0xa9),
    },
  },
];

fn theme_preset_index(name: &str) -> usize {
  THEME_PRESETS
    .iter()
    .position(|preset| preset.name.eq_ignore_ascii_case(name))
    .unwrap_or(0)
}

fn theme_palette(name: &str) -> ThemePalette {
  THEME_PRESETS[theme_preset_index(name)].palette
}

fn open_settings(state: &mut State) {
  state.settings_selected = theme_preset_index(&state.theme.preset);
  state.active_panel = Panel::Settings;
}

fn apply_selected_theme(state: &mut State) {
  let preset = THEME_PRESETS[state.settings_selected];
  state.theme.preset = preset.name.to_owned();
  match config::save_preferences(&state.saved_stations, state.volume, &state.theme) {
    Ok(()) => state.status = format!("Theme set to {}.", preset.name),
    Err(error) => state.status = format!("Could not save theme preset: {error:#}"),
  }
}

fn play_selected(
  state: &mut State,
  player: &Arc<LocalPlayer>,
  tx: &mpsc::UnboundedSender<TuneResult>,
) {
  if let Some(station) = state.selected_station() {
    begin_tune(state, player, tx, station);
  }
}

fn begin_tune(
  state: &mut State,
  player: &Arc<LocalPlayer>,
  tx: &mpsc::UnboundedSender<TuneResult>,
  station: Station,
) {
  stop(state, player);
  state.generation = state.generation.wrapping_add(1);
  state.status = format!("Connecting to {}…", station.name);
  spawn_tune(state.generation, station, tx.clone());
}

fn finish_tune(
  result: TuneResult,
  state: &mut State,
  player: &Arc<LocalPlayer>,
  mpris: &mpris::Manager,
) {
  match result {
    TuneResult::Ready(PreparedTune {
      generation,
      mut station,
      stream_name,
      now_playing,
      cancel,
      prepared,
    }) => {
      if generation != state.generation {
        cancel();
        return;
      }
      if station.name == station.url {
        if let Some(name) = stream_name {
          station.name = name;
        }
      }
      player.stop();
      player.set_volume(state.volume);
      match player.play_prepared(prepared) {
        Ok(()) => {
          state.status = format!("Playing {}", station.name);
          state.last_title = None;
          {
            mpris.set_metadata(station.name.clone(), station.name.clone());
            mpris.set_playing(true);
          }
          state.session = Some(Session {
            station,
            now_playing,
            cancel,
          });
        }
        Err(error) => {
          cancel();
          state.status = format!("Could not start audio: {error:#}");
        }
      }
    }
    TuneResult::Failed {
      generation,
      message,
    } if generation == state.generation => state.status = message,
    TuneResult::Failed { .. } => {}
  }
}

fn toggle(state: &mut State, player: &Arc<LocalPlayer>, mpris: &mpris::Manager) {
  if state.session.is_none() {
    return;
  }
  if player.is_paused() {
    player.resume();
    state.status = "Playing".to_owned();
    mpris.set_playing(true);
  } else {
    player.pause();
    state.status = "Paused".to_owned();
    mpris.set_playing(false);
  }
}

fn stop(state: &mut State, player: &Arc<LocalPlayer>) {
  state.generation = state.generation.wrapping_add(1);
  if let Some(session) = state.session.take() {
    (session.cancel)();
  }
  player.stop();
}

fn set_volume(state: &mut State, player: &Arc<LocalPlayer>, percent: u8, mpris: &mpris::Manager) {
  state.volume = percent.min(100);
  player.set_volume(state.volume);
  mpris.set_volume(state.volume);
}

fn select_next(state: &mut State) {
  match state.active_panel {
    Panel::Saved if !state.saved_stations.is_empty() => {
      state.saved_selected = (state.saved_selected + 1) % state.saved_stations.len();
    }
    Panel::Results if !state.stations.is_empty() => {
      state.result_selected = (state.result_selected + 1) % state.stations.len();
    }
    Panel::Settings => {
      state.settings_selected = (state.settings_selected + 1) % THEME_PRESETS.len()
    }
    _ => {}
  }
}

fn select_previous(state: &mut State) {
  match state.active_panel {
    Panel::Saved if !state.saved_stations.is_empty() => {
      state.saved_selected =
        (state.saved_selected + state.saved_stations.len() - 1) % state.saved_stations.len();
    }
    Panel::Results if !state.stations.is_empty() => {
      state.result_selected =
        (state.result_selected + state.stations.len() - 1) % state.stations.len();
    }
    Panel::Settings => {
      state.settings_selected =
        (state.settings_selected + THEME_PRESETS.len() - 1) % THEME_PRESETS.len()
    }
    _ => {}
  }
}

fn handle_mpris(
  event: mpris::Event,
  state: &mut State,
  player: &Arc<LocalPlayer>,
  tx: &mpsc::UnboundedSender<TuneResult>,
  mpris: &mpris::Manager,
) {
  match event {
    mpris::Event::PlayPause => {
      if state.session.is_some() {
        toggle(state, player, mpris);
      } else {
        play_selected(state, player, tx);
      }
    }
    mpris::Event::Play => {
      if state.session.is_some() {
        if player.is_paused() {
          toggle(state, player, mpris);
        }
      } else {
        play_selected(state, player, tx);
      }
    }
    mpris::Event::Pause => {
      if state.session.is_some() && !player.is_paused() {
        toggle(state, player, mpris);
      }
    }
    mpris::Event::Next => {
      select_next(state);
      play_selected(state, player, tx);
    }
    mpris::Event::Previous => {
      select_previous(state);
      play_selected(state, player, tx);
    }
    mpris::Event::Stop => {
      stop(state, player);
      mpris.set_stopped();
    }
    mpris::Event::SetVolume(percent) => set_volume(state, player, percent, mpris),
    mpris::Event::OpenUri(uri) => {
      let url = uri.strip_prefix("radio:").unwrap_or(&uri);
      if url::Url::parse(url)
        .ok()
        .is_some_and(|parsed| matches!(parsed.scheme(), "http" | "https"))
      {
        let station = if let Some(index) = state
          .saved_stations
          .iter()
          .position(|station| station.url == url)
        {
          state.active_panel = Panel::Saved;
          state.saved_selected = index;
          state.saved_stations[index].clone()
        } else if let Some(index) = state.stations.iter().position(|station| station.url == url) {
          state.active_panel = Panel::Results;
          state.result_selected = index;
          state.stations[index].clone()
        } else {
          Station {
            name: url.to_owned(),
            url: url.to_owned(),
          }
        };
        begin_tune(state, player, tx, station);
      } else {
        state.status = "MPRIS rejected a non-HTTP radio URI.".to_owned();
      }
    }
  }
}
