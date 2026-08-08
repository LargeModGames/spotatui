//! The DJ tool layer, shared by both front doors.
//!
//! There are two ways to drive the DJ and they deliberately share everything
//! below the model:
//!
//! * **MCP** (`mcp-server`) — spotatui exposes [`tools`] over an MCP stdio
//!   server, so Claude Code / Codex / any MCP client is the DJ. No API key.
//! * **In-TUI** (`ai-dj`) — a DJ screen whose "brain" is a local agent CLI, an
//!   API key, or a local model, and which uses the same tools internally.
//!
//! The split that matters for anyone extending this: [`brief`] and [`tools`]
//! only ever touch `App`, while [`resolve`] and [`library`] need the real Spotify
//! client. That is what decides which IoEvent lane a DJ event belongs on — see
//! `crate::infra::network::mod` (`runs_on_service_lane`), whose service lane
//! builds its `Network` with `None` for the client.

// `dj-core` is an implementation feature with no consumer of its own — it exists
// to be pulled in by `mcp-server` and `ai-dj`, exactly as `audio-decode` is
// pulled in by the media sources. Enabled on its own, everything here is
// legitimately unreachable, so scope the allow to that case rather than blanket
// it (cf. `#[cfg_attr(not(feature = "scripting"), allow(dead_code))]` in
// `infra::network`). Deliberately still `not(mcp-server)` rather than
// `not(any(mcp-server, ai-dj))`: an `ai-dj` build has the DJ's brains but not yet
// the screen that drives them, so the allow has to stay active there too. It
// grows its `ai-dj` arm when that screen lands.
#![cfg_attr(not(feature = "mcp-server"), allow(dead_code))]

/// The in-TUI DJ's model backends.
// Nothing drives them yet; the DJ screen, the next PR in this stack, is the consumer.
#[cfg(feature = "ai-dj")]
#[allow(dead_code)]
pub mod brain;
pub mod brief;
/// How a tool call reaches the live player. Shared by both front doors, and
/// gated on having one: it constructs `IoEvent::DjToolCall`, which bare
/// `dj-core` does not compile.
#[cfg(any(feature = "mcp-server", feature = "ai-dj"))]
pub mod exec;
/// The avoid-library filter's two lookups. Shared: the in-TUI DJ filters with it
/// by default, and the MCP front door uses it to mark `search_tracks` results as
/// owned and to honour `queue_tracks(exclude_owned)`.
pub mod library;
pub mod resolve;
/// Turn assembly for the in-TUI DJ.
// Nothing drives it yet; the DJ screen, the next PR in this stack, is the consumer.
#[cfg(feature = "ai-dj")]
#[allow(dead_code)]
pub mod session;
/// What AI the listener has, and which of its models: the data behind the DJ's
/// setup picker.
// Nothing draws it yet; the DJ screen, the next PR in this stack, is the consumer.
#[cfg(feature = "ai-dj")]
#[allow(dead_code)]
pub mod setup;
/// The tool surface, shared by both front doors: the MCP server publishes it
/// verbatim, and the in-TUI DJ's agent loop drives the same table.
pub mod tools;

pub use brief::dedupe_key;

/// Cap on the avoid-library crawl. A listener with more saved tracks than this
/// gets a partial index rather than an unbounded startup cost, and is told so —
/// silently filtering against half a library would look like the filter is
/// broken.
pub const MAX_LIBRARY_TRACKS: usize = 20_000;

/// What the listener already has, for the avoid-library filter.
///
/// Playlists only. Liked Songs are deliberately absent: `me/tracks/contains`
/// answers that question exactly, for a whole batch in one call, so caching it
/// would add staleness for nothing. See [`library`].
#[derive(Clone, Debug, Default)]
pub struct DjLibrary {
  /// [`dedupe_key`] per track, so a suggestion can be rejected before it costs a
  /// catalogue search.
  pub keys: std::collections::HashSet<String>,
  /// Spotify track IDs, for the exact gate after a suggestion resolves. Catches
  /// the case the key set misses: the model named the track differently enough to
  /// normalise apart, but search landed on the copy they own.
  pub ids: std::collections::HashSet<String>,
  pub playlists: usize,
  pub tracks: usize,
  /// Whether the crawl stopped at [`MAX_LIBRARY_TRACKS`].
  pub truncated: bool,
}

impl DjLibrary {
  /// One line for the transcript, so the cost of the crawl is visible.
  pub fn summary(&self) -> String {
    let mut text = format!(
      "Indexed {} track(s) across {} playlist(s)",
      self.tracks, self.playlists
    );
    if self.truncated {
      text.push_str(&format!(
        " (stopped at {MAX_LIBRARY_TRACKS}; the rest is not filtered)"
      ));
    }
    text
  }
}

/// `"Title — Artist"` for whatever is loaded, across every source.
///
/// Lives here rather than in [`tools`] because it is needed with or without a
/// front door compiled in.
pub fn current_track_label(app: &crate::core::app::App) -> Option<String> {
  let snapshot = crate::infra::media_metadata::current_playback_snapshot(app)?;
  Some(format!(
    "{} — {}",
    snapshot.metadata.title,
    snapshot.primary_artist()
  ))
}

/// One track a model proposed. Names only — resolution to a URI happens in
/// [`resolve`], which drops anything the catalogue cannot confidently match.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DjSuggestion {
  pub title: String,
  pub artist: String,
  /// Optional one-liner from the model. Shown in the DJ transcript when present;
  /// never used for matching.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub why: Option<String>,
}

impl DjSuggestion {
  pub fn label(&self) -> String {
    format!("{} — {}", self.title, self.artist)
  }
}

/// Who said a line in the DJ transcript.
// Only the in-TUI DJ uses this; the allow narrows to `not(ai-dj)` once it lands.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DjSpeaker {
  User,
  Dj,
  /// Machine-generated notes ("queued 5 tracks, 1 not found"), styled apart from
  /// the model's own prose.
  System,
}

// Only the in-TUI DJ uses this; the allow narrows to `not(ai-dj)` once it lands.
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DjLine {
  pub speaker: DjSpeaker,
  pub text: String,
}

// Only the in-TUI DJ uses this; the allow narrows to `not(ai-dj)` once it lands.
#[allow(dead_code)]
impl DjLine {
  pub fn user(text: impl Into<String>) -> Self {
    Self {
      speaker: DjSpeaker::User,
      text: text.into(),
    }
  }
  pub fn dj(text: impl Into<String>) -> Self {
    Self {
      speaker: DjSpeaker::Dj,
      text: text.into(),
    }
  }
  pub fn system(text: impl Into<String>) -> Self {
    Self {
      speaker: DjSpeaker::System,
      text: text.into(),
    }
  }
}

/// How many tracks the DJ asks for, and will accept, per round.
///
/// Capped for three converging reasons: on an external Connect device each
/// queued Spotify track costs its own Web API call
/// (`App::add_track_to_native_queue`); a deep queue cannot respond to a vibe
/// shift; and one model invocation per batch (rather than per track) is what
/// keeps latency and subscription usage sane.
// Only the in-TUI DJ uses this; the allow narrows to `not(ai-dj)` once it lands.
#[allow(dead_code)]
pub const DEFAULT_BATCH: usize = 6;
pub const MAX_BATCH: usize = 8;

/// Refill the queue when it drops to this many tracks. Two tracks is roughly
/// 6–8 minutes of runway, comfortably more than the worst case for a refill:
/// `agent::MAX_STEPS_MUST_ACT` brain calls at `behavior.dj_agent_timeout_secs`
/// each. That bound is why a refill gets fewer steps than a conversation.
// Only the in-TUI DJ uses this; the allow narrows to `not(ai-dj)` once it lands.
#[allow(dead_code)]
pub const QUEUE_LOW_WATER: usize = 2;

/// Everything the DJ keeps on `App`.
// Only the in-TUI DJ uses this; the allow narrows to `not(ai-dj)` once it lands.
#[allow(dead_code)]
#[derive(Clone, Debug, Default)]
pub struct DjState {
  /// Whether the DJ keeps the queue topped up as tracks finish.
  pub auto_queue: bool,
  /// Bumped whenever in-flight DJ work becomes irrelevant: DJ switched off, a
  /// vibe shift, a source change.
  ///
  /// **Load-bearing, not defensive.** A top-up dispatched from the advance path
  /// lands seconds later; without re-checking this value before writing, a stale
  /// batch queues tracks for a session the user already left. Same idiom as
  /// `App::desired_lyrics_identity` and `App::desired_cover_art_key`.
  pub generation: u64,
  pub transcript: Vec<DjLine>,
  /// The DJ's own progress flag. Deliberately *not* the global `App::is_loading`:
  /// a brain call can run for a minute or more, and pinning the global spinner
  /// that long is a UX bug.
  pub thinking: bool,
  /// Which step of how many the turn in flight is on, for the `…thinking` row.
  ///
  /// A multi-step turn on an agent CLI is minutes of silence otherwise, and
  /// "thinking" alone cannot be told apart from a hang.
  pub step: Option<(usize, usize)>,
  /// Which turn owns [`Self::thinking`].
  ///
  /// Distinct from [`Self::generation`], which says whether a turn's *results* are
  /// still wanted. This says who may clear the progress flag. An abandoned turn
  /// finishing would otherwise clear a flag its replacement had already set, and
  /// `wants_top_up` gates on exactly that flag — so the next tick would dispatch a
  /// second refill and two batches would land.
  pub turn_seq: u64,
  pub vibe: Option<String>,
  pub input: Vec<char>,
  pub input_idx: usize,
  pub input_cursor: u16,
  /// Scroll offset into the *wrapped* transcript rows, not the message list.
  pub scroll: u16,
  /// URIs of the tracks the DJ put into the native queue. A vibe shift drops
  /// exactly these, so the change is audible now rather than six tracks from
  /// now.
  ///
  /// Identity, not a count: the DJ's picks are not guaranteed to be a contiguous
  /// tail — the user can queue by hand after a batch lands, or delete a DJ pick
  /// from the queue screen — and truncating by count in either state drops the
  /// wrong tracks.
  pub queued_uris: std::collections::HashSet<String>,
  /// Reject anything the listener already has, rather than recommending it.
  ///
  /// Seeded from `behavior.dj_avoid_library` and toggleable at runtime, because
  /// which mode you want depends on the ask: "more like this" wants their
  /// favourites in scope, "find me something new" does not.
  pub avoid_library: bool,
  /// The playlist snapshot behind [`Self::avoid_library`]. `None` until the crawl
  /// has run; built lazily, since it costs one API call per 100 playlist tracks
  /// and a listener who never turns the filter on should never pay it.
  pub library: Option<DjLibrary>,
  /// A crawl is in flight. Guards against a second one being dispatched behind it
  /// (the toggle and the first turn can both ask).
  pub library_indexing: bool,
}

impl DjState {
  /// Invalidate in-flight work and return the new generation.
  ///
  /// Used by `set_dj_vibe` over MCP as well as by the in-TUI vibe shift.
  pub fn bump_generation(&mut self) -> u64 {
    self.generation = self.generation.wrapping_add(1);
    self.generation
  }

  /// Claim the progress flag for a turn about to be dispatched.
  ///
  /// Every path that starts a turn goes through here, so the returned value is
  /// the one thing allowed to clear `thinking` again.
  // Only the in-TUI DJ uses this; the allow narrows to `not(ai-dj)` once it lands.
  #[allow(dead_code)]
  pub fn begin_turn(&mut self) -> u64 {
    self.thinking = true;
    self.turn_seq = self.turn_seq.wrapping_add(1);
    self.turn_seq
  }

  /// Clear the progress flag, but only if `seq` still owns it.
  // Only the in-TUI DJ uses this; the allow narrows to `not(ai-dj)` once it lands.
  #[allow(dead_code)]
  pub fn finish_turn(&mut self, seq: u64) {
    if self.turn_seq == seq {
      self.thinking = false;
      self.step = None;
    }
  }

  // Only the in-TUI DJ uses this; the allow narrows to `not(ai-dj)` once it lands.
  #[allow(dead_code)]
  pub fn push_line(&mut self, line: DjLine) {
    self.transcript.push(line);
    // The transcript is a conversation, not a log; an unbounded Vec here would
    // grow for the life of the process and re-wrap on every draw.
    const MAX_LINES: usize = 200;
    if self.transcript.len() > MAX_LINES {
      let overflow = self.transcript.len() - MAX_LINES;
      self.transcript.drain(0..overflow);
    }
  }

  // Only the in-TUI DJ uses this; the allow narrows to `not(ai-dj)` once it lands.
  #[allow(dead_code)]
  pub fn take_input(&mut self) -> String {
    let text = self.input.iter().collect::<String>();
    self.input.clear();
    self.input_idx = 0;
    self.input_cursor = 0;
    text
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn generation_bump_invalidates() {
    let mut state = DjState::default();
    let first = state.bump_generation();
    let second = state.bump_generation();
    assert_ne!(first, second);
  }

  #[test]
  fn transcript_is_bounded() {
    let mut state = DjState::default();
    for i in 0..250 {
      state.push_line(DjLine::dj(format!("line {i}")));
    }
    assert_eq!(state.transcript.len(), 200);
    // Oldest dropped, newest kept.
    assert_eq!(state.transcript.last().unwrap().text, "line 249");
  }

  #[test]
  fn only_the_newest_turn_may_clear_the_progress_flag() {
    // The interleaving this exists for: turn A is in flight, the listener toggles
    // auto-queue off and on, the tick dispatches B — then A finishes. If A could
    // clear the flag, `wants_top_up` would see an idle DJ with a short queue and
    // dispatch C, and B and C would both queue a batch.
    let mut state = DjState::default();
    let a = state.begin_turn();
    let b = state.begin_turn();
    assert_ne!(a, b);

    state.finish_turn(a);
    assert!(state.thinking, "a stale turn does not own the flag");

    state.finish_turn(b);
    assert!(!state.thinking, "the turn that owns it does");
    assert!(state.step.is_none(), "and clears the step counter with it");
  }

  #[test]
  fn take_input_clears_cursor_state() {
    let mut state = DjState::default();
    state.input = "chill".chars().collect();
    state.input_idx = 5;
    state.input_cursor = 5;
    assert_eq!(state.take_input(), "chill");
    assert!(state.input.is_empty());
    assert_eq!(state.input_idx, 0);
    assert_eq!(state.input_cursor, 0);
  }

  #[test]
  fn suggestion_label_is_stable() {
    let suggestion = DjSuggestion {
      title: "Nude".into(),
      artist: "Radiohead".into(),
      why: None,
    };
    assert_eq!(suggestion.label(), "Nude — Radiohead");
  }
}
