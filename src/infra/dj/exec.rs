//! How a DJ tool call actually reaches the player.
//!
//! Shared by both front doors, which is why it lives here rather than under
//! `infra::mcp`: the MCP server drives it for a remote agent, and the in-TUI DJ's
//! agent loop drives it for the local brain. Anything that needs no Spotify
//! client is answered inline; the rest goes down the Spotify lane and waits on a
//! `oneshot`, because only the pump's `Network` has the real Spotify client.

use super::tools::{self, DjToolCall, ToolOutcome};
use crate::core::app::App;
use crate::infra::network::{IoEvent, Network};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};

/// How long to wait for the Spotify lane to answer a network-backed tool call.
///
/// The lane can be busy behind a library crawl, but a caller is waiting
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

  async fn announce(&self, outcome: &ToolOutcome) {
    let mut app = self.app.lock().await;
    // Picked here, not when the call started: the language may have changed.
    let message = outcome.display_text(app.language()).to_string();
    let strings = app.strings();
    app.set_status_message(strings.status.infra.mcp_status(&message), 5);
  }
}

impl ToolExecutor for AppExecutor {
  async fn call(&self, call: DjToolCall) -> ToolOutcome {
    let spec = tools::spec(call.tool_name());
    let announce = self.announce && !spec.is_some_and(|spec| spec.read_only);
    // The tool table says which tools can need the network at all; a call of
    // one may still not (other sources' URIs), which a test asserts.
    debug_assert!(
      spec.is_some_and(|spec| spec.needs_network || !call.needs_network()),
      "a call needs the network that its tool table entry rules out"
    );

    // Anything that needs no Spotify client is answered here and now.
    if !call.needs_network() {
      let outcome = match tools::execute_app_only(&self.app, &call).await {
        Some(outcome) => outcome,
        None => Network::answer_dj_tool_call_without_spotify(&self.app, call).await,
      };
      if announce && !outcome.is_error {
        self.announce(&outcome).await;
      }
      return outcome;
    }

    // The rest need the real Spotify client, which only exists on the Spotify
    // lane — the service lane builds a `Network` with `None` for it.
    let (tx, rx) = oneshot::channel();
    // Sent straight down the channel rather than through `App::dispatch`, which
    // would set the global `is_loading` spinner for the duration of the call.
    if self
      .io_tx
      .send(IoEvent::DjToolCall(Box::new((call, tx))))
      .is_err()
    {
      return ToolOutcome::error_localized(|strings| strings.status.tools.shutting_down);
    }

    let outcome = match tokio::time::timeout(TOOL_TIMEOUT, rx).await {
      Ok(Ok(outcome)) => outcome,
      Ok(Err(_)) => ToolOutcome::error_localized(|strings| strings.status.tools.dropped),
      Err(_) => ToolOutcome::error_localized(|strings| strings.status.tools.timed_out),
    };
    if announce && !outcome.is_error {
      self.announce(&outcome).await;
    }
    outcome
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn tool_announcements_localize_without_changing_protocol_text_or_user_braces() {
    use crate::core::i18n::Language;
    for language in [Language::En, Language::ZhHans, Language::ZhHant] {
      let mut config = crate::core::user_config::UserConfig::new();
      config.behavior.language = language;
      let (tx, _rx) = std::sync::mpsc::channel();
      let app = Arc::new(Mutex::new(App::new(tx.clone(), config, None)));
      let executor = AppExecutor::new(Arc::clone(&app), tx);
      let result = executor
        .call(DjToolCall::SetDjVibe {
          vibe: Some("{count} 夜曲".into()),
        })
        .await;
      assert!(result.text.starts_with("DJ vibe set to: {count} 夜曲."));
      let guard = app.lock().await;
      let status = guard.status_message().unwrap();
      assert!(status.contains("{count} 夜曲"));
      if language == Language::En {
        assert_eq!(result.display_text(language), result.text);
        assert_eq!(status, format!("MCP: {}", result.text));
      } else {
        assert!(!status.contains("DJ vibe set to"));
        assert!(status.contains(if language == Language::ZhHans {
          "DJ 氛围已设为"
        } else {
          "DJ 氛圍已設為"
        }));
      }
    }
  }

  #[tokio::test]
  async fn a_closed_pump_keeps_english_protocol_errors_and_chinese_display_errors() {
    use crate::core::i18n::Language;
    let mut config = crate::core::user_config::UserConfig::new();
    config.behavior.language = Language::ZhHant;
    let (tx, rx) = std::sync::mpsc::channel();
    drop(rx);
    let app = Arc::new(Mutex::new(App::new(tx.clone(), config, None)));
    let executor = AppExecutor::new(app, tx);
    let result = executor
      .call(DjToolCall::SearchTracks {
        query: "夜曲".into(),
        limit: 5,
      })
      .await;
    assert!(result.is_error);
    assert_eq!(
      result.text,
      "spotatui is shutting down; the request was not run"
    );
    assert_eq!(
      result.display_text(Language::ZhHant),
      "spotatui 正在關閉，未執行此請求"
    );
  }

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
