//! Pure helpers behind the `--debug` flag / `SPOTATUI_LOG` env var: level
//! resolution, the per-target `level_for` list, the startup header text, the
//! compiled-feature list, and the log-tail extraction the panic hook appends
//! to the crash log. Kept side-effect-free so they can be unit-tested without
//! touching the environment or installing the process-global logger.

use log::LevelFilter;

/// Resolves the effective log level from the `--debug` flag and the raw
/// `SPOTATUI_LOG` env value (the caller reads the env var; this fn never
/// touches the environment itself, so it stays testable).
///
/// When both are given, the MORE VERBOSE of the two wins. An unparseable env
/// value is treated as absent, and a warning naming it is returned for the
/// caller to log with `warn!` once the logger is up (it can't be logged
/// before that).
pub(super) fn resolve_log_level(
  debug_flag: bool,
  env_value: Option<&str>,
) -> (LevelFilter, Option<String>) {
  let flag_level = debug_flag.then_some(LevelFilter::Debug);

  let (env_level, warning) = match env_value {
    None => (None, None),
    Some(raw) => match parse_level(raw) {
      Some(level) => (Some(level), None),
      None => (
        None,
        Some(format!(
          "invalid SPOTATUI_LOG value '{raw}': expected one of off, error, warn, info, debug, trace"
        )),
      ),
    },
  };

  let resolved = match (flag_level, env_level) {
    (Some(a), Some(b)) => a.max(b),
    (Some(a), None) => a,
    (None, Some(b)) => b,
    (None, None) => LevelFilter::Info,
  };

  (resolved, warning)
}

fn parse_level(raw: &str) -> Option<LevelFilter> {
  match raw.trim().to_ascii_lowercase().as_str() {
    "off" => Some(LevelFilter::Off),
    "error" => Some(LevelFilter::Error),
    "warn" => Some(LevelFilter::Warn),
    "info" => Some(LevelFilter::Info),
    "debug" => Some(LevelFilter::Debug),
    "trace" => Some(LevelFilter::Trace),
    _ => None,
  }
}

/// Our own crates raised at `--debug`. `spotatui_librespot_playback` is
/// deliberately excluded here — it logs per audio packet and would drown the
/// file — and only joins the list at `trace`.
const DEBUG_TARGETS: &[&str] = &[
  "spotatui",
  "spotatui_librespot_core",
  "spotatui_librespot_connect",
  "spotatui_librespot_oauth",
  "spotatui_librespot_metadata",
  "spotatui_librespot_protocol",
  "spotatui_librespot_audio",
];

const PLAYBACK_TARGET: &str = "spotatui_librespot_playback";

/// The `level_for` overrides to layer on top of the global fern level.
///
/// fern's `level_for` matches on `::` module-path boundaries, so the target
/// `spotatui` does NOT also cover `spotatui_librespot_core` and friends —
/// each crate needs its own entry. Dependencies (reqwest, hyper, rustls, h2,
/// ...) are intentionally never listed: the global dispatch level already
/// covers them, so a new dependency added later can't accidentally get noisy
/// just because it wasn't added to a pin list here.
pub(super) fn target_levels(resolved: LevelFilter) -> Vec<(&'static str, LevelFilter)> {
  match resolved {
    LevelFilter::Trace => DEBUG_TARGETS
      .iter()
      .chain(std::iter::once(&PLAYBACK_TARGET))
      .map(|&target| (target, LevelFilter::Trace))
      .collect(),
    LevelFilter::Debug => DEBUG_TARGETS
      .iter()
      .map(|&target| (target, LevelFilter::Debug))
      .collect(),
    _ => Vec::new(),
  }
}

/// The compiled feature list for the startup header. Kept as one flat list
/// (source features included) rather than a separate "enabled sources"
/// field — a bug reporter just needs to see `local-files`/`subsonic`/... in
/// the same place as everything else.
pub(super) fn compiled_features() -> Vec<&'static str> {
  let mut features = Vec::new();
  macro_rules! push_if_enabled {
    ($feature:literal) => {
      if cfg!(feature = $feature) {
        features.push($feature);
      }
    };
  }
  push_if_enabled!("telemetry");
  push_if_enabled!("tui");
  push_if_enabled!("streaming");
  push_if_enabled!("audio-viz");
  push_if_enabled!("audio-viz-cpal");
  push_if_enabled!("cover-art");
  push_if_enabled!("art-decode");
  push_if_enabled!("scripting");
  push_if_enabled!("self-update");
  push_if_enabled!("mcp-server");
  push_if_enabled!("ai-dj");
  push_if_enabled!("dj-core");
  push_if_enabled!("mpris");
  push_if_enabled!("discord-rpc");
  push_if_enabled!("macos-media");
  push_if_enabled!("windows-media");
  push_if_enabled!("local-files");
  push_if_enabled!("subsonic");
  push_if_enabled!("internet-radio");
  push_if_enabled!("youtube");
  push_if_enabled!("qobuz");
  features
}

/// The always-on startup line written once at the top of every log, so a bug
/// report has the version/platform/build context without needing `--debug`.
#[allow(clippy::too_many_arguments)]
pub(super) fn startup_header(
  version: &str,
  os: &str,
  arch: &str,
  features: &[&str],
  term: Option<&str>,
  term_program: Option<&str>,
  wt_session: bool,
) -> String {
  let features_str = if features.is_empty() {
    "none".to_string()
  } else {
    features.join(", ")
  };
  format!(
    "spotatui {version} startup | os={os} arch={arch} | features: {features_str} | terminal: TERM={term} TERM_PROGRAM={term_program} Windows Terminal={wt}",
    term = term.unwrap_or("unknown"),
    term_program = term_program.unwrap_or("unknown"),
    wt = if wt_session { "yes" } else { "no" },
  )
}

/// The stderr line printed once `run_cli` finishes, naming the log file a bug
/// reporter should attach.
pub(super) fn exit_log_notice(log_path: &std::path::Path, verbose: bool) -> String {
  let mut notice = format!("Log file: {}", log_path.display());
  if !verbose {
    notice.push_str("\nRe-run with --debug for a more detailed log before reporting a bug.");
  }
  notice
}

/// Returns the last `max_lines` complete lines of `text`. Used both to unit
/// test the extraction logic in isolation and, via [`read_log_tail`], on the
/// (already-trimmed) tail window read off disk by the panic hook.
pub(super) fn tail_lines(text: &str, max_lines: usize) -> String {
  if max_lines == 0 || text.is_empty() {
    return String::new();
  }
  let lines: Vec<&str> = text.lines().collect();
  let start = lines.len().saturating_sub(max_lines);
  lines[start..].join("\n")
}

/// Reads at most the last `window_bytes` of `path` and returns its last
/// `max_lines` complete lines, dropping a leading partial line that the
/// window may have cut mid-write. Never panics: every fallible step returns
/// `None` on failure rather than unwrapping, since this runs from the panic
/// hook where a second panic aborts the process with no message at all.
pub(super) fn read_log_tail(
  path: &std::path::Path,
  window_bytes: u64,
  max_lines: usize,
) -> Option<String> {
  use std::io::{Read, Seek, SeekFrom};

  let mut file = std::fs::File::open(path).ok()?;
  let len = file.metadata().ok()?.len();
  let start = len.saturating_sub(window_bytes);
  file.seek(SeekFrom::Start(start)).ok()?;

  // Capped at the window rather than read to EOF: the logger keeps appending
  // while the panic hook runs, and an unbounded read would grow with it.
  let mut buf = Vec::new();
  file
    .take(len.saturating_sub(start))
    .read_to_end(&mut buf)
    .ok()?;
  let mut text = String::from_utf8_lossy(&buf).into_owned();

  if start > 0 {
    // The window didn't start at the beginning of the file, so its first
    // line is likely a partial line cut mid-write; drop it.
    match text.find('\n') {
      Some(idx) => text = text[idx + 1..].to_string(),
      None => text.clear(),
    }
  }

  Some(tail_lines(&text, max_lines))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn no_flag_and_no_env_resolves_to_info() {
    let (level, warning) = resolve_log_level(false, None);
    assert_eq!(level, LevelFilter::Info);
    assert!(warning.is_none());
  }

  #[test]
  fn debug_flag_alone_resolves_to_debug() {
    let (level, warning) = resolve_log_level(true, None);
    assert_eq!(level, LevelFilter::Debug);
    assert!(warning.is_none());
  }

  #[test]
  fn env_debug_alone_resolves_to_debug() {
    let (level, warning) = resolve_log_level(false, Some("debug"));
    assert_eq!(level, LevelFilter::Debug);
    assert!(warning.is_none());
  }

  #[test]
  fn env_value_is_trimmed_and_case_insensitive() {
    let (level, warning) = resolve_log_level(false, Some("  TRACE  "));
    assert_eq!(level, LevelFilter::Trace);
    assert!(warning.is_none());
  }

  #[test]
  fn debug_flag_and_trace_env_resolve_to_the_more_verbose_level() {
    let (level, warning) = resolve_log_level(true, Some("trace"));
    assert_eq!(level, LevelFilter::Trace);
    assert!(warning.is_none());
  }

  #[test]
  fn debug_flag_and_warn_env_resolve_to_the_more_verbose_level() {
    let (level, warning) = resolve_log_level(true, Some("warn"));
    assert_eq!(level, LevelFilter::Debug);
    assert!(warning.is_none());
  }

  #[test]
  fn warn_env_alone_resolves_to_warn() {
    let (level, warning) = resolve_log_level(false, Some("warn"));
    assert_eq!(level, LevelFilter::Warn);
    assert!(warning.is_none());
  }

  #[test]
  fn off_env_alone_resolves_to_off() {
    let (level, warning) = resolve_log_level(false, Some("off"));
    assert_eq!(level, LevelFilter::Off);
    assert!(warning.is_none());
  }

  #[test]
  fn invalid_env_value_falls_back_to_info_and_warns() {
    let (level, warning) = resolve_log_level(false, Some("verbose"));
    assert_eq!(level, LevelFilter::Info);
    let warning = warning.expect("an invalid value must produce a warning");
    assert!(warning.contains("verbose"));
    assert!(warning.contains("off"));
    assert!(warning.contains("trace"));
  }

  #[test]
  fn debug_flag_with_invalid_env_value_still_resolves_to_debug_and_warns() {
    let (level, warning) = resolve_log_level(true, Some("nope"));
    assert_eq!(level, LevelFilter::Debug);
    assert!(warning.is_some());
  }

  #[test]
  fn target_levels_is_empty_at_info() {
    assert!(target_levels(LevelFilter::Info).is_empty());
  }

  #[test]
  fn target_levels_is_empty_below_info() {
    assert!(target_levels(LevelFilter::Warn).is_empty());
    assert!(target_levels(LevelFilter::Error).is_empty());
    assert!(target_levels(LevelFilter::Off).is_empty());
  }

  #[test]
  fn target_levels_lists_seven_crates_at_debug_excluding_playback() {
    let targets = target_levels(LevelFilter::Debug);
    assert_eq!(targets.len(), 7);
    assert!(targets
      .iter()
      .all(|&(_, level)| level == LevelFilter::Debug));
    assert!(targets.iter().any(|&(t, _)| t == "spotatui"));
    assert!(targets.iter().any(|&(t, _)| t == "spotatui_librespot_core"));
    assert!(!targets
      .iter()
      .any(|&(t, _)| t == "spotatui_librespot_playback"));
  }

  #[test]
  fn target_levels_lists_all_eight_crates_at_trace_including_playback() {
    let targets = target_levels(LevelFilter::Trace);
    assert_eq!(targets.len(), 8);
    assert!(targets
      .iter()
      .all(|&(_, level)| level == LevelFilter::Trace));
    assert!(targets
      .iter()
      .any(|&(t, _)| t == "spotatui_librespot_playback"));
  }

  #[test]
  fn compiled_features_is_never_empty() {
    assert!(!compiled_features().is_empty());
  }

  #[test]
  fn compiled_features_contains_tui_when_the_tui_feature_is_enabled() {
    let features = compiled_features();
    assert_eq!(features.contains(&"tui"), cfg!(feature = "tui"));
  }

  #[test]
  fn startup_header_contains_version_os_arch_and_features() {
    let header = startup_header(
      "1.2.3",
      "linux",
      "x86_64",
      &["telemetry", "tui"],
      Some("xterm-256color"),
      Some("iTerm.app"),
      false,
    );
    assert!(header.contains("1.2.3"));
    assert!(header.contains("linux"));
    assert!(header.contains("x86_64"));
    assert!(header.contains("telemetry, tui"));
    assert!(header.contains("xterm-256color"));
    assert!(header.contains("iTerm.app"));
    assert!(header.contains("Windows Terminal=no"));
  }

  #[test]
  fn startup_header_reports_windows_terminal_yes_without_leaking_the_session_value() {
    let header = startup_header("1.2.3", "windows", "x86_64", &[], None, None, true);
    assert!(header.contains("Windows Terminal=yes"));
    assert!(!header.contains("WT_SESSION="));
  }

  #[test]
  fn startup_header_uses_placeholders_when_terminal_env_is_absent() {
    let header = startup_header("1.2.3", "linux", "x86_64", &[], None, None, false);
    assert!(header.contains("TERM=unknown"));
    assert!(header.contains("TERM_PROGRAM=unknown"));
  }

  #[test]
  fn exit_log_notice_contains_the_log_path() {
    let notice = exit_log_notice(std::path::Path::new("/tmp/example/spotatuilog123"), true);
    assert!(notice.contains("/tmp/example/spotatuilog123"));
    assert!(!notice.contains("--debug"));
  }

  #[test]
  fn exit_log_notice_suggests_debug_when_not_verbose() {
    let notice = exit_log_notice(std::path::Path::new("/tmp/example/spotatuilog123"), false);
    assert!(notice.contains("/tmp/example/spotatuilog123"));
    assert!(notice.contains("--debug"));
  }

  #[test]
  fn tail_lines_returns_all_when_fewer_lines_than_max() {
    assert_eq!(tail_lines("a\nb\nc", 10), "a\nb\nc");
  }

  #[test]
  fn tail_lines_returns_all_when_exactly_max_lines() {
    assert_eq!(tail_lines("a\nb\nc", 3), "a\nb\nc");
  }

  #[test]
  fn tail_lines_drops_leading_lines_when_more_than_max() {
    assert_eq!(tail_lines("a\nb\nc\nd", 2), "c\nd");
  }

  #[test]
  fn tail_lines_of_empty_input_is_empty() {
    assert_eq!(tail_lines("", 200), "");
  }

  #[test]
  fn tail_lines_handles_input_without_trailing_newline() {
    assert_eq!(tail_lines("a\nb\nc", 2), "b\nc");
  }

  #[test]
  fn read_log_tail_returns_none_for_a_missing_file() {
    let missing = std::path::Path::new("/nonexistent/path/spotatui-logging-test-missing.log");
    assert!(read_log_tail(missing, 1024, 200).is_none());
  }

  #[test]
  fn read_log_tail_drops_a_partial_leading_line_and_keeps_the_tail() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("test.log");
    // 30 short lines; a small window forces a mid-line cut.
    let contents: String = (0..30).map(|i| format!("line{i}\n")).collect();
    std::fs::write(&path, &contents).expect("write");

    let tail = read_log_tail(&path, 40, 200).expect("tail");
    assert!(!tail.starts_with("line0\n"));
    assert!(tail.ends_with("line29"));
    for line in tail.lines() {
      assert!(line.starts_with("line"));
    }
  }

  /// A single log line can be longer than the window now that response bodies
  /// are logged at trace. The window then holds no newline at all, and every
  /// byte of it is the tail of a partial line — returning it would put a
  /// fragment starting mid-token into a crash report.
  #[test]
  fn read_log_tail_returns_nothing_when_the_window_holds_no_complete_line() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("test.log");
    std::fs::write(&path, "x".repeat(500)).expect("write");

    let tail = read_log_tail(&path, 100, 200).expect("tail");
    assert!(tail.is_empty(), "expected no line, got {tail:?}");
  }

  #[test]
  fn read_log_tail_returns_the_whole_file_when_it_fits_in_the_window() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("test.log");
    std::fs::write(&path, "first\nsecond\n").expect("write");

    let tail = read_log_tail(&path, 64 * 1024, 200).expect("tail");
    assert_eq!(tail, "first\nsecond");
  }

  #[test]
  fn read_log_tail_handles_an_empty_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("test.log");
    std::fs::write(&path, "").expect("write");

    assert_eq!(read_log_tail(&path, 64 * 1024, 200).expect("tail"), "");
  }
}
