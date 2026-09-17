use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use tokio::sync::Mutex;

use super::{PlaylistRead, SyncClient};
use crate::core::app::App;
use crate::core::playlist_sync::{normalize_text, strip_title_suffix, SyncTrack};
use crate::infra::network::IoEvent;
use crate::infra::youtube::playlists::{self, StoredTrack};
use crate::infra::youtube::YouTubeSource;

/// Duration window for a YouTube match, wider than the catalog sources' because
/// uploads carry intros and outros.
const DURATION_TOLERANCE_MS: u64 = 3_000;

/// The local `youtube_playlists.yml` as a sync endpoint. The path is injected,
/// so a test points it at a tempdir instead of the user's config directory.
pub(crate) struct YouTubeSyncClient {
  source: YouTubeSource,
  path: PathBuf,
  app: Arc<Mutex<App>>,
}

impl YouTubeSyncClient {
  pub(crate) fn new(source: YouTubeSource, path: PathBuf, app: Arc<Mutex<App>>) -> Self {
    YouTubeSyncClient { source, path, app }
  }

  /// One load, mutate and save transaction against the injected file, on the blocking pool.
  async fn with_file<T, F>(&self, mutate: F) -> Result<T>
  where
    T: Send + 'static,
    F: FnOnce(&mut playlists::PlaylistsFile) -> Result<T> + Send + 'static,
  {
    let path = self.path.clone();
    tokio::task::spawn_blocking(move || {
      let mut file = playlists::load(&path)?;
      let out = mutate(&mut file)?;
      playlists::save(&path, &file)?;
      Ok(out)
    })
    .await
    .context("YouTube playlists task failed")?
  }
}

impl SyncClient for YouTubeSyncClient {
  async fn find_playlist(&self, name: &str) -> Result<Option<String>> {
    let path = self.path.clone();
    let file = tokio::task::spawn_blocking(move || playlists::load(&path))
      .await
      .context("YouTube playlists task failed")??;
    Ok(
      file
        .playlists
        .iter()
        .find(|playlist| super::same_name(&playlist.name, name))
        .map(|playlist| playlists::uri_for_playlist_id(&playlist.id)),
    )
  }

  async fn create_playlist(&self, name: &str) -> Result<String> {
    let name = name.to_string();
    let id = self
      .with_file(move |file| playlists::create_playlist(file, &name))
      .await?;
    let uri = playlists::uri_for_playlist_id(&id);
    self.app.lock().await.dispatch(IoEvent::GetYouTubePlaylists);
    Ok(uri)
  }

  async fn read_playlist(&self, playlist_uri: &str) -> Result<PlaylistRead> {
    let path = self.path.clone();
    let file = tokio::task::spawn_blocking(move || playlists::load(&path))
      .await
      .context("YouTube playlists task failed")??;
    let playlist = playlists::find_playlist(&file, playlist_uri)
      .ok_or_else(|| anyhow!("no such YouTube playlist: {playlist_uri}"))?;
    let tracks: Vec<SyncTrack> = playlist.tracks.iter().map(stored_to_sync_track).collect();
    Ok(PlaylistRead::from(tracks))
  }

  async fn resolve(&self, target: &SyncTrack) -> Result<Option<SyncTrack>> {
    let found = self
      .source
      .sync_search(&super::catalog_query(target))
      .await?;
    Ok(pick_youtube(target, &found).map(|index| found[index].clone()))
  }

  async fn add(&self, playlist_uri: &str, tracks: &[SyncTrack]) -> Result<()> {
    if tracks.is_empty() {
      return Ok(());
    }
    let uri = playlist_uri.to_string();
    let rows: Vec<StoredTrack> = tracks.iter().map(sync_to_stored_track).collect();
    self
      .with_file(move |file| {
        for row in rows {
          playlists::add_track(file, &uri, row)?;
        }
        Ok(())
      })
      .await?;
    self.app.lock().await.dispatch(IoEvent::GetYouTubePlaylists);
    Ok(())
  }

  async fn remove(&self, playlist_uri: &str, keys: &[String]) -> Result<()> {
    if keys.is_empty() {
      return Ok(());
    }
    let uri = playlist_uri.to_string();
    let keys = keys.to_vec();
    self
      .with_file(move |file| {
        for key in &keys {
          if let Err(e) = playlists::remove_track(file, &uri, key) {
            log::debug!("YouTube mirror: {e}");
          }
        }
        Ok(())
      })
      .await?;
    self.app.lock().await.dispatch(IoEvent::GetYouTubePlaylists);
    Ok(())
  }
}

/// Map a stored row onto the sync currency; a stored zero duration is unknown.
fn stored_to_sync_track(t: &StoredTrack) -> SyncTrack {
  SyncTrack {
    key: t.video_id.clone(),
    isrc: None,
    title: t.title.clone(),
    artist: t.channel.clone(),
    duration_ms: (t.duration_ms > 0).then_some(t.duration_ms),
  }
}

/// Map a sync row onto a stored one; an unknown duration stores as zero.
fn sync_to_stored_track(t: &SyncTrack) -> StoredTrack {
  StoredTrack {
    video_id: t.key.clone(),
    title: t.title.clone(),
    channel: t.artist.clone(),
    duration_ms: t.duration_ms.unwrap_or(0),
  }
}

/// Duration window for a video on another channel: a re-upload of the same
/// audio has to be this close, with both durations known.
const REUPLOAD_TOLERANCE_MS: u64 = 2_000;

/// The candidate that is the master track. First choice: the artist's own
/// channel (or a `- Topic` one), a title that contains the master title, and a
/// duration within [`DURATION_TOLERANCE_MS`], where an unknown duration never
/// vetoes. Second choice: any channel, when the title matches and both
/// durations are known and within [`REUPLOAD_TOLERANCE_MS`].
fn pick_youtube(target: &SyncTrack, candidates: &[SyncTrack]) -> Option<usize> {
  let title = normalize_text(&target.title);
  if title.is_empty() {
    return None;
  }
  let stripped = normalize_text(&strip_title_suffix(&target.title));
  let title_ok = |candidate: &SyncTrack| {
    let listed = normalize_text(&candidate.title);
    listed.contains(&title) || (!stripped.is_empty() && listed.contains(&stripped))
  };
  let own = candidates.iter().position(|candidate| {
    let within = match (target.duration_ms, candidate.duration_ms) {
      (Some(want), Some(got)) => want.abs_diff(got) <= DURATION_TOLERANCE_MS,
      _ => true,
    };
    let own_channel = candidate.artist.to_lowercase().ends_with(" - topic")
      || same_artist(&candidate.artist, &target.artist);
    within && own_channel && title_ok(candidate)
  });
  own.or_else(|| {
    candidates.iter().position(|candidate| {
      let close = match (target.duration_ms, candidate.duration_ms) {
        (Some(want), Some(got)) => want.abs_diff(got) <= REUPLOAD_TOLERANCE_MS,
        _ => false,
      };
      close && title_ok(candidate)
    })
  })
}

/// Whether a channel name is the artist's, ignoring spaces, case and symbols,
/// so `ImagineDragons` and `Axwell Λ Ingrosso` still count.
fn same_artist(channel: &str, artist: &str) -> bool {
  let (left, right) = (compact(channel), compact(artist));
  if left.is_empty() || right.is_empty() {
    return normalize_text(channel) == normalize_text(artist);
  }
  left == right
}

/// Lowercase ASCII letters and digits only.
fn compact(value: &str) -> String {
  value
    .chars()
    .filter(char::is_ascii_alphanumeric)
    .map(|ch| ch.to_ascii_lowercase())
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::user_config::UserConfig;
  use std::path::Path;
  use std::sync::mpsc::{channel, Receiver};

  fn track(key: &str, title: &str, artist: &str, duration_ms: Option<u64>) -> SyncTrack {
    SyncTrack {
      key: key.to_string(),
      isrc: None,
      title: title.to_string(),
      artist: artist.to_string(),
      duration_ms,
    }
  }

  fn client_for(path: &Path) -> (YouTubeSyncClient, Receiver<IoEvent>) {
    let (tx, rx) = channel();
    let app = Arc::new(Mutex::new(App::new(tx, UserConfig::new(), None)));
    let client = YouTubeSyncClient::new(YouTubeSource::new(None), path.to_path_buf(), app);
    (client, rx)
  }

  fn seed_playlist(path: &Path) -> String {
    let mut file = playlists::PlaylistsFile::default();
    let id = playlists::create_playlist(&mut file, "Focus").unwrap();
    playlists::save(path, &file).unwrap();
    playlists::uri_for_playlist_id(&id)
  }

  #[test]
  fn a_topic_channel_wins_over_an_earlier_upload() {
    let target = track("m1", "Creep", "Radiohead", Some(238_000));
    let candidates = vec![
      track("x0", "Creep (Live)", "Some Fan Channel", Some(238_000)),
      track("x1", "Creep", "Radiohead - Topic", Some(238_500)),
      track("x2", "Creep", "Radiohead", Some(238_000)),
    ];
    assert_eq!(pick_youtube(&target, &candidates), Some(1));

    let without_topic = vec![candidates[0].clone(), candidates[2].clone()];
    assert_eq!(pick_youtube(&target, &without_topic), Some(1));

    let reupload_only = vec![candidates[0].clone()];
    assert_eq!(pick_youtube(&target, &reupload_only), Some(0));

    let reupload_off = vec![track(
      "x0",
      "Creep (Lyrics)",
      "Some Fan Channel",
      Some(241_000),
    )];
    assert_eq!(pick_youtube(&target, &reupload_off), None);

    let reupload_blind = vec![track("x0", "Creep (Lyrics)", "Some Fan Channel", None)];
    assert_eq!(pick_youtube(&target, &reupload_blind), None);
  }

  #[test]
  fn a_channel_spelled_without_spaces_or_with_a_symbol_is_the_artists() {
    let target = track("m1", "Believer", "Imagine Dragons", Some(204_346));
    let official = vec![track(
      "x1",
      "Imagine Dragons - Believer (Audio)",
      "ImagineDragons",
      Some(203_000),
    )];
    assert_eq!(pick_youtube(&target, &official), Some(0));

    let target = track("m2", "Dreamer", "Axwell /\\ Ingrosso", Some(251_147));
    let official = vec![track(
      "x2",
      "Dreamer (Matisse & Sadko Remix)",
      "Axwell \u{39b} Ingrosso",
      Some(251_000),
    )];
    assert_eq!(pick_youtube(&target, &official), Some(0));
    assert!(!same_artist("Some Fan Channel", "Imagine Dragons"));
  }

  #[test]
  fn a_feat_or_edition_suffix_on_the_master_title_is_forgiven() {
    let target = track(
      "m1",
      "No Sleep (feat. Bonn)",
      "Martin Garrix",
      Some(207_094),
    );
    let candidates = vec![
      track(
        "x0",
        "Martin Garrix - High On Life",
        "Martin Garrix",
        Some(227_000),
      ),
      track(
        "x1",
        "Martin Garrix feat. Bonn - No Sleep (Official Video)",
        "Martin Garrix",
        Some(208_000),
      ),
    ];
    assert_eq!(pick_youtube(&target, &candidates), Some(1));
  }

  #[test]
  fn a_title_that_does_not_carry_the_master_title_is_no_candidate() {
    let target = track("m1", "Creep", "Radiohead", Some(238_000));
    let candidates = vec![track(
      "x1",
      "Karma Police",
      "Radiohead - Topic",
      Some(238_000),
    )];
    assert_eq!(pick_youtube(&target, &candidates), None);

    let blank = track("m2", "  ", "Radiohead", None);
    assert_eq!(pick_youtube(&blank, &candidates), None);
  }

  #[test]
  fn an_unknown_duration_never_reads_as_zero() {
    let target = track("m1", "Creep", "Radiohead", Some(238_000));
    let too_long = vec![track("x1", "Creep", "Radiohead - Topic", Some(243_000))];
    assert_eq!(pick_youtube(&target, &too_long), None);

    let edge = vec![track("x2", "Creep", "Radiohead - Topic", Some(241_000))];
    assert_eq!(pick_youtube(&target, &edge), Some(0));

    let unknown = vec![track("x3", "Creep", "Radiohead - Topic", None)];
    assert_eq!(pick_youtube(&target, &unknown), Some(0));
  }

  #[tokio::test]
  async fn creating_a_playlist_writes_the_file_and_answers_its_uri() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("yt_playlists.yml");
    let (client, rx) = client_for(&path);

    let uri = client.create_playlist("Road Trip").await.unwrap();

    assert!(matches!(rx.try_recv(), Ok(IoEvent::GetYouTubePlaylists)));
    assert!(uri.starts_with("youtube:playlist:"));
    let file = playlists::load(&path).unwrap();
    assert_eq!(
      playlists::find_playlist(&file, &uri).map(|p| p.name.as_str()),
      Some("Road Trip")
    );
    assert!(client.read_playlist(&uri).await.unwrap().tracks.is_empty());
    assert_eq!(
      client.find_playlist(" road trip ").await.unwrap(),
      Some(uri)
    );
    assert_eq!(client.find_playlist("Nope").await.unwrap(), None);
  }

  #[tokio::test]
  async fn a_playlist_read_and_write_round_trip_through_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("yt_playlists.yml");
    let uri = seed_playlist(&path);
    let (client, rx) = client_for(&path);

    assert!(client.read_playlist(&uri).await.unwrap().tracks.is_empty());

    client
      .add(
        &uri,
        &[track("vid1", "Get Lucky", "Daft Punk", Some(249_000))],
      )
      .await
      .unwrap();
    assert!(matches!(rx.try_recv(), Ok(IoEvent::GetYouTubePlaylists)));

    let read = client.read_playlist(&uri).await.unwrap();
    assert_eq!(read.keys(), vec!["vid1".to_string()]);
    assert_eq!(read.tracks[0].title, "Get Lucky");
    assert_eq!(read.tracks[0].artist, "Daft Punk");
    assert_eq!(read.tracks[0].duration_ms, Some(249_000));
    assert!(read.not_syncable.is_empty());

    client.remove(&uri, &["vid1".to_string()]).await.unwrap();
    assert!(client.read_playlist(&uri).await.unwrap().tracks.is_empty());
  }

  #[tokio::test]
  async fn removing_a_video_that_is_already_gone_is_not_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("yt_playlists.yml");
    let uri = seed_playlist(&path);
    let (client, _rx) = client_for(&path);

    client
      .add(
        &uri,
        &[track("vid1", "Get Lucky", "Daft Punk", Some(249_000))],
      )
      .await
      .unwrap();
    client
      .remove(&uri, &["ghost".to_string(), "vid1".to_string()])
      .await
      .unwrap();
    assert!(client.read_playlist(&uri).await.unwrap().tracks.is_empty());
  }
}
