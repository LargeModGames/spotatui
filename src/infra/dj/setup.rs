//! "Which AI do you have, and which of its models?" — the data behind the DJ's
//! setup picker.
//!
//! Two cost problems hide behind the same words, and this module exists because
//! the fix is different for each. The `anthropic` backend bills an API key per
//! token, so the answer there is a cheaper model id. The default `agent_cli`
//! backend spends the user's Claude Pro/Max (or ChatGPT) subscription, where a
//! continuous auto-queue is a fresh prompt every few tracks and the heaviest model
//! exhausts a Pro plan in a handful of turns. Which backend is in use therefore
//! changes what "cheaper" even means, and the picker has to say so per row.
//!
//! **Nothing here spawns a process or makes a request.** Detection stats `PATH`;
//! see [`on_path`].

use crate::core::user_config::BehaviorConfig;
use std::path::{Path, PathBuf};

use super::brain::agent_cli;

/// Is `name` an executable file on any `PATH` entry?
///
/// Stat only, never a spawn. This runs on the render thread when the picker opens,
/// and asking a coding agent CLI to print its version costs hundreds of
/// milliseconds per binary; four of those would be a visible stall.
pub fn on_path(name: &str) -> bool {
  let Some(path) = std::env::var_os("PATH") else {
    return false;
  };
  binary_in_dirs(name, &std::env::split_paths(&path).collect::<Vec<_>>())
}

/// Split out from [`on_path`] so the tests can point at a directory they built,
/// rather than mutating the process environment (which races other tests).
pub(crate) fn binary_in_dirs(name: &str, dirs: &[PathBuf]) -> bool {
  dirs.iter().any(|dir| is_executable(&dir.join(name)))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
  use std::os::unix::fs::PermissionsExt;
  std::fs::metadata(path)
    .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
    .unwrap_or(false)
}

/// On Windows the extension is what makes a file runnable, and `PATHEXT` is the
/// list. `is_file()` on the bare name would miss `claude.cmd`, which is how npm
/// shims land.
#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
  if path.is_file() {
    return true;
  }
  let exts = std::env::var("PATHEXT").unwrap_or_else(|_| ".EXE;.CMD;.BAT;.COM".to_string());
  exts.split(';').any(|ext| {
    let mut candidate = path.as_os_str().to_owned();
    candidate.push(ext);
    Path::new(&candidate).is_file()
  })
}

/// One row of the picker's first step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendOption {
  /// The `behavior.dj_backend` value this row selects.
  pub backend: &'static str,
  /// For `agent_cli` rows, the preset key, which is also the binary name. `None`
  /// for the HTTP backends **and** for the one row below that carries the user's
  /// own command; [`BackendOption::command`] is what tells those two apart.
  pub agent: Option<&'static str>,
  /// The user's own `dj_agent_command`, on the single row that shows it back to
  /// them, and `None` on every row spotatui owns.
  ///
  /// This is the explicit discriminator rather than a third meaning smuggled into
  /// `agent`: `agent: None` has always meant "an HTTP backend" and both
  /// [`models_for`] and [`apply_choice`] branch on it, so an unhandled third state
  /// there would write `dj_model` for a CLI. `Some` means "an agent CLI whose argv
  /// is the user's, and which this picker must not rewrite".
  pub command: Option<Vec<String>>,
  /// What the row is called.
  pub label: String,
  /// What it bills against, or what is still missing. The whole reason the picker
  /// exists is that "rides your Pro plan" and "$5 per MTok" are not
  /// interchangeable, so this is never empty.
  pub note: String,
  /// Usable right now with no further setup: the binary is on `PATH`, or the key is
  /// present. A `false` row is still selectable, because "I am about to install it"
  /// is a real answer.
  pub ready: bool,
}

impl BackendOption {
  /// Does this row run an agent CLI, whether one of spotatui's presets or the
  /// user's own command?
  ///
  /// The question every "which config field does this row write" decision actually
  /// asks. `agent.is_some()` alone answers it wrongly for the user's own command,
  /// which would send it down the HTTP branch and write `dj_model`.
  fn is_agent(&self) -> bool {
    self.agent.is_some() || self.command.is_some()
  }
}

/// Agent CLIs the picker offers even when they are not installed, so a user can
/// see what spotatui supports.
///
/// `gemini` is deliberately absent: Google superseded the Gemini CLI with
/// Antigravity's `agy`. It stays in [`agent_cli::PRESETS`] so an existing config
/// keeps working, and detection still offers it when the binary is actually
/// installed, but spotatui does not advertise it any more.
const RECOMMENDED_AGENTS: &[&str] = &["claude", "codex", "agy", "copilot", "opencode"];

/// What each known agent bills against. Paired with detection rather than baked
/// into the label, because "not on PATH" is appended to it.
fn agent_note(key: &str) -> &'static str {
  match key {
    "claude" => "uses your Claude Pro/Max plan",
    "codex" => "uses your ChatGPT plan",
    "agy" => "uses your Antigravity plan",
    "copilot" => "uses your GitHub Copilot plan",
    "opencode" => "uses whatever provider opencode is logged into",
    "gemini" => "legacy Gemini CLI, superseded by agy",
    // An unadvertised preset someone added: say the one thing that is true of
    // every agent CLI rather than inventing a plan name.
    _ => "uses that CLI's own subscription",
  }
}

/// Mirrors `session::resolve_api_key`'s precedence without pulling the session
/// module into the picker: the env var wins, the config field is the fallback.
///
/// The environment is a parameter rather than a read, for the same reason
/// `session::resolve_api_key_with` takes one: the test that needs "no key anywhere"
/// used to produce it with `remove_var`, which is a data race against every other
/// thread in the test binary.
fn anthropic_key_present(env_key: Option<&str>, behavior: &BehaviorConfig) -> bool {
  env_key.is_some_and(|key| !key.trim().is_empty())
    || behavior
      .dj_api_key
      .as_ref()
      .is_some_and(|key| !key.trim().is_empty())
}

/// The note on the row that carries the user's own command.
const OWN_COMMAND_NOTE: &str = "your own command, run exactly as written";

/// The suffix appended to an agent row whose binary is missing. A const because
/// the tests assert on it, and a note that silently stopped saying this would
/// otherwise still look right.
const NOT_INSTALLED: &str = " · not on PATH";

/// Which backends this machine can actually run, in the order the picker shows
/// them.
///
/// Driven off [`agent_cli::PRESETS`] rather than a second list of names, so
/// detection cannot drift from what `session::resolve_agent_command` supports.
pub fn detect_backends(behavior: &BehaviorConfig) -> Vec<BackendOption> {
  let env_key = std::env::var(super::session::API_KEY_ENV).ok();
  detect_backends_with(behavior, env_key.as_deref(), on_path)
}

/// [`detect_backends`] with both pieces of ambient state passed in.
///
/// Same seam as [`binary_in_dirs`], one level up: `ready` and the
/// [`NOT_INSTALLED`] suffix are the whole point of detection, and asserting on them
/// through the real `PATH` would make the test say something different on every
/// machine (green here, green on a CI runner with none of these CLIs installed,
/// green with the readiness check deleted).
pub(crate) fn detect_backends_with(
  behavior: &BehaviorConfig,
  env_key: Option<&str>,
  probe: impl Fn(&str) -> bool,
) -> Vec<BackendOption> {
  let mut rows: Vec<BackendOption> = own_command_row(behavior, &probe).into_iter().collect();

  rows.extend(
    agent_cli::PRESETS
      .iter()
      .filter(|preset| RECOMMENDED_AGENTS.contains(&preset.key) || probe(preset.key))
      .map(|preset| {
        let ready = probe(preset.key);
        BackendOption {
          backend: "agent_cli",
          agent: Some(preset.key),
          command: None,
          label: format!("{} (no API key)", preset.key),
          note: if ready {
            agent_note(preset.key).to_string()
          } else {
            format!("{}{NOT_INSTALLED}", agent_note(preset.key))
          },
          ready,
        }
      }),
  );

  let has_key = anthropic_key_present(env_key, behavior);
  rows.push(BackendOption {
    backend: "anthropic",
    agent: None,
    command: None,
    label: "anthropic (API key)".to_string(),
    note: if has_key {
      "Anthropic API, pay per token".to_string()
    } else {
      format!(
        "needs {} (or behavior.dj_api_key)",
        super::session::API_KEY_ENV
      )
    },
    ready: has_key,
  });

  // Always offered and always ready: "is Ollama running" is a question only a
  // request can answer, and this module makes no requests.
  rows.push(BackendOption {
    backend: "openai_compat",
    agent: None,
    command: None,
    label: "openai_compat (local or self-hosted)".to_string(),
    note: "any OpenAI-compatible endpoint, e.g. Ollama on localhost".to_string(),
    ready: true,
  });

  rows
}

/// The row for a `dj_agent_command` spotatui does not own, or `None` when it does.
///
/// Without it such a command has no row at all, so the picker opens on row 0
/// (`claude`) and two Enters replace the whole command with `["claude"]` — a config
/// the docs explicitly invite ("Any other headless command works too"), silently
/// overwritten by a modal that never showed it. It goes **first** so that is the
/// row the cursor opens on.
fn own_command_row(
  behavior: &BehaviorConfig,
  probe: impl Fn(&str) -> bool,
) -> Option<BackendOption> {
  if behavior.dj_backend != "agent_cli" || super::session::owns_agent_command(behavior) {
    return None;
  }
  let program = behavior.dj_agent_command.first()?.trim();
  if program.is_empty() {
    return None;
  }
  // Only a bare name is a `PATH` question. `/opt/bin/my-agent` or `npx` wrappers
  // are already answered, and dimming them with "not on PATH" would be a lie.
  let ready = if program.contains(std::path::is_separator) {
    true
  } else {
    probe(program)
  };
  Some(BackendOption {
    backend: "agent_cli",
    agent: None,
    command: Some(behavior.dj_agent_command.clone()),
    label: format!("{} (keep)", behavior.dj_agent_command.join(" ")),
    note: if ready {
      OWN_COMMAND_NOTE.to_string()
    } else {
      format!("{OWN_COMMAND_NOTE}{NOT_INSTALLED}")
    },
    ready,
  })
}

/// One row of the picker's second step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelOption {
  /// What gets written. `None` means "write nothing": no model flag for an agent
  /// CLI, `dj_model: null` for an HTTP backend. This row has to exist so a user can
  /// get back to the shipped behaviour after picking once.
  pub value: Option<String>,
  pub label: String,
  /// Price or quota note, which is the entire point of showing a list rather than
  /// asking for a string.
  pub note: String,
  /// Opens the free-text step instead of committing.
  pub custom: bool,
}

/// Public aliases the `claude` CLI accepts, cheapest first.
///
/// Cheapest FIRST, and the cursor starts on row 0 on purpose. This is the whole
/// reason the picker exists: `claude -p` spends the user's Claude Pro/Max quota,
/// and a continuous auto-queue is a fresh prompt every few tracks. `haiku` is the
/// alias that survives that on a Pro plan; anyone who wants `opus` is one row down.
const CLAUDE_CLI_MODELS: &[(&str, &str)] = &[
  ("haiku", "cheapest, easiest on a Pro plan"),
  ("sonnet", "balanced"),
  ("opus", "heaviest, a Pro plan hits its limit fast"),
  ("fable", "heaviest"),
];

/// Anthropic API model ids with list price per million tokens (input/output).
///
/// Row 0 is also [`crate::infra::dj::brain::anthropic::DEFAULT_MODEL`], so the
/// picker's starting cursor and the code's own fallback cannot disagree.
const ANTHROPIC_MODELS: &[(&str, &str)] = &[
  ("claude-haiku-4-5", "$1/$5 per MTok, 200K ctx"),
  ("claude-sonnet-5", "$3/$15 per MTok, 1M ctx"),
  ("claude-opus-5", "$5/$25 per MTok, 1M ctx"),
  ("claude-opus-4-8", "$5/$25 per MTok, 1M ctx"),
  ("claude-fable-5", "$10/$50 per MTok, 1M ctx"),
];

/// Antigravity's models as `agy models` printed them when this was written.
///
/// A snapshot, not a contract: Google changes this list server-side, which is why
/// the "Custom…" row is always there. Spotatui does not run `agy models` itself,
/// because a subprocess on the render path is exactly what the DJ's two-lane rule
/// exists to prevent.
const AGY_MODELS: &[(&str, &str)] = &[
  ("gemini-3.6-flash-low", "cheapest"),
  ("gemini-3.6-flash-medium", ""),
  ("gemini-3.6-flash-high", ""),
  ("gemini-3.5-flash-low", ""),
  ("gemini-3.1-pro-low", ""),
  ("gemini-3.1-pro-high", "heaviest"),
  ("claude-sonnet-4-6", ""),
  ("claude-opus-4-6-thinking", "heaviest"),
  ("gpt-oss-120b-medium", ""),
];

/// Which config field carries this row's model, trimmed to a real value.
///
/// The two fields are different namespaces and never merge: `dj_agent_model` is a
/// CLI alias spent against a subscription, `dj_model` is an API model id billed per
/// token.
fn current_model(row: &BackendOption, behavior: &BehaviorConfig) -> Option<String> {
  let field = if row.is_agent() {
    &behavior.dj_agent_model
  } else {
    &behavior.dj_model
  };
  field
    .as_deref()
    .map(str::trim)
    .filter(|model| !model.is_empty())
    .map(str::to_string)
}

/// The model an HTTP backend falls back on with `dj_model` unset.
fn builtin_default(backend: &str) -> &'static str {
  if backend == "anthropic" {
    super::brain::anthropic::DEFAULT_MODEL
  } else {
    super::brain::openai_compat::DEFAULT_MODEL
  }
}

/// What the free-text step starts with, so the common edit is a one-character fix
/// rather than a retype.
fn custom_prefill(row: &BackendOption, behavior: &BehaviorConfig) -> String {
  match current_model(row, behavior) {
    Some(model) => model,
    // "No model flag at all" is a real state for an agent CLI, so there is nothing
    // to prefill; the HTTP backends always have a model id in play, and starting
    // from it makes a near-miss a one-word edit.
    None if row.is_agent() => String::new(),
    None => builtin_default(row.backend).to_string(),
  }
}

/// The second step's rows for a chosen backend, plus which row the cursor starts
/// on.
///
/// Every backend spotatui can *configure* ends with a "use the backend's own
/// default" row and a free-text row, so no such backend is a dead end and every one
/// of them can be returned to the shipped behaviour. The single exception is the
/// row standing for a command spotatui does not own, which gets one row and no
/// model choice at all: there is nothing to escape to, because a model written
/// there would never reach the CLI. See the early return below.
///
/// There is deliberately no probe behind this: `agy models` and
/// `GET /v1/models` would each be a new flag/shape for spotatui to track, and a
/// stale suggestion costs one fast local error message ("There's an issue with the
/// selected model") while the free-text row is always right there.
pub fn models_for(row: &BackendOption, behavior: &BehaviorConfig) -> (Vec<ModelOption>, usize) {
  // The user's own command: one row, and it changes nothing. There is deliberately
  // no suggestion list and no free-text row here, because
  // `session::resolve_agent_command` provably drops the model flag for an argv
  // spotatui does not own — offering a model would write a `dj_agent_model` that
  // never reaches the CLI, which is exactly the lie `active_label` was fixed to stop
  // telling. Whatever model this command uses is in the command.
  if row.command.is_some() {
    return (
      vec![ModelOption {
        value: None,
        label: "Keep this command as it is".to_string(),
        note: "spotatui adds no model flag to a command it does not own".to_string(),
        custom: false,
      }],
      0,
    );
  }

  let suggestions: &[(&str, &str)] = match row.agent {
    Some("claude") => CLAUDE_CLI_MODELS,
    Some("agy") => AGY_MODELS,
    // `codex` and the legacy `gemini`: which ids their subscription accepts is not
    // knowable from here, so free text is the honest answer.
    Some(_) => &[],
    None if row.backend == "anthropic" => ANTHROPIC_MODELS,
    // `openai_compat`: a local model list is only knowable from the endpoint, and
    // this module makes no requests.
    None => &[],
  };

  let mut rows: Vec<ModelOption> = suggestions
    .iter()
    .map(|(value, note)| ModelOption {
      value: Some(value.to_string()),
      label: value.to_string(),
      note: note.to_string(),
      custom: false,
    })
    .collect();

  // Computed before the escape hatches are appended, so `unwrap_or(0)` lands on the
  // cheapest suggestion where there is one and on "use the default" where there is
  // not. That fallback is only correct when nothing is configured, hence the row
  // added just below.
  let current = current_model(row, behavior);
  let mut cursor = rows
    .iter()
    .position(|option| option.value == current)
    .unwrap_or(0);

  // A model the user typed at the "Custom…" step (`claude-sonnet-4-5-20250929`)
  // matches no curated suggestion, and only the suggestions carry a value. Without
  // a row of its own the cursor would fall back to row 0 — the cheapest suggestion —
  // so reopening the picker to *check* what is set and pressing Enter would replace
  // it with something else, silently. Inserted before the escape hatches so "use the
  // default" is still second-to-last and "Custom…" still last.
  if let Some(model) = current.filter(|model| {
    !rows
      .iter()
      .any(|option| option.value.as_deref() == Some(model.as_str()))
  }) {
    rows.push(ModelOption {
      value: Some(model.clone()),
      label: model,
      note: "currently configured".to_string(),
      custom: false,
    });
    cursor = rows.len() - 1;
  }

  if row.is_agent() {
    rows.push(ModelOption {
      value: None,
      label: "Use the CLI's default".to_string(),
      note: "no model flag".to_string(),
      custom: false,
    });
  } else {
    rows.push(ModelOption {
      value: None,
      label: format!(
        "Use the built-in default ({})",
        builtin_default(row.backend)
      ),
      note: String::new(),
      custom: false,
    });
  }
  rows.push(ModelOption {
    value: None,
    label: "Custom…".to_string(),
    // `opencode` rejects a bare model name, so the one CLI whose format is not
    // guessable says so here rather than failing at the first turn.
    note: match row.agent {
      Some("opencode") => "type a model name, as provider/model".to_string(),
      _ => "type a model name".to_string(),
    },
    custom: true,
  });

  (rows, cursor)
}

/// Which picker row the current config already names, so reopening reads as "here
/// is what you have" rather than "start again".
fn is_active_row(row: &BackendOption, behavior: &BehaviorConfig) -> bool {
  if row.backend != behavior.dj_backend {
    return false;
  }
  match (&row.command, row.agent) {
    // The user's own command matches the config it was built from, and nothing else.
    (Some(command), _) => command == &behavior.dj_agent_command,
    // `owns_agent_command` as well as the name: `["claude", "--verbose", "-p"]`
    // starts with `claude` but is not the `claude` preset row, and claiming it here
    // is what would let Enter rewrite it.
    (None, Some(agent)) => {
      super::session::owns_agent_command(behavior)
        && behavior
          .dj_agent_command
          .first()
          .is_some_and(|first| first.trim() == agent)
    }
    (None, None) => true,
  }
}

/// `claude/haiku`, `anthropic/claude-haiku-4-5`, or `None` when the backend is not
/// one spotatui knows.
///
/// Reports what config names, which is also exactly what the picker writes. Shared
/// by `ui::ai_dj::title` and the picker's initial cursor, so the screen and the
/// picker can never disagree about what is active.
pub fn active_label(behavior: &BehaviorConfig) -> Option<String> {
  let (name, model) = match behavior.dj_backend.as_str() {
    "agent_cli" => {
      let agent = behavior.dj_agent_command.first()?.trim();
      if agent.is_empty() {
        return None;
      }
      // Only an argv spotatui owns actually receives `--model`:
      // `session::resolve_agent_command` drops the flag for a hand-written command,
      // so naming `dj_agent_model` here would put a model in the title that the CLI
      // is never told about — the invisible-mode bug this title exists to prevent,
      // in the title itself.
      let model = super::session::owns_agent_command(behavior)
        .then(|| behavior.dj_agent_model.clone())
        .flatten();
      (agent.to_string(), model)
    }
    "anthropic" | "openai_compat" => (behavior.dj_backend.clone(), behavior.dj_model.clone()),
    // Load validates `dj_backend`, so this is only reachable from a config built in
    // code. Saying nothing beats inventing a name for it.
    _ => return None,
  };
  let model = model
    .map(|model| model.trim().to_string())
    .filter(|model| !model.is_empty());
  Some(match model {
    Some(model) => format!("{name}/{model}"),
    None => name,
  })
}

/// Which of the picker's three steps is on screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DjSetupStep {
  #[default]
  Backend,
  Model,
  /// Free-text model entry. A typing surface, so the key routing for this step is
  /// deliberately different from the other two.
  Custom,
}

/// The picker. Lives on `DjState`, so it must be `Clone + Debug`.
#[derive(Clone, Debug)]
pub struct DjSetup {
  pub step: DjSetupStep,
  /// Built once when the picker opens. Detection stats `PATH`, and re-statting on
  /// every keypress would be syscalls for nothing.
  pub backends: Vec<BackendOption>,
  pub backend_index: usize,
  pub models: Vec<ModelOption>,
  pub model_index: usize,
  /// Free-text buffer for the "Custom…" row.
  pub custom: String,
}

impl DjSetup {
  /// Open on the row that is already active.
  pub fn new(behavior: &BehaviorConfig) -> Self {
    let backends = detect_backends(behavior);
    let backend_index = backends
      .iter()
      .position(|row| is_active_row(row, behavior))
      .unwrap_or(0);
    Self {
      step: DjSetupStep::Backend,
      backends,
      backend_index,
      models: Vec::new(),
      model_index: 0,
      custom: String::new(),
    }
  }

  /// How many rows the current step has. The free-text step has none, which is
  /// what makes every navigation method a no-op there.
  fn rows(&self) -> usize {
    match self.step {
      DjSetupStep::Backend => self.backends.len(),
      DjSetupStep::Model => self.models.len(),
      DjSetupStep::Custom => 0,
    }
  }

  pub fn move_down(&mut self) {
    let last = self.rows().saturating_sub(1);
    match self.step {
      DjSetupStep::Backend => self.backend_index = (self.backend_index + 1).min(last),
      DjSetupStep::Model => self.model_index = (self.model_index + 1).min(last),
      DjSetupStep::Custom => {}
    }
  }

  pub fn move_up(&mut self) {
    match self.step {
      DjSetupStep::Backend => self.backend_index = self.backend_index.saturating_sub(1),
      DjSetupStep::Model => self.model_index = self.model_index.saturating_sub(1),
      DjSetupStep::Custom => {}
    }
  }

  /// Jump straight to row `n` (1-based), reporting whether there was such a row.
  ///
  /// The caller acts on `true` by advancing: the rows are numbered on screen, and a
  /// digit shortcut that still needs an Enter saves nothing.
  pub fn select_row(&mut self, n: usize) -> bool {
    if n == 0 || n > self.rows() {
      return false;
    }
    match self.step {
      DjSetupStep::Backend => self.backend_index = n - 1,
      DjSetupStep::Model => self.model_index = n - 1,
      DjSetupStep::Custom => return false,
    }
    true
  }

  /// Enter the model step for the selected backend.
  pub fn enter_model_step(&mut self, behavior: &BehaviorConfig) {
    let Some(row) = self.selected_backend().cloned() else {
      return;
    };
    let (models, index) = models_for(&row, behavior);
    self.models = models;
    self.model_index = index;
    self.step = DjSetupStep::Model;
  }

  /// Enter the free-text step, prefilled.
  pub fn enter_custom_step(&mut self, behavior: &BehaviorConfig) {
    self.custom = self
      .selected_backend()
      .map(|row| custom_prefill(row, behavior))
      .unwrap_or_default();
    self.step = DjSetupStep::Custom;
  }

  pub fn selected_backend(&self) -> Option<&BackendOption> {
    self.backends.get(self.backend_index)
  }

  pub fn selected_model(&self) -> Option<&ModelOption> {
    self.models.get(self.model_index)
  }

  /// The config edit this picker would make, or `None` if it is not finished.
  ///
  /// Pure, so the handler tests can assert on the decision without a filesystem.
  pub fn choice(&self) -> Option<DjSetupChoice> {
    let backend = self.selected_backend()?;
    let model = match self.step {
      // A backend on its own is not an answer: which model is the half the user
      // came for.
      DjSetupStep::Backend => return None,
      DjSetupStep::Model => {
        let row = self.selected_model()?;
        if row.custom {
          return None;
        }
        row.value.clone()
      }
      // An empty buffer means "use the default" rather than an error, so the
      // free-text step is never a dead end either.
      DjSetupStep::Custom => {
        let typed = self.custom.trim();
        (!typed.is_empty()).then(|| typed.to_string())
      }
    };
    Some(DjSetupChoice {
      backend: backend.backend.to_string(),
      agent_command: match &backend.command {
        // Reported so `describe` can name the command, not written: `keep_command`
        // tells `apply_choice` to leave every command field exactly as it found it.
        Some(command) => Some(command.clone()),
        None => backend.agent.map(|agent| vec![agent.to_string()]),
      },
      keep_command: backend.command.is_some(),
      model,
      ready: backend.ready,
    })
  }
}

/// The config edit a completed picker makes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DjSetupChoice {
  pub backend: String,
  /// A **bare** preset name for `agent_cli` (`["claude"]`), never a full argv, so
  /// `session::resolve_agent_command` keeps ownership of the flags and can keep
  /// inserting the model flag in the right place as the presets change. `None` for
  /// the HTTP backends, which leave `dj_agent_command` alone.
  pub agent_command: Option<Vec<String>>,
  /// The chosen row was the user's own `dj_agent_command`, so this choice writes
  /// **no** command and **no** model: it only records that the question was
  /// answered. Without it, the row would fall into the `Some(command)` arm of
  /// [`apply_choice`] and clear `dj_agent_prompt_via` and `dj_agent_model` behind
  /// the user's back — for `agy`, whose delivery mode is `arg`, clearing the
  /// delivery drops the prompt entirely.
  pub keep_command: bool,
  /// Goes to `dj_agent_model` for `agent_cli`, `dj_model` for the HTTP backends.
  pub model: Option<String>,
  /// Whether the chosen backend is usable right now, for the confirmation message.
  pub ready: bool,
}

impl DjSetupChoice {
  /// `claude/haiku`, `anthropic/claude-haiku-4-5`, `codex`.
  pub fn describe(&self) -> String {
    let name = self
      .agent_command
      .as_ref()
      .and_then(|command| command.first().cloned())
      .unwrap_or_else(|| self.backend.clone());
    match &self.model {
      Some(model) => format!("{name}/{model}"),
      None => name,
    }
  }
}

/// Write a finished choice into config. Pure and unit-testable: no IO, no save.
///
/// Clears the *other* backend's model field as well. A stale `dj_agent_model` left
/// behind by a switch to `anthropic` would silently reappear on the next switch
/// back, which is exactly the invisible-mode bug `ui::ai_dj::title` exists to
/// prevent.
pub fn apply_choice(behavior: &mut BehaviorConfig, choice: &DjSetupChoice) {
  behavior.dj_backend = choice.backend.clone();
  // "Keep what I wrote" is an answer, and the only correct way to honour it is to
  // write nothing at all — not even the identical command back, since the
  // `Some(command)` arm below also clears the delivery mode and the model.
  if choice.keep_command {
    return;
  }
  match &choice.agent_command {
    Some(command) => {
      behavior.dj_agent_command = command.clone();
      // Cleared so the preset decides. This is what makes `agy` work at all: it
      // ignores stdin, and every existing install has `stdin` on disk from an
      // earlier automatic save, which would silently drop the prompt.
      behavior.dj_agent_prompt_via = None;
      behavior.dj_agent_model = choice.model.clone();
      behavior.dj_model = None;
    }
    None => {
      behavior.dj_model = choice.model.clone();
      behavior.dj_agent_model = None;
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::user_config::UserConfig;
  use crate::infra::dj::session::API_KEY_ENV;

  fn behavior() -> BehaviorConfig {
    UserConfig::new().behavior
  }

  /// A per-process scratch dir with one file in it, `mode` deciding whether it
  /// counts as a binary.
  ///
  /// Per-process for the same reason `agent_cli`'s stub table is: a fixed path
  /// collides with a previous `cargo test` run that is still finishing.
  #[cfg(unix)]
  fn dir_containing(name: &str, mode: u32) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!(
      "spotatui-dj-detect-{}-{name}-{mode:o}",
      std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, "#!/bin/sh\ntrue\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
    dir
  }

  /// Detection with nothing ambient: no API key, and every agent CLI installed.
  ///
  /// The environment is passed, never mutated. `remove_var`/`set_var` around a live
  /// test binary is a data race with the sibling tests in `session.rs` that read the
  /// same variable on other threads, and `setenv`/`getenv` are not thread-safe —
  /// which is why edition 2024 makes them `unsafe`.
  fn detect_installed(behavior: &BehaviorConfig) -> Vec<BackendOption> {
    detect_backends_with(behavior, None, |_| true)
  }

  #[test]
  fn detection_ignores_a_binary_that_is_not_on_path() {
    assert!(!binary_in_dirs(
      "spotatui-definitely-not-a-real-binary",
      &[std::env::temp_dir()]
    ));
  }

  #[cfg(unix)]
  #[test]
  fn detection_finds_an_executable_on_a_path_entry() {
    let dir = dir_containing("pretend-agent", 0o755);
    assert!(binary_in_dirs("pretend-agent", &[dir]));
  }

  #[cfg(unix)]
  #[test]
  fn a_non_executable_file_with_the_right_name_is_not_a_binary() {
    // A README or a config named `claude` must not read as an installed CLI.
    let dir = dir_containing("inert-agent", 0o644);
    assert!(!binary_in_dirs("inert-agent", &[dir]));
  }

  #[test]
  fn every_offered_agent_row_names_a_real_preset() {
    // The drift guard: a row whose agent has no preset would resolve to a bare
    // binary name with no subcommand and no delivery flag.
    for row in detect_backends(&behavior()) {
      if let Some(agent) = row.agent {
        assert!(
          agent_cli::preset(agent).is_some(),
          "row {agent} has no preset"
        );
        assert_eq!(row.backend, "agent_cli");
      }
      assert!(!row.note.is_empty(), "{} has no note", row.label);
    }
  }

  #[test]
  fn the_deprecated_gemini_agent_is_not_advertised_but_still_resolves() {
    // Asserted against the list, not against detection: `RECOMMENDED_AGENTS
    // .contains(key) || probe(key)` is trivially true for anything installed, so
    // comparing "offered" with "on PATH" would hold even with gemini put back on the
    // advertised list — and the binary happens to be installed on the machine this
    // was written on.
    assert!(
      !RECOMMENDED_AGENTS.contains(&"gemini"),
      "the legacy CLI must not be advertised"
    );
    assert!(
      agent_cli::preset("gemini").is_some(),
      "but an existing config that names it still has to resolve"
    );
    assert!(
      !detect_backends_with(&behavior(), None, |_| false)
        .iter()
        .any(|row| row.agent == Some("gemini")),
      "so it appears only when the binary is actually there"
    );
  }

  #[test]
  fn an_agent_row_is_offered_but_dimmed_when_its_binary_is_missing() {
    // "I am about to install it" is a real answer, so a missing CLI is still a row —
    // it just has to say so. Driven off an injected probe rather than the real PATH:
    // with `on_path` this assertion says something different on every machine, and
    // passes on a bare CI runner even with the readiness check deleted.
    let rows = detect_backends_with(&behavior(), None, |_| false);
    let agents: Vec<_> = rows.iter().filter(|row| row.agent.is_some()).collect();
    assert_eq!(
      agents.len(),
      RECOMMENDED_AGENTS.len(),
      "nothing installed still offers every recommended CLI"
    );
    for row in agents {
      assert!(!row.ready, "{} claims to be usable", row.label);
      assert!(row.note.ends_with(NOT_INSTALLED), "{}", row.note);
    }
  }

  #[test]
  fn an_agent_row_is_ready_and_says_only_what_it_bills_when_the_binary_is_there() {
    for row in detect_installed(&behavior()) {
      if row.agent.is_none() {
        continue;
      }
      assert!(
        row.ready,
        "{} is installed but not offered as ready",
        row.label
      );
      assert!(!row.note.contains(NOT_INSTALLED), "{}", row.note);
    }
  }

  #[test]
  fn the_anthropic_row_is_offered_without_a_key_and_says_what_is_missing() {
    let mut behavior = behavior();
    behavior.dj_api_key = None;
    let rows = detect_backends_with(&behavior, None, |_| true);
    let row = rows
      .iter()
      .find(|row| row.backend == "anthropic")
      .expect("the anthropic row is always offered");
    assert!(!row.ready);
    assert!(row.note.contains(API_KEY_ENV), "{}", row.note);

    // And the other way round, from the same injected environment rather than from
    // whatever this machine happens to export.
    let rows = detect_backends_with(&behavior, Some("sk-ant-not-a-real-key"), |_| true);
    let row = rows
      .iter()
      .find(|row| row.backend == "anthropic")
      .expect("the anthropic row is always offered");
    assert!(row.ready);
    assert!(!row.note.contains(API_KEY_ENV), "{}", row.note);
  }

  #[test]
  fn a_blank_exported_key_does_not_count_as_a_key() {
    assert!(!anthropic_key_present(Some("   "), &behavior()));
    let mut behavior = behavior();
    behavior.dj_api_key = Some("from-config".to_string());
    assert!(anthropic_key_present(None, &behavior));
  }

  /// One picker row, found the way the picker itself finds it.
  fn row(matches: impl Fn(&BackendOption) -> bool) -> BackendOption {
    detect_backends(&behavior())
      .into_iter()
      .find(|row| matches(row))
      .expect("the row is offered")
  }

  fn agent_row(agent: &'static str) -> BackendOption {
    row(|row| row.agent == Some(agent))
  }

  fn backend_row(backend: &'static str) -> BackendOption {
    row(|row| row.agent.is_none() && row.backend == backend)
  }

  /// Drive the picker to the model step for one row, the way the handler does.
  fn at_model_step(behavior: &BehaviorConfig, wanted: &BackendOption) -> DjSetup {
    let mut setup = DjSetup::new(behavior);
    setup.backend_index = setup
      .backends
      .iter()
      .position(|row| row == wanted)
      .expect("the row is offered");
    setup.enter_model_step(behavior);
    setup
  }

  #[test]
  fn the_claude_model_list_starts_on_the_cheapest_alias() {
    // The reason the picker exists: `claude -p` spends a Pro/Max quota per turn, so
    // the cursor must not land the user on the model that exhausts it.
    let (models, cursor) = models_for(&agent_row("claude"), &behavior());
    assert_eq!(models[cursor].value.as_deref(), Some("haiku"));
  }

  #[test]
  fn the_claude_model_list_reopens_on_the_alias_already_chosen() {
    let mut behavior = behavior();
    behavior.dj_agent_model = Some("opus".to_string());
    let (models, cursor) = models_for(&agent_row("claude"), &behavior);
    assert_eq!(models[cursor].value.as_deref(), Some("opus"));
  }

  #[test]
  fn the_model_list_keeps_a_model_that_is_not_one_of_its_suggestions() {
    // The reopen-to-check case. A model typed at the "Custom…" step matches no
    // curated row, and every escape-hatch row has `value: None`, so without a row of
    // its own the cursor falls back to row 0 — `haiku` — and a reflexive Enter
    // replaces a deliberate choice with the cheapest suggestion.
    let mut behavior = behavior();
    behavior.dj_agent_model = Some("claude-sonnet-4-5-20250929".to_string());
    let (models, cursor) = models_for(&agent_row("claude"), &behavior);
    assert_eq!(
      models[cursor].value.as_deref(),
      Some("claude-sonnet-4-5-20250929"),
      "the cursor must open on what is configured, not on row 0"
    );
    assert_eq!(models[cursor].note, "currently configured");
    // And it lands in front of the escape hatches, so they keep their positions.
    assert!(models.last().expect("never empty").custom);
    assert!(models[models.len() - 2].value.is_none());

    // The same for an API model id that has aged out of the priced list.
    let mut behavior = behavior;
    behavior.dj_backend = "anthropic".to_string();
    behavior.dj_model = Some("claude-3-5-sonnet-20241022".to_string());
    let (models, cursor) = models_for(&backend_row("anthropic"), &behavior);
    assert_eq!(
      models[cursor].value.as_deref(),
      Some("claude-3-5-sonnet-20241022")
    );
  }

  #[test]
  fn the_anthropic_model_list_starts_on_the_default_model() {
    // Read from the const rather than repeated, so the picker cannot drift from the
    // model the brain would have used anyway.
    let (models, cursor) = models_for(&backend_row("anthropic"), &behavior());
    assert_eq!(
      models[cursor].value.as_deref(),
      Some(crate::infra::dj::brain::anthropic::DEFAULT_MODEL)
    );
  }

  #[test]
  fn the_anthropic_model_list_prices_every_row() {
    // A model list without prices is just as opaque as the config field it replaces.
    let (models, _) = models_for(&backend_row("anthropic"), &behavior());
    for option in models.iter().filter(|option| option.value.is_some()) {
      assert!(
        option.note.contains("per MTok"),
        "{} has no price",
        option.label
      );
    }
  }

  #[test]
  fn every_model_list_ends_with_a_free_text_row() {
    // The no-dead-end guarantee. There is no probe behind these lists, so a stale
    // suggestion set must never be the only way to name a model.
    //
    // Both configs, because the default one cannot produce the own-command row, and
    // that row is the single deliberate exception to the guarantee. Iterating only
    // the default config would let the exception widen silently to a backend that
    // does need an escape hatch, which is the failure this test exists to catch.
    for behavior in [behavior(), hand_written()] {
      for backend in detect_backends(&behavior) {
        let (models, cursor) = models_for(&backend, &behavior);
        assert!(
          cursor < models.len(),
          "{} starts out of range",
          backend.label
        );
        let last = models.last().expect("never empty");

        if backend.command.is_some() {
          // The exception, asserted rather than skipped: one row, and it is not a
          // model choice, because a model named here would never reach the CLI.
          assert_eq!(
            models.len(),
            1,
            "{} offers a model flag it cannot pass",
            backend.label
          );
          assert!(!last.custom && last.value.is_none(), "{}", backend.label);
          continue;
        }

        assert!(last.custom, "{} cannot type a model name", backend.label);
        let default_row = &models[models.len() - 2];
        assert!(
          default_row.value.is_none() && !default_row.custom,
          "{} cannot return to its own default",
          backend.label
        );
      }
    }
  }

  #[test]
  fn a_cli_with_no_known_models_still_offers_its_own_default_and_free_text() {
    // Which ids `codex` accepts is not knowable without asking it, and asking costs
    // a subprocess. Two rows is the honest answer, not an empty list.
    let (models, cursor) = models_for(&agent_row("codex"), &behavior());
    assert_eq!(models.len(), 2);
    assert_eq!(cursor, 0, "the cursor starts on 'use the CLI's default'");
    assert!(models[0].value.is_none());
    assert!(models[1].custom);
  }

  #[test]
  fn the_picker_opens_on_the_backend_already_configured() {
    let mut behavior = behavior();
    behavior.dj_backend = "anthropic".to_string();
    let setup = DjSetup::new(&behavior);
    assert_eq!(
      setup.selected_backend().map(|row| row.backend),
      Some("anthropic"),
      "reopening should read as 'here is what you have'"
    );
  }

  #[test]
  fn choosing_an_agent_writes_a_bare_command_so_the_preset_keeps_ownership() {
    let mut behavior = behavior();
    let setup = at_model_step(&behavior, &agent_row("agy"));
    let choice = setup.choice().expect("a model row is selected");
    apply_choice(&mut behavior, &choice);

    // Bare, not `["agy", "--model", …, "-p"]`: the argv shape is the preset's job,
    // and only the preset knows the model flag has to precede the delivery flag.
    assert_eq!(behavior.dj_agent_command, vec!["agy".to_string()]);
    assert_eq!(
      behavior.dj_agent_model.as_deref(),
      Some("gemini-3.6-flash-low")
    );
    assert_eq!(behavior.dj_backend, "agent_cli");
  }

  #[test]
  fn choosing_an_agent_clears_the_prompt_delivery_so_the_preset_decides() {
    // `agy` ignores stdin entirely, and every existing install has `stdin` on disk
    // from an automatic save. Leaving it there would deliver the prompt nowhere.
    let mut behavior = behavior();
    behavior.dj_agent_prompt_via = Some("stdin".to_string());
    let setup = at_model_step(&behavior, &agent_row("agy"));
    apply_choice(&mut behavior, &setup.choice().unwrap());
    assert_eq!(behavior.dj_agent_prompt_via, None);
  }

  #[test]
  fn choosing_an_http_backend_writes_dj_model_not_dj_agent_model() {
    let mut behavior = behavior();
    let setup = at_model_step(&behavior, &backend_row("anthropic"));
    apply_choice(&mut behavior, &setup.choice().unwrap());
    assert_eq!(
      behavior.dj_model.as_deref(),
      Some(crate::infra::dj::brain::anthropic::DEFAULT_MODEL)
    );
    assert_eq!(behavior.dj_agent_model, None);
    // An API backend has no argv to speak of, so the agent command is left alone.
    assert_eq!(
      behavior.dj_agent_command,
      crate::core::user_config::default_dj_agent_command()
    );
  }

  #[test]
  fn switching_backends_clears_the_other_backends_model() {
    // `haiku` and `claude-haiku-4-5` are different namespaces. A leftover would
    // reappear on the next switch back, as a mode nobody chose.
    let mut behavior = behavior();
    let setup = at_model_step(&behavior, &agent_row("claude"));
    apply_choice(&mut behavior, &setup.choice().unwrap());
    assert_eq!(behavior.dj_agent_model.as_deref(), Some("haiku"));

    let setup = at_model_step(&behavior, &backend_row("anthropic"));
    apply_choice(&mut behavior, &setup.choice().unwrap());
    assert_eq!(behavior.dj_agent_model, None);
    assert!(behavior.dj_model.is_some());

    let setup = at_model_step(&behavior, &agent_row("claude"));
    apply_choice(&mut behavior, &setup.choice().unwrap());
    assert_eq!(behavior.dj_model, None);
  }

  #[test]
  fn the_use_the_default_row_clears_the_model_instead_of_writing_one() {
    let mut behavior = behavior();
    behavior.dj_agent_model = Some("opus".to_string());
    let mut setup = at_model_step(&behavior, &agent_row("claude"));
    // The row before "Custom…" is always "use the default".
    setup.model_index = setup.models.len() - 2;
    apply_choice(&mut behavior, &setup.choice().unwrap());
    assert_eq!(
      behavior.dj_agent_model, None,
      "picking once must not be a one-way door"
    );
  }

  #[test]
  fn the_free_text_row_is_not_itself_an_answer() {
    let mut setup = at_model_step(&behavior(), &agent_row("claude"));
    setup.model_index = setup.models.len() - 1;
    assert!(
      setup.choice().is_none(),
      "'Custom…' opens the typing step, it does not commit"
    );
  }

  #[test]
  fn a_typed_model_commits_and_an_empty_one_falls_back_to_the_default() {
    let behavior = behavior();
    let mut setup = at_model_step(&behavior, &agent_row("codex"));
    setup.enter_custom_step(&behavior);
    setup.custom = "  gpt-5-codex  ".to_string();
    assert_eq!(
      setup.choice().unwrap().model.as_deref(),
      Some("gpt-5-codex"),
      "trimmed, because a trailing space in argv is a different model name"
    );

    setup.custom = "   ".to_string();
    assert_eq!(
      setup.choice().unwrap().model,
      None,
      "an empty buffer means 'use the default', not an error"
    );
  }

  #[test]
  fn the_free_text_step_for_an_http_backend_starts_from_its_built_in_default() {
    // `openai_compat` has no suggestion list, so the buffer is the only place the
    // model id can come from; starting empty would make the common case a retype.
    let behavior = behavior();
    let mut setup = at_model_step(&behavior, &backend_row("openai_compat"));
    setup.enter_custom_step(&behavior);
    assert_eq!(
      setup.custom,
      crate::infra::dj::brain::openai_compat::DEFAULT_MODEL
    );
  }

  #[test]
  fn a_backend_on_its_own_is_not_a_finished_choice() {
    let setup = DjSetup::new(&behavior());
    assert_eq!(setup.step, DjSetupStep::Backend);
    assert!(setup.choice().is_none(), "the model is half the question");
  }

  #[test]
  fn navigation_saturates_at_both_ends() {
    let mut setup = DjSetup::new(&behavior());
    let rows = setup.backends.len();
    for _ in 0..rows + 5 {
      setup.move_down();
    }
    assert_eq!(setup.backend_index, rows - 1);
    for _ in 0..rows + 5 {
      setup.move_up();
    }
    assert_eq!(setup.backend_index, 0);
    assert!(!setup.select_row(rows + 1), "out of range is a no-op");
    assert!(setup.select_row(2));
    assert_eq!(setup.backend_index, 1);
  }

  #[test]
  fn active_label_reports_the_backend_and_model_pair() {
    let mut behavior = behavior();
    assert_eq!(active_label(&behavior).as_deref(), Some("claude"));

    behavior.dj_agent_model = Some("haiku".to_string());
    assert_eq!(active_label(&behavior).as_deref(), Some("claude/haiku"));

    behavior.dj_backend = "anthropic".to_string();
    behavior.dj_model = Some("claude-sonnet-5".to_string());
    assert_eq!(
      active_label(&behavior).as_deref(),
      Some("anthropic/claude-sonnet-5")
    );
  }

  /// The config `docs/ai-dj.md` invites: "Any other headless command works too."
  ///
  /// `arg` delivery and a model are both set, because both are things the picker
  /// used to quietly discard for this config.
  fn hand_written() -> BehaviorConfig {
    let mut behavior = behavior();
    behavior.dj_agent_command = vec!["my-agent".to_string(), "--headless".to_string()];
    behavior.dj_agent_prompt_via = Some("arg".to_string());
    behavior.dj_agent_model = Some("haiku".to_string());
    behavior
  }

  #[test]
  fn a_hand_written_command_gets_the_first_row_and_survives_two_enters() {
    let behavior = hand_written();
    let mut setup = DjSetup::new(&behavior);
    let row = setup.selected_backend().expect("a row is selected").clone();
    assert_eq!(
      row.command.as_ref(),
      Some(&behavior.dj_agent_command),
      "the picker has to open on the command it found, not on `claude`"
    );
    assert!(row.label.contains("my-agent --headless"), "{}", row.label);

    // One row, and it is not a model choice. `resolve_agent_command` drops the model
    // flag for an argv spotatui does not own, so any suggestion here would write a
    // `dj_agent_model` the CLI is never told about.
    let (models, cursor) = models_for(&row, &behavior);
    assert_eq!(models.len(), 1, "{models:?}");
    assert_eq!(cursor, 0);
    assert!(models[0].value.is_none() && !models[0].custom);

    let mut after = behavior.clone();
    setup.enter_model_step(&behavior);
    apply_choice(
      &mut after,
      &setup.choice().expect("the single row is an answer"),
    );
    assert_eq!(after.dj_agent_command, behavior.dj_agent_command);
    assert_eq!(
      after.dj_agent_prompt_via, behavior.dj_agent_prompt_via,
      "clearing it would hand an arg-delivery CLI its prompt on stdin, i.e. nowhere"
    );
    assert_eq!(after.dj_agent_model, behavior.dj_agent_model);
    assert_eq!(after.dj_model, behavior.dj_model);
  }

  #[test]
  fn the_title_omits_a_model_the_cli_is_never_told_about() {
    let mut behavior = behavior();
    behavior.dj_agent_command = vec![
      "claude".to_string(),
      "--verbose".to_string(),
      "-p".to_string(),
    ];
    behavior.dj_agent_model = Some("haiku".to_string());
    assert_eq!(
      active_label(&behavior).as_deref(),
      Some("claude"),
      "the spawned argv has no --model, so the title must not advertise one"
    );

    // The very same field is reported once the command is one spotatui owns, which
    // is what makes the title and the resolved argv the same statement.
    behavior.dj_agent_command = vec!["claude".to_string()];
    assert_eq!(active_label(&behavior).as_deref(), Some("claude/haiku"));
  }

  #[test]
  fn the_picker_always_opens_on_a_row_that_preserves_what_is_configured() {
    // One invariant over six configs: the row the cursor opens on either names
    // exactly what is configured, or is the row that explicitly preserves it.
    // Anything else is a modal that misreports the config and then overwrites it on
    // Enter — which is the same user-visible failure whether the mismatch is in the
    // backend list, the model list, or the title.
    //
    // Every expectation below is a literal. Deriving them from the helper the code
    // under test uses (`session::owns_agent_command`) would let a bug in it cancel
    // out on both sides.
    let mut bare = behavior();
    bare.dj_agent_command = vec!["claude".to_string()];
    let mut preset_and_model = bare.clone();
    preset_and_model.dj_agent_model = Some("opus".to_string());
    let mut typed_model = bare.clone();
    typed_model.dj_agent_model = Some("claude-sonnet-4-5-20250929".to_string());
    let mut flagged_preset = behavior();
    flagged_preset.dj_agent_command = vec![
      "claude".to_string(),
      "--verbose".to_string(),
      "-p".to_string(),
    ];
    flagged_preset.dj_agent_model = Some("haiku".to_string());
    let mut api = behavior();
    api.dj_backend = "anthropic".to_string();
    api.dj_model = Some("claude-opus-5".to_string());

    struct Case {
      name: &'static str,
      behavior: BehaviorConfig,
      /// The row the picker must open on: `(backend, agent, is it the user's own
      /// command?)`.
      row: (&'static str, Option<&'static str>, bool),
      /// The model row the cursor must land on.
      model: Option<&'static str>,
      /// What the DJ title says before the picker runs, and after Enter, Enter.
      /// Equal for every config that already names something.
      title: (&'static str, &'static str),
      /// Whether Enter, Enter has to leave every config field exactly as it was.
      untouched: bool,
    }

    for case in [
      Case {
        name: "a fresh default install",
        behavior: behavior(),
        row: ("agent_cli", Some("claude"), false),
        model: Some("haiku"),
        title: ("claude", "claude/haiku"),
        untouched: false,
      },
      Case {
        name: "a bare preset name",
        behavior: bare,
        row: ("agent_cli", Some("claude"), false),
        model: Some("haiku"),
        title: ("claude", "claude/haiku"),
        untouched: false,
      },
      Case {
        name: "a preset with a chosen alias",
        behavior: preset_and_model,
        row: ("agent_cli", Some("claude"), false),
        model: Some("opus"),
        title: ("claude/opus", "claude/opus"),
        untouched: true,
      },
      Case {
        name: "a model typed at the Custom step",
        behavior: typed_model,
        row: ("agent_cli", Some("claude"), false),
        model: Some("claude-sonnet-4-5-20250929"),
        title: (
          "claude/claude-sonnet-4-5-20250929",
          "claude/claude-sonnet-4-5-20250929",
        ),
        untouched: true,
      },
      Case {
        name: "a hand-written multi-part command",
        behavior: hand_written(),
        row: ("agent_cli", None, true),
        model: None,
        // No model, either side: this argv never receives the `--model` flag, so
        // `dj_agent_model: haiku` is set but not in effect.
        title: ("my-agent", "my-agent"),
        untouched: true,
      },
      Case {
        name: "a preset name with a flag added by hand",
        behavior: flagged_preset,
        row: ("agent_cli", None, true),
        model: None,
        // The trap case: it starts with `claude`, so the preset row would happily
        // claim it and Enter would drop `--verbose`.
        title: ("claude", "claude"),
        untouched: true,
      },
      Case {
        name: "an API backend with a model id",
        behavior: api,
        row: ("anthropic", None, false),
        model: Some("claude-opus-5"),
        title: ("anthropic/claude-opus-5", "anthropic/claude-opus-5"),
        untouched: true,
      },
    ] {
      let Case {
        name,
        behavior,
        row: (backend, agent, own_command),
        model,
        title,
        untouched,
      } = case;
      assert_eq!(
        active_label(&behavior).as_deref(),
        Some(title.0),
        "{name}: the title misreports the config before the picker even opens"
      );

      let mut setup = DjSetup::new(&behavior);
      let row = setup
        .selected_backend()
        .unwrap_or_else(|| panic!("{name}: nothing selected"));
      assert_eq!(row.backend, backend, "{name}");
      assert_eq!(row.agent, agent, "{name}");
      assert_eq!(
        row.command.as_ref(),
        own_command.then_some(&behavior.dj_agent_command),
        "{name}"
      );

      setup.enter_model_step(&behavior);
      let selected = setup
        .selected_model()
        .unwrap_or_else(|| panic!("{name}: no model row"));
      assert_eq!(selected.value.as_deref(), model, "{name}");

      let mut after = behavior.clone();
      apply_choice(
        &mut after,
        &setup
          .choice()
          .unwrap_or_else(|| panic!("{name}: the open row is not an answer")),
      );
      assert_eq!(
        active_label(&after).as_deref(),
        Some(title.1),
        "{name}: the title after the picker ran"
      );
      if untouched {
        assert_eq!(after.dj_backend, behavior.dj_backend, "{name}");
        assert_eq!(
          after.dj_agent_command, behavior.dj_agent_command,
          "{name}: the command was rewritten"
        );
        assert_eq!(after.dj_agent_model, behavior.dj_agent_model, "{name}");
        assert_eq!(after.dj_model, behavior.dj_model, "{name}");
        assert_eq!(
          after.dj_agent_prompt_via, behavior.dj_agent_prompt_via,
          "{name}: prompt delivery was cleared"
        );
      } else {
        // Nothing was configured to preserve, so the picker is free to suggest, and
        // deliberately does: cheapest first, which is the reason it exists. What it
        // still may not change is which CLI is being run.
        assert_eq!(
          after.dj_agent_command.first(),
          behavior.dj_agent_command.first(),
          "{name}"
        );
        assert_eq!(after.dj_agent_model.as_deref(), model, "{name}");
      }
    }
  }

  #[test]
  fn active_label_says_nothing_for_a_backend_it_does_not_know() {
    let mut behavior = behavior();
    behavior.dj_backend = "carrier-pigeon".to_string();
    assert_eq!(active_label(&behavior), None);
  }

  #[test]
  fn describe_names_the_agent_rather_than_the_backend() {
    // "agent_cli/haiku" would tell the listener nothing about which CLI is running.
    let choice = DjSetupChoice {
      backend: "agent_cli".to_string(),
      agent_command: Some(vec!["claude".to_string()]),
      keep_command: false,
      model: Some("haiku".to_string()),
      ready: true,
    };
    assert_eq!(choice.describe(), "claude/haiku");
    let choice = DjSetupChoice {
      model: None,
      ..choice
    };
    assert_eq!(choice.describe(), "claude");
  }
}
