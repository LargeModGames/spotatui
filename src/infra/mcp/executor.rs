//! How an MCP tool call gets run.
//!
//! [`AppExecutor`] and the [`ToolExecutor`] trait live in
//! [`crate::infra::dj::exec`], because the in-TUI DJ's agent loop drives the same
//! tools against the same player. Only the no-player stand-in is MCP's own.

pub use crate::infra::dj::exec::{AppExecutor, ToolExecutor};

use crate::infra::dj::tools::{DjToolCall, ToolOutcome};

/// Stands in when there is no running player to talk to.
pub struct OfflineExecutor {
  reason: String,
}

impl OfflineExecutor {
  pub fn new(reason: impl Into<String>) -> Self {
    Self {
      reason: reason.into(),
    }
  }
}

impl ToolExecutor for OfflineExecutor {
  async fn call(&self, _call: DjToolCall) -> ToolOutcome {
    // An execution error rather than a protocol error: this is exactly the
    // "actionable feedback the model can act on" case, and the agent can tell
    // the user to start spotatui.
    ToolOutcome::error(format!(
      "spotatui is not available: {}. Start spotatui and enable \
       `behavior.mcp_enabled` in its config, then try again.",
      self.reason
    ))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn offline_executor_reports_an_actionable_error() {
    let executor = OfflineExecutor::new("no control socket found");
    let outcome = executor.call(DjToolCall::GetNowPlaying).await;
    assert!(outcome.is_error);
    assert!(outcome.text.contains("no control socket found"));
    // Tells the agent what the user has to do, not just that it failed.
    assert!(outcome.text.contains("mcp_enabled"));
  }
}
