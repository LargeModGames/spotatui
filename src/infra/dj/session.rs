//! Assembling a DJ turn: build the brain, build the brief, snapshot the context.
//!
//! This is the glue between config and [`super::agent`], which runs the turn. It
//! is used from the **service** lane — a brain call is slow (up to a couple of
//! minutes for an agent CLI) and touches only `App` plus its own HTTP client or
//! subprocess, so parking it on the serial pump would freeze every other event
//! behind it.
//!
//! Resolution deliberately does *not* happen here: it needs the real Spotify
//! client, which the service lane's `Network` does not have. The loop's tool
//! calls cross to the serial lane through `IoEvent::DjToolCall` instead.

use super::brain::{
  agent_cli::{AgentCliBrain, PromptDelivery},
  anthropic::AnthropicBrain,
  openai_compat::OpenAiCompatBrain,
  DjBrain, DjRequest, ToolExchange,
};
use super::brief::{self, TasteBrief};
use super::{DjLine, DjSpeaker};
use crate::core::app::App;
use crate::core::user_config::BehaviorConfig;
use crate::infra::history::RecapPeriod;
use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

/// Environment variable that wins over `behavior.dj_api_key` at request time.
///
/// Mirrors how the Subsonic password is handled: the config field exists for
/// convenience, but the env var is the recommended route because it never lands
/// on disk.
pub const API_KEY_ENV: &str = "SPOTATUI_DJ_API_KEY";

/// Where agent CLIs are run from.
///
/// **Not** the user's current directory. Coding agents read `CLAUDE.md` /
/// `AGENTS.md` and project files from their working directory, so an agent
/// launched inside a repository answers with that repository on its mind.
///
/// Resolved once per process, so every step of every turn runs from the same
/// directory and the fallback is reported once. The config dir is preferred; a
/// private subdirectory of the OS temp dir is the fallback (#478: a build
/// sandbox sets `HOME` to a path that cannot be created, and `spawn` with a
/// missing cwd fails with an ENOENT that looks like a missing binary).
fn agent_scratch_dir() -> PathBuf {
  static DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
  DIR
    .get_or_init(|| {
      scratch_dir_from(
        crate::core::user_config::default_app_config_dir().map(|dir| dir.join("dj-scratch")),
        &std::env::temp_dir(),
      )
    })
    .clone()
}

/// [`agent_scratch_dir`] with the inputs passed in rather than read.
///
/// The config scratch dir is used when it can be created, entered, and written.
/// Otherwise a fresh, uniquely named directory is made under `temp`. Never a
/// fixed name there: a shared temp root lets another local user plant that name
/// (or a symlink to a directory of theirs) and so choose the agent's working
/// directory, instruction files included. A relative temp dir is skipped, as
/// `paths.rs` skips a relative XDG value: it would resolve against the process
/// cwd, the one place an agent must not run. When nothing qualifies, the
/// preferred path is returned and `spawn` reports the cwd it could not enter.
fn scratch_dir_from(preferred: Option<PathBuf>, temp: &Path) -> PathBuf {
  if let Some(dir) = preferred.as_ref().filter(|dir| usable_dir(dir)) {
    return dir.clone();
  }
  match temp.is_absolute().then(|| fresh_temp_dir(temp)).flatten() {
    Some(dir) => {
      log::warn!(
        "DJ: could not use the scratch dir {preferred:?}; running agent CLIs from {}",
        dir.display()
      );
      dir
    }
    None => {
      log::warn!(
        "DJ: no usable scratch dir (config {preferred:?}, temp {}); agent CLIs will not start",
        temp.display()
      );
      preferred.unwrap_or_else(|| temp.join("dj-scratch"))
    }
  }
}

/// Whether `Command::current_dir` can enter `dir` and the agent can write in it.
///
/// `create_dir_all` alone is not enough: it succeeds on a directory that already
/// exists without search permission, and `spawn` then fails on the `chdir` with
/// the same misleading error as a missing cwd. Creating and removing a file
/// inside needs search and write on the directory, which is what the agent
/// needs too.
fn usable_dir(dir: &Path) -> bool {
  if std::fs::create_dir_all(dir).is_err() {
    return false;
  }
  let probe = dir.join(format!(".probe-{}", std::process::id()));
  let written = std::fs::write(&probe, b"").is_ok();
  let _ = std::fs::remove_file(&probe);
  written
}

/// A new, uniquely named, owner-only directory under `temp`, or `None` when
/// none could be made. A non-recursive create refuses an existing entry, a
/// symlink included, so the result is always one this process made. It is not
/// removed on exit: temp is the one location the platform clears on its own,
/// and the per-process log file already relies on that.
fn fresh_temp_dir(temp: &Path) -> Option<PathBuf> {
  // Owner-only from the start, whatever the umask: the agent's working
  // directory must not be readable or writable by other local users.
  #[cfg(unix)]
  let builder = {
    use std::os::unix::fs::DirBuilderExt;
    let mut builder = std::fs::DirBuilder::new();
    builder.mode(0o700);
    builder
  };
  #[cfg(not(unix))]
  let builder = std::fs::DirBuilder::new();
  let pid = std::process::id();
  (0..8).find_map(|attempt| {
    let nanos = std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)
      .map(|since| since.subsec_nanos())
      .unwrap_or(0);
    let dir = temp.join(format!("spotatui-dj-scratch-{pid}-{nanos}-{attempt}"));
    builder.create(&dir).ok().map(|()| dir)
  })
}

/// The API-key precedence rule: the env var wins, the config field is the
/// fallback, and a blank value from either counts as unset.
///
/// Blank has to mean unset on both sides, because `setup::anthropic_key_present`
/// reads them that way when it decides whether the picker shows a key as present.
/// A blank config key that got through here would be sent as an empty bearer
/// token, which fails less clearly than no credential at all.
///
/// Split out so the tests can drive both branches without calling `set_var`:
/// `setenv`/`getenv` are not thread-safe (which is why they are `unsafe` in edition
/// 2024), and every test in this binary shares one process. A test that exported
/// `SPOTATUI_DJ_API_KEY` for a moment raced every other thread reading it —
/// including the picker's own `detect_backends`.
fn resolve_api_key_with(env_key: Option<&str>, behavior: &BehaviorConfig) -> Option<String> {
  env_key
    .filter(|key| !key.trim().is_empty())
    .or_else(|| {
      behavior
        .dj_api_key
        .as_deref()
        .filter(|key| !key.trim().is_empty())
    })
    .map(str::to_string)
}

pub fn period_from_config(value: &str) -> RecapPeriod {
  match value {
    "7d" => RecapPeriod::SevenDays,
    "month" => RecapPeriod::Month,
    "year" => RecapPeriod::Year,
    "all" => RecapPeriod::All,
    // Validated on load, so anything else is already impossible; default rather
    // than panic if that ever changes.
    _ => RecapPeriod::ThirtyDays,
  }
}

/// Build the configured brain, or explain why it cannot be built.
pub fn build_brain(behavior: &BehaviorConfig) -> Result<DjBrain> {
  let from_env = std::env::var(API_KEY_ENV).ok();
  build_brain_with(from_env.as_deref(), behavior)
}

/// [`build_brain`] with the environment passed in rather than read.
///
/// Same reason as [`resolve_api_key_with`]: "no key anywhere" is a case the tests
/// have to be able to produce, and producing it by unsetting a process-global was
/// a data race against every other test thread.
fn build_brain_with(env_key: Option<&str>, behavior: &BehaviorConfig) -> Result<DjBrain> {
  match behavior.dj_backend.as_str() {
    "agent_cli" => {
      let (command, delivery) = resolve_agent_command(behavior);
      Ok(DjBrain::AgentCli(AgentCliBrain::new(
        command,
        delivery,
        Duration::from_secs(behavior.dj_agent_timeout_secs),
        agent_scratch_dir(),
      )?))
    }
    "anthropic" => {
      let key = resolve_api_key_with(env_key, behavior).unwrap_or_default();
      Ok(DjBrain::Anthropic(AnthropicBrain::new(
        key,
        behavior.dj_model.clone(),
        behavior.dj_base_url.clone(),
      )?))
    }
    "openai_compat" => Ok(DjBrain::OpenAiCompat(OpenAiCompatBrain::new(
      resolve_api_key_with(env_key, behavior),
      behavior.dj_model.clone(),
      behavior.dj_base_url.clone(),
    ))),
    other => Err(anyhow!(
      "unknown behavior.dj_backend '{other}'; expected agent_cli, anthropic, or openai_compat"
    )),
  }
}

/// Which preset, if any, this configured argv is "just the default shape" of.
///
/// Two shapes count. A bare binary name (`["codex"]`) is the documented
/// convenience. An argv byte-equal to what that preset produces with no model
/// (`["claude", "-p"]`) counts too, because that is the argv *spotatui shipped*
/// and then wrote to disk itself on the first unrelated `save_config` — it is not
/// something the user typed. Anything else is theirs, including its model flag.
///
/// This is what keeps the "an explicit multi-part command is never rewritten"
/// invariant honest while still letting a default install pick a model: with
/// `dj_agent_model` unset the resolved argv is unchanged either way.
fn expandable_preset(command: &[String]) -> Option<&'static super::brain::agent_cli::AgentPreset> {
  let first = command.first()?.trim();
  let preset = super::brain::agent_cli::preset(first)?;
  if command.len() == 1 || command == preset.argv(None).as_slice() {
    Some(preset)
  } else {
    None
  }
}

/// Does spotatui own this install's `dj_agent_command`?
///
/// The single answer to "is this argv ours to rewrite", shared by the three places
/// that would otherwise each invent their own and disagree:
///
/// - [`resolve_agent_command`] threads `dj_agent_model` in as a model flag only
///   when this is `true`;
/// - `setup::active_label` names that model in the DJ title only when this is
///   `true`, so the title cannot advertise a model the CLI never receives;
/// - `setup::detect_backends` gives a `false` command a picker row of its own,
///   because a hand-written argv has no preset row and Enter would otherwise
///   replace it with `["claude"]`.
///
/// A `false` here means the argv is the user's: it is passed through verbatim,
/// including whatever model flag they put in it themselves.
pub(crate) fn owns_agent_command(behavior: &BehaviorConfig) -> bool {
  expandable_preset(&behavior.dj_agent_command).is_some()
}

/// Expand a bare agent name to its known argv, so `dj_agent_command: ["codex"]`
/// just works instead of silently running `codex` with no subcommand, and thread
/// `dj_agent_model` through as that CLI's own model flag.
///
/// An explicit multi-part command is always honoured as written — the presets are
/// a convenience, never an override, and a hand-written argv keeps ownership of
/// its own flags including the model one.
fn resolve_agent_command(behavior: &BehaviorConfig) -> (Vec<String>, PromptDelivery) {
  let configured_delivery = behavior
    .dj_agent_prompt_via
    .as_deref()
    .and_then(PromptDelivery::from_config_str);
  if let Some(preset) = expandable_preset(&behavior.dj_agent_command) {
    let model = behavior
      .dj_agent_model
      .as_deref()
      .map(str::trim)
      .filter(|model| !model.is_empty());
    // The preset's delivery mode wins unless the user set one explicitly, since
    // the two have to agree for the CLI to receive the prompt at all.
    return (
      preset.argv(model),
      configured_delivery.unwrap_or(preset.delivery),
    );
  }
  (
    behavior.dj_agent_command.clone(),
    configured_delivery.unwrap_or_default(),
  )
}

/// Build the taste brief, off the async runtime and off the `App` lock.
pub async fn build_taste_brief(app: &Arc<Mutex<App>>, period: RecapPeriod) -> Result<TasteBrief> {
  // `load_listens` is blocking and reads the whole (unbounded, append-only) file.
  let listens = tokio::task::spawn_blocking(crate::infra::history::load_listens)
    .await
    .map_err(|e| anyhow!("listening-history task failed: {e}"))?
    .map_err(|e| anyhow!("could not read listening history: {e}"))?;

  let mut summary = brief::build_brief(&listens, period);
  {
    let app = app.lock().await;
    summary.now_playing = super::current_track_label(&app);
    summary.vibe = app.dj.vibe.clone();
  }
  Ok(summary)
}

/// Everything a brain call needs, snapshotted off the `App` lock.
pub struct TurnContext {
  pub brief: TasteBrief,
  pub history: Vec<(String, String)>,
  pub want: usize,
  /// Whether the listener has the avoid-library filter on. Reaches the prompt so
  /// the model aims away from their favourites, and reaches the resolve step so it
  /// enforces what the prompt asked for.
  pub avoid_library: bool,
}

impl TurnContext {
  /// Dedupe keys for the recently-played window, for the loop's queue policy.
  pub fn recent_keys(&self) -> Vec<String> {
    self.brief.recent_keys.clone()
  }
}

/// How many transcript lines the brain gets.
///
/// Sized for a back-and-forth, not a single request and answer: now that the DJ
/// can ask a question, the reply to it is worthless without the several turns of
/// preference-narrowing that led there.
const HISTORY_LINES: usize = 24;

/// Turn the transcript into `(speaker, text)` pairs for the brain, oldest last.
///
/// Pure and separate from [`turn_context`] so the mapping is unit-testable: the
/// async path pulls in the taste brief, which reads the user's real listening
/// history file.
///
/// `extra` is appended as a final `Listener` line. It is for instructions that are
/// deliberately *absent* from the transcript (the vibe shift); anything the
/// listener actually typed is already in `transcript` and must not be passed here,
/// or the brain sees it twice.
fn history_from_transcript(transcript: &[DjLine], extra: Option<&str>) -> Vec<(String, String)> {
  let mut history = transcript
    .iter()
    // Machine notes ("queued 5 tracks") are for the user, not the model.
    .filter(|line| line.speaker != DjSpeaker::System)
    .map(|line| {
      let speaker = match line.speaker {
        DjSpeaker::User => "Listener",
        _ => "DJ",
      };
      (speaker.to_string(), line.text.clone())
    })
    .rev()
    .take(HISTORY_LINES)
    .collect::<Vec<_>>();
  history.reverse();
  if let Some(line) = extra {
    history.push(("Listener".to_string(), line.to_string()));
  }
  history
}

/// Snapshot the conversation and settings for one turn.
pub async fn turn_context(
  app: &Arc<Mutex<App>>,
  extra_instruction: Option<&str>,
) -> Result<TurnContext> {
  let (period, want, history, avoid_library) = {
    let app = app.lock().await;
    let behavior = &app.user_config.behavior;
    (
      period_from_config(&behavior.dj_history_period),
      behavior.dj_batch_size,
      history_from_transcript(&app.dj.transcript, extra_instruction),
      // The live toggle, not the config default: the config only seeds it.
      app.dj.avoid_library,
    )
  };

  let brief = build_taste_brief(app, period).await?;
  Ok(TurnContext {
    brief,
    history,
    want,
    avoid_library,
  })
}

impl TurnContext {
  /// One step's request: the standing context, plus whatever the loop has run so
  /// far this turn.
  pub fn to_request(&self, scratch: Vec<ToolExchange>, must_act: bool) -> DjRequest {
    DjRequest {
      brief: self.brief.clone(),
      history: self.history.clone(),
      scratch,
      want: self.want,
      avoid_library: self.avoid_library,
      must_act,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::user_config::UserConfig;

  fn behavior(backend: &str) -> BehaviorConfig {
    let mut config = UserConfig::new();
    config.behavior.dj_backend = backend.to_string();
    config.behavior
  }

  #[test]
  fn builds_each_configured_backend() {
    assert!(matches!(
      build_brain(&behavior("agent_cli")).unwrap(),
      DjBrain::AgentCli(_)
    ));
    assert!(matches!(
      build_brain(&behavior("openai_compat")).unwrap(),
      DjBrain::OpenAiCompat(_)
    ));
  }

  #[test]
  fn anthropic_without_a_key_fails_with_the_env_var_named() {
    // `None` for the environment rather than `remove_var`: a machine that exports
    // the key would otherwise fail this for the wrong reason, and unsetting it for a
    // moment would race every other test thread reading the same variable.
    let err = build_brain_with(None, &behavior("anthropic"))
      .unwrap_err()
      .to_string();
    assert!(err.contains(API_KEY_ENV), "{err}");
  }

  #[test]
  fn an_unknown_backend_is_reported_with_the_valid_options() {
    let err = build_brain(&behavior("telepathy")).unwrap_err().to_string();
    assert!(err.contains("agent_cli"));
    assert!(err.contains("openai_compat"));
  }

  #[test]
  fn the_env_var_wins_over_the_config_field() {
    // The environment is a parameter here, not a global. `set_var` around a live
    // test binary is a data race against every other thread that reads it, and the
    // picker's own detection reads exactly this variable.
    let mut behavior = behavior("openai_compat");
    behavior.dj_api_key = Some("from-config".to_string());
    assert_eq!(
      resolve_api_key_with(Some("from-env"), &behavior).as_deref(),
      Some("from-env")
    );
  }

  #[test]
  fn a_blank_env_var_falls_back_to_the_config_field() {
    let mut behavior = behavior("openai_compat");
    behavior.dj_api_key = Some("from-config".to_string());
    assert_eq!(
      resolve_api_key_with(Some("   "), &behavior).as_deref(),
      Some("from-config"),
      "an exported-but-empty variable is not a key"
    );
    assert_eq!(
      resolve_api_key_with(None, &behavior).as_deref(),
      Some("from-config"),
      "and neither is an unset one"
    );
  }

  #[test]
  fn a_blank_config_key_is_no_key_at_all() {
    // The picker reads it that way, so this has to agree: a config key of spaces
    // showing as "not ready" while the session sent it as a bearer token is a
    // failure with no visible cause.
    let mut behavior = behavior("openai_compat");
    behavior.dj_api_key = Some("   ".to_string());
    assert_eq!(resolve_api_key_with(None, &behavior), None);
    assert_eq!(
      resolve_api_key_with(Some("from-env"), &behavior).as_deref(),
      Some("from-env")
    );
  }

  #[test]
  fn every_configured_period_string_maps_to_a_recap_period() {
    for (value, expected) in [
      ("7d", RecapPeriod::SevenDays),
      ("30d", RecapPeriod::ThirtyDays),
      ("month", RecapPeriod::Month),
      ("year", RecapPeriod::Year),
      ("all", RecapPeriod::All),
    ] {
      assert_eq!(period_from_config(value), expected);
    }
  }

  #[test]
  fn a_bare_agent_name_expands_to_its_preset_argv() {
    let mut behavior = behavior("agent_cli");
    behavior.dj_agent_command = vec!["codex".to_string()];
    let (argv, _) = resolve_agent_command(&behavior);
    assert_eq!(argv, vec!["codex", "exec", "-"]);
  }

  #[test]
  fn a_bare_agy_name_also_adopts_its_arg_delivery() {
    let mut behavior = behavior("agent_cli");
    behavior.dj_agent_command = vec!["agy".to_string()];
    // Unset, which is a state the real load path can now produce, so the preset's
    // own mode is adopted; `agy` reads the prompt from argv and ignores stdin, and
    // getting this wrong means it receives nothing at all.
    behavior.dj_agent_prompt_via = None;
    let (argv, delivery) = resolve_agent_command(&behavior);
    assert_eq!(argv, vec!["agy", "-p"]);
    assert_eq!(delivery, PromptDelivery::Arg);
  }

  #[test]
  fn an_explicit_prompt_delivery_still_overrides_the_preset() {
    let mut behavior = behavior("agent_cli");
    behavior.dj_agent_command = vec!["agy".to_string()];
    behavior.dj_agent_prompt_via = Some("stdin".to_string());
    let (_, delivery) = resolve_agent_command(&behavior);
    assert_eq!(delivery, PromptDelivery::Stdin);
  }

  #[test]
  fn an_explicit_command_is_never_rewritten_by_a_preset() {
    let mut behavior = behavior("agent_cli");
    behavior.dj_agent_command = vec!["claude".to_string(), "--custom".to_string()];
    let (argv, _) = resolve_agent_command(&behavior);
    assert_eq!(argv, vec!["claude", "--custom"]);
  }

  #[test]
  fn an_unknown_bare_name_is_left_alone() {
    let mut behavior = behavior("agent_cli");
    behavior.dj_agent_command = vec!["my-own-agent".to_string()];
    let (argv, _) = resolve_agent_command(&behavior);
    assert_eq!(argv, vec!["my-own-agent"]);
  }

  #[test]
  fn an_install_with_no_model_configured_resolves_exactly_the_argv_it_did_before() {
    // The guarantee: adding the model flag must change nothing for anyone who
    // never picked a model.
    let behavior = behavior("agent_cli");
    assert_eq!(behavior.dj_agent_model, None, "the shipped default");
    let (argv, delivery) = resolve_agent_command(&behavior);
    assert_eq!(argv, vec!["claude", "-p"]);
    assert_eq!(delivery, PromptDelivery::Stdin);
  }

  #[test]
  fn the_shipped_default_command_still_accepts_a_model_because_it_is_the_preset_shape() {
    // `["claude", "-p"]` is on disk in every existing install, written there by an
    // automatic `save_config` rather than typed, so it has to stay expandable.
    let mut behavior = behavior("agent_cli");
    behavior.dj_agent_model = Some("haiku".to_string());
    let (argv, _) = resolve_agent_command(&behavior);
    assert_eq!(argv, vec!["claude", "--model", "haiku", "-p"]);
  }

  #[test]
  fn ownership_is_exactly_which_commands_receive_a_model_flag() {
    // The picker and the DJ title both branch on `owns_agent_command`, so it has to
    // BE the rule `resolve_agent_command` follows rather than merely resemble it.
    // Both halves are asserted here for that reason: the helper's answer, and the
    // argv that answer is supposed to predict.
    for (command, owned) in [
      (vec!["claude"], true),
      (vec!["claude", "-p"], true),
      (vec!["claude", "--verbose", "-p"], false),
      (vec!["my-own-agent"], false),
    ] {
      let mut behavior = behavior("agent_cli");
      behavior.dj_agent_command = command.iter().map(|part| part.to_string()).collect();
      assert_eq!(owns_agent_command(&behavior), owned, "{command:?}");

      behavior.dj_agent_model = Some("haiku".to_string());
      let (argv, _) = resolve_agent_command(&behavior);
      assert_eq!(
        argv.contains(&"haiku".to_string()),
        owned,
        "{command:?} disagrees with the argv it resolves to"
      );
    }
  }

  #[test]
  fn a_hand_written_command_never_gains_a_model_flag() {
    let mut behavior = behavior("agent_cli");
    behavior.dj_agent_command = vec![
      "claude".to_string(),
      "--verbose".to_string(),
      "-p".to_string(),
    ];
    behavior.dj_agent_model = Some("haiku".to_string());
    let (argv, _) = resolve_agent_command(&behavior);
    assert_eq!(argv, vec!["claude", "--verbose", "-p"]);
  }

  #[test]
  fn a_typed_prompt_reaches_the_brain_exactly_once() {
    // Regression guard: the handler pushes the listener's line and the brain reads
    // it from the transcript. When `ask_dj` also pushed it and it was *also* passed
    // as an extra line, one request arrived three times over.
    let transcript = vec![DjLine::user("play something like Remind Me to Forget")];
    let history = history_from_transcript(&transcript, None);
    assert_eq!(
      history,
      vec![(
        "Listener".to_string(),
        "play something like Remind Me to Forget".to_string()
      )]
    );
  }

  #[test]
  fn machine_notes_are_kept_out_of_the_history() {
    let transcript = vec![
      DjLine::user("something chill"),
      DjLine::dj("Six downtempo cuts."),
      DjLine::system("queued 6"),
    ];
    let history = history_from_transcript(&transcript, None);
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].0, "Listener");
    assert_eq!(history[1].0, "DJ");
  }

  #[test]
  fn an_extra_instruction_lands_last_and_is_attributed_to_the_listener() {
    // The vibe shift's steer: never in the transcript, so it has to be appended.
    let transcript = vec![DjLine::user("something chill")];
    let history = history_from_transcript(&transcript, Some("Change direction."));
    assert_eq!(history.len(), 2);
    assert_eq!(
      history.last().unwrap(),
      &("Listener".to_string(), "Change direction.".to_string())
    );
  }

  #[test]
  fn the_history_window_keeps_the_newest_lines_oldest_first() {
    // Built off the constant, so raising the window does not silently stop this
    // testing a window at all.
    let total = HISTORY_LINES + 5;
    let transcript = (0..total)
      .map(|i| DjLine::user(format!("line {i}")))
      .collect::<Vec<_>>();
    let history = history_from_transcript(&transcript, None);
    assert_eq!(history.len(), HISTORY_LINES);
    assert_eq!(history.first().unwrap().1, "line 5", "oldest kept, first");
    assert_eq!(
      history.last().unwrap().1,
      format!("line {}", total - 1),
      "newest kept, last"
    );
  }

  #[test]
  fn each_step_carries_the_filter_flag_and_the_results_so_far() {
    let context = context_with_filter();
    let first = context.to_request(vec![], false);
    assert!(first.avoid_library, "the prompt has to know");
    assert!(first.scratch.is_empty(), "nothing has run yet");
    assert!(!first.must_act);

    // The context is reusable across steps — the loop calls it once per step, and
    // only the scratch grows.
    let second = context.to_request(
      vec![ToolExchange {
        name: "get_queue".into(),
        arguments: serde_json::json!({}),
        result: "empty".into(),
      }],
      true,
    );
    assert_eq!(second.scratch.len(), 1);
    assert!(second.must_act);
  }

  fn context_with_filter() -> TurnContext {
    TurnContext {
      brief: TasteBrief::default(),
      history: vec![],
      want: 6,
      avoid_library: true,
    }
  }

  #[test]
  fn the_agent_scratch_dir_is_not_the_current_directory() {
    // Regression guard for the polluted-context trap: an agent run in a repo
    // reads its CLAUDE.md and answers about code instead of music.
    let scratch = agent_scratch_dir();
    let cwd = std::env::current_dir().unwrap();
    assert_ne!(scratch, cwd);
    assert!(scratch.ends_with("dj-scratch"));
  }

  #[test]
  fn the_config_scratch_dir_is_used_when_it_can_be_created() {
    let temp = tempfile::tempdir().unwrap();
    let preferred = temp.path().join("config").join("dj-scratch");
    let chosen = scratch_dir_from(Some(preferred.clone()), temp.path());
    assert_eq!(chosen, preferred);
    assert!(chosen.is_dir(), "it is created, not only named");
  }

  #[test]
  fn an_uncreatable_config_dir_falls_back_to_a_fresh_private_temp_dir() {
    // #478: a build sandbox sets HOME to a path that cannot be created. The
    // agent must still get a directory of its own, never the shared temp root.
    let temp = tempfile::tempdir().unwrap();
    let blocker = temp.path().join("not-a-dir");
    std::fs::write(&blocker, b"").unwrap();
    let chosen = scratch_dir_from(Some(blocker.join("dj-scratch")), temp.path());
    assert!(chosen.is_dir(), "{}", chosen.display());
    assert_eq!(chosen.parent(), Some(temp.path()), "{}", chosen.display());
    // Fresh every time: never a fixed name another local user can plant.
    let again = scratch_dir_from(Some(blocker.join("dj-scratch")), temp.path());
    assert_ne!(again, chosen);
    // And private: group and others get nothing, whatever the umask.
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt;
      let mode = std::fs::metadata(&chosen).unwrap().permissions().mode();
      assert_eq!(mode & 0o077, 0, "mode {mode:o}");
    }
  }

  #[cfg(unix)]
  #[test]
  fn a_config_dir_without_search_permission_is_not_used() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let preferred = temp.path().join("dj-scratch");
    std::fs::create_dir(&preferred).unwrap();
    // Read and write, no search: `create_dir_all` is content, `chdir` is not.
    std::fs::set_permissions(&preferred, std::fs::Permissions::from_mode(0o600)).unwrap();
    if std::fs::write(preferred.join("probe"), b"").is_ok() {
      // A privileged user ignores mode bits; there is nothing to test here.
      return;
    }
    let chosen = scratch_dir_from(Some(preferred.clone()), temp.path());
    std::fs::set_permissions(&preferred, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert_ne!(chosen, preferred);
    assert!(chosen.is_dir(), "{}", chosen.display());
  }
}
