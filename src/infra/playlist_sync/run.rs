//! One sync run: load the store, walk the links, and bring every mirror up to
//! date through the engine's plan.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use tokio::sync::Mutex;

use super::{SyncClient, SyncClients};
use crate::core::app::App;
use crate::core::playlist_sync::store;
use crate::core::playlist_sync::{
  pick_candidate, plan, Endpoint, Link, LinkOutcome, LinkReport, Mirror, MirrorRun, SyncReport,
  SyncTrack, UnmatchReason, Unmatched,
};
use crate::core::source::Source;
use crate::infra::network::requests;

/// Mutable state one run threads through its links.
struct RunState<'a> {
  app: &'a Arc<Mutex<App>>,
  /// The store, for the checkpoint saves between batches.
  path: PathBuf,
  /// One RFC 3339 stamp for the whole run.
  now: String,
  dry_run: bool,
  /// Whether tracks the last run found no candidate for are searched again.
  retry_unmatched: bool,
  /// Set when a rate limit stopped the run; no further link is started.
  stop: Option<String>,
}

/// Every link in the store, or the one `filter` names.
pub(crate) async fn run_all<C: SyncClients>(
  clients: &C,
  app: &Arc<Mutex<App>>,
  path: &Path,
  filter: Option<&str>,
  dry_run: bool,
  retry_unmatched: bool,
) -> SyncReport {
  let mut report = SyncReport {
    dry_run,
    ..Default::default()
  };

  let loaded = {
    let path = path.to_owned();
    tokio::task::spawn_blocking(move || store::load(&path)).await
  };
  let mut file = match loaded {
    Ok(Ok(file)) => file,
    Ok(Err(e)) => {
      report.error = Some(format!("{e:#}"));
      return report;
    }
    Err(e) => {
      report.error = Some(e.to_string());
      return report;
    }
  };
  app.lock().await.set_playlist_sync_links(file.links.clone());

  let selected: Vec<usize> = match filter {
    Some(filter) => file
      .links
      .iter()
      .enumerate()
      .filter(|(_, link)| link.matches_filter(filter))
      .map(|(index, _)| index)
      .collect(),
    None => (0..file.links.len()).collect(),
  };
  if selected.is_empty() {
    if let Some(filter) = filter {
      report.error = Some(format!("no link matches \"{filter}\""));
    }
    return report;
  }

  log::info!(
    "playlist sync: {} of {} links, dry_run={dry_run}",
    selected.len(),
    file.links.len()
  );
  let mut state = RunState {
    app,
    path: path.to_owned(),
    now: chrono::Utc::now().to_rfc3339(),
    dry_run,
    retry_unmatched,
    stop: None,
  };

  for index in selected {
    log::info!("playlist sync: link {} starts", file.links[index].id);
    let entry = sync_link(clients, &mut state, &mut file.links[index]).await;
    log::info!("playlist sync: {}", entry.line());
    report.links.push(entry);

    if !dry_run {
      let synced = file.links[index].clone();
      let path = path.to_owned();
      let saved = tokio::task::spawn_blocking(move || save_synced_link(&path, synced)).await;
      match saved {
        Ok(Ok(links)) => app.lock().await.set_playlist_sync_links(links),
        Ok(Err(e)) => {
          report.error = Some(format!("{e:#}"));
          break;
        }
        Err(e) => {
          report.error = Some(e.to_string());
          break;
        }
      }
    }

    if let Some(text) = &state.stop {
      report.error = Some(text.clone());
      break;
    }
  }

  report
}

/// Splice one synced link into a fresh load of the store, so a link added or
/// removed while the run was working survives; answers the links as saved.
fn save_synced_link(path: &Path, mut synced: Link) -> Result<Vec<Link>> {
  let mut current = store::load(path)?;
  let Some(slot) = current.links.iter_mut().find(|link| link.id == synced.id) else {
    return Ok(current.links);
  };
  for mirror in slot.mirrors.drain(..) {
    if !synced
      .mirrors
      .iter()
      .any(|known| known.endpoint.playlist_uri == mirror.endpoint.playlist_uri)
    {
      synced.mirrors.push(mirror);
    }
  }
  *slot = synced;
  store::save(path, &current)?;
  Ok(current.links)
}

/// Adopt the mirror source's playlist named after the master, or create one; store
/// the link, publish it, and answer the link id and whether a playlist was adopted.
pub(crate) async fn link_mirror<C: SyncClients>(
  clients: &C,
  app: &Arc<Mutex<App>>,
  path: &Path,
  master: Endpoint,
  mirror: Source,
) -> Result<(String, bool)> {
  let client = clients
    .open(&Endpoint {
      source: mirror,
      playlist_uri: String::new(),
      name: master.name.clone(),
    })
    .await?;
  let (playlist_uri, adopted) = match client.find_playlist(&master.name).await? {
    Some(uri) => (uri, true),
    None => (client.create_playlist(&master.name).await?, false),
  };
  log::info!(
    "playlist sync: {} mirror {} {playlist_uri}",
    mirror.label(),
    if adopted { "adopted" } else { "created" }
  );
  let endpoint = Endpoint {
    source: mirror,
    playlist_uri,
    name: master.name.clone(),
  };
  let owned = path.to_owned();
  let (id, links) = tokio::task::spawn_blocking(move || -> Result<(String, Vec<Link>)> {
    let mut file = store::load(&owned)?;
    let id = file.link(master, endpoint);
    store::save(&owned, &file)?;
    Ok((id, file.links))
  })
  .await
  .context("playlist sync task failed")??;
  app.lock().await.set_playlist_sync_links(links);
  Ok((id, adopted))
}

/// Drop the link with `id` from the store and publish the rest.
pub(crate) async fn remove_link(app: &Arc<Mutex<App>>, path: &Path, id: &str) -> Result<()> {
  let owned = path.to_owned();
  let id = id.to_string();
  let links = tokio::task::spawn_blocking(move || -> Result<Vec<Link>> {
    let mut file = store::load(&owned)?;
    if !file.remove_link(&id) {
      bail!("no link with id {id}");
    }
    store::save(&owned, &file)?;
    Ok(file.links)
  })
  .await
  .context("playlist sync task failed")??;
  app.lock().await.set_playlist_sync_links(links);
  Ok(())
}

/// One link: read the master once, then bring every mirror up to date.
async fn sync_link<C: SyncClients>(
  clients: &C,
  state: &mut RunState<'_>,
  link: &mut Link,
) -> LinkReport {
  let mut report = LinkReport::new(&link.id, &link.master.name);
  if link.mirrors.is_empty() {
    report.note(LinkOutcome::Skipped("no mirrors".to_string()));
    return report;
  }

  let client = match clients.open(&link.master).await {
    Ok(client) => client,
    Err(e) => {
      report.note(LinkOutcome::Skipped(format!("{e:#}")));
      return report;
    }
  };
  let read = match client.read_playlist(&link.master.playlist_uri).await {
    Ok(read) => read,
    Err(e) => {
      report.note(classify(link.master.source, &e, &mut state.stop));
      return report;
    }
  };

  for index in 0..link.mirrors.len() {
    let mirror_client = match clients.open(&link.mirrors[index].endpoint).await {
      Ok(client) => client,
      Err(e) => {
        report.note(LinkOutcome::Skipped(format!("{e:#}")));
        continue;
      }
    };
    let run = sync_mirror(
      &mirror_client,
      state,
      link,
      index,
      &read.tracks,
      &read.not_syncable,
    )
    .await;
    report.absorb(run);
    if state.stop.is_some() {
      return report;
    }
  }

  report
}

/// Resolves per progress line, per batch of adds, and per checkpoint save.
const BATCH: usize = 10;

/// One mirror: plan, resolve in batches, write each batch as it lands, and
/// close the run on its match cache. `index` names the mirror inside `link`,
/// so a checkpoint can save the whole link between batches.
async fn sync_mirror<C: SyncClient>(
  client: &C,
  state: &mut RunState<'_>,
  link: &mut Link,
  index: usize,
  master: &[SyncTrack],
  not_syncable: &[SyncTrack],
) -> MirrorRun {
  let endpoint = link.mirrors[index].endpoint.clone();
  let uri = endpoint.playlist_uri.as_str();
  let read = match client.read_playlist(uri).await {
    Ok(read) => read,
    Err(e) => {
      return MirrorRun {
        outcome: classify(endpoint.source, &e, &mut state.stop),
        ..Default::default()
      }
    }
  };
  let mut on_mirror = read.keys();
  let mut updated = link.mirrors[index].matches.clone();
  let mut first = plan(master, &on_mirror, &updated);
  let mut pending: Vec<(String, String)> = Vec::new();
  let mut resolved: BTreeMap<String, SyncTrack> = BTreeMap::new();

  // Pair what the mirror already holds before any search is paid for.
  let mut to_resolve = Vec::with_capacity(first.to_resolve.len());
  for track in std::mem::take(&mut first.to_resolve) {
    match pick_candidate(&track, &read.tracks) {
      Some(found) => {
        let found = read.tracks[found].clone();
        pending.push((track.key.clone(), found.key.clone()));
        updated.insert(track.key.clone(), found.key.clone());
        resolved.insert(found.key.clone(), found);
      }
      None => to_resolve.push(track),
    }
  }
  first.to_resolve = to_resolve;

  let mut unmatched: Vec<Unmatched> = not_syncable
    .iter()
    .map(|track| Unmatched::new(track, UnmatchReason::NotSyncable))
    .collect();
  if !state.retry_unmatched {
    // A startup run keeps last time's "no candidate" verdicts; a manual run searches again.
    let held: Vec<Unmatched> = link.mirrors[index]
      .unmatched
      .iter()
      .filter(|entry| entry.reason == UnmatchReason::NoCandidate)
      .filter(|entry| {
        first
          .to_resolve
          .iter()
          .any(|track| track.key == entry.master_key)
      })
      .cloned()
      .collect();
    first
      .to_resolve
      .retain(|track| !held.iter().any(|entry| entry.master_key == track.key));
    unmatched.extend(held);
  }
  let mut added = 0usize;

  // Cached matches the mirror lost come back before any search is paid for.
  if !state.dry_run {
    if let Err(e) = flush_adds(
      client,
      uri,
      master,
      &mut on_mirror,
      &updated,
      &resolved,
      &mut added,
    )
    .await
    {
      return MirrorRun {
        added,
        unmatched: unmatched.len(),
        outcome: classify(endpoint.source, &e, &mut state.stop),
        ..Default::default()
      };
    }
  }

  let total = first.to_resolve.len();
  for (done, track) in (1..).zip(first.to_resolve.iter()) {
    match client.resolve(track).await {
      Ok(Some(found)) => {
        pending.push((track.key.clone(), found.key.clone()));
        updated.insert(track.key.clone(), found.key.clone());
        resolved.insert(found.key.clone(), found);
      }
      Ok(None) => unmatched.push(Unmatched::new(track, UnmatchReason::NoCandidate)),
      Err(e) => {
        if requests::is_rate_limited_error(&e) {
          // Keep the searches this run paid for, so the next run resumes past them.
          if !state.dry_run {
            commit(&mut link.mirrors[index], &mut pending);
          }
          return MirrorRun {
            added,
            unmatched: unmatched.len(),
            outcome: classify(endpoint.source, &e, &mut state.stop),
            ..Default::default()
          };
        }
        unmatched.push(Unmatched::new(
          track,
          UnmatchReason::SearchFailed(format!("{e:#}")),
        ));
      }
    }
    if done % BATCH != 0 || done == total {
      continue;
    }
    state.app.lock().await.set_status_message(
      format!("Playlist sync: {} {done}/{total}", endpoint.name),
      8,
    );
    if state.dry_run {
      continue;
    }
    commit(&mut link.mirrors[index], &mut pending);
    if let Err(e) = flush_adds(
      client,
      uri,
      master,
      &mut on_mirror,
      &updated,
      &resolved,
      &mut added,
    )
    .await
    {
      return MirrorRun {
        added,
        unmatched: unmatched.len(),
        outcome: classify(endpoint.source, &e, &mut state.stop),
        ..Default::default()
      };
    }
    checkpoint(state, link).await;
  }

  if state.dry_run {
    let writes = plan(master, &on_mirror, &updated);
    let rows = add_rows(master, &writes.to_add, &resolved);
    return MirrorRun {
      added: rows.len(),
      removed: writes.to_remove.len(),
      unmatched: unmatched.len(),
      outcome: LinkOutcome::Ran,
    };
  }

  commit(&mut link.mirrors[index], &mut pending);
  if let Err(e) = flush_adds(
    client,
    uri,
    master,
    &mut on_mirror,
    &updated,
    &resolved,
    &mut added,
  )
  .await
  {
    return MirrorRun {
      added,
      unmatched: unmatched.len(),
      outcome: classify(endpoint.source, &e, &mut state.stop),
      ..Default::default()
    };
  }
  let writes = plan(master, &on_mirror, &updated);
  if !writes.to_remove.is_empty() {
    if let Err(e) = client.remove(uri, &writes.to_remove).await {
      return MirrorRun {
        added,
        unmatched: unmatched.len(),
        outcome: classify(endpoint.source, &e, &mut state.stop),
        ..Default::default()
      };
    }
  }

  let mirror = &mut link.mirrors[index];
  mirror.drop_stale(&writes.stale);
  let count = unmatched.len();
  mirror.finish_run(unmatched, state.now.clone());

  MirrorRun {
    added,
    removed: writes.to_remove.len(),
    unmatched: count,
    outcome: LinkOutcome::Ran,
  }
}

/// Send every planned add the mirror does not hold yet, in master order.
async fn flush_adds<C: SyncClient>(
  client: &C,
  uri: &str,
  master: &[SyncTrack],
  on_mirror: &mut Vec<String>,
  matches: &BTreeMap<String, String>,
  resolved: &BTreeMap<String, SyncTrack>,
  added: &mut usize,
) -> Result<()> {
  let writes = plan(master, on_mirror, matches);
  let rows = add_rows(master, &writes.to_add, resolved);
  for chunk in rows.chunks(BATCH) {
    client.add(uri, chunk).await?;
    on_mirror.extend(chunk.iter().map(|row| row.key.clone()));
    *added += chunk.len();
  }
  Ok(())
}

/// Record the resolutions since the last commit on the mirror's match cache.
fn commit(mirror: &mut Mirror, pending: &mut Vec<(String, String)>) {
  for (master_key, mirror_key) in pending.drain(..) {
    mirror.record_match(&master_key, &mirror_key);
  }
}

/// Save the link as it stands and publish the links; a failure here is logged,
/// and the link's own save reports it.
async fn checkpoint(state: &RunState<'_>, link: &Link) {
  let path = state.path.clone();
  let synced = link.clone();
  match tokio::task::spawn_blocking(move || save_synced_link(&path, synced)).await {
    Ok(Ok(links)) => state.app.lock().await.set_playlist_sync_links(links),
    Ok(Err(e)) => log::warn!("playlist sync: checkpoint failed: {e:#}"),
    Err(e) => log::warn!("playlist sync: checkpoint failed: {e}"),
  }
}

/// The rows to send a mirror: this run's candidate when it resolved one, else
/// the master track under the mirror's key.
fn add_rows(
  master: &[SyncTrack],
  to_add: &[(String, String)],
  resolved: &BTreeMap<String, SyncTrack>,
) -> Vec<SyncTrack> {
  let mut by_key: BTreeMap<&str, &SyncTrack> = BTreeMap::new();
  for track in master {
    by_key.entry(track.key.as_str()).or_insert(track);
  }

  let mut rows = Vec::with_capacity(to_add.len());
  for (master_key, mirror_key) in to_add {
    if let Some(found) = resolved.get(mirror_key) {
      rows.push(found.clone());
      continue;
    }
    if let Some(track) = by_key.get(master_key.as_str()) {
      rows.push(SyncTrack {
        key: mirror_key.clone(),
        ..(*track).clone()
      });
    }
  }
  rows
}

/// How a failed client call ends this mirror: a rate limit stops the run, an
/// unreachable source is a skip, anything else is a failure.
fn classify(source: Source, e: &anyhow::Error, stop: &mut Option<String>) -> LinkOutcome {
  if requests::is_rate_limited_error(e) {
    let text = format!("{e:#}");
    *stop = Some(text.clone());
    return LinkOutcome::Failed(text);
  }
  if requests::is_transient_network_error(e) {
    return LinkOutcome::Skipped(format!("{} unreachable: {e}", source.label()));
  }
  LinkOutcome::Failed(format!("{e:#}"))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::playlist_sync::Endpoint;
  use crate::core::user_config::UserConfig;
  use crate::infra::network::IoEvent;
  use crate::infra::playlist_sync::PlaylistRead;
  use anyhow::{anyhow, Result};
  use std::path::PathBuf;
  use tempfile::TempDir;

  /// Scripted reads and resolutions, recording every write.
  #[derive(Default)]
  struct FakeClient {
    tracks: Vec<SyncTrack>,
    not_syncable: Vec<SyncTrack>,
    /// The mirror candidate per master key; a missing key resolves to nothing.
    hits: BTreeMap<String, SyncTrack>,
    read_error: Option<String>,
    /// The error `resolve` answers with, per master key.
    search_errors: BTreeMap<String, String>,
    searched: std::sync::Mutex<Vec<String>>,
    added: std::sync::Mutex<Vec<Vec<SyncTrack>>>,
    removed: std::sync::Mutex<Vec<Vec<String>>>,
    created: std::sync::Mutex<Vec<String>>,
    /// Playlists this source already has, by exact name.
    existing: BTreeMap<String, String>,
    /// The add call, counted from one, from which every add fails.
    fail_add_from: Option<usize>,
  }

  impl FakeClient {
    fn added_rows(&self) -> Vec<Vec<SyncTrack>> {
      self.added.lock().unwrap().clone()
    }

    fn added_keys(&self) -> Vec<Vec<String>> {
      self
        .added_rows()
        .iter()
        .map(|rows| rows.iter().map(|row| row.key.clone()).collect())
        .collect()
    }

    fn removed_keys(&self) -> Vec<Vec<String>> {
      self.removed.lock().unwrap().clone()
    }

    fn searched_keys(&self) -> Vec<String> {
      self.searched.lock().unwrap().clone()
    }

    fn created_names(&self) -> Vec<String> {
      self.created.lock().unwrap().clone()
    }
  }

  impl SyncClient for Arc<FakeClient> {
    async fn find_playlist(&self, name: &str) -> Result<Option<String>> {
      Ok(self.existing.get(name).cloned())
    }

    async fn create_playlist(&self, name: &str) -> Result<String> {
      self.created.lock().unwrap().push(name.to_string());
      let count = self.created.lock().unwrap().len();
      Ok(format!("fake:playlist:{count}"))
    }

    async fn read_playlist(&self, _playlist_uri: &str) -> Result<PlaylistRead> {
      if let Some(text) = &self.read_error {
        return Err(anyhow!("{text}"));
      }
      Ok(PlaylistRead {
        tracks: self.tracks.clone(),
        not_syncable: self.not_syncable.clone(),
      })
    }

    async fn resolve(&self, target: &SyncTrack) -> Result<Option<SyncTrack>> {
      self.searched.lock().unwrap().push(target.key.clone());
      if let Some(text) = self.search_errors.get(&target.key) {
        return Err(anyhow!("{text}"));
      }
      Ok(self.hits.get(&target.key).cloned())
    }

    async fn add(&self, _playlist_uri: &str, tracks: &[SyncTrack]) -> Result<()> {
      let call = self.added.lock().unwrap().len() + 1;
      if self.fail_add_from.is_some_and(|from| call >= from) {
        return Err(anyhow!("the mirror refused the write"));
      }
      self.added.lock().unwrap().push(tracks.to_vec());
      Ok(())
    }

    async fn remove(&self, _playlist_uri: &str, keys: &[String]) -> Result<()> {
      self.removed.lock().unwrap().push(keys.to_vec());
      Ok(())
    }
  }

  /// Opens the client registered for an endpoint's playlist URI.
  struct FakeClients {
    by_uri: BTreeMap<String, Arc<FakeClient>>,
  }

  impl SyncClients for FakeClients {
    type Client = Arc<FakeClient>;

    async fn open(&self, endpoint: &Endpoint) -> Result<Arc<FakeClient>> {
      let label = endpoint.source.label();
      self
        .by_uri
        .get(&endpoint.playlist_uri)
        .cloned()
        .ok_or_else(|| anyhow!("{label} is not compiled into this build"))
    }
  }

  fn test_app() -> (Arc<Mutex<App>>, std::sync::mpsc::Receiver<IoEvent>) {
    let (tx, rx) = std::sync::mpsc::channel();
    let app = App::new(tx, UserConfig::new(), None);
    (Arc::new(Mutex::new(app)), rx)
  }

  fn track(key: &str, title: &str) -> SyncTrack {
    SyncTrack {
      key: key.to_string(),
      isrc: None,
      title: title.to_string(),
      artist: "Radiohead".to_string(),
      duration_ms: Some(238_000),
    }
  }

  fn endpoint(source: Source, uri: &str, name: &str) -> Endpoint {
    Endpoint {
      source,
      playlist_uri: uri.to_string(),
      name: name.to_string(),
    }
  }

  fn mirror_at(uri: &str, matches: &[(&str, &str)]) -> Mirror {
    Mirror {
      endpoint: endpoint(Source::Qobuz, uri, "Mirror"),
      matches: matches
        .iter()
        .map(|(master, mirror)| (master.to_string(), mirror.to_string()))
        .collect(),
      unmatched: Vec::new(),
      last_run: None,
    }
  }

  fn link_at(id: &str, name: &str, master_uri: &str, mirrors: Vec<Mirror>) -> Link {
    Link {
      id: id.to_string(),
      master: endpoint(Source::Spotify, master_uri, name),
      mirrors,
    }
  }

  fn fake(tracks: Vec<SyncTrack>) -> FakeClient {
    FakeClient {
      tracks,
      ..Default::default()
    }
  }

  fn hits(pairs: Vec<(&str, SyncTrack)>) -> BTreeMap<String, SyncTrack> {
    pairs
      .into_iter()
      .map(|(master_key, found)| (master_key.to_string(), found))
      .collect()
  }

  fn fake_clients(clients: Vec<(&str, FakeClient)>) -> FakeClients {
    FakeClients {
      by_uri: clients
        .into_iter()
        .map(|(uri, client)| (uri.to_string(), Arc::new(client)))
        .collect(),
    }
  }

  fn seeded_store(dir: &TempDir, links: Vec<Link>) -> PathBuf {
    let path = dir.path().join("playlist_sync.yml");
    let file = store::PlaylistSyncFile {
      links,
      ..Default::default()
    };
    store::save(&path, &file).unwrap();
    path
  }

  fn saved_mirror(path: &Path, link: usize, mirror: usize) -> Mirror {
    store::load(path).unwrap().links[link].mirrors[mirror].clone()
  }

  #[tokio::test]
  async fn a_run_resolves_adds_and_removes_in_one_pass() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _rx) = test_app();
    let path = seeded_store(
      &dir,
      vec![link_at(
        "aaa",
        "Road Trip",
        "spotify:playlist:1",
        vec![mirror_at("qobuz:playlist:1", &[("m_gone", "x_old")])],
      )],
    );
    let clients = fake_clients(vec![
      (
        "spotify:playlist:1",
        fake(vec![track("m1", "Alpha"), track("m2", "Beta")]),
      ),
      (
        "qobuz:playlist:1",
        FakeClient {
          tracks: vec![track("foreign", "Theirs"), track("x_old", "Dropped")],
          hits: hits(vec![
            ("m1", track("x1", "Alpha")),
            ("m2", track("x2", "Beta")),
          ]),
          ..Default::default()
        },
      ),
    ]);

    let report = run_all(&clients, &app, &path, None, false, true).await;

    let mirror = &clients.by_uri["qobuz:playlist:1"];
    assert_eq!(mirror.added_keys(), vec![vec!["x1", "x2"]]);
    assert_eq!(mirror.removed_keys(), vec![vec!["x_old"]]);
    assert_eq!(
      (report.added(), report.removed(), report.unmatched()),
      (2, 1, 0)
    );
    assert_eq!(
      report.summary(),
      "Playlist sync: 2 added, 1 removed, 0 unmatched"
    );
    assert_eq!(
      saved_mirror(&path, 0, 0).matches,
      BTreeMap::from([
        ("m1".to_string(), "x1".to_string()),
        ("m2".to_string(), "x2".to_string()),
      ])
    );
    assert_eq!(app.lock().await.playlist_sync_links().len(), 1);
  }

  #[tokio::test]
  async fn additions_keep_master_order_and_skip_keys_already_on_the_mirror() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _rx) = test_app();
    let path = seeded_store(
      &dir,
      vec![link_at(
        "aaa",
        "Road Trip",
        "spotify:playlist:1",
        vec![mirror_at("qobuz:playlist:1", &[])],
      )],
    );
    let clients = fake_clients(vec![
      (
        "spotify:playlist:1",
        fake(vec![
          track("m3", "Third"),
          track("m1", "First"),
          track("m2", "Second"),
        ]),
      ),
      (
        "qobuz:playlist:1",
        FakeClient {
          tracks: vec![track("x1", "First")],
          hits: hits(vec![
            ("m3", track("x3", "Third")),
            ("m1", track("x1", "First")),
            ("m2", track("x2", "Second")),
          ]),
          ..Default::default()
        },
      ),
    ]);

    let report = run_all(&clients, &app, &path, None, false, true).await;

    let mirror = &clients.by_uri["qobuz:playlist:1"];
    assert_eq!(mirror.added_keys(), vec![vec!["x3", "x2"]]);
    assert!(mirror.removed_keys().is_empty());
    assert_eq!(report.added(), 2);
  }

  #[tokio::test]
  async fn a_track_the_mirror_already_holds_is_paired_and_never_added_twice() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _rx) = test_app();
    let path = seeded_store(
      &dir,
      vec![link_at(
        "aaa",
        "Road Trip",
        "spotify:playlist:1",
        vec![mirror_at("qobuz:playlist:1", &[])],
      )],
    );
    let clients = fake_clients(vec![
      ("spotify:playlist:1", fake(vec![track("m1", "Alpha")])),
      (
        "qobuz:playlist:1",
        FakeClient {
          tracks: vec![track("x1", "Alpha")],
          hits: hits(vec![("m1", track("x1", "Alpha"))]),
          ..Default::default()
        },
      ),
    ]);

    let first = run_all(&clients, &app, &path, None, false, true).await;
    let second = run_all(&clients, &app, &path, None, false, true).await;

    let mirror = &clients.by_uri["qobuz:playlist:1"];
    assert!(mirror.added_keys().is_empty());
    assert!(mirror.searched_keys().is_empty());
    assert_eq!((first.added(), second.added()), (0, 0));
    assert_eq!(
      saved_mirror(&path, 0, 0).matches,
      BTreeMap::from([("m1".to_string(), "x1".to_string())])
    );
  }

  #[tokio::test]
  async fn a_cached_match_absent_from_the_mirror_is_added_from_the_master_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _rx) = test_app();
    let path = seeded_store(
      &dir,
      vec![link_at(
        "aaa",
        "Road Trip",
        "spotify:playlist:1",
        vec![mirror_at("qobuz:playlist:1", &[("m1", "x1")])],
      )],
    );
    let clients = fake_clients(vec![
      ("spotify:playlist:1", fake(vec![track("m1", "Alpha")])),
      ("qobuz:playlist:1", fake(Vec::new())),
    ]);

    let report = run_all(&clients, &app, &path, None, false, true).await;

    let mirror = &clients.by_uri["qobuz:playlist:1"];
    let rows = mirror.added_rows();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0][0].key, "x1");
    assert_eq!(rows[0][0].title, "Alpha");
    assert!(mirror.searched_keys().is_empty());
    assert_eq!(report.added(), 1);
  }

  #[tokio::test]
  async fn a_removal_is_dropped_when_another_master_track_still_wants_that_key() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _rx) = test_app();
    let path = seeded_store(
      &dir,
      vec![link_at(
        "aaa",
        "Road Trip",
        "spotify:playlist:1",
        vec![mirror_at("qobuz:playlist:1", &[("m_old", "x1")])],
      )],
    );
    let clients = fake_clients(vec![
      ("spotify:playlist:1", fake(vec![track("m_new", "Alpha")])),
      (
        "qobuz:playlist:1",
        FakeClient {
          tracks: vec![track("x1", "Alpha")],
          hits: hits(vec![("m_new", track("x1", "Alpha"))]),
          ..Default::default()
        },
      ),
    ]);

    let report = run_all(&clients, &app, &path, None, false, true).await;

    let mirror = &clients.by_uri["qobuz:playlist:1"];
    assert!(mirror.added_keys().is_empty());
    assert!(mirror.removed_keys().is_empty());
    assert_eq!(
      saved_mirror(&path, 0, 0).matches,
      BTreeMap::from([("m_new".to_string(), "x1".to_string())])
    );
    assert_eq!((report.added(), report.removed()), (0, 0));
  }

  #[tokio::test]
  async fn a_dry_run_writes_nothing_to_the_mirror_or_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _rx) = test_app();
    let path = seeded_store(
      &dir,
      vec![link_at(
        "aaa",
        "Road Trip",
        "spotify:playlist:1",
        vec![mirror_at("qobuz:playlist:1", &[("m_gone", "x_old")])],
      )],
    );
    let clients = fake_clients(vec![
      (
        "spotify:playlist:1",
        fake(vec![track("m1", "Alpha"), track("m2", "Beta")]),
      ),
      (
        "qobuz:playlist:1",
        FakeClient {
          tracks: vec![track("x_old", "Dropped")],
          hits: hits(vec![
            ("m1", track("x1", "Alpha")),
            ("m2", track("x2", "Beta")),
          ]),
          ..Default::default()
        },
      ),
    ]);
    let before = std::fs::read_to_string(&path).unwrap();

    let report = run_all(&clients, &app, &path, None, true, true).await;

    let mirror = &clients.by_uri["qobuz:playlist:1"];
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
    assert!(mirror.added_keys().is_empty());
    assert!(mirror.removed_keys().is_empty());
    assert_eq!((report.added(), report.removed()), (2, 1));
    assert_eq!(
      report.summary(),
      "Playlist sync (dry run): 2 added, 1 removed, 0 unmatched"
    );
  }

  #[tokio::test]
  async fn a_failed_mirror_fails_its_link_but_keeps_what_its_sibling_resolved() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _rx) = test_app();
    let path = seeded_store(
      &dir,
      vec![link_at(
        "aaa",
        "Road Trip",
        "spotify:playlist:1",
        vec![
          mirror_at("qobuz:playlist:1", &[]),
          mirror_at("qobuz:playlist:2", &[]),
        ],
      )],
    );
    let clients = fake_clients(vec![
      ("spotify:playlist:1", fake(vec![track("m1", "Alpha")])),
      (
        "qobuz:playlist:1",
        FakeClient {
          hits: hits(vec![("m1", track("x1", "Alpha"))]),
          ..Default::default()
        },
      ),
      (
        "qobuz:playlist:2",
        FakeClient {
          read_error: Some("the mirror said no".to_string()),
          ..Default::default()
        },
      ),
    ]);

    let report = run_all(&clients, &app, &path, None, false, true).await;

    assert_eq!(
      saved_mirror(&path, 0, 0).matches,
      BTreeMap::from([("m1".to_string(), "x1".to_string())])
    );
    assert!(saved_mirror(&path, 0, 1).matches.is_empty());
    assert_eq!(report.links[0].added, 1);
    assert!(matches!(report.links[0].outcome, LinkOutcome::Failed(_)));
    assert!(report.failed());
  }

  #[tokio::test]
  async fn a_rate_limit_stops_the_run_and_is_reported() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _rx) = test_app();
    let path = seeded_store(
      &dir,
      vec![
        link_at(
          "aaa",
          "Road Trip",
          "spotify:playlist:1",
          vec![mirror_at("qobuz:playlist:1", &[])],
        ),
        link_at(
          "bbb",
          "Dinner",
          "spotify:playlist:2",
          vec![mirror_at("qobuz:playlist:2", &[])],
        ),
      ],
    );
    let clients = fake_clients(vec![
      (
        "spotify:playlist:1",
        fake(vec![track("m0", "Zero"), track("m1", "Alpha")]),
      ),
      (
        "qobuz:playlist:1",
        FakeClient {
          hits: hits(vec![("m0", track("x0", "Zero"))]),
          search_errors: [("m1".to_string(), "429 Too Many Requests".to_string())]
            .into_iter()
            .collect(),
          ..Default::default()
        },
      ),
      ("spotify:playlist:2", fake(vec![track("m2", "Beta")])),
      (
        "qobuz:playlist:2",
        FakeClient {
          hits: hits(vec![("m2", track("x2", "Beta"))]),
          ..Default::default()
        },
      ),
    ]);

    let report = run_all(&clients, &app, &path, None, false, true).await;

    assert_eq!(report.links.len(), 1);
    assert!(report
      .error
      .as_deref()
      .is_some_and(|text| text.contains("429")));
    assert!(report.failed());
    assert!(clients.by_uri["qobuz:playlist:2"].added_keys().is_empty());
    assert_eq!(
      saved_mirror(&path, 0, 0).matches,
      BTreeMap::from([("m0".to_string(), "x0".to_string())])
    );
  }

  #[tokio::test]
  async fn tracks_already_on_the_mirror_are_paired_without_a_search() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _rx) = test_app();
    let path = seeded_store(
      &dir,
      vec![link_at(
        "aaa",
        "Road Trip",
        "spotify:playlist:1",
        vec![mirror_at("qobuz:playlist:1", &[])],
      )],
    );
    let clients = fake_clients(vec![
      (
        "spotify:playlist:1",
        fake(vec![track("m1", "Alpha"), track("m2", "Beta")]),
      ),
      (
        "qobuz:playlist:1",
        FakeClient {
          tracks: vec![track("x1", "Alpha")],
          hits: hits(vec![("m2", track("x2", "Beta"))]),
          ..Default::default()
        },
      ),
    ]);

    let report = run_all(&clients, &app, &path, None, false, true).await;

    let mirror = &clients.by_uri["qobuz:playlist:1"];
    assert_eq!(mirror.searched_keys(), vec!["m2"]);
    assert_eq!(mirror.added_keys(), vec![vec!["x2"]]);
    assert_eq!(report.added(), 1);
    assert_eq!(
      saved_mirror(&path, 0, 0).matches,
      BTreeMap::from([
        ("m1".to_string(), "x1".to_string()),
        ("m2".to_string(), "x2".to_string()),
      ])
    );
  }

  #[tokio::test]
  async fn a_failed_add_keeps_the_batches_that_landed_before_it() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _rx) = test_app();
    let path = seeded_store(
      &dir,
      vec![link_at(
        "aaa",
        "Road Trip",
        "spotify:playlist:1",
        vec![mirror_at("qobuz:playlist:1", &[])],
      )],
    );
    let master: Vec<SyncTrack> = (1..=25)
      .map(|n| track(&format!("m{n}"), &format!("Song {n}")))
      .collect();
    let found: Vec<(String, SyncTrack)> = (1..=25)
      .map(|n| {
        (
          format!("m{n}"),
          track(&format!("x{n}"), &format!("Song {n}")),
        )
      })
      .collect();
    let clients = fake_clients(vec![
      ("spotify:playlist:1", fake(master)),
      (
        "qobuz:playlist:1",
        FakeClient {
          hits: found.into_iter().collect(),
          fail_add_from: Some(2),
          ..Default::default()
        },
      ),
    ]);

    let report = run_all(&clients, &app, &path, None, false, true).await;

    let mirror = &clients.by_uri["qobuz:playlist:1"];
    assert_eq!(mirror.added_keys().len(), 1);
    assert_eq!(report.links[0].added, 10);
    assert!(report.failed());
    assert_eq!(saved_mirror(&path, 0, 0).matches.len(), 20);
  }

  #[tokio::test]
  async fn a_startup_run_keeps_a_no_candidate_verdict_and_a_manual_run_searches_again() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _rx) = test_app();
    let mut mirror = mirror_at("qobuz:playlist:1", &[]);
    mirror.finish_run(
      vec![Unmatched::new(
        &track("m1", "Alpha"),
        UnmatchReason::NoCandidate,
      )],
      "2026-09-17T10:00:00Z".to_string(),
    );
    let path = seeded_store(
      &dir,
      vec![link_at(
        "aaa",
        "Road Trip",
        "spotify:playlist:1",
        vec![mirror],
      )],
    );
    let clients = fake_clients(vec![
      ("spotify:playlist:1", fake(vec![track("m1", "Alpha")])),
      (
        "qobuz:playlist:1",
        FakeClient {
          hits: hits(vec![("m1", track("x1", "Alpha"))]),
          ..Default::default()
        },
      ),
    ]);

    let startup = run_all(&clients, &app, &path, None, false, false).await;
    let mirror = &clients.by_uri["qobuz:playlist:1"];
    assert!(mirror.searched_keys().is_empty());
    assert!(mirror.added_keys().is_empty());
    assert_eq!(startup.unmatched(), 1);
    assert_eq!(saved_mirror(&path, 0, 0).unmatched.len(), 1);

    let manual = run_all(&clients, &app, &path, None, false, true).await;
    assert_eq!(mirror.searched_keys(), vec!["m1"]);
    assert_eq!(mirror.added_keys(), vec![vec!["x1"]]);
    assert_eq!(manual.unmatched(), 0);
    assert!(saved_mirror(&path, 0, 0).unmatched.is_empty());
  }

  #[tokio::test]
  async fn adds_land_in_batches_and_survive_a_rate_limit_after_two_of_them() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _rx) = test_app();
    let path = seeded_store(
      &dir,
      vec![link_at(
        "aaa",
        "Road Trip",
        "spotify:playlist:1",
        vec![mirror_at("qobuz:playlist:1", &[])],
      )],
    );
    let master: Vec<SyncTrack> = (1..=25)
      .map(|n| track(&format!("m{n}"), &format!("Song {n}")))
      .collect();
    let found: Vec<(String, SyncTrack)> = (1..=25)
      .map(|n| {
        (
          format!("m{n}"),
          track(&format!("x{n}"), &format!("Song {n}")),
        )
      })
      .collect();
    let clients = fake_clients(vec![
      ("spotify:playlist:1", fake(master)),
      (
        "qobuz:playlist:1",
        FakeClient {
          hits: found.into_iter().collect(),
          search_errors: [("m22".to_string(), "429 Too Many Requests".to_string())]
            .into_iter()
            .collect(),
          ..Default::default()
        },
      ),
    ]);

    let report = run_all(&clients, &app, &path, None, false, true).await;

    let mirror = &clients.by_uri["qobuz:playlist:1"];
    let batches: Vec<usize> = mirror.added_keys().iter().map(Vec::len).collect();
    assert_eq!(batches, vec![10, 10]);
    assert_eq!(mirror.added_keys()[1][0], "x11");
    assert_eq!(report.links[0].added, 20);
    assert!(report.failed());
    assert_eq!(saved_mirror(&path, 0, 0).matches.len(), 21);
  }

  #[test]
  fn the_save_keeps_a_link_removed_and_a_mirror_added_while_the_run_worked() {
    let dir = tempfile::tempdir().unwrap();
    let path = seeded_store(
      &dir,
      vec![
        link_at(
          "aaa",
          "Road Trip",
          "spotify:playlist:1",
          vec![mirror_at("qobuz:playlist:1", &[])],
        ),
        link_at("bbb", "Dinner", "spotify:playlist:2", Vec::new()),
      ],
    );
    let mut synced = store::load(&path).unwrap().links[0].clone();
    synced.mirrors[0].record_match("m1", "x1");

    let mut meanwhile = store::load(&path).unwrap();
    meanwhile.remove_link("bbb");
    meanwhile.links[0]
      .mirrors
      .push(mirror_at("qobuz:playlist:9", &[]));
    store::save(&path, &meanwhile).unwrap();

    let links = save_synced_link(&path, synced).unwrap();

    assert_eq!(links.len(), 1);
    assert_eq!(links[0].mirrors.len(), 2);
    assert_eq!(
      saved_mirror(&path, 0, 0)
        .matches
        .get("m1")
        .map(String::as_str),
      Some("x1")
    );
    assert_eq!(
      saved_mirror(&path, 0, 1).endpoint.playlist_uri,
      "qobuz:playlist:9"
    );

    let gone = link_at("bbb", "Dinner", "spotify:playlist:2", Vec::new());
    assert_eq!(save_synced_link(&path, gone).unwrap().len(), 1);
  }

  #[tokio::test]
  async fn an_unopenable_master_and_an_unreachable_mirror_are_skipped_not_failed() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _rx) = test_app();
    let mut orphan = link_at(
      "aaa",
      "Orphan",
      "qobuz:playlist:missing",
      vec![mirror_at("qobuz:playlist:3", &[])],
    );
    orphan.master.source = Source::Qobuz;
    let path = seeded_store(
      &dir,
      vec![
        orphan,
        link_at(
          "bbb",
          "Road Trip",
          "spotify:playlist:2",
          vec![
            mirror_at("qobuz:playlist:2", &[]),
            mirror_at("qobuz:playlist:3", &[]),
          ],
        ),
      ],
    );
    let clients = fake_clients(vec![
      ("spotify:playlist:2", fake(vec![track("m1", "Alpha")])),
      (
        "qobuz:playlist:2",
        FakeClient {
          read_error: Some("error sending request for url (https://mirror.example)".to_string()),
          ..Default::default()
        },
      ),
      (
        "qobuz:playlist:3",
        FakeClient {
          hits: hits(vec![("m1", track("x1", "Alpha"))]),
          ..Default::default()
        },
      ),
    ]);

    let report = run_all(&clients, &app, &path, None, false, true).await;

    assert_eq!(report.links.len(), 2);
    let LinkOutcome::Skipped(missing) = &report.links[0].outcome else {
      panic!("a master that cannot be opened skips its link");
    };
    assert!(missing.contains("not compiled into this build"));
    let LinkOutcome::Skipped(unreachable) = &report.links[1].outcome else {
      panic!("an unreachable mirror skips its link, never fails it");
    };
    assert!(unreachable.contains("unreachable"));
    assert_eq!(report.links[1].added, 1);
    assert!(!report.failed());
  }

  #[tokio::test]
  async fn a_link_filter_runs_only_the_link_it_names() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _rx) = test_app();
    let path = seeded_store(
      &dir,
      vec![
        link_at(
          "aaa",
          "Road Trip",
          "spotify:playlist:1",
          vec![mirror_at("qobuz:playlist:1", &[])],
        ),
        link_at(
          "bbb",
          "Dinner",
          "spotify:playlist:2",
          vec![mirror_at("qobuz:playlist:2", &[])],
        ),
      ],
    );
    let clients = fake_clients(vec![
      ("spotify:playlist:1", fake(Vec::new())),
      ("qobuz:playlist:1", fake(Vec::new())),
      ("spotify:playlist:2", fake(Vec::new())),
      ("qobuz:playlist:2", fake(Vec::new())),
    ]);

    let by_name = run_all(&clients, &app, &path, Some("  road TRIP "), false, true).await;
    assert_eq!(by_name.links.len(), 1);
    assert_eq!(by_name.links[0].id, "aaa");

    let by_id = run_all(&clients, &app, &path, Some("bbb"), false, true).await;
    assert_eq!(by_id.links.len(), 1);
    assert_eq!(by_id.links[0].name, "Dinner");

    let missing = run_all(&clients, &app, &path, Some("nope"), false, true).await;
    assert!(missing.links.is_empty());
    assert_eq!(missing.error.as_deref(), Some("no link matches \"nope\""));
  }

  #[tokio::test]
  async fn a_not_syncable_track_a_missing_candidate_and_a_failed_search_are_all_unmatched() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _rx) = test_app();
    let path = seeded_store(
      &dir,
      vec![link_at(
        "aaa",
        "Road Trip",
        "spotify:playlist:1",
        vec![mirror_at("qobuz:playlist:1", &[])],
      )],
    );
    let clients = fake_clients(vec![
      (
        "spotify:playlist:1",
        FakeClient {
          tracks: vec![track("m1", "Alpha"), track("m2", "Beta")],
          not_syncable: vec![track("spotify:local:ns", "A local file")],
          ..Default::default()
        },
      ),
      (
        "qobuz:playlist:1",
        FakeClient {
          search_errors: [("m2".to_string(), "boom".to_string())]
            .into_iter()
            .collect(),
          ..Default::default()
        },
      ),
    ]);

    let report = run_all(&clients, &app, &path, None, false, true).await;

    let mirror = &clients.by_uri["qobuz:playlist:1"];
    assert_eq!(mirror.searched_keys(), vec!["m1", "m2"]);
    let saved = saved_mirror(&path, 0, 0);
    let reasons: Vec<UnmatchReason> = saved
      .unmatched
      .iter()
      .map(|entry| entry.reason.clone())
      .collect();
    assert_eq!(
      reasons,
      vec![
        UnmatchReason::NotSyncable,
        UnmatchReason::NoCandidate,
        UnmatchReason::SearchFailed("boom".to_string()),
      ]
    );
    assert_eq!(report.unmatched(), 3);
  }

  #[tokio::test]
  async fn linking_creates_the_mirror_playlist_stores_the_link_and_publishes_it() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _rx) = test_app();
    let path = seeded_store(&dir, Vec::new());
    let clients = fake_clients(vec![("", FakeClient::default())]);

    let (id, adopted) = link_mirror(
      &clients,
      &app,
      &path,
      endpoint(Source::Spotify, "spotify:playlist:1", "Road Trip"),
      Source::Qobuz,
    )
    .await
    .unwrap();

    assert!(!adopted);
    assert_eq!(id.len(), 12);
    assert_eq!(clients.by_uri[""].created_names(), vec!["Road Trip"]);
    let file = store::load(&path).unwrap();
    assert_eq!(file.links.len(), 1);
    assert_eq!(file.links[0].id, id);
    assert_eq!(file.links[0].master.playlist_uri, "spotify:playlist:1");
    assert_eq!(
      file.links[0].mirrors[0].endpoint,
      endpoint(Source::Qobuz, "fake:playlist:1", "Road Trip")
    );
    assert_eq!(app.lock().await.playlist_sync_links().len(), 1);
  }

  #[tokio::test]
  async fn linking_adopts_an_existing_playlist_with_the_masters_name() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _rx) = test_app();
    let path = seeded_store(&dir, Vec::new());
    let clients = fake_clients(vec![(
      "",
      FakeClient {
        existing: BTreeMap::from([("Road Trip".to_string(), "fake:playlist:kept".to_string())]),
        ..Default::default()
      },
    )]);

    let (_, adopted) = link_mirror(
      &clients,
      &app,
      &path,
      endpoint(Source::Spotify, "spotify:playlist:1", "Road Trip"),
      Source::Qobuz,
    )
    .await
    .unwrap();

    assert!(adopted);
    assert!(clients.by_uri[""].created_names().is_empty());
    let file = store::load(&path).unwrap();
    assert_eq!(
      file.links[0].mirrors[0].endpoint.playlist_uri,
      "fake:playlist:kept"
    );
  }

  #[tokio::test]
  async fn linking_the_same_master_twice_appends_a_second_mirror() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _rx) = test_app();
    let path = seeded_store(&dir, Vec::new());
    let clients = fake_clients(vec![("", FakeClient::default())]);
    let master = endpoint(Source::Spotify, "spotify:playlist:1", "Road Trip");

    let (first, _) = link_mirror(&clients, &app, &path, master.clone(), Source::Qobuz)
      .await
      .unwrap();
    let (second, _) = link_mirror(&clients, &app, &path, master, Source::Subsonic)
      .await
      .unwrap();

    assert_eq!(first, second);
    let file = store::load(&path).unwrap();
    assert_eq!(file.links.len(), 1);
    let uris: Vec<&str> = file.links[0]
      .mirrors
      .iter()
      .map(|mirror| mirror.endpoint.playlist_uri.as_str())
      .collect();
    assert_eq!(uris, ["fake:playlist:1", "fake:playlist:2"]);
  }

  #[tokio::test]
  async fn removing_a_link_forgets_it_and_an_unknown_id_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _rx) = test_app();
    let path = seeded_store(
      &dir,
      vec![
        link_at("aaa", "Road Trip", "spotify:playlist:1", Vec::new()),
        link_at("bbb", "Dinner", "spotify:playlist:2", Vec::new()),
      ],
    );

    remove_link(&app, &path, "aaa").await.unwrap();

    let ids: Vec<String> = store::load(&path)
      .unwrap()
      .links
      .iter()
      .map(|link| link.id.clone())
      .collect();
    assert_eq!(ids, ["bbb"]);
    assert_eq!(app.lock().await.playlist_sync_links().len(), 1);
    assert!(remove_link(&app, &path, "aaa").await.is_err());
  }
}
