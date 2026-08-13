//! Ratchet counters for the GUI substrate migration: each counter measures a
//! form of TUI/core coupling that must reach zero before a second frontend
//! can share the core (`crate::tui` imports outside `tui/`, direct `App`
//! field writes in handlers, raw `IoEvent` dispatch from the TUI, mouse
//! handling that synthesizes keystrokes, wildcard arms in the action tree).
//!
//! `tools/gates.count` holds the measured baselines. Each counter may only
//! move toward its target: lower the baseline in the same PR that improves
//! it, and never raise one. `test_attribute_total` is a floor instead, so a
//! refactor cannot silently delete a test module's worth of tests. (These
//! are text counts over `src/`, so the floor catches deletions, not a
//! module that still compiles under a stale feature gate but stops running.)

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
  let Ok(entries) = fs::read_dir(dir) else {
    return;
  };
  for entry in entries.flatten() {
    let path = entry.path();
    if path.is_dir() {
      collect_rs_files(&path, out);
    } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
      out.push(path);
    }
  }
}

fn read_source(path: &Path) -> String {
  fs::read_to_string(path).unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()))
}

/// Sums non-overlapping occurrences of `needle` across every `.rs` file under
/// `dirs` (recursive) plus the standalone `files`.
fn count_occurrences(dirs: &[&str], files: &[&str], needle: &str) -> usize {
  let root = repo_root();
  let mut paths = Vec::new();
  for dir in dirs {
    collect_rs_files(&root.join(dir), &mut paths);
  }
  for file in files {
    paths.push(root.join(file));
  }
  paths
    .iter()
    .map(|path| read_source(path).matches(needle).count())
    .sum()
}

/// Counts direct `App` field writes: `app.<field>` (optionally a deeper
/// `.field` chain) followed on the same line by `=` or a compound assignment
/// operator. Comparisons (`==`), match guards (`=>`), method calls, and
/// indexed writes do not count. This is the exact matcher the
/// `app_field_writes_in_tui_handlers` baseline was measured with.
fn count_app_field_writes(source: &str) -> usize {
  let bytes = source.as_bytes();
  let mut count = 0;
  let mut search_from = 0;
  while let Some(found) = source[search_from..].find("app.") {
    let start = search_from + found;
    search_from = start + "app.".len();
    if start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_') {
      continue; // part of a longer identifier, e.g. `wrapped_app.`
    }
    // Walk the `field(.field)*` chain after `app.`.
    let chain_start = start + "app.".len();
    let mut pos = chain_start;
    loop {
      let ident_start = pos;
      while pos < bytes.len() && (bytes[pos].is_ascii_alphanumeric() || bytes[pos] == b'_') {
        pos += 1;
      }
      if pos == ident_start {
        break;
      }
      if pos < bytes.len() && bytes[pos] == b'.' {
        pos += 1;
        continue;
      }
      break;
    }
    if pos == chain_start {
      continue; // `app.` with no field name after it
    }
    // Only same-line spacing may separate the chain from the operator.
    while pos < bytes.len() && (bytes[pos] == b' ' || bytes[pos] == b'\t') {
      pos += 1;
    }
    let is_write = match bytes.get(pos) {
      Some(b'+' | b'-' | b'*' | b'/' | b'%' | b'|' | b'&' | b'^') => {
        bytes.get(pos + 1) == Some(&b'=')
      }
      Some(shift @ (b'<' | b'>')) => {
        bytes.get(pos + 1) == Some(shift) && bytes.get(pos + 2) == Some(&b'=')
      }
      Some(b'=') => !matches!(bytes.get(pos + 1), Some(b'=') | Some(b'>')),
      _ => false,
    };
    if is_write {
      count += 1;
    }
  }
  count
}

fn load_baselines() -> BTreeMap<String, usize> {
  let path = repo_root().join("tools").join("gates.count");
  let text = read_source(&path);
  let mut map = BTreeMap::new();
  for raw in text.lines() {
    let line = raw.split('#').next().unwrap_or("").trim();
    if line.is_empty() {
      continue;
    }
    let (key, value) = line
      .split_once('=')
      .unwrap_or_else(|| panic!("malformed line in tools/gates.count: {raw}"));
    let value = value
      .trim()
      .parse::<usize>()
      .unwrap_or_else(|_| panic!("bad number in tools/gates.count: {raw}"));
    map.insert(key.trim().to_string(), value);
  }
  map
}

#[test]
fn ratchet_counters_match_the_checked_in_baselines() {
  let root = repo_root();

  let mut handler_files = Vec::new();
  collect_rs_files(&root.join("src/tui/handlers"), &mut handler_files);
  let app_field_writes: usize = handler_files
    .iter()
    .map(|path| count_app_field_writes(&read_source(path)))
    .sum();

  // Needles that would otherwise count their own literal in this file are
  // assembled with concat! (this file lives under the `src/` scan).
  let test_attribute_total = count_occurrences(&["src"], &[], concat!("#[", "test]"))
    + count_occurrences(&["src"], &[], concat!("#[", "tokio::test"));

  // Every Rust file outside `src/tui/` is in scope, so a new top-level module
  // (or a future `src/gui.rs`) cannot re-import the TUI unseen. This file is
  // the one exclusion: it names the needle in its own doc and data, and being
  // test-only it cannot couple anything to `tui/`.
  let mut non_tui_files = Vec::new();
  collect_rs_files(&root.join("src"), &mut non_tui_files);
  let tui_dir = root.join("src").join("tui");
  let gates_file = root.join("src").join("gates.rs");
  non_tui_files.retain(|path| !path.starts_with(&tui_dir) && *path != gates_file);
  let crate_tui_refs: usize = non_tui_files
    .iter()
    .map(|path| read_source(path).matches("crate::tui").count())
    .sum();

  // (name, measured value, is_floor). Ratchets must match their baseline
  // exactly; the floor may only rise.
  let measured: [(&str, usize, bool); 6] = [
    ("crate_tui_refs_outside_tui", crate_tui_refs, false),
    ("app_field_writes_in_tui_handlers", app_field_writes, false),
    (
      "ioevent_refs_in_tui",
      count_occurrences(&["src/tui"], &[], "IoEvent::"),
      false,
    ),
    (
      "synthetic_keys_in_mouse_handler",
      count_occurrences(&[], &["src/tui/handlers/mouse.rs"], "handler(Key::"),
      false,
    ),
    (
      "wildcard_arms_in_action_tree",
      count_occurrences(&["src/core/action"], &[], "_ =>"),
      false,
    ),
    ("test_attribute_total", test_attribute_total, true),
  ];

  let mut baselines = load_baselines();
  let mut report = String::new();
  for (name, actual, is_floor) in measured {
    let Some(baseline) = baselines.remove(name) else {
      report.push_str(&format!(
        "{name}: missing from tools/gates.count (measured {actual})\n"
      ));
      continue;
    };
    if is_floor {
      if actual < baseline {
        report.push_str(&format!(
          "{name}: fell from {baseline} to {actual}; tests were deleted, restore them or \
           lower the floor deliberately\n"
        ));
      }
    } else if actual > baseline {
      report.push_str(&format!(
        "{name}: rose from {baseline} to {actual}; this PR reintroduces coupling the \
         ratchet exists to burn down, fix the code instead of the baseline\n"
      ));
    } else if actual < baseline {
      report.push_str(&format!(
        "{name}: improved from {baseline} to {actual}; lower the baseline in \
         tools/gates.count in this same PR\n"
      ));
    }
  }
  for (name, baseline) in baselines {
    report.push_str(&format!(
      "{name} = {baseline}: unknown counter in tools/gates.count, remove or fix the name\n"
    ));
  }
  assert!(report.is_empty(), "\nratchet violations:\n{report}");
}

#[test]
fn field_write_matcher_counts_assignment_operators() {
  let src = "app.x = 1;\napp.a.b += 2;\napp.c -= 3;\napp.d *= 4;\napp.e <<= 1;\napp.f >>= 2;\n";
  assert_eq!(count_app_field_writes(src), 6);
}

#[test]
fn field_write_matcher_ignores_comparisons_guards_calls_and_other_idents() {
  let src = "if app.x == 1 {}\n\
             Key::Up if app.flag => {}\n\
             app.list.push(1);\n\
             foo(app.x);\n\
             if app.x != y {}\n\
             if app.x <= y {}\n\
             if app.x >= y {}\n\
             let s = app.y << 2;\n\
             wrapped_app.x = 1;\n";
  assert_eq!(count_app_field_writes(src), 0);
}
