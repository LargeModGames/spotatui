//! A development aid: with `SPOTATUI_DEV_INSPECT=<dir>`, a line `<id>\t<wait_ms>\t<key>...` written to
//! `<dir>/in` types its keys as the terminal would, and the frame drawn after them lands in
//! `<dir>/frame.json`. Keys use the config names (`j`, `enter`, `ctrl-f`); `text:<chars>` types each char.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::Duration;
use std::{fs, thread};

use ratatui::buffer::Buffer;
use ratatui::style::Style;
use serde_json::{json, Value};
use unicode_width::UnicodeWidthStr;

use super::event::{Event, Key};
use crate::core::user_config::parse_key_public;

const ENV: &str = "SPOTATUI_DEV_INSPECT";
const POLL: Duration = Duration::from_millis(50);

pub struct Inspector {
  dir: PathBuf,
}

impl Inspector {
  /// Starts the inbox watcher when `SPOTATUI_DEV_INSPECT` names a directory.
  pub fn from_env(tx: Sender<Event>) -> Option<Self> {
    Self::with_dir(std::env::var_os(ENV).map(PathBuf::from), tx)
  }

  fn with_dir(dir: Option<PathBuf>, tx: Sender<Event>) -> Option<Self> {
    let dir = dir.filter(|dir| !dir.as_os_str().is_empty())?;
    if let Err(error) = fs::create_dir_all(&dir) {
      log::warn!("[dev-inspect] cannot create {}: {error}", dir.display());
      return None;
    }
    let inbox = dir.join("in");
    let _ = fs::remove_file(&inbox);
    thread::spawn(move || watch(&inbox, &tx));
    log::info!("[dev-inspect] watching {}", dir.display());
    Some(Self { dir })
  }

  pub fn write_frame(&self, snapshot: &Snapshot, route: &str, buffer: &Buffer) {
    let tmp = self.dir.join("frame.json.tmp");
    let body = frame_json(snapshot, route, buffer).to_string();
    if let Err(error) =
      fs::write(&tmp, body).and_then(|()| fs::rename(&tmp, self.dir.join("frame.json")))
    {
      log::warn!("[dev-inspect] cannot write the frame: {error}");
    }
  }
}

/// A request for the next frame, with the reason its keys were not typed if they were not.
pub struct Snapshot {
  pub id: u64,
  pub error: Option<String>,
}

fn watch(inbox: &Path, tx: &Sender<Event>) {
  loop {
    thread::sleep(POLL);
    let Ok(text) = fs::read_to_string(inbox) else {
      continue;
    };
    if !text.ends_with('\n') {
      continue;
    }
    if fs::remove_file(inbox).is_err() {
      continue;
    }
    for line in text.lines().filter(|line| !line.is_empty()) {
      let (id, outcome) = parse_request(line);
      let error = match outcome {
        Ok((wait, keys)) => {
          for key in keys {
            if tx.send(Event::Input(key)).is_err() {
              return;
            }
          }
          thread::sleep(wait);
          None
        }
        Err(error) => Some(error),
      };
      if tx.send(Event::Snapshot(Snapshot { id, error })).is_err() {
        return;
      }
    }
  }
}

type Parsed = Result<(Duration, Vec<Key>), String>;

fn parse_request(line: &str) -> (u64, Parsed) {
  let mut fields = line.split('\t');
  let Some(id) = fields.next().and_then(|id| id.parse().ok()) else {
    return (0, Err(format!("no numeric id in the request {line:?}")));
  };
  let Some(wait) = fields.next().and_then(|wait| wait.parse().ok()) else {
    return (id, Err(format!("no wait_ms in the request {line:?}")));
  };
  let keys = fields.try_fold(Vec::new(), |mut keys, field| {
    match field.strip_prefix("text:") {
      Some(text) => keys.extend(text.chars().map(Key::Char)),
      None => keys.push(parse_key_public(field.to_string()).map_err(|error| error.to_string())?),
    }
    Ok(keys)
  });
  (id, keys.map(|keys| (Duration::from_millis(wait), keys)))
}

/// The frame as text rows for reading and as style runs `[x, width, fg, bg, modifier bits, text]` for drawing.
fn frame_json(snapshot: &Snapshot, route: &str, buffer: &Buffer) -> Value {
  let area = buffer.area;
  let mut rows = Vec::with_capacity(usize::from(area.height));
  let mut runs = Vec::with_capacity(usize::from(area.height));
  for y in area.top()..area.bottom() {
    let mut row = String::new();
    let mut row_runs: Vec<(u16, u16, Style, String)> = Vec::new();
    let mut skip = 0;
    for x in area.left()..area.right() {
      if skip > 0 {
        skip -= 1;
        continue;
      }
      let cell = &buffer[(x, y)];
      let symbol = cell.symbol();
      let width = symbol.width().max(1) as u16;
      skip = width - 1;
      row.push_str(symbol);
      let style = cell.style();
      match row_runs.last_mut() {
        Some(run) if run.2 == style => {
          run.1 += width;
          run.3.push_str(symbol);
        }
        _ => row_runs.push((x - area.left(), width, style, symbol.to_string())),
      }
    }
    rows.push(row.trim_end().to_string());
    runs.push(
      row_runs
        .into_iter()
        .map(|(x, width, style, text)| {
          let fg = style.fg.unwrap_or_default().to_string();
          let bg = style.bg.unwrap_or_default().to_string();
          json!([x, width, fg, bg, style.add_modifier.bits(), text])
        })
        .collect::<Vec<_>>(),
    );
  }
  json!({
    "id": snapshot.id,
    "error": snapshot.error,
    "route": route,
    "width": area.width,
    "height": area.height,
    "rows": rows,
    "runs": runs,
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use ratatui::layout::Rect;
  use ratatui::style::{Color, Modifier};

  #[test]
  fn a_request_types_named_keys_and_text_after_its_wait() {
    let (id, parsed) = parse_request("7\t250\tdown\ttext:a b\tctrl-f");
    let (wait, keys) = parsed.unwrap();
    assert_eq!(id, 7);
    assert_eq!(wait, Duration::from_millis(250));
    assert_eq!(
      keys,
      vec![
        Key::Down,
        Key::Char('a'),
        Key::Char(' '),
        Key::Char('b'),
        Key::Ctrl('f')
      ]
    );
  }

  #[test]
  fn a_request_with_an_unknown_key_keeps_its_id_and_types_nothing() {
    let (id, parsed) = parse_request("3\t0\tj\tnosuchkey");
    assert_eq!(id, 3);
    assert!(parsed.unwrap_err().contains("nosuchkey"));
  }

  #[test]
  fn a_request_with_a_bad_id_types_nothing() {
    let (_, parsed) = parse_request("bad-id\t0\tenter");
    assert!(parsed.unwrap_err().contains("no numeric id"));
  }

  #[test]
  fn the_frame_holds_trimmed_rows_and_one_run_per_style_change() {
    let mut buffer = Buffer::empty(Rect::new(0, 0, 6, 1));
    buffer.set_string(0, 0, "ab", Style::default().fg(Color::Red));
    buffer.set_string(2, 0, "界", Style::default().add_modifier(Modifier::BOLD));
    let snapshot = Snapshot { id: 1, error: None };
    let frame = frame_json(&snapshot, "Home", &buffer);
    assert_eq!(frame["rows"], json!(["ab界"]));
    assert_eq!(
      frame["runs"][0],
      json!([
        [0, 2, "Red", "Reset", 0, "ab"],
        [2, 2, "Reset", "Reset", 1, "界"],
        [4, 2, "Reset", "Reset", 0, "  "]
      ])
    );
  }
}
