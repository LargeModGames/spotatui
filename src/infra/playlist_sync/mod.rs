//! Cross-source playlist sync: opening a client per endpoint and running the
//! engine's plan against it. Compiled unconditionally; the Spotify arm is always
//! present and the other three follow their source's feature, so a slim build
//! reduces to Spotify-to-Spotify links.

mod run;
mod spotify;
#[cfg(feature = "youtube")]
mod youtube;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use rspotify::AuthCodePkceSpotify;
use tokio::sync::Mutex;

use crate::core::app::App;
use crate::core::playlist_sync::{Endpoint, SyncReport, SyncTrack};
use crate::core::source::Source;
use crate::infra::network::IoEvent;

/// Candidates asked of a mirror's catalog for one master track.
#[cfg(any(feature = "qobuz", feature = "subsonic"))]
const CANDIDATE_LIMIT: u32 = 10;

/// Everything a run needs from boot in order to open clients.
pub struct SyncContext {
  spotify: Option<AuthCodePkceSpotify>,
  token_cache_path: PathBuf,
  app: Arc<Mutex<App>>,
}

impl SyncContext {
  /// The boot pieces a run opens its clients from.
  pub fn new(
    spotify: Option<AuthCodePkceSpotify>,
    token_cache_path: PathBuf,
    app: Arc<Mutex<App>>,
  ) -> Self {
    SyncContext {
      spotify,
      token_cache_path,
      app,
    }
  }

  /// The `App` handle the run writes its status messages through.
  pub(crate) fn app(&self) -> &Arc<Mutex<App>> {
    &self.app
  }
}

/// One playlist as a source reports it: the tracks a sync can move, and the
/// items it never can, such as a local file or an episode.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PlaylistRead {
  pub tracks: Vec<SyncTrack>,
  pub not_syncable: Vec<SyncTrack>,
}

impl PlaylistRead {
  /// The keys of the syncable tracks, in playlist order.
  pub(crate) fn keys(&self) -> Vec<String> {
    self.tracks.iter().map(|track| track.key.clone()).collect()
  }
}

impl From<Vec<SyncTrack>> for PlaylistRead {
  /// A source with nothing unsyncable; `not_syncable` stays empty.
  fn from(tracks: Vec<SyncTrack>) -> Self {
    PlaylistRead {
      tracks,
      not_syncable: Vec::new(),
    }
  }
}

/// One playlist endpoint a run reads and writes, in [`SyncTrack`] terms.
pub(crate) trait SyncClient {
  /// The URI of this user's own playlist named `name`, if one exists.
  async fn find_playlist(&self, name: &str) -> Result<Option<String>>;

  /// A new, empty, private playlist named `name`; returns its URI on this source.
  async fn create_playlist(&self, name: &str) -> Result<String>;

  /// Every track of `playlist_uri`, split into what can sync and what cannot.
  async fn read_playlist(&self, playlist_uri: &str) -> Result<PlaylistRead>;

  /// The track on this source that is `target`, searched and picked here.
  async fn resolve(&self, target: &SyncTrack) -> Result<Option<SyncTrack>>;

  /// Append `tracks` to `playlist_uri`, in the order given.
  async fn add(&self, playlist_uri: &str, tracks: &[SyncTrack]) -> Result<()>;

  /// Remove every occurrence of each key from `playlist_uri`.
  async fn remove(&self, playlist_uri: &str, keys: &[String]) -> Result<()>;
}

/// Opens one client per endpoint; the production implementor is [`SyncContext`].
pub(crate) trait SyncClients {
  type Client: SyncClient;

  /// A client for `endpoint`, or why this build or this session cannot open one.
  async fn open(&self, endpoint: &Endpoint) -> Result<Self::Client>;
}

impl SyncClients for SyncContext {
  type Client = SourceClient;

  async fn open(&self, endpoint: &Endpoint) -> Result<SourceClient> {
    for_source(endpoint, self).await
  }
}

/// The one client type a run uses; the three optional arms follow their source's feature.
pub(crate) enum SourceClient {
  Spotify(Box<spotify::SpotifyClient>),
  #[cfg(feature = "qobuz")]
  Qobuz(crate::infra::qobuz::QobuzSource),
  #[cfg(feature = "subsonic")]
  Subsonic(crate::infra::subsonic::SubsonicSource),
  #[cfg(feature = "youtube")]
  YouTube(youtube::YouTubeSyncClient),
}

impl SyncClient for SourceClient {
  async fn find_playlist(&self, name: &str) -> Result<Option<String>> {
    match self {
      SourceClient::Spotify(client) => client.find_playlist(name).await,
      #[cfg(feature = "qobuz")]
      SourceClient::Qobuz(source) => Ok(named_playlist(
        &crate::core::source::MediaSource::playlists(source).await?,
        "qobuz:playlist:",
        name,
      )),
      #[cfg(feature = "subsonic")]
      SourceClient::Subsonic(source) => Ok(named_playlist(
        &crate::core::source::MediaSource::playlists(source).await?,
        "subsonic:playlist:",
        name,
      )),
      #[cfg(feature = "youtube")]
      SourceClient::YouTube(client) => client.find_playlist(name).await,
    }
  }

  async fn create_playlist(&self, name: &str) -> Result<String> {
    match self {
      SourceClient::Spotify(client) => client.create_playlist(name).await,
      #[cfg(feature = "qobuz")]
      SourceClient::Qobuz(source) => Ok(format!(
        "qobuz:playlist:{}",
        source.create_playlist(name).await?
      )),
      #[cfg(feature = "subsonic")]
      SourceClient::Subsonic(source) => Ok(format!(
        "subsonic:playlist:{}",
        source.create_playlist(name).await?
      )),
      #[cfg(feature = "youtube")]
      SourceClient::YouTube(client) => client.create_playlist(name).await,
    }
  }

  async fn read_playlist(&self, playlist_uri: &str) -> Result<PlaylistRead> {
    match self {
      SourceClient::Spotify(client) => client.read_playlist(playlist_uri).await,
      #[cfg(feature = "qobuz")]
      SourceClient::Qobuz(source) => Ok(source.sync_playlist_tracks(playlist_uri).await?.into()),
      #[cfg(feature = "subsonic")]
      SourceClient::Subsonic(source) => Ok(source.sync_playlist_tracks(playlist_uri).await?.into()),
      #[cfg(feature = "youtube")]
      SourceClient::YouTube(client) => client.read_playlist(playlist_uri).await,
    }
  }

  async fn resolve(&self, target: &SyncTrack) -> Result<Option<SyncTrack>> {
    match self {
      SourceClient::Spotify(client) => client.resolve(target).await,
      #[cfg(feature = "qobuz")]
      SourceClient::Qobuz(source) => {
        let found = source
          .sync_search(&catalog_query(target), CANDIDATE_LIMIT)
          .await?;
        Ok(
          crate::core::playlist_sync::pick_candidate(target, &found)
            .map(|index| found[index].clone()),
        )
      }
      #[cfg(feature = "subsonic")]
      SourceClient::Subsonic(source) => {
        let found = source
          .sync_search(&catalog_query(target), CANDIDATE_LIMIT)
          .await?;
        Ok(
          crate::core::playlist_sync::pick_candidate(target, &found)
            .map(|index| found[index].clone()),
        )
      }
      #[cfg(feature = "youtube")]
      SourceClient::YouTube(client) => client.resolve(target).await,
    }
  }

  async fn add(&self, playlist_uri: &str, tracks: &[SyncTrack]) -> Result<()> {
    match self {
      SourceClient::Spotify(client) => client.add(playlist_uri, tracks).await,
      #[cfg(feature = "qobuz")]
      SourceClient::Qobuz(source) => {
        crate::core::source::PlaylistWriter::add_tracks(source, playlist_uri, &keys_of(tracks))
          .await
      }
      #[cfg(feature = "subsonic")]
      SourceClient::Subsonic(source) => {
        crate::core::source::PlaylistWriter::add_tracks(source, playlist_uri, &keys_of(tracks))
          .await
      }
      #[cfg(feature = "youtube")]
      SourceClient::YouTube(client) => client.add(playlist_uri, tracks).await,
    }
  }

  async fn remove(&self, playlist_uri: &str, keys: &[String]) -> Result<()> {
    match self {
      SourceClient::Spotify(client) => client.remove(playlist_uri, keys).await,
      #[cfg(feature = "qobuz")]
      SourceClient::Qobuz(source) => {
        crate::core::source::PlaylistWriter::remove_tracks(source, playlist_uri, keys).await
      }
      #[cfg(feature = "subsonic")]
      SourceClient::Subsonic(source) => {
        crate::core::source::PlaylistWriter::remove_tracks(source, playlist_uri, keys).await
      }
      #[cfg(feature = "youtube")]
      SourceClient::YouTube(client) => client.remove(playlist_uri, keys).await,
    }
  }
}

/// Whether two playlist names are the same, trimmed and ignoring case.
pub(crate) fn same_name(left: &str, right: &str) -> bool {
  left.trim().eq_ignore_ascii_case(right.trim())
}

/// The first listed playlist under `prefix` named `name`, as a URI.
#[cfg(any(feature = "qobuz", feature = "subsonic"))]
fn named_playlist(
  listing: &[crate::core::plugin_api::PlaylistInfo],
  prefix: &str,
  name: &str,
) -> Option<String> {
  listing
    .iter()
    .find(|playlist| playlist.uri.starts_with(prefix) && same_name(&playlist.name, name))
    .map(|playlist| playlist.uri.clone())
}

/// The query a catalog search uses for one master track.
#[cfg(any(feature = "qobuz", feature = "subsonic", feature = "youtube"))]
fn catalog_query(target: &SyncTrack) -> String {
  format!(
    "{} {}",
    target.artist,
    crate::core::playlist_sync::strip_title_suffix(&target.title)
  )
  .trim()
  .to_string()
}

/// The source-native ids of `tracks`, which is what both playlist writers take.
#[cfg(any(feature = "qobuz", feature = "subsonic"))]
fn keys_of(tracks: &[SyncTrack]) -> Vec<String> {
  tracks.iter().map(|track| track.key.clone()).collect()
}

/// The Cargo feature this build is missing to sync `source`, if any.
pub(crate) fn missing_sync_feature(source: Source) -> Option<&'static str> {
  match source {
    Source::Qobuz => (!cfg!(feature = "qobuz")).then_some("qobuz"),
    Source::Subsonic => (!cfg!(feature = "subsonic")).then_some("subsonic"),
    Source::YouTube => (!cfg!(feature = "youtube")).then_some("youtube"),
    Source::Spotify | Source::Local | Source::Radio => None,
  }
}

/// A client for one endpoint, or why it cannot be opened now.
pub(crate) async fn for_source(endpoint: &Endpoint, ctx: &SyncContext) -> Result<SourceClient> {
  match endpoint.source {
    Source::Spotify => ctx
      .spotify
      .clone()
      .map(|spotify| {
        SourceClient::Spotify(Box::new(spotify::SpotifyClient::new(
          spotify,
          ctx.token_cache_path.clone(),
          Arc::clone(&ctx.app),
        )))
      })
      .ok_or_else(|| anyhow!("Spotify is not connected")),
    #[cfg(feature = "qobuz")]
    Source::Qobuz => Ok(SourceClient::Qobuz(
      crate::infra::qobuz::dispatch::build_sync_source(&ctx.app).await?,
    )),
    #[cfg(feature = "subsonic")]
    Source::Subsonic => Ok(SourceClient::Subsonic(
      crate::infra::subsonic::dispatch::build_sync_source(&ctx.app).await?,
    )),
    #[cfg(feature = "youtube")]
    Source::YouTube => Ok(SourceClient::YouTube(youtube::YouTubeSyncClient::new(
      crate::infra::youtube::dispatch::build_source(&ctx.app).await,
      crate::infra::youtube::playlists::default_playlists_path()?,
      Arc::clone(&ctx.app),
    ))),
    other => Err(anyhow!(match missing_sync_feature(other) {
      Some(feature) => format!(
        "{} is not compiled into this build (feature `{feature}`)",
        other.label()
      ),
      None => format!("{} playlists cannot be synced", other.label()),
    })),
  }
}

/// One whole run, including the in-flight guard, the status message and the
/// `App` bookkeeping.
pub async fn run_guarded(
  ctx: SyncContext,
  filter: Option<String>,
  dry_run: bool,
  retry_unmatched: bool,
) -> SyncReport {
  log::info!("playlist sync: run requested (filter={filter:?}, dry_run={dry_run})");
  if !ctx.app.lock().await.begin_playlist_sync() {
    ctx
      .app
      .lock()
      .await
      .set_status_message("Playlist sync already running", 4);
    return SyncReport {
      error: Some("Playlist sync already running".to_string()),
      dry_run,
      ..Default::default()
    };
  }

  let report = match crate::core::playlist_sync::store::default_path() {
    Ok(path) => {
      run::run_all(
        &ctx,
        ctx.app(),
        &path,
        filter.as_deref(),
        dry_run,
        retry_unmatched,
      )
      .await
    }
    Err(e) => SyncReport {
      error: Some(format!("{e:#}")),
      dry_run,
      ..Default::default()
    },
  };

  if !report.links.is_empty() || report.error.is_some() {
    if report.failed() {
      ctx
        .app
        .lock()
        .await
        .set_error_status_message(report.summary(), 10);
    } else {
      ctx.app.lock().await.set_status_message(report.summary(), 8);
    }
  }
  ctx.app.lock().await.finish_playlist_sync(report.clone());
  report
}

/// Start a run on a detached task, for the `IoEvent` handler.
pub fn spawn_run(
  spotify: Option<AuthCodePkceSpotify>,
  token_cache_path: PathBuf,
  app: Arc<Mutex<App>>,
  retry_unmatched: bool,
) {
  let ctx = SyncContext::new(spotify, token_cache_path, app);
  tokio::spawn(run_guarded(ctx, None, false, retry_unmatched));
}

/// Refresh the sidebar list of `source` after a playlist was created there.
fn refresh_playlists_event(source: Source) -> Option<IoEvent> {
  match source {
    Source::Spotify => Some(IoEvent::GetPlaylists),
    Source::Qobuz => Some(IoEvent::GetQobuzPlaylists),
    Source::Subsonic => Some(IoEvent::GetSubsonicPlaylists),
    // The YouTube client reloads the sidebar itself after every write.
    Source::YouTube | Source::Local | Source::Radio => None,
  }
}

/// Create the mirror playlist, record the link, refresh the sidebar and sync that link.
pub async fn link_guarded(ctx: SyncContext, master: Endpoint, mirror: Source) {
  let path = match crate::core::playlist_sync::store::default_path() {
    Ok(path) => path,
    Err(e) => {
      ctx
        .app
        .lock()
        .await
        .set_error_status_message(format!("Playlist link failed: {e:#}"), 10);
      return;
    }
  };
  let name = master.name.clone();
  let linked = run::link_mirror(&ctx, ctx.app(), &path, master, mirror).await;
  match linked {
    Ok((id, adopted)) => {
      if adopted {
        ctx.app.lock().await.set_status_message(
          format!("Using the existing {} playlist \"{name}\"", mirror.label()),
          6,
        );
      } else if let Some(event) = refresh_playlists_event(mirror) {
        ctx.app.lock().await.dispatch(event);
      }
      run_guarded(ctx, Some(id), false, true).await;
    }
    Err(e) => {
      ctx
        .app
        .lock()
        .await
        .set_error_status_message(format!("Playlist link failed: {e:#}"), 10);
    }
  }
}

/// Start [`link_guarded`] on a detached task, for the `IoEvent` handler.
pub fn spawn_link(
  spotify: Option<AuthCodePkceSpotify>,
  token_cache_path: PathBuf,
  app: Arc<Mutex<App>>,
  master: Endpoint,
  mirror: Source,
) {
  let ctx = SyncContext::new(spotify, token_cache_path, app);
  tokio::spawn(link_guarded(ctx, master, mirror));
}

/// Forget one link on a detached task, for the `IoEvent` handler.
pub fn spawn_remove_link(app: Arc<Mutex<App>>, id: String) {
  tokio::spawn(async move {
    let outcome = match crate::core::playlist_sync::store::default_path() {
      Ok(path) => run::remove_link(&app, &path, &id).await,
      Err(e) => Err(e),
    };
    let mut app = app.lock().await;
    match outcome {
      Ok(()) => app.set_status_message("Playlist link removed; the mirror playlists stay", 6),
      Err(e) => app.set_error_status_message(format!("Removing the link failed: {e:#}"), 10),
    }
  });
}
