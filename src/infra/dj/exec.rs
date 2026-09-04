//! How a DJ tool call actually reaches the player.
//!
//! Shared by both front doors, which is why it lives here rather than under
//! `infra::mcp`: the MCP server drives it for a remote agent, and the in-TUI DJ's
//! agent loop drives it for the local brain. Anything that only needs the `App`
//! lock is answered inline; the rest goes down the serial IoEvent lane and waits
//! on a `oneshot`, because only that lane has the real Spotify client.

use super::tools::{self, DjToolCall, ToolOutcome};
use crate::core::app::App;
use crate::infra::network::IoEvent;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};

/// How long to wait for the IoEvent pump to answer a network-backed tool call.
///
/// The serial lane can be busy behind other Spotify work, but a caller is waiting
/// on the other end, so this bounds it rather than hanging them.
const TOOL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);

pub trait ToolExecutor {
  async fn call(&self, call: DjToolCall) -> ToolOutcome;
}

/// Executes against the live player.
pub struct AppExecutor {
  app: Arc<Mutex<App>>,
  io_tx: Sender<IoEvent>,
  /// Whether a mutating call raises a status-bar toast.
  ///
  /// On for MCP, where an agent is changing the player from outside and the user
  /// should see it happen without reading a log. Off for the in-TUI DJ, whose
  /// transcript already shows every call it made.
  announce: bool,
}

impl AppExecutor {
  /// For the MCP front door: mutating calls are announced in the status bar.
  #[cfg_attr(not(feature = "mcp-server"), allow(dead_code))]
  pub fn new(app: Arc<Mutex<App>>, io_tx: Sender<IoEvent>) -> Self {
    Self {
      app,
      io_tx,
      announce: true,
    }
  }

  /// For the in-TUI DJ, which reports its own tool calls in the transcript.
  #[cfg_attr(not(feature = "ai-dj"), allow(dead_code))]
  pub fn silent(app: Arc<Mutex<App>>, io_tx: Sender<IoEvent>) -> Self {
    Self {
      app,
      io_tx,
      announce: false,
    }
  }

  async fn announce(&self, message: String) {
    let mut app = self.app.lock().await;
    app.set_status_message(format!("MCP: {message}"), 5);
  }
}

impl ToolExecutor for AppExecutor {
  async fn call(&self, call: DjToolCall) -> ToolOutcome {
    let spec = tools::spec(call.tool_name());
    let announce = self.announce && !spec.is_some_and(|spec| spec.read_only);
    // The tool table is the single source of truth for which lane a call needs;
    // `DjToolCall::needs_network` must agree with it, which a test asserts.
    debug_assert_eq!(
      spec.map(|spec| spec.needs_network),
      Some(call.needs_network()),
      "tool table and DjToolCall disagree about needing the network"
    );

    // Anything that only needs `App` is answered here and now.
    if !call.needs_network() {
      if let Some(outcome) = tools::execute_app_only(&self.app, &call).await {
        if announce && !outcome.is_error {
          self.announce(outcome.text.clone()).await;
        }
        return outcome;
      }
    }

    // The rest need the real Spotify client, which only exists on the serial
    // IoEvent lane — the service lane builds a `Network` with `None` for it.
    let (tx, rx) = oneshot::channel();
    // Sent straight down the channel rather than through `App::dispatch`, which
    // would set the global `is_loading` spinner for the duration of the call.
    if self
      .io_tx
      .send(IoEvent::DjToolCall(Box::new((call, tx))))
      .is_err()
    {
      return ToolOutcome::error("spotatui is shutting down; the request was not run");
    }

    let outcome = match tokio::time::timeout(TOOL_TIMEOUT, rx).await {
      Ok(Ok(outcome)) => outcome,
      Ok(Err(_)) => ToolOutcome::error("spotatui dropped the request before answering"),
      Err(_) => ToolOutcome::error(
        "spotatui did not answer in time; it may be busy. Try again in a moment.",
      ),
    };
    if announce && !outcome.is_error {
      self.announce(outcome.text.clone()).await;
    }
    outcome
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn app_executor_reports_a_closed_pump() {
    let (tx, rx) = std::sync::mpsc::channel();
    drop(rx);
    let app = Arc::new(Mutex::new(App::default()));
    let executor = AppExecutor::new(app, tx);
    // `search_tracks` needs the network, so it takes the IoEvent path.
    let outcome = executor
      .call(DjToolCall::SearchTracks {
        query: "nude".into(),
        limit: 5,
      })
      .await;
    assert!(outcome.is_error);
    assert!(outcome.text.contains("shutting down"));
  }

  #[tokio::test]
  async fn app_executor_answers_app_only_calls_without_the_pump() {
    let (tx, rx) = std::sync::mpsc::channel();
    drop(rx);
    let app = Arc::new(Mutex::new(App::default()));
    let executor = AppExecutor::new(app, tx);
    // Nothing is playing in a default App, but the call still succeeds — it
    // never touches the (closed) IoEvent channel.
    let outcome = executor.call(DjToolCall::GetNowPlaying).await;
    assert!(!outcome.is_error);
    assert!(outcome.text.contains("Nothing is playing"));
  }

  #[tokio::test]
  async fn the_silent_executor_leaves_the_status_bar_alone() {
    // The in-TUI DJ shows its own calls in the transcript, so an "MCP: …" toast
    // for its work would be both duplicated and wrongly attributed.
    let (tx, _rx) = std::sync::mpsc::channel();
    let app = Arc::new(Mutex::new(App::default()));
    let executor = AppExecutor::silent(Arc::clone(&app), tx);
    let outcome = executor
      .call(DjToolCall::SetDjVibe {
        vibe: Some("mellow".into()),
      })
      .await;
    assert!(!outcome.is_error);
    assert!(app.lock().await.status_message().is_none());
  }
}
