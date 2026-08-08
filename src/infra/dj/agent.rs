//! The in-TUI DJ's turn loop.
//!
//! One turn is several steps: ask the brain what to do, run whatever it asked
//! for, hand the results back, repeat. A step that calls nothing ends the turn —
//! which is what a plain conversational reply is.
//!
//! Runs on the **service** lane, where a brain call may take minutes. The tools
//! themselves do not run here: [`exec::AppExecutor`] answers the read-only ones
//! from the `App` lock and sends the rest down the serial lane, which is the only
//! one with a Spotify client.
//!
//! The loop is bounded because every step of an `agent_cli` turn is a fresh
//! subprocess, so an unbounded turn would spend the listener's subscription and
//! their patience.

use super::brain::{DjBrain, ToolExchange, ToolInvocation};
use super::exec::ToolExecutor;
use super::session::TurnContext;
use super::tools::{self, DjToolCall, ToolCallError};
use super::{DjLine, DjSpeaker};
use crate::core::app::App;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Steps a conversational turn may take.
///
/// Four is room to look something up, search, queue, and report — while bounding
/// an agent-CLI turn to four subprocesses.
pub const MAX_STEPS: usize = 4;

/// Steps an auto-queue refill or a vibe shift may take.
///
/// Half, because the queue runway is the budget. `QUEUE_LOW_WATER` is two tracks,
/// roughly six to eight minutes; four steps at the default 90-second agent
/// timeout could eat all of it, and the refill would land after the music had
/// already stopped. `queue_tracks` resolves names itself, so a refill has no need
/// of a search step anyway.
pub const MAX_STEPS_MUST_ACT: usize = 2;

/// Tool calls executed from a single step. A model that asks for twenty in one
/// breath gets the first few run and the rest reported back unrun.
const MAX_CALLS_PER_STEP: usize = 4;

/// How a turn ended.
pub struct TurnOutcome {
  /// Whether any tool call actually changed the queue or playback. Drives whether
  /// the listener's words become the standing vibe.
  pub acted: bool,
  /// Set when the turn was abandoned because the generation moved on.
  pub abandoned: bool,
  /// Whether the model set the standing vibe itself, which its own words should
  /// not then be overwritten by.
  pub vibe_set: bool,
}

/// Everything one turn needs.
pub struct Turn<'a, E: ToolExecutor> {
  pub app: &'a Arc<Mutex<App>>,
  pub brain: &'a DjBrain,
  pub context: &'a TurnContext,
  pub executor: &'a E,
  /// The generation this turn belongs to. A bump the turn did not cause means the
  /// listener has moved on and everything after it is unwanted.
  pub generation: u64,
  /// Whether this turn has to end in music rather than words.
  pub must_act: bool,
}

impl<E: ToolExecutor> Turn<'_, E> {
  pub async fn run(mut self) -> anyhow::Result<TurnOutcome> {
    let max_steps = if self.must_act {
      MAX_STEPS_MUST_ACT
    } else {
      MAX_STEPS
    };
    let mut scratch: Vec<ToolExchange> = Vec::new();
    let mut acted = false;
    let mut vibe_set = false;

    for step_number in 1..=max_steps {
      if self.abandoned().await {
        return Ok(TurnOutcome {
          acted,
          abandoned: true,
          vibe_set,
        });
      }
      self.show_step(step_number, max_steps).await;

      let request = self.context.to_request(scratch.clone(), self.must_act);
      let step = self.brain.step(&request).await?;

      if let Some(say) = step.say.filter(|say| !say.trim().is_empty()) {
        self.push(DjLine::dj(say)).await;
      }
      if step.calls.is_empty() {
        return Ok(TurnOutcome {
          acted,
          abandoned: false,
          vibe_set,
        });
      }

      for call in step.calls.iter().take(MAX_CALLS_PER_STEP) {
        match self.run_call(call).await {
          CallResult::Ran {
            exchange,
            mutated,
            set_vibe,
          } => {
            acted |= mutated;
            vibe_set |= set_vibe;
            scratch.push(exchange);
          }
          // The listener has moved on mid-turn. Stop before running anything else
          // against a session they have left.
          CallResult::Abandoned => {
            return Ok(TurnOutcome {
              acted,
              abandoned: true,
              vibe_set,
            })
          }
        }
      }
      if step.calls.len() > MAX_CALLS_PER_STEP {
        scratch.push(ToolExchange {
          name: "spotatui".into(),
          arguments: serde_json::json!({}),
          result: format!(
            "Only the first {MAX_CALLS_PER_STEP} of your {} tool calls were run. Ask for the rest \
             next step if you still need them.",
            step.calls.len()
          ),
        });
      }
    }

    // Out of steps with the model still working. Said out loud: silence here looks
    // identical to a hang, and on an agent CLI the turn has just cost real quota.
    self
      .push(DjLine::system(format!(
        "Stopped after {max_steps} steps. Ask again if that is not where you wanted to end up."
      )))
      .await;
    Ok(TurnOutcome {
      acted,
      abandoned: false,
      vibe_set,
    })
  }

  /// Parse, apply the in-TUI policy, execute, and report one call.
  async fn run_call(&mut self, call: &ToolInvocation) -> CallResult {
    let parsed = match tools::parse_call(&call.name, &call.arguments) {
      Ok(parsed) => self.apply_policy(parsed),
      // Handed back to the model rather than aborting: a malformed call is
      // something it can correct on the next step, which is the entire benefit of
      // running a loop.
      Err(e) => {
        let detail = match &e {
          ToolCallError::UnknownTool(name) => {
            format!("There is no tool called {name}. Use one of the tools listed above.")
          }
          ToolCallError::InvalidArguments(detail) => format!("Invalid arguments: {detail}"),
        };
        self.show_call(&call.name, &detail).await;
        return CallResult::Ran {
          exchange: ToolExchange {
            name: call.name.clone(),
            arguments: call.arguments.clone(),
            result: detail,
          },
          mutated: false,
          set_vibe: false,
        };
      }
    };

    let mutated = !tools::spec(parsed.tool_name()).is_some_and(|spec| spec.read_only);
    // Checked immediately before the call, not merely at the top of the step: a
    // brain call takes long enough for the listener to shift the vibe or leave,
    // and queueing the old direction's tracks after that is the bug this prevents.
    if mutated && self.abandoned().await {
      return CallResult::Abandoned;
    }

    // Noted before the call consumes it: `set_dj_vibe` bumps the generation
    // itself, and that one bump has to be told apart from the listener's.
    let sets_vibe = matches!(parsed, DjToolCall::SetDjVibe { .. });
    let before = self.generation;
    let outcome = self.executor.call(parsed).await;

    // A serial-lane call takes seconds, which is ample time for the listener to
    // hit vibe-shift. Adopting every bump would swallow theirs and leave a stale
    // turn queueing alongside the one they just started, so only the single bump
    // our own `set_dj_vibe` makes is adopted. Their bump on top of ours moves the
    // counter by two, and still aborts.
    let after = { self.app.lock().await.dj.generation };
    if after != before {
      if sets_vibe && !outcome.is_error && after == before.wrapping_add(1) {
        self.generation = after;
      } else {
        return CallResult::Abandoned;
      }
    }
    self.show_call(&call.name, &outcome.text).await;

    CallResult::Ran {
      exchange: ToolExchange {
        name: call.name.clone(),
        arguments: call.arguments.clone(),
        result: outcome.text,
      },
      mutated: mutated && !outcome.is_error,
      set_vibe: sets_vibe && !outcome.is_error,
    }
  }

  /// The rules that make this the in-TUI DJ rather than a bare MCP client.
  fn apply_policy(&self, call: DjToolCall) -> DjToolCall {
    match call {
      DjToolCall::QueueTracks {
        items,
        exclude_owned,
        ..
      } => DjToolCall::QueueTracks {
        items,
        // In-TUI the filter is a toggle the listener set, so it applies whether or
        // not the model thought to ask. Over MCP the agent decides for itself.
        exclude_owned: exclude_owned || self.context.avoid_library,
        // The recently-played window. Enforced here rather than in
        // `App::dj_skip_keys` so an MCP agent told to queue a specific track still
        // gets it, however recently it played.
        extra_skip_keys: self.context.recent_keys(),
      },
      other => other,
    }
  }

  async fn abandoned(&self) -> bool {
    self.app.lock().await.dj.generation != self.generation
  }

  async fn push(&self, line: DjLine) {
    let mut app = self.app.lock().await;
    app.dj.push_line(line);
  }

  /// One transcript line per tool call, so the listener sees the DJ work rather
  /// than watching it stall.
  async fn show_call(&self, name: &str, result: &str) {
    let summary = first_line(result);
    self
      .push(DjLine {
        speaker: DjSpeaker::System,
        text: format!("· {name} → {summary}"),
      })
      .await;
  }

  async fn show_step(&self, step: usize, of: usize) {
    let mut app = self.app.lock().await;
    app.dj.step = Some((step, of));
  }
}

enum CallResult {
  Ran {
    exchange: ToolExchange,
    mutated: bool,
    set_vibe: bool,
  },
  Abandoned,
}

/// The first line of a tool result, bounded.
///
/// A queue report runs to several lines and a search to twenty; the transcript
/// wants the headline, and the model gets the whole thing through `scratch`
/// either way.
fn first_line(text: &str) -> String {
  const MAX: usize = 120;
  let line = text
    .lines()
    .find(|line| !line.trim().is_empty())
    .unwrap_or("");
  if line.chars().count() <= MAX {
    return line.to_string();
  }
  line.chars().take(MAX).collect::<String>() + "…"
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::infra::dj::brief::TasteBrief;
  use crate::infra::dj::tools::ToolOutcome;

  fn context(avoid_library: bool) -> TurnContext {
    TurnContext {
      brief: TasteBrief {
        recent_keys: vec!["nightcall|kavinsky".to_string()],
        ..TasteBrief::default()
      },
      history: vec![],
      want: 6,
      avoid_library,
    }
  }

  /// Records what it was asked to run and answers from a script.
  struct ScriptedExecutor {
    calls: std::sync::Mutex<Vec<DjToolCall>>,
  }

  impl ScriptedExecutor {
    fn new() -> Self {
      Self {
        calls: std::sync::Mutex::new(Vec::new()),
      }
    }
    fn seen(&self) -> Vec<DjToolCall> {
      self.calls.lock().unwrap().clone()
    }
  }

  impl ToolExecutor for ScriptedExecutor {
    async fn call(&self, call: DjToolCall) -> ToolOutcome {
      self.calls.lock().unwrap().push(call);
      ToolOutcome::ok("done")
    }
  }

  fn turn<'a>(
    app: &'a Arc<Mutex<App>>,
    brain: &'a DjBrain,
    context: &'a TurnContext,
    executor: &'a ScriptedExecutor,
  ) -> Turn<'a, ScriptedExecutor> {
    Turn {
      app,
      brain,
      context,
      executor,
      generation: 0,
      must_act: false,
    }
  }

  #[test]
  fn the_filter_toggle_is_applied_whether_or_not_the_model_asked() {
    // In-TUI the filter is the listener's toggle, so it cannot depend on the model
    // remembering to pass `exclude_owned`.
    let app = Arc::new(Mutex::new(App::default()));
    let brain = DjBrain::OpenAiCompat(super::super::brain::openai_compat::OpenAiCompatBrain::new(
      None, None, None,
    ));
    let executor = ScriptedExecutor::new();
    let filtered = context(true);
    let turn = turn(&app, &brain, &filtered, &executor);

    let call = DjToolCall::QueueTracks {
      items: vec![],
      exclude_owned: false,
      extra_skip_keys: vec![],
    };
    match turn.apply_policy(call) {
      DjToolCall::QueueTracks {
        exclude_owned,
        extra_skip_keys,
        ..
      } => {
        assert!(exclude_owned, "the toggle has to win over the model");
        // And the recently-played window travels with the call rather than living
        // in the shared skip set, where it would also filter MCP callers.
        assert_eq!(extra_skip_keys, vec!["nightcall|kavinsky".to_string()]);
      }
      other => panic!("policy changed the call into {other:?}"),
    }
  }

  #[test]
  fn an_unfiltered_session_leaves_the_models_own_choice_alone() {
    let app = Arc::new(Mutex::new(App::default()));
    let brain = DjBrain::OpenAiCompat(super::super::brain::openai_compat::OpenAiCompatBrain::new(
      None, None, None,
    ));
    let executor = ScriptedExecutor::new();
    let unfiltered = context(false);
    let turn = turn(&app, &brain, &unfiltered, &executor);

    for asked in [true, false] {
      let call = DjToolCall::QueueTracks {
        items: vec![],
        exclude_owned: asked,
        extra_skip_keys: vec![],
      };
      match turn.apply_policy(call) {
        DjToolCall::QueueTracks { exclude_owned, .. } => assert_eq!(exclude_owned, asked),
        other => panic!("policy changed the call into {other:?}"),
      }
    }
    assert!(
      executor.seen().is_empty(),
      "policy must not execute anything"
    );
  }

  #[test]
  fn a_tool_result_is_summarised_to_one_line_for_the_transcript() {
    let long = format!("Queued 6 track(s):\n{}", "A — B\n".repeat(10));
    assert_eq!(first_line(&long), "Queued 6 track(s):");
    assert!(first_line(&"x".repeat(400)).ends_with('…'));
    assert_eq!(first_line("\n\n  \nreal line"), "real line");
    assert_eq!(first_line(""), "");
  }

  #[test]
  fn a_refill_gets_fewer_steps_than_a_conversation() {
    // The queue runway is the budget: four agent-CLI steps can outlast it.
    assert!(MAX_STEPS_MUST_ACT < MAX_STEPS);
  }

  // --- the loop itself, driven end to end ---------------------------------
  //
  // A stub agent CLI stands in for the brain, because it is the only backend
  // whose "model" can be scripted without a network. Each step is a fresh
  // invocation, so a stub that branches on whether the prompt already contains
  // tool results is exactly a multi-step model.

  /// Every stub these tests exec, written before any of them spawns anything.
  ///
  /// The ordering is load-bearing, for the reason `agent_cli`'s own `stubs()`
  /// documents: `fork` duplicates every open fd, so a test writing a stub while
  /// another forks leaks a *writable* fd for that file into the child, and
  /// exec'ing it then fails with `ETXTBSY`. Renaming into place does **not** help
  /// — the fd refers to the inode, which the rename carries with it. Writing the
  /// whole set behind one `OnceLock` does, because by the time any fork happens
  /// there is no writable fd left to inherit.
  #[cfg(unix)]
  fn stubs() -> &'static std::collections::HashMap<&'static str, std::path::PathBuf> {
    static STUBS: std::sync::OnceLock<std::collections::HashMap<&'static str, std::path::PathBuf>> =
      std::sync::OnceLock::new();
    STUBS.get_or_init(|| {
      use std::os::unix::fs::PermissionsExt;
      let dir = std::env::temp_dir().join(format!("spotatui-agent-stubs-{}", std::process::id()));
      std::fs::create_dir_all(&dir).unwrap();
      [
        // Calls a tool, then answers once it can see the result.
        (
          "two-step",
          r#"PROMPT=$(cat)
case "$PROMPT" in
  *"Tools you have already run"*) echo '{"say":"Two tracks waiting.","tool_calls":[]}' ;;
  *) echo '{"say":"Let me look.","tool_calls":[{"name":"get_queue","arguments":{}}]}' ;;
esac"#,
        ),
        (
          "queues-once",
          r#"cat >/dev/null
echo '{"say":"Queueing.","tool_calls":[{"name":"queue_tracks","arguments":{"tracks":[{"title":"Nude","artist":"Radiohead"}]}}]}'"#,
        ),
        (
          "sets-vibe",
          r#"PROMPT=$(cat)
case "$PROMPT" in
  *"Tools you have already run"*) echo '{"say":"Set.","tool_calls":[]}' ;;
  *) echo '{"say":"Noting that.","tool_calls":[{"name":"set_dj_vibe","arguments":{"vibe":"deep focus"}}]}' ;;
esac"#,
        ),
        // Never stops, so the step cap is the only thing that ends the turn.
        (
          "never-stops",
          r#"cat >/dev/null
echo '{"say":"Still going.","tool_calls":[{"name":"get_queue","arguments":{}}]}'"#,
        ),
      ]
      .into_iter()
      .map(|(name, body)| {
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        (name, path)
      })
      .collect()
    })
  }

  #[cfg(unix)]
  fn stub_brain(name: &str) -> DjBrain {
    use super::super::brain::agent_cli::{AgentCliBrain, PromptDelivery};
    let path = stubs()[name].clone();
    let dir = path.parent().unwrap().to_path_buf();
    DjBrain::AgentCli(
      AgentCliBrain::new(
        vec![path.to_string_lossy().to_string()],
        PromptDelivery::Stdin,
        std::time::Duration::from_secs(10),
        dir,
      )
      .unwrap(),
    )
  }

  /// Answers a scripted outcome and can disturb the app while it "runs".
  struct SlowExecutor {
    app: Arc<Mutex<App>>,
    /// Simulates the listener hitting vibe-shift during a serial-lane call.
    bump_during_call: bool,
    calls: std::sync::Mutex<Vec<DjToolCall>>,
  }

  impl ToolExecutor for SlowExecutor {
    async fn call(&self, call: DjToolCall) -> ToolOutcome {
      self.calls.lock().unwrap().push(call);
      if self.bump_during_call {
        self.app.lock().await.dj.bump_generation();
      }
      ToolOutcome::ok("done")
    }
  }

  fn slow_executor(app: &Arc<Mutex<App>>, bump_during_call: bool) -> SlowExecutor {
    SlowExecutor {
      app: Arc::clone(app),
      bump_during_call,
      calls: std::sync::Mutex::new(Vec::new()),
    }
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn a_tool_result_feeds_the_next_step_and_the_turn_ends_on_words() {
    // The loop's whole contract in one test: call, feed the result back, and stop
    // when the model has something to say and nothing to run.
    let brain = stub_brain("two-step");
    let app = Arc::new(Mutex::new(App::default()));
    let executor = slow_executor(&app, false);
    let context = context(false);

    let outcome = Turn {
      app: &app,
      brain: &brain,
      context: &context,
      executor: &executor,
      generation: 0,
      must_act: false,
    }
    .run()
    .await
    .unwrap();

    assert!(!outcome.abandoned);
    assert!(!outcome.acted, "get_queue only reads");
    assert_eq!(executor.calls.lock().unwrap().len(), 1);

    let transcript: Vec<String> = app
      .lock()
      .await
      .dj
      .transcript
      .iter()
      .map(|line| line.text.clone())
      .collect();
    assert!(
      transcript.iter().any(|t| t == "Let me look."),
      "{transcript:?}"
    );
    // Only reachable if the second invocation saw the first call's result.
    assert!(
      transcript.iter().any(|t| t == "Two tracks waiting."),
      "{transcript:?}"
    );
    assert!(
      transcript.iter().any(|t| t.starts_with("· get_queue")),
      "the listener has to see the DJ work: {transcript:?}"
    );
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn a_listener_bump_during_a_call_abandons_the_turn() {
    // The race the generation guard exists for: a serial-lane call takes seconds,
    // and a vibe shift landing inside that window must stop this turn rather than
    // have it queue on alongside the one the listener just started.
    let brain = stub_brain("queues-once");
    let app = Arc::new(Mutex::new(App::default()));
    let executor = slow_executor(&app, true);
    let context = context(false);

    let outcome = Turn {
      app: &app,
      brain: &brain,
      context: &context,
      executor: &executor,
      generation: 0,
      must_act: false,
    }
    .run()
    .await
    .unwrap();

    assert!(
      outcome.abandoned,
      "a bump the loop did not cause has to end the turn"
    );
    assert_eq!(
      executor.calls.lock().unwrap().len(),
      1,
      "and nothing may run after it"
    );
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn the_dj_s_own_vibe_call_does_not_abort_its_own_turn() {
    // `set_dj_vibe` bumps the generation itself. Treating that as the listener
    // moving on would have the DJ kill its turn the moment it set a vibe.
    let brain = stub_brain("sets-vibe");
    let app = Arc::new(Mutex::new(App::default()));
    // The real tool, so the real bump: a stub executor would not reproduce this.
    let (tx, _rx) = std::sync::mpsc::channel();
    let executor = super::super::exec::AppExecutor::silent(Arc::clone(&app), tx);
    let context = context(false);

    let outcome = Turn {
      app: &app,
      brain: &brain,
      context: &context,
      executor: &executor,
      generation: 0,
      must_act: false,
    }
    .run()
    .await
    .unwrap();

    assert!(!outcome.abandoned, "the DJ must survive its own vibe call");
    assert!(outcome.vibe_set, "so the typed words do not overwrite it");
    assert_eq!(app.lock().await.dj.vibe.as_deref(), Some("deep focus"));
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn a_model_that_never_stops_is_capped_and_says_so() {
    let brain = stub_brain("never-stops");
    let app = Arc::new(Mutex::new(App::default()));
    let executor = slow_executor(&app, false);
    let context = context(false);

    let outcome = Turn {
      app: &app,
      brain: &brain,
      context: &context,
      executor: &executor,
      generation: 0,
      must_act: false,
    }
    .run()
    .await
    .unwrap();

    assert!(!outcome.abandoned);
    assert_eq!(executor.calls.lock().unwrap().len(), MAX_STEPS);
    let transcript: Vec<String> = app
      .lock()
      .await
      .dj
      .transcript
      .iter()
      .map(|line| line.text.clone())
      .collect();
    // Silence after a capped turn is indistinguishable from a hang, and on an
    // agent CLI it has just cost four subprocesses of real quota.
    assert!(
      transcript
        .iter()
        .any(|t| t.contains(&format!("Stopped after {MAX_STEPS} steps"))),
      "{transcript:?}"
    );
  }
}
