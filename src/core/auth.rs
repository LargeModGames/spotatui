//! The Spotify login, as every build sees it, and private file writes.

use anyhow::{anyhow, Result};
use std::{fs, path::Path};

/// A cached Spotify login exists. A build without the `spotify-session`
/// feature never has one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpotifyLogin(());

impl SpotifyLogin {
  /// The cached login, if there is one.
  pub fn cached() -> Option<Self> {
    #[cfg(feature = "spotify-session")]
    return crate::infra::spotify::credentials::streaming_credentials_are_cached()
      .unwrap_or(false)
      .then_some(Self(()));
    #[cfg(not(feature = "spotify-session"))]
    None
  }

  #[cfg(any(test, feature = "spotify-session"))]
  pub(crate) fn new() -> Self {
    Self(())
  }
}

/// Logs in through Spotify's pairing page before the UI starts: shows the
/// code, opens the browser and waits for the approval.
pub async fn log_in_to_spotify(
  onboarding: &dyn crate::core::onboarding::Onboarding,
) -> Result<SpotifyLogin> {
  #[cfg(feature = "spotify-session")]
  {
    use crate::infra::spotify::{credentials, session};
    let pending = credentials::begin_login().await?;
    onboarding.info(&format!(
      "Log in to Spotify: open {} and confirm the code {}\n",
      pending.url(),
      pending.code()
    ));
    if let Err(e) = open::that_detached(pending.url()) {
      log::warn!("could not open the browser: {e}");
    }
    let credentials = pending.finish().await?;
    session::log_in(credentials)
      .await
      .map_err(|reason| anyhow!(reason.message()))?;
    onboarding.info("Logged in to Spotify.\n");
    Ok(SpotifyLogin(()))
  }
  #[cfg(not(feature = "spotify-session"))]
  {
    let _ = onboarding;
    Err(anyhow!("This build of spotatui has no Spotify support"))
  }
}

/// Forgets the cached Spotify login.
pub async fn forget_spotify_login() {
  #[cfg(feature = "spotify-session")]
  crate::infra::spotify::session::forget().await;
}
/// Atomically replace `path` with `contents`, written privately (0o600 on Unix).
///
/// The bytes go to a process-unique temporary sibling of the resolved target
/// first, then a rename swaps it over that target, so a symlinked `path` keeps
/// its link. The unique suffix (pid plus a per-process counter) keeps two
/// concurrent writers from sharing one temp file and renaming each other's
/// half-written bytes into place. On a failed write or rename the temp file is
/// removed best-effort so aborted writes do not accumulate.
pub(crate) fn write_private_file_atomic(path: &Path, contents: &[u8]) -> Result<()> {
  // Follow the link chain by hand: `canonicalize` fails on a dangling link and
  // would let the rename replace the link itself. The bound stops a link loop.
  // `join` keeps an absolute link and resolves a relative one against the
  // link's directory.
  let mut target = path.to_path_buf();
  for _ in 0..40 {
    let Ok(link) = fs::read_link(&target) else {
      break;
    };
    target = target.parent().unwrap_or(Path::new("")).join(link);
  }
  if fs::read_link(&target).is_ok() {
    return Err(anyhow!(
      "{} is a symlink loop or chain too long",
      path.display()
    ));
  }
  let tmp = unique_temp_path(&target);
  // Clean up on either failure (write or rename). Each attempt uses a fresh
  // unique name, so a leaked temp from a partial write would otherwise persist.
  if let Err(error) = create_private_file_exclusive(&tmp)
    .and_then(|mut file| std::io::Write::write_all(&mut file, contents))
    .and_then(|()| std::fs::rename(&tmp, &target))
  {
    let _ = std::fs::remove_file(&tmp);
    return Err(error.into());
  }
  Ok(())
}

/// Create a new private file; an existing path, a symlink included, is refused.
fn create_private_file_exclusive(path: &Path) -> std::io::Result<std::fs::File> {
  let mut options = std::fs::OpenOptions::new();
  options.write(true).create_new(true);
  #[cfg(unix)]
  {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
  }
  options.open(path)
}

/// A temporary sibling of `path` that no other process (or concurrent save in
/// this one) will pick, so atomic writes never collide on a shared temp file.
fn unique_temp_path(path: &Path) -> std::path::PathBuf {
  use std::sync::atomic::{AtomicU64, Ordering};
  static COUNTER: AtomicU64 = AtomicU64::new(0);
  let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
  let mut name = path
    .file_name()
    .map(|name| name.to_os_string())
    .unwrap_or_default();
  name.push(format!(".{}.{}.tmp", std::process::id(), seq));
  path.with_file_name(name)
}

#[cfg(test)]
mod tests {
  use super::*;
  #[cfg(unix)]
  use std::os::unix::fs::symlink as symlink_file;
  #[cfg(windows)]
  use std::os::windows::fs::symlink_file;
  use std::path::PathBuf;

  #[test]
  fn a_test_build_has_no_cached_login() {
    #[cfg(not(feature = "spotify-session"))]
    assert_eq!(SpotifyLogin::cached(), None);
    assert_eq!(SpotifyLogin::new(), SpotifyLogin::new());
  }

  #[cfg(not(feature = "spotify-session"))]
  #[tokio::test]
  async fn a_build_without_spotify_cannot_log_in() {
    let onboarding = crate::core::test_helpers::ScriptedOnboarding::with_answers(&[]);
    let error = log_in_to_spotify(&onboarding).await.unwrap_err();
    assert!(error.to_string().contains("no Spotify support"), "{error}");
  }
  /// A `config.yml` link in one directory to a target in another (the dotfiles
  /// shape); `None` when the platform refuses to create symlinks.
  fn linked_config(seed: Option<&[u8]>) -> Option<(tempfile::TempDir, PathBuf, PathBuf)> {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("store").join("config.yml");
    let link = root.path().join("config").join("config.yml");
    fs::create_dir(target.parent().unwrap()).unwrap();
    fs::create_dir(link.parent().unwrap()).unwrap();
    if let Some(bytes) = seed {
      fs::write(&target, bytes).unwrap();
    }
    symlink_file(&target, &link).ok()?;
    Some((root, target, link))
  }

  fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path).unwrap().file_type().is_symlink()
  }

  #[test]
  fn atomic_write_through_a_dangling_symlink_keeps_the_link() {
    let Some((_root, target, link)) = linked_config(None) else {
      return;
    };

    write_private_file_atomic(&link, b"new").unwrap();

    assert!(is_symlink(&link));
    assert_eq!(fs::read(&target).unwrap(), b"new");
  }

  #[test]
  fn atomic_write_through_a_symlink_keeps_the_link() {
    let Some((_root, target, link)) = linked_config(Some(b"old")) else {
      return;
    };

    write_private_file_atomic(&link, b"new").unwrap();

    assert!(is_symlink(&link));
    assert_eq!(fs::read(&target).unwrap(), b"new");
  }

  #[test]
  fn atomic_write_onto_a_read_only_target_keeps_the_link() {
    let Some((_root, target, link)) = linked_config(Some(b"old")) else {
      return;
    };
    let store = target.parent().unwrap();
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt;
      fs::set_permissions(store, fs::Permissions::from_mode(0o500)).unwrap();
    }
    #[cfg(windows)]
    {
      let mut permissions = fs::metadata(&target).unwrap().permissions();
      permissions.set_readonly(true);
      fs::set_permissions(&target, permissions).unwrap();
    }

    let _ = write_private_file_atomic(&link, b"new");

    assert!(is_symlink(&link));
    // No temp sibling is left behind, on a failed write or on a root-run success.
    assert_eq!(fs::read_dir(store).unwrap().count(), 1);
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt;
      fs::set_permissions(store, fs::Permissions::from_mode(0o700)).unwrap();
    }
  }

  #[test]
  fn atomic_write_into_a_symlink_loop_fails_and_keeps_both_links() {
    let Some((_root, target, link)) = linked_config(None) else {
      return;
    };
    symlink_file(&link, &target).unwrap();

    assert!(write_private_file_atomic(&link, b"new").is_err());

    assert!(is_symlink(&link));
    assert!(is_symlink(&target));
  }

  #[test]
  fn an_exclusive_create_refuses_a_planted_symlink() {
    let Some((_root, target, link)) = linked_config(None) else {
      return;
    };

    let error = create_private_file_exclusive(&link).unwrap_err();

    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    assert!(!target.exists());
  }
}
