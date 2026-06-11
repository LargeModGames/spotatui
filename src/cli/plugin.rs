//! `spotatui plugin` subcommand: a thin git-based installer for Lua plugins.
//!
//! Plugins are git repositories cloned into `~/.config/spotatui/plugins/<name>/`, where the
//! engine loads `main.lua` (or `init.lua`) at startup. Installed plugins are tracked in a
//! `plugins.lock` file next to the `plugins/` directory so they can be listed and updated.

use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use anyhow::{anyhow, bail, Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde::{Deserialize, Serialize};

/// Build the `plugin` subcommand tree.
pub fn plugin_subcommand() -> Command {
  Command::new("plugin")
    .version(env!("CARGO_PKG_VERSION"))
    .author(env!("CARGO_PKG_AUTHORS"))
    .about("Install and manage Lua plugins")
    .long_about(
      "Manage Lua plugins installed from git repositories. Plugins are cloned into \
~/.config/spotatui/plugins/<name>/ and loaded at startup (main.lua, or init.lua). \
Requires `git` on your PATH.",
    )
    .subcommand_required(true)
    .arg_required_else_help(true)
    .subcommand(
      Command::new("add")
        .about("Install a plugin from a git repository")
        .long_about(
          "Install a plugin by cloning a git repository. Accepts a GitHub shorthand \
(owner/repo) or any git URL (https://..., git@...).",
        )
        .arg(
          Arg::new("repo")
            .required(true)
            .value_name("REPO")
            .help("owner/repo (GitHub) or a full git URL"),
        )
        .arg(
          Arg::new("force")
            .short('f')
            .long("force")
            .action(ArgAction::SetTrue)
            .help("Reinstall if the plugin is already present"),
        ),
    )
    .subcommand(
      Command::new("list")
        .visible_alias("ls")
        .about("List installed plugins"),
    )
    .subcommand(
      Command::new("remove")
        .visible_alias("rm")
        .about("Remove an installed plugin")
        .arg(
          Arg::new("name")
            .required(true)
            .value_name("NAME")
            .help("Name of the plugin to remove"),
        ),
    )
    .subcommand(
      Command::new("update")
        .about("Update installed plugins to their latest commit")
        .arg(
          Arg::new("name")
            .value_name("NAME")
            .help("Plugin to update (updates all plugins if omitted)"),
        ),
    )
}

/// Entry point dispatched from `runtime.rs`. Resolves the config dir and runs the chosen action.
pub fn handle_plugin_command(matches: &ArgMatches) -> Result<()> {
  let config_dir = crate::core::user_config::default_app_config_dir()
    .context("could not determine the spotatui config directory (no home directory found)")?;

  match matches.subcommand() {
    Some(("add", m)) => {
      let repo = m.get_one::<String>("repo").expect("repo is required");
      add_plugin(&config_dir, repo, m.get_flag("force"))
    }
    Some(("list", _)) => list_plugins(&config_dir),
    Some(("remove", m)) => {
      let name = m.get_one::<String>("name").expect("name is required");
      remove_plugin(&config_dir, name)
    }
    Some(("update", m)) => update_plugins(&config_dir, m.get_one::<String>("name")),
    _ => unreachable!("clap enforces a subcommand"),
  }
}

// --- actions ---

fn add_plugin(config_dir: &Path, spec: &str, force: bool) -> Result<()> {
  let spec = parse_repo_spec(spec)?;
  ensure_git()?;

  let plugins_dir = config_dir.join("plugins");
  std::fs::create_dir_all(&plugins_dir)
    .with_context(|| format!("creating {}", plugins_dir.display()))?;
  let dest = plugins_dir.join(&spec.name);

  if dest.exists() {
    if !force {
      bail!(
        "plugin '{}' is already installed. Use `--force` to reinstall.",
        spec.name
      );
    }
    std::fs::remove_dir_all(&dest)
      .with_context(|| format!("removing existing {}", dest.display()))?;
  }

  println!("Cloning {} into {} ...", spec.url, dest.display());
  git_clone(&spec.url, &dest)?;
  let rev = git_head_rev(&dest).unwrap_or_default();

  let lock_path = lock_path(config_dir);
  let mut lock = load_lock(&lock_path)?;
  lock.upsert(LockedPlugin {
    name: spec.name.clone(),
    repo: spec.repo.clone(),
    url: spec.url.clone(),
    rev: rev.clone(),
  });
  save_lock(&lock_path, &lock)?;

  println!("Installed plugin '{}' ({})", spec.name, short_rev(&rev));
  println!(
    "Restart spotatui to load it. Bind any commands it registers via the `plugin_commands` map \
in config.yml."
  );
  Ok(())
}

fn list_plugins(config_dir: &Path) -> Result<()> {
  let lock = load_lock(&lock_path(config_dir))?;
  let plugins_dir = config_dir.join("plugins");

  if lock.plugins.is_empty() {
    println!("No plugins installed.");
    println!("Install one with: spotatui plugin add owner/repo");
  } else {
    println!("Installed plugins:");
    for p in &lock.plugins {
      let present = plugins_dir.join(&p.name).is_dir();
      let marker = if present { "" } else { "  (missing on disk)" };
      println!(
        "  {:<20} {:<24} {}{}",
        p.name,
        p.repo,
        short_rev(&p.rev),
        marker
      );
    }
  }

  // Surface plugins on disk that the lockfile doesn't track (hand-installed or single-file).
  let tracked: Vec<&str> = lock.plugins.iter().map(|p| p.name.as_str()).collect();
  let mut untracked = untracked_plugins(&plugins_dir, &tracked);
  untracked.sort();
  if !untracked.is_empty() {
    println!("\nUntracked plugins (not managed by `spotatui plugin`):");
    for name in untracked {
      println!("  {name}");
    }
  }
  Ok(())
}

fn remove_plugin(config_dir: &Path, name: &str) -> Result<()> {
  if !valid_plugin_name(name) {
    bail!("invalid plugin name '{name}'");
  }
  let lock_path = lock_path(config_dir);
  let mut lock = load_lock(&lock_path)?;
  let dest = config_dir.join("plugins").join(name);

  let was_tracked = lock.remove(name);
  let existed_on_disk = dest.is_dir();

  if !was_tracked && !existed_on_disk {
    bail!("no plugin named '{name}' is installed");
  }
  if existed_on_disk {
    std::fs::remove_dir_all(&dest).with_context(|| format!("removing {}", dest.display()))?;
  }
  if was_tracked {
    save_lock(&lock_path, &lock)?;
  }

  println!("Removed plugin '{name}'.");
  Ok(())
}

fn update_plugins(config_dir: &Path, name: Option<&String>) -> Result<()> {
  ensure_git()?;
  let lock_path = lock_path(config_dir);
  let mut lock = load_lock(&lock_path)?;
  let plugins_dir = config_dir.join("plugins");

  let targets: Vec<LockedPlugin> = match name {
    Some(name) => {
      let found = lock
        .plugins
        .iter()
        .find(|p| p.name == *name)
        .cloned()
        .ok_or_else(|| anyhow!("no plugin named '{name}' is installed"))?;
      vec![found]
    }
    None => lock.plugins.clone(),
  };

  if targets.is_empty() {
    println!("No plugins to update.");
    return Ok(());
  }

  let mut changed = false;
  for target in targets {
    let dir = plugins_dir.join(&target.name);
    if !dir.is_dir() {
      println!(
        "  {}: missing on disk, skipping (reinstall with `spotatui plugin add {}`)",
        target.name, target.repo
      );
      continue;
    }
    match git_update(&dir) {
      Ok(()) => {
        let new_rev = git_head_rev(&dir).unwrap_or_default();
        if new_rev != target.rev {
          println!(
            "  {}: {} -> {}",
            target.name,
            short_rev(&target.rev),
            short_rev(&new_rev)
          );
          if let Some(entry) = lock.plugins.iter_mut().find(|p| p.name == target.name) {
            entry.rev = new_rev;
            changed = true;
          }
        } else {
          println!(
            "  {}: already up to date ({})",
            target.name,
            short_rev(&new_rev)
          );
        }
      }
      Err(e) => println!("  {}: update failed: {e}", target.name),
    }
  }

  if changed {
    save_lock(&lock_path, &lock)?;
  }
  Ok(())
}

// --- repo spec parsing ---

struct RepoSpec {
  /// Local plugin directory name (the repo's last path segment, minus `.git`).
  name: String,
  /// Human-facing source label (e.g. `owner/repo` or the raw URL).
  repo: String,
  /// Git clone URL.
  url: String,
}

/// Parse a plugin spec into a clone URL and a local name.
///
/// Accepts `owner/repo` (GitHub shorthand), `https://host/owner/repo(.git)`, and
/// `git@host:owner/repo(.git)`.
fn parse_repo_spec(spec: &str) -> Result<RepoSpec> {
  let spec = spec.trim();
  if spec.is_empty() {
    bail!("empty plugin spec");
  }

  let is_url = spec.contains("://") || spec.starts_with("git@");
  let (url, repo) = if is_url {
    (spec.to_string(), repo_label_from_url(spec))
  } else {
    // GitHub shorthand: exactly owner/repo.
    let parts: Vec<&str> = spec.split('/').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
      bail!("expected `owner/repo` or a git URL, got '{spec}'");
    }
    let repo = format!("{}/{}", parts[0], strip_git_suffix(parts[1]));
    (format!("https://github.com/{repo}.git"), repo)
  };

  let name = name_from_repo_path(spec);
  if !valid_plugin_name(&name) {
    bail!("could not derive a valid plugin name from '{spec}'");
  }

  Ok(RepoSpec { name, repo, url })
}

/// Last path segment of a repo spec/URL, minus any `.git` suffix.
fn name_from_repo_path(spec: &str) -> String {
  let tail = spec
    .trim_end_matches('/')
    .rsplit(['/', ':'])
    .next()
    .unwrap_or(spec);
  strip_git_suffix(tail).to_string()
}

/// Best-effort `owner/repo` label from a URL, falling back to the full URL.
fn repo_label_from_url(url: &str) -> String {
  let trimmed = url.trim_end_matches('/');
  // Take the path after the host. For scp-like git@host:owner/repo and https://host/owner/repo.
  let path = trimmed
    .rsplit_once("://")
    .map(|(_, rest)| rest)
    .unwrap_or(trimmed);
  let path = path
    .split_once(['/', ':'])
    .map(|(_, rest)| rest)
    .unwrap_or(path);
  let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
  if segments.len() >= 2 {
    let owner = segments[segments.len() - 2];
    let repo = strip_git_suffix(segments[segments.len() - 1]);
    format!("{owner}/{repo}")
  } else {
    url.to_string()
  }
}

fn strip_git_suffix(s: &str) -> &str {
  s.strip_suffix(".git").unwrap_or(s)
}

/// A plugin name must be a single safe path component.
fn valid_plugin_name(name: &str) -> bool {
  // Reject leading dots: covers `.`, `..`, and hidden names like `.foo` that the engine loader
  // skips (so the installer never reports a dead install the runtime ignores).
  !name.is_empty()
    && !name.starts_with('.')
    && !name.contains('/')
    && !name.contains('\\')
    && name
      .chars()
      .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

// --- lockfile ---

fn lock_version() -> u32 {
  1
}

#[derive(Serialize, Deserialize)]
struct PluginLock {
  #[serde(default = "lock_version")]
  version: u32,
  #[serde(default)]
  plugins: Vec<LockedPlugin>,
}

impl Default for PluginLock {
  fn default() -> Self {
    PluginLock {
      version: lock_version(),
      plugins: Vec::new(),
    }
  }
}

#[derive(Serialize, Deserialize, Clone)]
struct LockedPlugin {
  name: String,
  repo: String,
  url: String,
  #[serde(default)]
  rev: String,
}

impl PluginLock {
  /// Insert a plugin, replacing any existing entry with the same name.
  fn upsert(&mut self, plugin: LockedPlugin) {
    if let Some(existing) = self.plugins.iter_mut().find(|p| p.name == plugin.name) {
      *existing = plugin;
    } else {
      self.plugins.push(plugin);
    }
    self.plugins.sort_by(|a, b| a.name.cmp(&b.name));
  }

  /// Remove a plugin by name. Returns whether an entry was removed.
  fn remove(&mut self, name: &str) -> bool {
    let before = self.plugins.len();
    self.plugins.retain(|p| p.name != name);
    self.plugins.len() != before
  }
}

fn lock_path(config_dir: &Path) -> PathBuf {
  config_dir.join("plugins.lock")
}

/// Read the lockfile. A missing file yields an empty lock; a malformed file is an error so we
/// never silently clobber a user's record.
fn load_lock(path: &Path) -> Result<PluginLock> {
  match std::fs::read_to_string(path) {
    Ok(contents) if contents.trim().is_empty() => Ok(PluginLock::default()),
    Ok(contents) => serde_json::from_str(&contents)
      .with_context(|| format!("parsing lockfile {}", path.display())),
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(PluginLock::default()),
    Err(e) => Err(e).with_context(|| format!("reading lockfile {}", path.display())),
  }
}

fn save_lock(path: &Path, lock: &PluginLock) -> Result<()> {
  let mut json = serde_json::to_string_pretty(lock).context("serializing lockfile")?;
  json.push('\n');
  // Write to a sibling temp file and atomically rename, so a crash or full disk mid-write can't
  // leave a truncated lockfile (which load_lock would then treat as a hard parse error).
  let tmp = path.with_extension("lock.tmp");
  std::fs::write(&tmp, json).with_context(|| format!("writing lockfile {}", tmp.display()))?;
  std::fs::rename(&tmp, path).with_context(|| format!("replacing lockfile {}", path.display()))
}

/// Plugin entries on disk the lockfile doesn't track: directory plugins not in `tracked`, plus
/// any loose single-file `*.lua` plugins (which are never lockfile-tracked). Hidden entries are
/// ignored.
fn untracked_plugins(plugins_dir: &Path, tracked: &[&str]) -> Vec<String> {
  let Ok(entries) = std::fs::read_dir(plugins_dir) else {
    return Vec::new();
  };
  entries
    .flatten()
    .filter_map(|e| {
      let path = e.path();
      let name = e.file_name().into_string().ok()?;
      if name.starts_with('.') {
        None
      } else if path.is_dir() {
        (!tracked.contains(&name.as_str())).then_some(name)
      } else if path.extension().and_then(|x| x.to_str()) == Some("lua") {
        Some(name)
      } else {
        None
      }
    })
    .collect()
}

fn short_rev(rev: &str) -> &str {
  if rev.len() >= 7 {
    &rev[..7]
  } else if rev.is_empty() {
    "unknown"
  } else {
    rev
  }
}

// --- git (thin wrappers over the system `git`) ---

fn ensure_git() -> Result<()> {
  let out = ProcessCommand::new("git")
    .arg("--version")
    .output()
    .map_err(|_| anyhow!("`git` was not found on your PATH. Install git to manage plugins."))?;
  if !out.status.success() {
    bail!("`git --version` failed; check your git installation.");
  }
  Ok(())
}

fn git_clone(url: &str, dest: &Path) -> Result<()> {
  // `--` separates options from positionals so a URL is never mistaken for a flag.
  let status = ProcessCommand::new("git")
    .args(["clone", "--depth", "1", "--"])
    .arg(url)
    .arg(dest)
    .status()
    .context("running `git clone`")?;
  if !status.success() {
    bail!("git clone failed for {url}");
  }
  Ok(())
}

fn git_head_rev(dir: &Path) -> Result<String> {
  let out = ProcessCommand::new("git")
    .arg("-C")
    .arg(dir)
    .args(["rev-parse", "HEAD"])
    .output()
    .context("running `git rev-parse`")?;
  if !out.status.success() {
    bail!("git rev-parse failed in {}", dir.display());
  }
  Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Fast-forward a shallow clone to the remote default branch's latest commit.
fn git_update(dir: &Path) -> Result<()> {
  run_git(dir, &["fetch", "--depth", "1", "origin", "HEAD"])?;
  run_git(dir, &["reset", "--hard", "FETCH_HEAD"])?;
  Ok(())
}

fn run_git(dir: &Path, args: &[&str]) -> Result<()> {
  let out = ProcessCommand::new("git")
    .arg("-C")
    .arg(dir)
    .args(args)
    .output()
    .with_context(|| format!("running `git {}`", args.join(" ")))?;
  if !out.status.success() {
    let stderr = String::from_utf8_lossy(&out.stderr);
    bail!("git {} failed: {}", args.join(" "), stderr.trim());
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parse_github_shorthand() {
    let s = parse_repo_spec("owner/cool-plugin").unwrap();
    assert_eq!(s.name, "cool-plugin");
    assert_eq!(s.repo, "owner/cool-plugin");
    assert_eq!(s.url, "https://github.com/owner/cool-plugin.git");
  }

  #[test]
  fn parse_github_shorthand_strips_git_suffix() {
    let s = parse_repo_spec("owner/repo.git").unwrap();
    assert_eq!(s.name, "repo");
    assert_eq!(s.repo, "owner/repo");
    assert_eq!(s.url, "https://github.com/owner/repo.git");
  }

  #[test]
  fn parse_https_url() {
    let s = parse_repo_spec("https://gitlab.com/owner/repo.git").unwrap();
    assert_eq!(s.name, "repo");
    assert_eq!(s.repo, "owner/repo");
    assert_eq!(s.url, "https://gitlab.com/owner/repo.git");
  }

  #[test]
  fn parse_scp_style_url() {
    let s = parse_repo_spec("git@github.com:owner/repo.git").unwrap();
    assert_eq!(s.name, "repo");
    assert_eq!(s.repo, "owner/repo");
    assert_eq!(s.url, "git@github.com:owner/repo.git");
  }

  #[test]
  fn parse_rejects_garbage() {
    assert!(parse_repo_spec("").is_err());
    assert!(parse_repo_spec("just-a-name").is_err());
    assert!(parse_repo_spec("a/b/c").is_err());
  }

  #[test]
  fn parse_rejects_path_traversal_name() {
    // A spec whose tail would be `..` must be rejected.
    assert!(parse_repo_spec("owner/..").is_err());
  }

  #[test]
  fn parse_rejects_dotfile_name() {
    // A leading-dot name would be cloned but silently ignored by the loader; reject it.
    assert!(parse_repo_spec("owner/.hidden").is_err());
    assert!(parse_repo_spec("https://example.com/owner/.foo").is_err());
  }

  #[test]
  fn valid_names() {
    assert!(valid_plugin_name("lyrics"));
    assert!(valid_plugin_name("my-plugin_2.0"));
    assert!(!valid_plugin_name(""));
    assert!(!valid_plugin_name(".."));
    assert!(!valid_plugin_name("."));
    assert!(!valid_plugin_name(".hidden"));
    assert!(!valid_plugin_name("a/b"));
    assert!(!valid_plugin_name("a\\b"));
    assert!(!valid_plugin_name("space name"));
  }

  #[test]
  fn lock_roundtrip_and_mutations() {
    let mut lock = PluginLock::default();
    lock.upsert(LockedPlugin {
      name: "b".into(),
      repo: "o/b".into(),
      url: "u".into(),
      rev: "1".into(),
    });
    lock.upsert(LockedPlugin {
      name: "a".into(),
      repo: "o/a".into(),
      url: "u".into(),
      rev: "2".into(),
    });
    // Sorted by name.
    assert_eq!(lock.plugins[0].name, "a");
    assert_eq!(lock.plugins[1].name, "b");

    // Upsert replaces in place.
    lock.upsert(LockedPlugin {
      name: "a".into(),
      repo: "o/a".into(),
      url: "u".into(),
      rev: "99".into(),
    });
    assert_eq!(lock.plugins.len(), 2);
    assert_eq!(lock.plugins[0].rev, "99");

    let json = serde_json::to_string(&lock).unwrap();
    let back: PluginLock = serde_json::from_str(&json).unwrap();
    assert_eq!(back.plugins.len(), 2);
    assert_eq!(back.version, 1);

    assert!(lock.remove("a"));
    assert!(!lock.remove("a"));
    assert_eq!(lock.plugins.len(), 1);
  }

  #[test]
  fn load_lock_missing_is_empty() {
    let path =
      std::env::temp_dir().join(format!("spotatui_lock_missing_{}.json", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let lock = load_lock(&path).unwrap();
    assert!(lock.plugins.is_empty());
  }

  #[test]
  fn save_then_load_lock() {
    let path = std::env::temp_dir().join(format!("spotatui_lock_rt_{}.json", std::process::id()));
    let mut lock = PluginLock::default();
    lock.upsert(LockedPlugin {
      name: "lyrics".into(),
      repo: "owner/lyrics".into(),
      url: "https://github.com/owner/lyrics.git".into(),
      rev: "abcdef1234567890".into(),
    });
    save_lock(&path, &lock).unwrap();
    let back = load_lock(&path).unwrap();
    assert_eq!(back.plugins.len(), 1);
    assert_eq!(back.plugins[0].name, "lyrics");
    let _ = std::fs::remove_file(&path);
  }

  #[test]
  fn short_rev_handling() {
    assert_eq!(short_rev("abcdef1234"), "abcdef1");
    assert_eq!(short_rev("abc"), "abc");
    assert_eq!(short_rev(""), "unknown");
  }

  #[test]
  fn untracked_lists_loose_files_and_untracked_dirs() {
    let dir = std::env::temp_dir().join(format!("spotatui_untracked_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("tracked-plugin")).unwrap();
    std::fs::create_dir_all(dir.join("hand-installed")).unwrap();
    std::fs::create_dir_all(dir.join(".git")).unwrap();
    std::fs::write(dir.join("loose.lua"), "-- loose").unwrap();
    std::fs::write(dir.join("notes.txt"), "ignore me").unwrap();

    let mut got = untracked_plugins(&dir, &["tracked-plugin"]);
    got.sort();
    assert_eq!(
      got,
      vec!["hand-installed".to_string(), "loose.lua".to_string()]
    );

    let _ = std::fs::remove_dir_all(&dir);
  }
}
