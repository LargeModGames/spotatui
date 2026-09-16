use super::config::Station;
use super::directory;
use super::mpris;
use super::{spawn_tune, LocalPlayer, PreparedTune, Session, TuneResult};
use anyhow::{Context, Result};
use crossterm::event::{self, Event as TerminalEvent, KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
  disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
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
  Search(Result<Vec<Station>, String>),
}

pub(super) struct State {
  stations: Vec<Station>,
  selected: usize,
  volume: u8,
  status: String,
  search_input: Option<String>,
  searching: bool,
  session: Option<Session>,
  generation: u64,
  last_title: Option<String>,
}

impl State {
  pub(super) fn new(stations: Vec<Station>, volume: u8) -> Self {
    let status = if stations.is_empty() {
      "No saved stations. Press / to search the radio directory.".to_owned()
    } else {
      "Enter play  / search  Space pause  +/- volume  q quit".to_owned()
    };
    Self {
      stations,
      selected: 0,
      volume,
      status,
      search_input: None,
      searching: false,
      session: None,
      generation: 0,
      last_title: None,
    }
  }

  fn selected_station(&self) -> Option<Station> {
    self.stations.get(self.selected).cloned()
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
      terminal.draw(|frame| draw(frame, &state))?;
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
          UiEvent::Search(result) => {
            state.searching = false;
            match result {
              Ok(stations) if stations.is_empty() => state.status = "No stations found.".to_owned(),
              Ok(stations) => {
                let count = stations.len();
                state.stations = stations;
                state.selected = 0;
                state.status = format!("Found {count} stations. Enter plays the selection.");
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
  execute!(stdout, EnterAlternateScreen).context("entering alternate screen")?;
  Terminal::new(CrosstermBackend::new(stdout)).context("creating terminal")
}

fn close_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
  disable_raw_mode().context("disabling terminal raw mode")?;
  execute!(terminal.backend_mut(), LeaveAlternateScreen).context("leaving alternate screen")?;
  terminal.show_cursor().context("showing cursor")
}

fn spawn_input_thread(running: Arc<AtomicBool>, tx: mpsc::UnboundedSender<UiEvent>) {
  std::thread::spawn(move || {
    while running.load(Ordering::Relaxed) {
      if event::poll(Duration::from_millis(100)).unwrap_or(false) {
        if let Ok(TerminalEvent::Key(key)) = event::read() {
          let _ = tx.send(UiEvent::Key(key));
        }
      }
    }
  });
}

fn draw(frame: &mut ratatui::Frame<'_>, state: &State) {
  let area = frame.area();
  let chunks = Layout::default()
    .direction(Direction::Vertical)
    .constraints([
      Constraint::Length(3),
      Constraint::Min(4),
      Constraint::Length(3),
    ])
    .split(area);

  let title = state
    .session
    .as_ref()
    .map(|session| {
      state
        .now_playing()
        .unwrap_or_else(|| format!("LIVE — {}", session.station.name))
    })
    .unwrap_or_else(|| "Spotatui Radio".to_owned());
  frame.render_widget(
    Paragraph::new(Line::styled(
      title,
      Style::default()
        .fg(Color::Green)
        .add_modifier(Modifier::BOLD),
    ))
    .block(Block::default().borders(Borders::ALL)),
    chunks[0],
  );

  let items = state
    .stations
    .iter()
    .map(|station| ListItem::new(station.name.clone()))
    .collect::<Vec<_>>();
  let list = List::new(items)
    .block(Block::default().borders(Borders::ALL).title("Stations"))
    .highlight_symbol("▶ ")
    .highlight_style(
      Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD),
    );
  let mut list_state =
    ListState::default().with_selected((!state.stations.is_empty()).then_some(state.selected));
  frame.render_stateful_widget(list, chunks[1], &mut list_state);

  let footer = if let Some(input) = &state.search_input {
    format!(
      "Search: {input}{}",
      if state.searching { " …" } else { "_" }
    )
  } else {
    format!("{}  |  Volume {}%", state.status, state.volume)
  };
  frame.render_widget(
    Paragraph::new(footer).block(Block::default().borders(Borders::ALL)),
    chunks[2],
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
    KeyCode::Char('q') | KeyCode::Esc => return Ok(true),
    KeyCode::Up | KeyCode::Char('k') => select_previous(state),
    KeyCode::Down | KeyCode::Char('j') => select_next(state),
    KeyCode::Enter => play_selected(state, player, tune_tx),
    KeyCode::Char('/') => state.search_input = Some(String::new()),
    KeyCode::Char(' ') => toggle(state, player, mpris),
    KeyCode::Char('+') | KeyCode::Char('=') => {
      set_volume(state, player, state.volume.saturating_add(5), mpris)
    }
    KeyCode::Char('-') => set_volume(state, player, state.volume.saturating_sub(5), mpris),
    KeyCode::Char('s') => {
      stop(state, player);
      mpris.set_stopped();
    }
    _ => {}
  }
  Ok(false)
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
  if !state.stations.is_empty() {
    state.selected = (state.selected + 1) % state.stations.len();
  }
}

fn select_previous(state: &mut State) {
  if !state.stations.is_empty() {
    state.selected = (state.selected + state.stations.len() - 1) % state.stations.len();
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
        let station = state
          .stations
          .iter()
          .position(|station| station.url == url)
          .map(|index| {
            state.selected = index;
            state.stations[index].clone()
          })
          .unwrap_or_else(|| Station {
            name: url.to_owned(),
            url: url.to_owned(),
          });
        begin_tune(state, player, tx, station);
      } else {
        state.status = "MPRIS rejected a non-HTTP radio URI.".to_owned();
      }
    }
  }
}
