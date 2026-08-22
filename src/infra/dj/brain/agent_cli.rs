//! The agent-CLI brain: reuse a coding agent the user already has.
//!
//! `claude`, `codex`, `agy` (Antigravity, which replaced Google's Gemini CLI),
//! and friends all run headless and read a prompt from stdin or argv, so spotatui
//! can borrow whichever one the user already has installed and authenticated.
//! **No API key is involved** — the agent uses the subscription it is already
//! logged into.
//!
//! The command line is a **config field, not a hardcoded table**: the user
//! supplies argv, spotatui writes the prompt and reads stdout. Presets exist as
//! conveniences, but this is what makes the backend genuinely
//! provider-agnostic, and it means spotatui never has to track any CLI's flag
//! churn.
//!
//! Each step is a fresh subprocess with several seconds of startup cost, so a
//! multi-step turn is genuinely expensive here — which is why [`super::super::agent`]
//! caps the number of steps rather than letting a turn run as long as it likes.

use super::{parse_step, system_prompt, user_prompt, DjRequest, DjStep};
use anyhow::{anyhow, Context, Result};
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// How the prompt reaches the CLI.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PromptDelivery {
  /// Written to the child's stdin. Verified for `claude -p` and `codex exec -`.
  #[default]
  Stdin,
  /// Appended as the final argv entry, so the preceding flag receives it as its
  /// value. `agy` (and the legacy `gemini`) **require** this: `agy` ignores stdin
  /// entirely, and a prompt written there is silently dropped in favour of
  /// whatever the CLI decides to answer on its own.
  Arg,
}

impl PromptDelivery {
  pub fn from_config_str(value: &str) -> Option<Self> {
    match value.trim().to_ascii_lowercase().as_str() {
      "stdin" => Some(Self::Stdin),
      "arg" | "argv" | "lastarg" | "last-arg" => Some(Self::Arg),
      _ => None,
    }
  }

  pub fn to_config_str(self) -> &'static str {
    match self {
      Self::Stdin => "stdin",
      Self::Arg => "arg",
    }
  }
}

/// A known agent CLI: how to run it headless, and how to name a model.
///
/// Split into `base` + `delivery_argv` rather than one flat argv because the
/// model flag has to land **between** them. [`AgentCliBrain::run`] appends the
/// prompt as the last argv entry for [`PromptDelivery::Arg`], so a flag appended
/// to the tail would swallow the prompt: `agy -p --model X "<prompt>"` passes
/// `--model` as the value of `-p`. Verified argv shapes:
///
/// ```text
/// claude --model haiku -p                 (prompt on stdin)
/// codex exec --model gpt-5 -              (prompt on stdin, `-` is the positional)
/// agy --model gemini-3.6-flash-low -p "…" (prompt as the value of -p)
/// copilot --no-color --model … -p "…"     (prompt as the value of -p)
/// opencode run -m openai/gpt-5.4 "…"      (prompt as a bare trailing positional)
/// gemini -m … -p "…"                      (legacy)
/// ```
pub struct AgentPreset {
  /// Lookup key. Also the binary name, which is what detection stats on `PATH`.
  pub key: &'static str,
  /// argv before the model flag. `base[0]` is the program.
  pub base: &'static [&'static str],
  /// argv after the model flag: the flag or positional that carries the prompt.
  ///
  /// Empty for a CLI that takes the prompt as a bare trailing positional
  /// (`opencode run "…"`), since [`AgentCliBrain::run`] appends it either way.
  pub delivery_argv: &'static [&'static str],
  /// How this CLI names a model. `None` means it cannot be told.
  pub model_flag: Option<&'static str>,
  pub delivery: PromptDelivery,
}

impl AgentPreset {
  /// Full argv, with the model flag inserted before the delivery token.
  ///
  /// `None` reproduces byte-for-byte what the flat preset table produced before
  /// the model flag existed, which is what keeps an install that never chose a
  /// model on exactly its previous behaviour.
  pub fn argv(&self, model: Option<&str>) -> Vec<String> {
    let mut argv: Vec<String> = self.base.iter().map(|part| part.to_string()).collect();
    if let (Some(flag), Some(model)) = (self.model_flag, model) {
      argv.push(flag.to_string());
      argv.push(model.to_string());
    }
    argv.extend(self.delivery_argv.iter().map(|part| part.to_string()));
    argv
  }
}

/// Convenience presets. Verified against the installed CLIs' own `--help`.
pub const PRESETS: &[AgentPreset] = &[
  AgentPreset {
    key: "claude",
    base: &["claude"],
    delivery_argv: &["-p"],
    model_flag: Some("--model"),
    delivery: PromptDelivery::Stdin,
  },
  AgentPreset {
    key: "codex",
    base: &["codex", "exec"],
    delivery_argv: &["-"],
    model_flag: Some("--model"),
    delivery: PromptDelivery::Stdin,
  },
  // Antigravity, which replaced the Gemini CLI. `agy` IGNORES stdin: the prompt
  // has to be the value of `-p`, so `Arg` here is load-bearing, not a preference.
  AgentPreset {
    key: "agy",
    base: &["agy"],
    delivery_argv: &["-p"],
    model_flag: Some("--model"),
    delivery: PromptDelivery::Arg,
  },
  // `--no-color` keeps ANSI escapes out of the JSON. A plain completion needs no
  // `--allow-all-tools`, despite what the non-interactive help implies.
  AgentPreset {
    key: "copilot",
    base: &["copilot", "--no-color"],
    delivery_argv: &["-p"],
    model_flag: Some("--model"),
    delivery: PromptDelivery::Arg,
  },
  // The prompt is a trailing positional here, not a flag value, so nothing
  // carries it and `delivery_argv` is empty.
  AgentPreset {
    key: "opencode",
    base: &["opencode", "run"],
    delivery_argv: &[],
    model_flag: Some("-m"),
    delivery: PromptDelivery::Arg,
  },
  // Legacy. Google superseded it with `agy`; kept so an existing config that
  // names it keeps working.
  AgentPreset {
    key: "gemini",
    base: &["gemini"],
    delivery_argv: &["-p"],
    model_flag: Some("-m"),
    delivery: PromptDelivery::Arg,
  },
];

/// Look up a preset by the binary name, for config convenience.
pub fn preset(name: &str) -> Option<&'static AgentPreset> {
  PRESETS.iter().find(|preset| preset.key == name)
}

#[derive(Debug)]
pub struct AgentCliBrain {
  command: Vec<String>,
  prompt_via: PromptDelivery,
  timeout: Duration,
  cwd: PathBuf,
}

impl AgentCliBrain {
  /// `cwd` should be a scratch directory, **not** the user's current directory.
  ///
  /// Coding agents read `CLAUDE.md` / `AGENTS.md` and project files from their
  /// working directory. Pointed at a repository, the DJ gets a prompt polluted
  /// with codebase context and starts suggesting tracks about Rust.
  pub fn new(
    command: Vec<String>,
    prompt_via: PromptDelivery,
    timeout: Duration,
    cwd: PathBuf,
  ) -> Result<Self> {
    if command.is_empty() {
      return Err(anyhow!(
        "behavior.dj_agent_command is empty; set it to the agent CLI to run, e.g. [\"claude\", \"-p\"]"
      ));
    }
    Ok(Self {
      command,
      prompt_via,
      timeout,
      cwd,
    })
  }

  pub async fn step(&self, request: &DjRequest) -> Result<DjStep> {
    let prompt = format!("{}\n\n{}", system_prompt(), user_prompt(request));
    let stdout = self.run(&prompt).await?;
    parse_step(&stdout).with_context(|| {
      format!(
        "could not read a decision out of `{}` output",
        self.command.join(" ")
      )
    })
  }

  async fn run(&self, prompt: &str) -> Result<String> {
    // The scratch directory is created lazily; a missing cwd makes `spawn` fail
    // with a confusing ENOENT that looks like a missing binary.
    if let Err(e) = tokio::fs::create_dir_all(&self.cwd).await {
      log::debug!("DJ: could not create the agent scratch dir: {e}");
    }

    let (program, args) = self.command.split_first().expect("checked non-empty");
    let mut command = Command::new(program);
    command
      .args(args)
      .current_dir(&self.cwd)
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .kill_on_drop(true);

    match self.prompt_via {
      PromptDelivery::Stdin => {
        command.stdin(Stdio::piped());
      }
      PromptDelivery::Arg => {
        command.arg(prompt).stdin(Stdio::null());
      }
    }

    // The OS error and the cwd go in the message: `spawn` fails with the same
    // ENOENT for a missing binary and for a cwd it cannot enter (#478), and the
    // transcript shows only the top of the error chain.
    let mut child = command.spawn().map_err(|e| {
      anyhow!(
        "could not run `{program}` from `{}` ({e}). Is it installed and on PATH? (behavior.dj_agent_command)",
        self.cwd.display()
      )
    })?;

    let stdin = match self.prompt_via {
      PromptDelivery::Stdin => Some(
        child
          .stdin
          .take()
          .ok_or_else(|| anyhow!("could not open stdin for `{program}`"))?,
      ),
      PromptDelivery::Arg => None,
    };

    // The write and the wait have to run *together*. The prompt is not small —
    // it carries every tool schema plus this turn's tool results — and an agent
    // CLI starts printing to stdout immediately. Writing it all first deadlocks
    // once both pipes fill: we wait for the child to read, the child waits for
    // us to read. Worse, that hang is in `write_all`, before the timeout below
    // is ever armed, so the serial IoEvent pump stops for good.
    let feed = async {
      // Dropping stdin is what tells the CLI the prompt is complete; without it
      // the child waits for more input and we hit the timeout instead.
      if let Some(mut stdin) = stdin {
        stdin.write_all(prompt.as_bytes()).await?;
        stdin.shutdown().await?;
      }
      Ok::<_, std::io::Error>(())
    };
    let feed_and_wait = async {
      let (written, output) = tokio::join!(feed, child.wait_with_output());
      written.with_context(|| format!("could not send the prompt to `{program}`"))?;
      output.with_context(|| format!("`{program}` failed to run"))
    };

    let output = match tokio::time::timeout(self.timeout, feed_and_wait).await {
      Ok(result) => result?,
      // `kill_on_drop` reaps the child when the future is dropped here.
      Err(_) => {
        return Err(anyhow!(
          "`{program}` did not answer within {}s",
          self.timeout.as_secs()
        ))
      }
    };

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    if !output.status.success() {
      let stderr = String::from_utf8_lossy(&output.stderr);
      // Some CLIs write a usable answer to stdout and still exit non-zero, so
      // only fail outright when there is nothing to read.
      if stdout.trim().is_empty() {
        return Err(anyhow!(
          "`{program}` exited with {}: {}",
          output.status,
          stderr.trim().chars().take(300).collect::<String>()
        ));
      }
      log::debug!("DJ: `{program}` exited non-zero but produced output; using it");
    }
    Ok(stdout)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn request() -> DjRequest {
    DjRequest {
      want: 2,
      ..DjRequest::default()
    }
  }

  /// Every agent stand-in these tests exec, keyed by name.
  ///
  /// **All of them are written before any test spawns anything, and that is the
  /// point** — it is not merely tidier than writing each on demand.
  ///
  /// `fork` duplicates every file descriptor open in the process at that instant.
  /// A test writing its stub while another test forks leaks a *writable* fd for
  /// that stub into the new child, which holds it for the sliver of time before its
  /// own `exec`. Exec'ing a file that any process holds open for writing fails with
  /// `ETXTBSY`, so the first test lost a coin flip roughly one run in five.
  ///
  /// Renaming the file into place does **not** fix this: the stray fd refers to the
  /// inode, which the rename carries along with it. Only ordering fixes it. Every
  /// test calls [`stub`] before it spawns, and the first call writes the whole set
  /// while the rest block on the `OnceLock`, so by the time any `fork` happens
  /// there is no writable fd left to inherit.
  ///
  /// The directory is per-process for the adjacent reason: a fixed path collides
  /// with a previous `cargo test` run still finishing.
  fn stubs() -> &'static std::collections::HashMap<&'static str, PathBuf> {
    static STUBS: std::sync::OnceLock<std::collections::HashMap<&'static str, PathBuf>> =
      std::sync::OnceLock::new();
    STUBS.get_or_init(|| {
      let dir = std::env::temp_dir().join(format!("spotatui-dj-stubs-{}", std::process::id()));
      std::fs::create_dir_all(&dir).unwrap();
      [
        (
          "fake-dj",
          r#"cat >/dev/null
echo 'Here you go: ```json'
echo '{"say":"Some mellow stuff.","tool_calls":[{"name":"queue_tracks","arguments":{"tracks":[{"title":"Weird Fishes","artist":"Radiohead"}]}}]}'
echo '```'"#,
        ),
        // Echoes back what it read, so a prompt that never arrived fails the test
        // rather than silently producing a generic answer.
        (
          "echo-dj",
          r#"PROMPT=$(cat)
case "$PROMPT" in
  *"Take the next step now"*) echo '{"say":"got it","tool_calls":[]}' ;;
  *) echo '{"say":"no prompt received","tool_calls":[]}' ;;
esac"#,
        ),
        (
          "arg-dj",
          r#"case "$*" in
  *"Take the next step now"*) echo '{"say":"via argv","tool_calls":[]}' ;;
  *) echo '{"say":"missing","tool_calls":[]}' ;;
esac"#,
        ),
        (
          "grumpy-dj",
          r#"cat >/dev/null
echo '{"say":"fine","tool_calls":[]}'
exit 3"#,
        ),
        (
          "broken-dj",
          r#"cat >/dev/null
echo 'not logged in' >&2
exit 1"#,
        ),
        ("chatty-dj", "cat >/dev/null\necho 'I would rather not.'"),
        // Fills its stdout pipe *before* it reads a byte of stdin, which is what
        // a real agent CLI does with its progress chatter. Feed it a prompt
        // larger than the pipe buffer and only a concurrent write survives.
        (
          "backpressure-dj",
          r#"i=0
while [ $i -lt 2000 ]; do
  echo 'still thinking .............................................................'
  i=$((i + 1))
done
PROMPT=$(cat)
case "$PROMPT" in
  *"Take the next step now"*) echo '{"say":"drained","tool_calls":[]}' ;;
  *) echo '{"say":"no prompt received","tool_calls":[]}' ;;
esac"#,
        ),
        ("slow-dj", "sleep 30"),
      ]
      .into_iter()
      .map(|(name, body)| {
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        #[cfg(unix)]
        {
          use std::os::unix::fs::PermissionsExt;
          std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        (name, path)
      })
      .collect()
    })
  }

  /// Path of one stand-in, as an argv entry.
  fn stub(name: &str) -> String {
    stubs()[name].to_string_lossy().to_string()
  }

  fn scratch() -> PathBuf {
    std::env::temp_dir().join("spotatui-dj-scratch")
  }

  fn brain(argv: Vec<String>, delivery: PromptDelivery) -> AgentCliBrain {
    AgentCliBrain::new(argv, delivery, Duration::from_secs(10), scratch()).unwrap()
  }

  #[test]
  fn an_empty_command_is_rejected_with_an_actionable_message() {
    let err = AgentCliBrain::new(
      vec![],
      PromptDelivery::Stdin,
      Duration::from_secs(1),
      scratch(),
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("dj_agent_command"));
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn reads_a_fenced_reply_from_stdout() {
    let command = stub("fake-dj");
    let reply = brain(vec![command.clone()], PromptDelivery::Stdin)
      .step(&request())
      .await
      .unwrap();
    assert_eq!(reply.say.as_deref(), Some("Some mellow stuff."));
    assert_eq!(reply.calls[0].name, "queue_tracks");
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn a_prior_tool_result_reaches_the_next_step() {
    // The loop's whole premise: what a tool returned has to be in the next
    // prompt, or the DJ cannot act on what it just learned.
    let command = stub("echo-dj");
    let request = DjRequest {
      want: 2,
      scratch: vec![super::super::ToolExchange {
        name: "search_tracks".into(),
        arguments: serde_json::json!({"query": "nude radiohead"}),
        result: "Nude — Radiohead [spotify:track:abc] [new]".into(),
      }],
      ..DjRequest::default()
    };
    let prompt = user_prompt(&request);
    assert!(prompt.contains("search_tracks"), "{prompt}");
    assert!(prompt.contains("spotify:track:abc"), "{prompt}");
    // And the stub still answers, so the enlarged prompt is still deliverable.
    let reply = brain(vec![command], PromptDelivery::Stdin)
      .step(&request)
      .await
      .unwrap();
    assert_eq!(reply.say.as_deref(), Some("got it"));
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn stdin_delivery_actually_sends_the_prompt() {
    // Echoes back what it read, so a prompt that never arrived would fail here
    // rather than silently producing a generic answer.
    let command = stub("echo-dj");
    let reply = brain(vec![command.clone()], PromptDelivery::Stdin)
      .step(&request())
      .await
      .unwrap();
    assert_eq!(reply.say.as_deref(), Some("got it"));
  }

  /// A prompt bigger than the pipe buffer, against a CLI that talks first.
  ///
  /// Writing the whole prompt before collecting the output deadlocks here, and the
  /// outer timeout is the point of the test: that hang lands in `write_all`,
  /// *before* the brain arms its own timeout, so a regression would otherwise stop
  /// the suite rather than fail it.
  #[cfg(unix)]
  #[tokio::test]
  async fn a_prompt_larger_than_the_pipe_buffer_does_not_deadlock() {
    let command = stub("backpressure-dj");
    let request = DjRequest {
      want: 2,
      scratch: vec![super::super::ToolExchange {
        name: "search_tracks".into(),
        arguments: serde_json::json!({"query": "everything"}),
        result: "Nude — Radiohead [spotify:track:abc] [new]\n".repeat(4000),
      }],
      ..DjRequest::default()
    };
    let prompt = format!("{}\n\n{}", system_prompt(), user_prompt(&request));
    assert!(
      prompt.len() > 64 * 1024,
      "the prompt must exceed the pipe buffer to exercise this at all, was {}",
      prompt.len()
    );

    let reply = tokio::time::timeout(
      Duration::from_secs(30),
      brain(vec![command], PromptDelivery::Stdin).step(&request),
    )
    .await
    .expect("deadlocked: the prompt write must run alongside the output read")
    .unwrap();
    assert_eq!(reply.say.as_deref(), Some("drained"));
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn arg_delivery_passes_the_prompt_as_the_final_argument() {
    let command = stub("arg-dj");
    let reply = brain(vec![command.clone()], PromptDelivery::Arg)
      .step(&request())
      .await
      .unwrap();
    assert_eq!(reply.say.as_deref(), Some("via argv"));
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn a_nonzero_exit_with_usable_output_is_still_accepted() {
    let command = stub("grumpy-dj");
    let reply = brain(vec![command.clone()], PromptDelivery::Stdin)
      .step(&request())
      .await
      .unwrap();
    assert_eq!(reply.say.as_deref(), Some("fine"));
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn a_nonzero_exit_with_no_output_reports_stderr() {
    let command = stub("broken-dj");
    let err = brain(vec![command.clone()], PromptDelivery::Stdin)
      .step(&request())
      .await
      .unwrap_err()
      .to_string();
    assert!(err.contains("not logged in"), "{err}");
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn garbage_output_reports_which_command_produced_it() {
    let command = stub("chatty-dj");
    let err = brain(vec![command.clone()], PromptDelivery::Stdin)
      .step(&request())
      .await
      .unwrap_err()
      .to_string();
    assert!(err.contains("chatty-dj"), "{err}");
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn a_hanging_cli_hits_the_timeout_instead_of_blocking_forever() {
    let command = stub("slow-dj");
    let brain = AgentCliBrain::new(
      vec![command.clone()],
      PromptDelivery::Arg,
      Duration::from_millis(150),
      scratch(),
    )
    .unwrap();
    let err = brain.step(&request()).await.unwrap_err().to_string();
    assert!(err.contains("did not answer within"), "{err}");
  }

  #[tokio::test]
  async fn a_missing_binary_says_so_rather_than_failing_obscurely() {
    let err = brain(
      vec!["spotatui-definitely-not-a-real-binary".to_string()],
      PromptDelivery::Stdin,
    )
    .step(&request())
    .await
    .unwrap_err()
    .to_string();
    assert!(err.contains("Is it installed"), "{err}");
    // The cwd and the OS error are part of the diagnostic (#478): a cwd that
    // cannot be entered fails `spawn` with the same ENOENT as a missing binary.
    // The OS error sits in the parentheses after the cwd; its wording is the
    // platform's, so only the structure is pinned everywhere.
    assert!(
      err.contains(&format!("from `{}` (", scratch().display())),
      "{err}"
    );
    #[cfg(unix)]
    assert!(err.contains("No such file or directory"), "{err}");
  }

  #[test]
  fn presets_cover_the_installed_agents_and_round_trip() {
    for entry in PRESETS {
      let found = preset(entry.key).expect("every preset is findable by its key");
      // The key doubles as the binary name, which is what PATH detection stats.
      assert_eq!(found.base[0], entry.key);
      assert_eq!(
        PromptDelivery::from_config_str(entry.delivery.to_config_str()),
        Some(entry.delivery)
      );
    }
    assert!(preset("nonexistent-agent").is_none());
  }

  #[test]
  fn a_preset_with_no_model_produces_exactly_the_argv_it_did_before() {
    assert_eq!(preset("claude").unwrap().argv(None), vec!["claude", "-p"]);
    assert_eq!(
      preset("codex").unwrap().argv(None),
      vec!["codex", "exec", "-"]
    );
    assert_eq!(preset("gemini").unwrap().argv(None), vec!["gemini", "-p"]);
  }

  #[test]
  fn the_model_flag_lands_before_the_delivery_flag_for_arg_delivery() {
    // `run` appends the prompt *after* this argv, so `-p` has to stay last or
    // `--model` becomes the value of `-p` and the prompt is never delivered.
    assert_eq!(
      preset("agy").unwrap().argv(Some("gemini-3.6-flash-low")),
      vec!["agy", "--model", "gemini-3.6-flash-low", "-p"]
    );
  }

  #[test]
  fn the_model_flag_lands_before_the_trailing_stdin_positional() {
    assert_eq!(
      preset("codex").unwrap().argv(Some("gpt-5")),
      vec!["codex", "exec", "--model", "gpt-5", "-"]
    );
  }

  #[test]
  fn the_opencode_preset_appends_the_prompt_as_a_bare_positional() {
    // `delivery_argv` is empty here, so nothing may be left dangling after the
    // model flag for `run` to read as a value.
    assert_eq!(
      preset("opencode").unwrap().argv(Some("openai/gpt-5.4")),
      vec!["opencode", "run", "-m", "openai/gpt-5.4"]
    );
    assert_eq!(
      preset("opencode").unwrap().argv(None),
      vec!["opencode", "run"]
    );
  }

  #[test]
  fn the_copilot_preset_keeps_p_last_so_it_receives_the_prompt() {
    assert_eq!(
      preset("copilot").unwrap().argv(Some("claude-sonnet-4.5")),
      vec![
        "copilot",
        "--no-color",
        "--model",
        "claude-sonnet-4.5",
        "-p"
      ]
    );
  }

  #[test]
  fn the_agy_preset_delivers_the_prompt_as_an_argument() {
    // Not a preference: `agy` reads nothing from stdin, so `Stdin` here would make
    // it answer a question nobody asked.
    assert_eq!(preset("agy").unwrap().delivery, PromptDelivery::Arg);
  }

  #[test]
  fn prompt_delivery_parses_the_forms_a_user_might_write() {
    assert_eq!(
      PromptDelivery::from_config_str("STDIN"),
      Some(PromptDelivery::Stdin)
    );
    assert_eq!(
      PromptDelivery::from_config_str(" arg "),
      Some(PromptDelivery::Arg)
    );
    assert_eq!(PromptDelivery::from_config_str("carrier pigeon"), None);
  }
}
