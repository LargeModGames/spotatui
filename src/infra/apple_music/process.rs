//! A bounded child runner. No shell, process-name lookup, or process-group kill.
use super::CommandError;
use anyhow::{bail, Context, Result};
use std::{process::Stdio, time::Duration};
use tokio::{io::AsyncReadExt, process::Command};

const OUTPUT_LIMIT: u64 = 2 * 1024 * 1024;

/// The Apple Event error number in the helper's stderr, e.g. `-1728` from
/// "... (-1728)". Only the number is kept: the rest can quote library names.
fn apple_error_code(stderr: &str) -> Option<&str> {
  stderr.rsplit('(').find_map(|part| {
    let code = part.split(')').next()?;
    let digits = code.strip_prefix('-')?;
    (!digits.is_empty() && digits.len() <= 6 && digits.bytes().all(|b| b.is_ascii_digit()))
      .then_some(code)
  })
}

/// Run `command` with a deadline. `label` names the operation in errors (an
/// op name such as `playlists`, never an argument).
pub(super) async fn run(mut command: Command, deadline: Duration, label: &str) -> Result<String> {
  command
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .kill_on_drop(true);
  let mut child = command.spawn().map_err(CommandError::HelperSpawn)?;
  let mut stdout = child
    .stdout
    .take()
    .context("Missing helper stdout")?
    .take(OUTPUT_LIMIT + 1);
  let mut stderr = child
    .stderr
    .take()
    .context("Missing helper stderr")?
    .take(16_385);
  let mut out = Vec::new();
  let mut err = Vec::new();
  let result = tokio::time::timeout(deadline, async {
    tokio::try_join!(
      child.wait(),
      stdout.read_to_end(&mut out),
      stderr.read_to_end(&mut err)
    )
  })
  .await;
  let status = match result {
    Ok(Ok((status, _, _))) => status,
    other => {
      // This handle is the exact process we spawned; wait reaps that child.
      let _ = child.kill().await;
      let _ = child.wait().await;
      return match other {
        Err(_) => Err(anyhow::anyhow!(
          "Music helper timed out ({label}); Music may be busy, or macOS Automation permission may be missing"
        )),
        Ok(Err(error)) => Err(error.into()),
        Ok(Ok(_)) => unreachable!(),
      };
    }
  };
  if !status.success() {
    // Do not log library strings, script arguments, or arbitrary stderr.
    let stderr = String::from_utf8_lossy(&err);
    let code = apple_error_code(&stderr);
    if code == Some("-1743") {
      return Err(CommandError::AutomationDenied.into());
    }
    let code = code.map_or_else(String::new, |code| format!(", error {code}"));
    bail!("Music helper failed on `{label}` ({status}{code}); check that the item is available in Music and Automation permission is enabled");
  }
  if out.len() as u64 > OUTPUT_LIMIT {
    bail!("Music response exceeded the size limit");
  }
  String::from_utf8(out).context("Music returned invalid UTF-8")
}

/// A readiness probe failed or ran out of time before the real command was
/// sent, so Music never got that command. A typed error keeps its type.
fn not_delivered(error: anyhow::Error) -> anyhow::Error {
  if error.is::<CommandError>() {
    error
  } else {
    CommandError::LaunchFailed(error).into()
  }
}

/// `open` can return before Music answers. Give readiness its own bounded
/// window, then give the requested command its full timeout. Readiness never
/// retries a mutating command whose result could be delayed.
pub(super) async fn answer_after_launch<R, Ready, A, Answer>(
  wait: Duration,
  retry: Duration,
  command_timeout: Duration,
  mut ready: R,
  answer: A,
) -> Result<String>
where
  R: FnMut(Duration) -> Ready,
  Ready: std::future::Future<Output = Result<bool>>,
  A: FnOnce(Duration) -> Answer,
  Answer: std::future::Future<Output = Result<String>>,
{
  let deadline = tokio::time::Instant::now() + wait;
  loop {
    let left = deadline.saturating_duration_since(tokio::time::Instant::now());
    if left.is_zero() {
      return Err(CommandError::LaunchFailed(anyhow::anyhow!("Music did not start")).into());
    }
    if ready(left).await.map_err(not_delivered)? {
      return answer(command_timeout).await;
    }
    let left = deadline.saturating_duration_since(tokio::time::Instant::now());
    tokio::time::sleep(retry.min(left)).await;
  }
}

#[cfg(all(test, unix))]
mod tests {
  use super::*;
  use std::sync::atomic::{AtomicUsize, Ordering};

  #[tokio::test]
  async fn a_launched_music_is_asked_again_until_it_answers() {
    let asked = AtomicUsize::new(0);
    let output = answer_after_launch(
      Duration::from_secs(2),
      Duration::from_millis(10),
      Duration::from_secs(8),
      |_| {
        let n = asked.fetch_add(1, Ordering::SeqCst);
        async move {
          // Snapshot uses running:false, not the not_running launch hint.
          let json = if n < 3 {
            r#"{"running":false,"playing":false,"track":null,"position":0,"volume":0}"#
          } else {
            r#"{"running":true,"playing":false,"track":null,"position":0,"volume":50}"#
          };
          Ok(crate::infra::apple_music::parse_snapshot(json)?.running)
        }
      },
      |_| async { Ok("command answered".into()) },
    )
    .await
    .unwrap();
    assert_eq!(output, "command answered");
    assert_eq!(asked.load(Ordering::SeqCst), 4);
  }

  #[tokio::test]
  async fn a_music_that_never_answers_fails_without_sending_the_command() {
    let start = std::time::Instant::now();
    let error = answer_after_launch(
      Duration::from_millis(200),
      Duration::from_millis(20),
      Duration::from_secs(8),
      |left| {
        assert!(left <= Duration::from_millis(200));
        async { Ok(false) }
      },
      |_| async { panic!("a command must not be sent before Music is ready") },
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("did not start"));
    // The real command was never sent: the claim may be released.
    assert!(crate::infra::apple_music::event_not_delivered(&error));
    assert!(start.elapsed() < Duration::from_secs(1));
  }

  #[tokio::test]
  async fn a_failed_readiness_probe_counts_as_not_delivered() {
    let probe_failed = answer_after_launch(
      Duration::from_secs(1),
      Duration::from_millis(10),
      Duration::from_secs(8),
      |_| async { Err(anyhow::anyhow!("probe failed")) },
      |_| async { panic!("a command must not be sent after a failed probe") },
    )
    .await
    .unwrap_err();
    assert!(crate::infra::apple_music::event_not_delivered(
      &probe_failed
    ));

    // A typed probe failure keeps its own type (and message).
    let denied = answer_after_launch(
      Duration::from_secs(1),
      Duration::from_millis(10),
      Duration::from_secs(8),
      |_| async { Err(CommandError::AutomationDenied.into()) },
      |_| async { panic!("a command must not be sent after a denied probe") },
    )
    .await
    .unwrap_err();
    assert!(matches!(
      denied.downcast_ref::<CommandError>(),
      Some(CommandError::AutomationDenied)
    ));
  }

  #[tokio::test]
  async fn a_slow_launch_leaves_the_real_command_its_full_timeout() {
    let full_timeout = Duration::from_secs(8);
    let output = answer_after_launch(
      Duration::from_millis(200),
      Duration::from_millis(10),
      full_timeout,
      |_| async {
        tokio::time::sleep(Duration::from_millis(150)).await;
        Ok(true)
      },
      |timeout| async move {
        assert_eq!(timeout, full_timeout);
        // The command finishes beyond the readiness window and still succeeds.
        tokio::time::timeout(timeout, tokio::time::sleep(Duration::from_millis(100))).await?;
        Ok("played".into())
      },
    )
    .await
    .unwrap();
    assert_eq!(output, "played");
  }

  #[tokio::test]
  async fn apple_music_readiness_failure_preserves_its_type_and_never_sends_play() {
    let error = answer_after_launch(
      Duration::from_secs(1),
      Duration::from_millis(10),
      Duration::from_secs(8),
      |_| async { Err(CommandError::AutomationDenied.into()) },
      |_| async { panic!("permission denial must not send the real command") },
    )
    .await
    .unwrap_err();
    assert!(matches!(
      error.downcast_ref::<CommandError>(),
      Some(CommandError::AutomationDenied)
    ));
  }

  #[tokio::test]
  async fn apple_music_helper_timeout_kills_and_reaps_only_its_child() {
    let mut unrelated = Command::new("sleep")
      .arg("20")
      .kill_on_drop(true)
      .spawn()
      .unwrap();
    let mut command = Command::new("sleep");
    command.arg("20");
    let start = std::time::Instant::now();
    let error = run(command, Duration::from_millis(30), "sleep")
      .await
      .unwrap_err();
    let unrelated_alive = unrelated.try_wait().unwrap().is_none();
    unrelated.kill().await.unwrap();
    unrelated.wait().await.unwrap();
    assert!(unrelated_alive);
    assert!(error.to_string().contains("timed out (sleep)"));
    assert!(!crate::infra::apple_music::event_not_delivered(&error));
    assert!(start.elapsed() < Duration::from_secs(3));
    let mut echo = Command::new("echo");
    echo.arg("still alive");
    assert_eq!(
      run(echo, Duration::from_secs(1), "echo").await.unwrap(),
      "still alive\n"
    );
  }

  #[test]
  fn apple_music_errors_keep_only_the_error_number() {
    assert_eq!(
      apple_error_code("execution error: Error: Can't get \"(-3) Mix\". (-1728)"),
      Some("-1728")
    );
    assert_eq!(apple_error_code("Not authorized (-1743)\n"), Some("-1743"));
    assert_eq!(apple_error_code("no code here (abc)"), None);
  }

  #[tokio::test]
  async fn apple_music_failure_names_the_operation_and_code_but_no_stderr_text() {
    let mut command = Command::new("sh");
    command.args(["-c", "echo 'secret playlist name (-1728)' >&2; exit 1"]);
    let error = run(command, Duration::from_secs(1), "playlists")
      .await
      .unwrap_err();
    assert!(!crate::infra::apple_music::event_not_delivered(&error));
    assert!(error.to_string().contains("`playlists`"));
    assert!(error.to_string().contains("error -1728"));
    assert!(!error.to_string().contains("secret"));
  }

  #[tokio::test]
  async fn apple_music_permission_denial_is_a_typed_undelivered_event() {
    let mut command = Command::new("sh");
    command.args(["-c", "echo 'secret playlist name (-1743)' >&2; exit 1"]);
    let error = run(command, Duration::from_secs(1), "play")
      .await
      .unwrap_err();
    assert!(matches!(
      error.downcast_ref::<CommandError>(),
      Some(CommandError::AutomationDenied)
    ));
    assert!(crate::infra::apple_music::event_not_delivered(&error));
    assert!(!error.to_string().contains("secret"));
  }

  #[tokio::test]
  async fn apple_music_helper_spawn_failure_is_a_typed_undelivered_event() {
    let dir = tempfile::tempdir().unwrap();
    for label in ["launch", "play"] {
      let command = Command::new(dir.path().join("missing-helper"));
      let error = run(command, Duration::from_secs(1), label)
        .await
        .unwrap_err();
      assert!(matches!(
        error.downcast_ref::<CommandError>(),
        Some(CommandError::HelperSpawn(_))
      ));
      assert!(crate::infra::apple_music::event_not_delivered(&error));
    }
  }
}
