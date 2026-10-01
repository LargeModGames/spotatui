//! The only production Apple boundary. No Music calls at import or startup.
use super::{process, Command};
use anyhow::Result;
use std::time::Duration;

const SCRIPT: &str = include_str!("music.js");
const TIMEOUT: Duration = Duration::from_secs(8);
/// How long a launched Music gets to answer its first command.
const LAUNCH_WAIT: Duration = Duration::from_secs(5);

pub(super) struct MacClient;

impl super::dispatch::Client for MacClient {
  async fn execute(&self, operation: Command) -> Result<String> {
    let args = operation.arguments()?;
    let invoke = || {
      let mut child = tokio::process::Command::new("/usr/bin/osascript");
      child
        .args(["-l", "JavaScript", "-e", SCRIPT, "--"])
        .args(&args);
      child
    };
    let label = args[0].as_str();
    let output = process::run(invoke(), TIMEOUT, label).await?;
    if serde_json::from_str::<serde_json::Value>(&output)?["not_running"] == true
      && operation.may_launch()
    {
      let mut open = tokio::process::Command::new("/usr/bin/open");
      // Launch hidden, without bringing Music or an existing window forward.
      open.args(["-g", "-j", "-b", "com.apple.Music"]);
      process::run(open, TIMEOUT, "launch").await?;
      return answer_after_launch(LAUNCH_WAIT, Duration::from_millis(500), |left| {
        process::run(invoke(), left.min(TIMEOUT), label)
      })
      .await;
    }
    Ok(output)
  }
}

/// `open` can return before Music answers: ask again until it does, within
/// `wait` in all, or a cold start fails, and the status reads would take it
/// for a quit. `ask` gets the time left.
async fn answer_after_launch<F, Fut>(wait: Duration, retry: Duration, mut ask: F) -> Result<String>
where
  F: FnMut(Duration) -> Fut,
  Fut: std::future::Future<Output = Result<String>>,
{
  let deadline = tokio::time::Instant::now() + wait;
  loop {
    let left = deadline.saturating_duration_since(tokio::time::Instant::now());
    if left.is_zero() {
      anyhow::bail!("Music did not start");
    }
    let output = ask(left).await?;
    if serde_json::from_str::<serde_json::Value>(&output)?["not_running"] != true {
      return Ok(output);
    }
    tokio::time::sleep(retry.min(left)).await;
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::sync::atomic::{AtomicUsize, Ordering};

  #[tokio::test]
  async fn a_launched_music_is_asked_again_until_it_answers() {
    let asked = AtomicUsize::new(0);
    let output = answer_after_launch(Duration::from_secs(2), Duration::from_millis(10), |_| {
      let n = asked.fetch_add(1, Ordering::SeqCst);
      async move {
        Ok(
          if n < 3 {
            r#"{"not_running":true}"#
          } else {
            r#"{"running":true}"#
          }
          .to_string(),
        )
      }
    })
    .await
    .unwrap();
    assert_eq!(output, r#"{"running":true}"#);
    assert_eq!(asked.load(Ordering::SeqCst), 4);
  }

  #[tokio::test]
  async fn a_music_that_never_answers_fails_within_the_wait() {
    let start = std::time::Instant::now();
    let error = answer_after_launch(
      Duration::from_millis(200),
      Duration::from_millis(20),
      |left| {
        assert!(left <= Duration::from_millis(200));
        async { Ok(r#"{"not_running":true}"#.to_string()) }
      },
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("did not start"));
    assert!(start.elapsed() < Duration::from_secs(1));
  }
}
