//! The only production Apple boundary. No Music calls at import or startup.
use super::{process, Command};
use anyhow::Result;
use std::time::Duration;

const SCRIPT: &str = include_str!("music.js");
const TIMEOUT: Duration = Duration::from_secs(8);
/// How long a launched Music gets to report ready, before the real command.
const LAUNCH_WAIT: Duration = Duration::from_secs(5);

pub(super) struct MacClient;

impl super::dispatch::Client for MacClient {
  async fn execute(&self, operation: Command) -> Result<String> {
    let args = operation.arguments()?;
    let invoke = |args: &[String]| {
      let mut child = tokio::process::Command::new("/usr/bin/osascript");
      child
        .args(["-l", "JavaScript", "-e", SCRIPT, "--"])
        .args(args);
      child
    };
    let label = args[0].as_str();
    let output = process::run(invoke(&args), TIMEOUT, label).await?;
    if serde_json::from_str::<serde_json::Value>(&output)?["not_running"] == true
      && operation.may_launch()
    {
      let mut open = tokio::process::Command::new("/usr/bin/open");
      // Launch hidden, without bringing Music or an existing window forward.
      open.args(["-g", "-j", "-b", "com.apple.Music"]);
      // Even if `open` itself ran, a launch failure happens before the real
      // Music command is sent and must not retain a playback claim.
      process::run(open, TIMEOUT, "launch")
        .await
        .map_err(super::CommandError::LaunchFailed)?;
      let readiness_args = Command::Snapshot.arguments()?;
      return process::answer_after_launch(
        LAUNCH_WAIT,
        Duration::from_millis(500),
        TIMEOUT,
        |left| {
          let child = invoke(&readiness_args);
          async move {
            let output = process::run(child, left, "snapshot").await?;
            Ok(super::parse_snapshot(&output)?.running)
          }
        },
        |timeout| process::run(invoke(&args), timeout, label),
      )
      .await;
    }
    Ok(output)
  }
}
