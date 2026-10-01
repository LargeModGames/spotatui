//! A bounded child runner. No shell, process-name lookup, or process-group kill.
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
  let mut child = command.spawn().context("Cannot start Music helper")?;
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
      bail!("Music automation was denied. Allow your terminal/spotatui to control Music in System Settings > Privacy & Security > Automation");
    }
    let code = code.map_or_else(String::new, |code| format!(", error {code}"));
    bail!("Music helper failed on `{label}` ({status}{code}); check that the item is available in Music and Automation permission is enabled");
  }
  if out.len() as u64 > OUTPUT_LIMIT {
    bail!("Music response exceeded the size limit");
  }
  String::from_utf8(out).context("Music returned invalid UTF-8")
}

#[cfg(all(test, unix))]
mod tests {
  use super::*;
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
      .unwrap_err()
      .to_string();
    assert!(error.contains("`playlists`"));
    assert!(error.contains("error -1728"));
    assert!(!error.contains("secret"));
  }
}
