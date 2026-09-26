//! Qobuz media source.
//!
//! Browses the user's Qobuz library and plays tracks through the encrypted CMAF
//! stream: each track is decrypted and rebuilt as a FLAC tempfile while the
//! shared [`LocalPlayer`](crate::infra::audio::LocalPlayer) decodes it
//! (see [`stream::progressive`]).
//! The three bundle constants are scraped at runtime (see [`auth`]); nothing
//! secret is embedded.
//!
//! ## URIs
//!
//! Tracks: `qobuz:track:<id>`. Sidebar rows (each opens the shared track table):
//! `qobuz:favorites:tracks`, `qobuz:playlist:<id>`, `qobuz:album:<id>`.

pub mod auth;
pub mod dispatch;
mod sign;
pub mod stream;
mod types;

use std::fmt;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde::de::{DeserializeOwned, IgnoredAny};
use tokio::sync::Mutex;

use crate::core::playlist_sync::SyncTrack;
use crate::core::plugin_api::{ArtistRef, PlaylistInfo, SearchResults, TrackInfo};
use crate::core::source::{MediaSource, PlaylistWriter, Searcher};
use crate::infra::audio::LocalPlayer;
use stream::cmaf::InitSegment;
use stream::download;
use stream::progressive::SegmentStream;

pub use stream::cmaf::StreamQuality;

/// The active Qobuz playback session; the playbar reads position and pause
/// live from `player`, like the Subsonic twin.
pub struct QobuzPlaybackState {
  pub player: Arc<LocalPlayer>,
  /// Source handle, reused to download each track on Next/advance.
  pub source: Arc<QobuzSource>,
  /// The playing listing's tracks in order; the playbar reads `tracks[index]`.
  pub tracks: Vec<TrackInfo>,
  pub index: usize,
  /// Set until the track starts to play so the tick never reads the empty
  /// sink as end-of-track.
  pub advancing: bool,
  /// The current track's file, filled while it plays; `None` until playback
  /// starts.
  pub tempfile: Option<tempfile::NamedTempFile>,
  /// The delivered format of the current track; `None` until playback starts.
  pub quality: Option<StreamQuality>,
  /// Backup of the pre-shuffle order while shuffle is on.
  pub shuffle_backup: Option<crate::infra::queue::ShuffleBackup>,
  /// Stamp of the download in flight; a finished download with another stamp is dropped.
  pub fetch_id: u64,
  /// A seek and pause to apply when the next track is staged (session restore,
  /// device recovery, the native queue's resume).
  pub resume_at: Option<ResumePoint>,
  /// The download task in flight; aborted when the session is replaced or restamped.
  pub fetch: Option<tokio::task::AbortHandle>,
}

impl Drop for QobuzPlaybackState {
  fn drop(&mut self) {
    if let Some(fetch) = self.fetch.take() {
      fetch.abort();
    }
  }
}

pub use crate::infra::queue::ResumePoint;

impl QobuzPlaybackState {
  /// The currently playing track, if `index` is in range.
  pub fn current(&self) -> Option<&TrackInfo> {
    self.tracks.get(self.index)
  }

  /// Turn in-place shuffle on or off (see `infra::queue::toggle_shuffle`).
  pub fn set_shuffle(&mut self, on: bool) {
    crate::infra::queue::toggle_shuffle(
      &mut self.tracks,
      &mut self.index,
      &mut self.shuffle_backup,
      on,
    );
  }
}

const API_BASE: &str = "https://www.qobuz.com/api.json/0.2/";

const TRACK_PREFIX: &str = "qobuz:track:";
const PLAYLIST_PREFIX: &str = "qobuz:playlist:";
const ALBUM_PREFIX: &str = "qobuz:album:";
const FAVORITES_URI: &str = "qobuz:favorites:tracks";

/// Per-request caps; they bound each segment GET, never a whole track.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

const PAGE_LIMIT: u32 = 500;
const MAX_ITEMS: usize = 10_000;
const SEARCH_LIMIT: u32 = 20;
/// Ids per playlist write call.
const WRITE_CHUNK: usize = 50;
/// Playlist writes go out like every endpoint but the two stream-session ones:
/// GET, unsigned. Both flip here if Qobuz rejects that shape.
const WRITE_METHOD: Method = Method::Get;
const SIGN_PLAYLIST_WRITES: bool = false;
/// Renew the stream session this long before `expires_at`.
const SESSION_MARGIN_SECS: u64 = 60;

/// A Qobuz call answered HTTP 401: the saved token is no longer valid.
#[derive(Debug)]
pub struct Unauthorized;

impl fmt::Display for Unauthorized {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str("login expired")
  }
}

impl std::error::Error for Unauthorized {}

/// A `session/start` session with its derived key.
#[derive(Clone)]
pub struct StreamSession {
  pub session_id: String,
  pub expires_at: u64,
  pub session_key: [u8; 16],
}

impl StreamSession {
  fn is_valid_at(&self, now: u64) -> bool {
    self.expires_at > now + SESSION_MARGIN_SECS
  }
}

/// What `file/url` returned for one track, with the content key unwrapped.
struct TrackStream {
  url_template: String,
  content_key: [u8; 16],
}

fn unix_now() -> u64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map(|d| d.as_secs())
    .unwrap_or(0)
}

/// Process-wide HTTP client with per-request timeouts (see Subsonic's twin).
pub fn shared_qobuz_client() -> Client {
  static CLIENT: std::sync::OnceLock<Client> = std::sync::OnceLock::new();
  CLIENT
    .get_or_init(|| {
      Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .unwrap_or_default()
    })
    .clone()
}

/// The one stream session per process; sources are rebuilt per event.
fn shared_session() -> &'static Mutex<Option<StreamSession>> {
  static SESSION: std::sync::OnceLock<Mutex<Option<StreamSession>>> = std::sync::OnceLock::new();
  SESSION.get_or_init(|| Mutex::new(None))
}

#[derive(Clone, Copy)]
enum Method {
  Get,
  PostForm,
}

/// Which track listing a sidebar URI names.
enum Listing {
  Favorites,
  Playlist(String),
  Album(String),
}

/// A media source backed by the Qobuz API.
pub struct QobuzSource {
  app_id: String,
  secret: String,
  token: String,
  /// API root, with its trailing slash; tests point it at a loopback listener.
  base: String,
  http: Client,
}

impl QobuzSource {
  pub fn new(
    app_id: impl Into<String>,
    secret: impl Into<String>,
    token: impl Into<String>,
  ) -> Self {
    QobuzSource {
      app_id: app_id.into(),
      secret: secret.into(),
      token: token.into(),
      base: API_BASE.to_string(),
      http: shared_qobuz_client(),
    }
  }

  #[cfg(test)]
  fn with_base(base: impl Into<String>) -> Self {
    QobuzSource {
      base: base.into(),
      ..QobuzSource::new("app-id", "secret", "token")
    }
  }

  /// One API call; `signed` adds `request_ts` and `request_sig` over `args`.
  async fn request<T: DeserializeOwned>(
    &self,
    method: Method,
    endpoint: &str,
    args: &[(&str, String)],
    signed: bool,
    session_id: Option<&str>,
  ) -> Result<T> {
    let mut query: Vec<(&str, String)> = args.to_vec();
    if signed {
      let ts = unix_now();
      let sig_args: Vec<(&str, &str)> = args.iter().map(|(k, v)| (*k, v.as_str())).collect();
      query.push(("request_ts", ts.to_string()));
      query.push((
        "request_sig",
        sign::request_sig(endpoint, &sig_args, ts, &self.secret),
      ));
    }
    let url = format!("{}{endpoint}", self.base);
    let mut request = match method {
      Method::Get => self.http.get(&url).query(&query),
      Method::PostForm => self.http.post(&url).form(&query),
    };
    request = request.header("X-App-Id", &self.app_id);
    if !self.token.is_empty() {
      request = request.header("X-User-Auth-Token", &self.token);
    }
    if let Some(id) = session_id {
      request = request.header("X-Session-Id", id);
    }
    // `without_url`: a reqwest error would otherwise print the query, which
    // carries the OAuth code and the private key on `oauth/callback`.
    let response = request
      .send()
      .await
      .map_err(reqwest::Error::without_url)
      .with_context(|| format!("{endpoint} request"))?;
    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
      return Err(Unauthorized.into());
    }
    let body = response
      .text()
      .await
      .map_err(reqwest::Error::without_url)
      .with_context(|| format!("{endpoint} body"))?;
    if !status.is_success() {
      let excerpt: String = body.chars().take(120).collect();
      return Err(anyhow!("{endpoint} returned HTTP {status}: {excerpt}"));
    }
    // A playlist write may acknowledge with no body at all.
    let body = if body.trim().is_empty() {
      "null"
    } else {
      body.as_str()
    };
    serde_json::from_str(body).with_context(|| format!("{endpoint} response parse"))
  }

  async fn get<T: DeserializeOwned>(&self, endpoint: &str, args: &[(&str, String)]) -> Result<T> {
    self.request(Method::Get, endpoint, args, false, None).await
  }

  // -------------------------------------------------------------------------
  // Stream session and file URL
  // -------------------------------------------------------------------------

  /// `POST session/start`: the signed profile as a form body (the live shape).
  pub async fn session_start(&self) -> Result<StreamSession> {
    let started: types::SessionStart = self
      .request(
        Method::PostForm,
        "session/start",
        &[("profile", "qbz-1".to_string())],
        true,
        None,
      )
      .await
      .context("session start")?;
    let (salt, info) = started
      .infos
      .split_once('.')
      .ok_or_else(|| anyhow!("session infos has no separator"))?;
    let session_key = stream::crypto::session_key(
      &self.secret,
      &auth::decode_base64(salt)?,
      &auth::decode_base64(info)?,
    )?;
    Ok(StreamSession {
      session_id: started.session_id,
      expires_at: started.expires_at,
      session_key,
    })
  }

  /// The shared session, renewed when expired.
  async fn stream_session(&self) -> Result<StreamSession> {
    let mut slot = shared_session().lock().await;
    if let Some(session) = slot.as_ref().filter(|s| s.is_valid_at(unix_now())) {
      return Ok(session.clone());
    }
    let session = self.session_start().await?;
    *slot = Some(session.clone());
    Ok(session)
  }

  /// `GET file/url` for one track at `format_id` (5, 6, 7, or 27).
  async fn file_url(
    &self,
    session: &StreamSession,
    track_id: &str,
    format_id: u8,
  ) -> Result<TrackStream> {
    let file: types::FileUrl = self
      .request(
        Method::Get,
        "file/url",
        &[
          ("track_id", track_id.to_string()),
          ("format_id", format_id.to_string()),
          ("intent", "stream".to_string()),
        ],
        true,
        Some(&session.session_id),
      )
      .await
      .context("file url")?;
    let parts: Vec<&str> = file.key.split('.').collect();
    if parts.len() < 3 {
      return Err(anyhow!(
        "file url key has {} parts, expected 3",
        parts.len()
      ));
    }
    let content_key = stream::crypto::unwrap_content_key(
      &session.session_key,
      &auth::decode_base64(parts[1])?,
      &auth::decode_base64(parts[2])?,
    )?;
    Ok(TrackStream {
      url_template: file.url_template,
      content_key,
    })
  }

  /// The stream URL and key of a track; a stale session is renewed once.
  async fn track_stream(&self, track_id: &str, format_id: u8) -> Result<TrackStream> {
    let session = self.stream_session().await?;
    match self.file_url(&session, track_id, format_id).await {
      Ok(stream) => Ok(stream),
      Err(e) if e.downcast_ref::<Unauthorized>().is_some() => Err(e),
      Err(_) => {
        *shared_session().lock().await = None;
        let session = self.stream_session().await?;
        self.file_url(&session, track_id, format_id).await
      }
    }
  }

  /// Start a progressive stream of a track: segment 0 is fetched, so the
  /// total size and the delivered format are known before any audio.
  pub async fn begin_stream(
    &self,
    track_id: &str,
    format_id: u8,
  ) -> Result<(InitSegment, SegmentStream)> {
    let stream = self.track_stream(track_id, format_id).await?;
    let init = download::fetch_init(&self.http, &stream.url_template).await?;
    let segments = SegmentStream::new(
      self.http.clone(),
      stream.url_template,
      stream.content_key,
      &init,
    );
    Ok((init, segments))
  }

  // -------------------------------------------------------------------------
  // Catalog
  // -------------------------------------------------------------------------

  async fn user_playlists(&self) -> Result<Vec<types::Playlist>> {
    paginate(|offset| async move {
      let page: types::UserPlaylists = self
        .get(
          "playlist/getUserPlaylists",
          &[
            ("limit", PAGE_LIMIT.to_string()),
            ("offset", offset.to_string()),
          ],
        )
        .await?;
      Ok((page.playlists.items, page.playlists.total as usize))
    })
    .await
  }

  async fn favorite_albums(&self) -> Result<Vec<types::Album>> {
    paginate(|offset| async move {
      let page: types::Favorites = self
        .get(
          "favorite/getUserFavorites",
          &[
            ("type", "albums".to_string()),
            ("limit", PAGE_LIMIT.to_string()),
            ("offset", offset.to_string()),
          ],
        )
        .await?;
      let albums = page.albums.unwrap_or_default();
      Ok((albums.items, albums.total as usize))
    })
    .await
  }

  async fn favorite_track_count(&self) -> Result<u32> {
    let page: types::Favorites = self
      .get(
        "favorite/getUserFavorites",
        &[
          ("type", "tracks".to_string()),
          ("limit", "1".to_string()),
          ("offset", "0".to_string()),
        ],
      )
      .await?;
    Ok(page.tracks.map(|t| t.total).unwrap_or(0))
  }

  /// One page of a listing: the tracks plus the album context, when any.
  async fn listing_page(
    &self,
    listing: &Listing,
    offset: usize,
  ) -> Result<(types::Page<types::Track>, Option<types::Album>)> {
    let limit = PAGE_LIMIT.to_string();
    let offset = offset.to_string();
    match listing {
      Listing::Favorites => {
        let page: types::Favorites = self
          .get(
            "favorite/getUserFavorites",
            &[
              ("type", "tracks".to_string()),
              ("limit", limit),
              ("offset", offset),
            ],
          )
          .await?;
        Ok((page.tracks.unwrap_or_default(), None))
      }
      Listing::Playlist(id) => {
        let playlist: types::Playlist = self
          .get(
            "playlist/get",
            &[
              ("playlist_id", id.clone()),
              ("extra", "tracks".to_string()),
              ("limit", limit),
              ("offset", offset),
            ],
          )
          .await?;
        Ok((playlist.tracks.unwrap_or_default(), None))
      }
      Listing::Album(id) => {
        let mut album: types::Album = self
          .get(
            "album/get",
            &[
              ("album_id", id.clone()),
              ("limit", limit),
              ("offset", offset),
            ],
          )
          .await?;
        let tracks = album.tracks.take().unwrap_or_default();
        Ok((tracks, Some(album)))
      }
    }
  }

  async fn listing_tracks(&self, listing: &Listing) -> Result<Vec<TrackInfo>> {
    paginate(|offset| async move {
      let (page, album) = self.listing_page(listing, offset).await?;
      let tracks = page
        .items
        .iter()
        .map(|t| track_to_track_info(t, album.as_ref()))
        .collect();
      Ok((tracks, page.total as usize))
    })
    .await
  }

  // -------------------------------------------------------------------------
  // Playlist writes
  // -------------------------------------------------------------------------

  /// One playlist write, per [`WRITE_METHOD`] and [`SIGN_PLAYLIST_WRITES`].
  async fn playlist_write<T: DeserializeOwned>(
    &self,
    endpoint: &str,
    args: &[(&str, String)],
  ) -> Result<T> {
    self
      .request(WRITE_METHOD, endpoint, args, SIGN_PLAYLIST_WRITES, None)
      .await
  }

  /// Every track of a playlist with its item ids, which `listing_tracks` drops.
  async fn playlist_items(&self, playlist_id: &str) -> Result<Vec<types::Track>> {
    let listing = Listing::Playlist(playlist_id.to_string());
    let listing = &listing;
    paginate(|offset| async move {
      let (page, _) = self.listing_page(listing, offset).await?;
      let total = page.total as usize;
      Ok((page.items, total))
    })
    .await
  }

  /// Every track of a playlist as sync candidates, with the ISRC `listing_tracks` drops.
  pub(crate) async fn sync_playlist_tracks(&self, playlist_uri: &str) -> Result<Vec<SyncTrack>> {
    let id = playlist_id_from_uri(playlist_uri)?;
    Ok(
      self
        .playlist_items(id)
        .await?
        .iter()
        .map(track_to_sync_track)
        .collect(),
    )
  }

  /// Catalog search results as sync candidates, ISRC included.
  pub(crate) async fn sync_search(&self, query: &str, limit: u32) -> Result<Vec<SyncTrack>> {
    let found: types::Search = self
      .get(
        "catalog/search",
        &[("query", query.to_string()), ("limit", limit.to_string())],
      )
      .await?;
    Ok(
      found
        .tracks
        .unwrap_or_default()
        .items
        .iter()
        .map(track_to_sync_track)
        .collect(),
    )
  }

  /// Create a private playlist and return its id.
  pub async fn create_playlist(&self, name: &str) -> Result<String> {
    let created: types::Playlist = self
      .playlist_write(
        "playlist/create",
        &[
          ("name", name.to_string()),
          ("is_public", "false".to_string()),
          ("is_collaborative", "false".to_string()),
        ],
      )
      .await?;
    Ok(created.id)
  }
}

/// Collect every page of a listing; `fetch(offset)` returns one page's items
/// and the listing's total.
async fn paginate<T, F, Fut>(mut fetch: F) -> Result<Vec<T>>
where
  F: FnMut(usize) -> Fut,
  Fut: std::future::Future<Output = Result<(Vec<T>, usize)>>,
{
  let mut out = Vec::new();
  loop {
    let (items, total) = fetch(out.len()).await?;
    let got = items.len();
    out.extend(items);
    if got == 0 || out.len() >= total || out.len() >= MAX_ITEMS {
      return Ok(out);
    }
  }
}

// ---------------------------------------------------------------------------
// URIs and domain type conversions
// ---------------------------------------------------------------------------

/// Strip the `qobuz:track:` prefix and return the raw track id.
pub fn track_id_from_uri(uri: &str) -> Result<&str> {
  uri
    .strip_prefix(TRACK_PREFIX)
    .ok_or_else(|| anyhow!("Not a qobuz track URI: {}", uri))
}

/// Strip the `qobuz:playlist:` prefix; favorites and albums are not writable.
fn playlist_id_from_uri(uri: &str) -> Result<&str> {
  uri
    .strip_prefix(PLAYLIST_PREFIX)
    .ok_or_else(|| anyhow!("Not a qobuz playlist URI: {}", uri))
}

/// A `qobuz:track:<id>` URI or a bare id.
fn track_id_of(uri: &str) -> &str {
  uri.strip_prefix(TRACK_PREFIX).unwrap_or(uri)
}

/// The playlist item ids of `track_ids`, in playlist order.
/// The item id of the last occurrence of each wanted track: the sync adds one
/// row per track, so one row per track goes and an earlier hand-added copy stays.
fn item_ids_for(items: &[types::Track], track_ids: &[&str]) -> Vec<String> {
  track_ids
    .iter()
    .filter_map(|id| items.iter().rev().find(|t| t.id == *id))
    .filter_map(|t| t.playlist_track_id.clone())
    .collect()
}

fn listing_from_uri(uri: &str) -> Result<Listing> {
  if uri == FAVORITES_URI {
    Ok(Listing::Favorites)
  } else if let Some(id) = uri.strip_prefix(PLAYLIST_PREFIX) {
    Ok(Listing::Playlist(id.to_string()))
  } else if let Some(id) = uri.strip_prefix(ALBUM_PREFIX) {
    Ok(Listing::Album(id.to_string()))
  } else {
    Err(anyhow!("Not a qobuz playlist URI: {}", uri))
  }
}

fn favorites_playlist(track_count: u32) -> PlaylistInfo {
  PlaylistInfo {
    uri: FAVORITES_URI.to_string(),
    name: "Favorite tracks".to_string(),
    owner: String::new(),
    track_count,
    id: None,
    owner_id: None,
    collaborative: false,
    public: Some(false),
    image_url: None,
  }
}

fn playlist_to_playlist_info(p: &types::Playlist) -> PlaylistInfo {
  PlaylistInfo {
    uri: format!("{PLAYLIST_PREFIX}{}", p.id),
    name: p.name.clone(),
    owner: p.owner.as_ref().map(|o| o.name.clone()).unwrap_or_default(),
    track_count: p.tracks_count,
    id: Some(p.id.clone()),
    owner_id: p.owner.as_ref().and_then(|o| o.id.clone()),
    collaborative: false,
    public: p.is_public,
    image_url: p.images300.first().cloned(),
  }
}

fn album_to_playlist_info(a: &types::Album) -> PlaylistInfo {
  let artist = a.artist.as_ref().map(|n| n.name.as_str()).unwrap_or("");
  PlaylistInfo {
    uri: format!("{ALBUM_PREFIX}{}", a.id),
    name: if artist.is_empty() {
      a.title.clone()
    } else {
      format!("{} - {artist}", a.title)
    },
    owner: artist.to_string(),
    track_count: a.tracks_count,
    id: Some(a.id.clone()),
    owner_id: None,
    collaborative: false,
    public: Some(true),
    image_url: a.image.large.clone(),
  }
}

/// Map a Qobuz track onto [`TrackInfo`]; `parent` is the album of an `album/get`.
fn track_to_track_info(t: &types::Track, parent: Option<&types::Album>) -> TrackInfo {
  let album = t.album.as_ref().or(parent);
  let performer = t
    .performer
    .as_ref()
    .or(album.and_then(|a| a.artist.as_ref()));
  let artist_refs: Vec<ArtistRef> = performer
    .filter(|n| !n.name.is_empty())
    .map(|n| {
      vec![ArtistRef {
        id: n.id.clone(),
        name: n.name.clone(),
      }]
    })
    .unwrap_or_default();
  let name = match t.version.as_deref().filter(|v| !v.is_empty()) {
    Some(version) => format!("{} ({version})", t.title),
    None => t.title.clone(),
  };
  TrackInfo {
    uri: Some(format!("{TRACK_PREFIX}{}", t.id)),
    name,
    artists: artist_refs.iter().map(|a| a.name.clone()).collect(),
    album: album.map(|a| a.title.clone()).unwrap_or_default(),
    duration_ms: t.duration * 1000,
    id: Some(t.id.clone()),
    album_id: album.map(|a| a.id.clone()),
    artist_refs,
    is_playable: t.streamable,
    is_local: false,
    track_number: t.track_number,
    explicit: t.parental_warning,
    image_url: album.and_then(|a| a.image.large.clone()),
  }
}

/// Map a Qobuz track onto the sync currency: the bare title, never the version suffix.
fn track_to_sync_track(t: &types::Track) -> SyncTrack {
  let performer = t
    .performer
    .as_ref()
    .or(t.album.as_ref().and_then(|a| a.artist.as_ref()));
  SyncTrack {
    key: t.id.clone(),
    isrc: t.isrc.clone(),
    title: t.title.clone(),
    artist: performer.map(|n| n.name.clone()).unwrap_or_default(),
    duration_ms: (t.duration > 0).then_some(t.duration * 1000),
  }
}

// ---------------------------------------------------------------------------
// Trait implementations
// ---------------------------------------------------------------------------

impl MediaSource for QobuzSource {
  fn name(&self) -> &str {
    "Qobuz"
  }

  fn scheme(&self) -> &str {
    "qobuz"
  }

  /// Favorite tracks, then the user's playlists, then favorite albums.
  async fn playlists(&self) -> Result<Vec<PlaylistInfo>> {
    let (favorite_count, playlists, albums) = tokio::try_join!(
      self.favorite_track_count(),
      self.user_playlists(),
      self.favorite_albums()
    )?;
    let mut out = vec![favorites_playlist(favorite_count)];
    out.extend(playlists.iter().map(playlist_to_playlist_info));
    out.extend(albums.iter().map(album_to_playlist_info));
    Ok(out)
  }

  async fn tracks(&self, playlist_uri: &str) -> Result<Vec<TrackInfo>> {
    let listing = listing_from_uri(playlist_uri)?;
    self.listing_tracks(&listing).await
  }
}

impl Searcher for QobuzSource {
  /// Tracks only: album and artist rows would route to Spotify-bound events.
  async fn search(&self, query: &str) -> Result<SearchResults> {
    let found: types::Search = self
      .get(
        "catalog/search",
        &[
          ("query", query.to_string()),
          ("limit", SEARCH_LIMIT.to_string()),
        ],
      )
      .await?;
    let tracks = found
      .tracks
      .unwrap_or_default()
      .items
      .iter()
      .map(|t| track_to_track_info(t, None))
      .collect();
    Ok(SearchResults {
      tracks,
      albums: vec![],
      artists: vec![],
      playlists: vec![],
      shows: vec![],
    })
  }
}

impl PlaylistWriter for QobuzSource {
  /// Append tracks, [`WRITE_CHUNK`] ids per call; Qobuz dedupes server-side.
  async fn add_tracks(&self, playlist_uri: &str, track_uris: &[String]) -> Result<()> {
    let playlist_id = playlist_id_from_uri(playlist_uri)?;
    let ids: Vec<&str> = track_uris
      .iter()
      .map(|uri| track_id_of(uri.as_str()))
      .collect();
    for chunk in ids.chunks(WRITE_CHUNK) {
      let _: IgnoredAny = self
        .playlist_write(
          "playlist/addTracks",
          &[
            ("playlist_id", playlist_id.to_string()),
            ("track_ids", chunk.join(",")),
            ("no_duplicate", "true".to_string()),
          ],
        )
        .await?;
    }
    Ok(())
  }

  /// Remove tracks by playlist item id; the playlist is read first to map them.
  async fn remove_tracks(&self, playlist_uri: &str, track_uris: &[String]) -> Result<()> {
    let playlist_id = playlist_id_from_uri(playlist_uri)?;
    if track_uris.is_empty() {
      return Ok(());
    }
    let items = self.playlist_items(playlist_id).await?;
    let wanted: Vec<&str> = track_uris
      .iter()
      .map(|uri| track_id_of(uri.as_str()))
      .collect();
    for chunk in item_ids_for(&items, &wanted).chunks(WRITE_CHUNK) {
      let _: IgnoredAny = self
        .playlist_write(
          "playlist/deleteTracks",
          &[
            ("playlist_id", playlist_id.to_string()),
            ("playlist_track_ids", chunk.join(",")),
          ],
        )
        .await?;
    }
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
  use tokio::net::TcpListener;

  /// Serve `responses` in order, collecting each request line and body.
  async fn serve(
    responses: Vec<(&'static str, &'static str)>,
  ) -> (String, tokio::task::JoinHandle<Vec<(String, String)>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/", listener.local_addr().unwrap());
    let handle = tokio::spawn(async move {
      let mut seen = Vec::new();
      for (status, payload) in responses {
        let (mut stream, _) = listener.accept().await.unwrap();
        let (read_half, mut write_half) = stream.split();
        let mut reader = BufReader::new(read_half);
        let mut request_line = String::new();
        reader.read_line(&mut request_line).await.unwrap();
        let mut content_length = 0usize;
        loop {
          let mut line = String::new();
          reader.read_line(&mut line).await.unwrap();
          if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            content_length = value.trim().parse().unwrap_or(0);
          }
          if line == "\r\n" || line.is_empty() {
            break;
          }
        }
        let mut body = vec![0u8; content_length];
        if content_length > 0 {
          reader.read_exact(&mut body).await.unwrap();
        }
        seen.push((
          request_line.trim_end().to_string(),
          String::from_utf8_lossy(&body).to_string(),
        ));
        write_half
          .write_all(
            format!(
              "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{payload}",
              payload.len()
            )
            .as_bytes(),
          )
          .await
          .unwrap();
        write_half.flush().await.unwrap();
      }
      seen
    });
    (base, handle)
  }

  /// Request line and body joined, so a param assertion holds under either
  /// [`WRITE_METHOD`].
  fn sent(entry: &(String, String)) -> String {
    format!("{} {}", entry.0, entry.1)
  }

  const USER_PLAYLISTS: &str = r#"{
    "playlists": {
      "offset": 0, "limit": 500, "total": 2,
      "items": [
        { "id": 111, "name": "Morning", "owner": { "id": 9, "name": "jay" },
          "tracks_count": 12, "is_public": false, "images300": ["https://img/1.jpg"] },
        { "id": 222, "name": "Evening", "owner": { "id": 9, "name": "jay" },
          "tracks_count": 3, "is_public": true }
      ]
    }
  }"#;

  const PLAYLIST_TRACKS: &str = r#"{
    "id": 111, "name": "Morning",
    "tracks": {
      "offset": 0, "limit": 500, "total": 1,
      "items": [
        { "id": 5001, "title": "Around the World", "version": "Radio Edit",
          "duration": 429, "track_number": 7,
          "performer": { "id": 36819, "name": "Daft Punk" },
          "album": { "id": "0060254730301", "title": "Homework",
                     "artist": { "id": 36819, "name": "Daft Punk" },
                     "image": { "large": "https://img/large.jpg", "small": "https://img/s.jpg" } },
          "streamable": true, "parental_warning": false }
      ]
    }
  }"#;

  const ALBUM: &str = r#"{
    "id": "0060254730301", "title": "Homework", "tracks_count": 2,
    "artist": { "id": 36819, "name": "Daft Punk" },
    "image": { "large": "https://img/album.jpg" },
    "tracks": { "total": 2, "items": [
      { "id": 1, "title": "Daftendirekt", "duration": 164, "track_number": 1 },
      { "id": 2, "title": "WDPK 83.7 FM", "duration": 28, "track_number": 2, "streamable": false }
    ] }
  }"#;

  const FAVORITES: &str = r#"{
    "albums": { "total": 1, "items": [
      { "id": "abc", "title": "Discovery", "artist": { "id": 36819, "name": "Daft Punk" },
        "tracks_count": 14, "image": { "large": "https://img/d.jpg" } }
    ] }
  }"#;

  const SESSION_START: &str = r#"{ "session_id": "sess-1", "expires_at": 1700000000,
    "infos": "c2FsdA.aW5mbw" }"#;

  const FILE_URL: &str = r#"{ "url_template": "https://cdn/$SEGMENT$.m4s", "n_segments": 3,
    "key": "p.d3JhcHBlZA.aXY", "mime_type": "audio/flac", "format_id": 27 }"#;

  const PLAYLIST_ITEMS: &str = r#"{
    "id": 111, "name": "Morning",
    "tracks": {
      "offset": 0, "limit": 500, "total": 3,
      "items": [
        { "id": 5001, "title": "A", "isrc": "GBAAA0000001", "playlist_track_id": 90001 },
        { "id": 5002, "title": "B", "playlist_track_id": 90002 },
        { "id": 5001, "title": "A", "isrc": "GBAAA0000001", "playlist_track_id": 90003 }
      ]
    }
  }"#;

  const CREATED_PLAYLIST: &str = r#"{ "id": 777, "name": "Mirror", "is_public": false }"#;

  const WRITE_OK: &str = r#"{ "status": "success" }"#;

  const SYNC_PLAYLIST: &str = r#"{
    "id": 111, "name": "Morning",
    "tracks": {
      "offset": 0, "limit": 500, "total": 2,
      "items": [
        { "id": 5001, "title": "Around the World", "version": "Radio Edit",
          "duration": 429, "isrc": "gb-aaa-00-00001",
          "performer": { "id": 36819, "name": "Daft Punk" } },
        { "id": 5002, "title": "Veridis Quo",
          "album": { "id": "0060254730302", "title": "Discovery",
                     "artist": { "id": 36819, "name": "Daft Punk" } } }
      ]
    }
  }"#;

  const SYNC_SEARCH: &str = r#"{
    "tracks": {
      "offset": 0, "limit": 10, "total": 1,
      "items": [
        { "id": 5001, "title": "Around the World", "duration": 429,
          "isrc": "GBAAA0000001",
          "performer": { "id": 36819, "name": "Daft Punk" } }
      ]
    }
  }"#;

  #[test]
  fn user_playlists_map_to_playlist_info() {
    let page: types::UserPlaylists = serde_json::from_str(USER_PLAYLISTS).unwrap();
    assert_eq!(page.playlists.total, 2);
    let info = playlist_to_playlist_info(&page.playlists.items[0]);
    assert_eq!(info.uri, "qobuz:playlist:111");
    assert_eq!(info.name, "Morning");
    assert_eq!(info.owner, "jay");
    assert_eq!(info.owner_id.as_deref(), Some("9"));
    assert_eq!(info.track_count, 12);
    assert_eq!(info.public, Some(false));
    assert_eq!(info.image_url.as_deref(), Some("https://img/1.jpg"));
    assert!(playlist_to_playlist_info(&page.playlists.items[1])
      .image_url
      .is_none());
  }

  #[test]
  fn playlist_tracks_map_to_track_info() {
    let playlist: types::Playlist = serde_json::from_str(PLAYLIST_TRACKS).unwrap();
    let tracks = playlist.tracks.unwrap();
    let track = track_to_track_info(&tracks.items[0], None);
    assert_eq!(track.uri.as_deref(), Some("qobuz:track:5001"));
    assert_eq!(track.name, "Around the World (Radio Edit)");
    assert_eq!(track.artists, vec!["Daft Punk"]);
    assert_eq!(track.album, "Homework");
    assert_eq!(track.duration_ms, 429_000);
    assert_eq!(track.track_number, 7);
    assert_eq!(track.id.as_deref(), Some("5001"));
    assert_eq!(track.album_id.as_deref(), Some("0060254730301"));
    assert_eq!(track.artist_refs[0].id.as_deref(), Some("36819"));
    assert_eq!(track.image_url.as_deref(), Some("https://img/large.jpg"));
    assert!(track.is_playable);
    assert!(!track.explicit);
  }

  #[test]
  fn album_tracks_take_the_parent_album_as_context() {
    let album: types::Album = serde_json::from_str(ALBUM).unwrap();
    let tracks = album.tracks.as_ref().unwrap();
    let first = track_to_track_info(&tracks.items[0], Some(&album));
    assert_eq!(first.album, "Homework");
    assert_eq!(first.artists, vec!["Daft Punk"]);
    assert_eq!(first.image_url.as_deref(), Some("https://img/album.jpg"));
    let second = track_to_track_info(&tracks.items[1], Some(&album));
    assert!(!second.is_playable);
    let row = album_to_playlist_info(&album);
    assert_eq!(row.uri, "qobuz:album:0060254730301");
    assert_eq!(row.name, "Homework - Daft Punk");
    assert_eq!(row.track_count, 2);
  }

  #[test]
  fn favorite_albums_parse_and_favorites_row_is_first() {
    let favorites: types::Favorites = serde_json::from_str(FAVORITES).unwrap();
    assert!(favorites.tracks.is_none());
    let albums = favorites.albums.unwrap();
    assert_eq!(
      album_to_playlist_info(&albums.items[0]).uri,
      "qobuz:album:abc"
    );
    let row = favorites_playlist(7);
    assert_eq!(row.uri, FAVORITES_URI);
    assert_eq!(row.track_count, 7);
  }

  #[test]
  fn session_start_and_file_url_parse() {
    let session: types::SessionStart = serde_json::from_str(SESSION_START).unwrap();
    assert_eq!(session.session_id, "sess-1");
    let (salt, info) = session.infos.split_once('.').unwrap();
    assert_eq!(auth::decode_base64(salt).unwrap(), b"salt");
    assert_eq!(auth::decode_base64(info).unwrap(), b"info");
    let file: types::FileUrl = serde_json::from_str(FILE_URL).unwrap();
    assert_eq!(file.url_template, "https://cdn/$SEGMENT$.m4s");
    assert_eq!(file.key.split('.').count(), 3);
  }

  #[test]
  fn listing_from_uri_covers_every_sidebar_shape() {
    assert!(matches!(
      listing_from_uri(FAVORITES_URI),
      Ok(Listing::Favorites)
    ));
    assert!(matches!(listing_from_uri("qobuz:playlist:5"), Ok(Listing::Playlist(id)) if id == "5"));
    assert!(matches!(listing_from_uri("qobuz:album:x"), Ok(Listing::Album(id)) if id == "x"));
    assert!(listing_from_uri("spotify:playlist:5").is_err());
    assert_eq!(track_id_from_uri("qobuz:track:9").unwrap(), "9");
    assert!(track_id_from_uri("qobuz:album:9").is_err());
  }

  #[test]
  fn session_validity_keeps_a_renewal_margin() {
    let session = StreamSession {
      session_id: String::new(),
      expires_at: 1_000,
      session_key: [0; 16],
    };
    assert!(session.is_valid_at(1_000 - SESSION_MARGIN_SECS - 1));
    assert!(!session.is_valid_at(1_000 - SESSION_MARGIN_SECS));
  }

  #[test]
  fn playlist_items_carry_isrc_and_their_playlist_track_id() {
    let playlist: types::Playlist = serde_json::from_str(PLAYLIST_ITEMS).unwrap();
    let items = playlist.tracks.unwrap().items;
    assert_eq!(items[0].isrc.as_deref(), Some("GBAAA0000001"));
    assert_eq!(items[0].playlist_track_id.as_deref(), Some("90001"));
    assert_eq!(items[1].isrc, None);
    assert_eq!(items[2].playlist_track_id.as_deref(), Some("90003"));
    let legacy: types::Playlist = serde_json::from_str(PLAYLIST_TRACKS).unwrap();
    let legacy = legacy.tracks.unwrap();
    assert_eq!(legacy.items[0].isrc, None);
    assert_eq!(legacy.items[0].playlist_track_id, None);
  }

  #[test]
  fn item_ids_map_track_ids_to_playlist_item_ids() {
    let playlist: types::Playlist = serde_json::from_str(PLAYLIST_ITEMS).unwrap();
    let items = playlist.tracks.unwrap().items;
    assert_eq!(item_ids_for(&items, &["5001"]), vec!["90003"]);
    assert_eq!(item_ids_for(&items, &["5002"]), vec!["90002"]);
    assert!(item_ids_for(&items, &["9999"]).is_empty());
  }

  #[test]
  fn playlist_writes_accept_a_track_uri_or_a_bare_id() {
    assert_eq!(track_id_of("qobuz:track:5001"), "5001");
    assert_eq!(track_id_of("5001"), "5001");
  }

  #[test]
  fn playlist_writes_reject_favorites_and_album_uris() {
    assert_eq!(playlist_id_from_uri("qobuz:playlist:111").unwrap(), "111");
    assert!(playlist_id_from_uri(FAVORITES_URI).is_err());
    assert!(playlist_id_from_uri("qobuz:album:x").is_err());
  }

  #[tokio::test]
  async fn create_playlist_sends_an_unsigned_private_playlist() {
    let (base, server) = serve(vec![("200 OK", CREATED_PLAYLIST)]).await;
    let id = QobuzSource::with_base(base)
      .create_playlist("Mirror")
      .await
      .unwrap();
    assert_eq!(id, "777");
    let seen = server.await.unwrap();
    assert!(seen[0].0.contains("/playlist/create"));
    assert!(sent(&seen[0]).contains("name=Mirror"));
    assert!(sent(&seen[0]).contains("is_public=false"));
    assert!(sent(&seen[0]).contains("is_collaborative=false"));
    assert!(!sent(&seen[0]).contains("request_sig"));
  }

  #[tokio::test]
  async fn add_tracks_sends_a_comma_list_in_chunks_of_fifty() {
    let (base, server) = serve(vec![("200 OK", WRITE_OK), ("200 OK", WRITE_OK)]).await;
    let uris: Vec<String> = (0..51).map(|i| format!("qobuz:track:{i}")).collect();
    QobuzSource::with_base(base)
      .add_tracks("qobuz:playlist:111", &uris)
      .await
      .unwrap();
    let seen = server.await.unwrap();
    assert_eq!(seen.len(), 2);
    assert!(seen[0].0.contains("/playlist/addTracks"));
    assert!(sent(&seen[0]).contains("playlist_id=111"));
    assert!(sent(&seen[0]).contains("no_duplicate=true"));
    assert!(sent(&seen[0]).contains("track_ids=0%2C1%2C2%2C"));
    assert_eq!(sent(&seen[0]).matches("%2C").count(), 49);
    assert!(sent(&seen[1]).contains("track_ids=50"));
    assert_eq!(sent(&seen[1]).matches("%2C").count(), 0);
  }

  #[tokio::test]
  async fn add_tracks_accepts_an_empty_success_body() {
    let (base, server) = serve(vec![("200 OK", "")]).await;
    QobuzSource::with_base(base)
      .add_tracks("qobuz:playlist:111", &["qobuz:track:1".to_string()])
      .await
      .unwrap();
    assert_eq!(server.await.unwrap().len(), 1);
  }

  #[tokio::test]
  async fn adding_no_tracks_makes_no_request() {
    QobuzSource::with_base("http://127.0.0.1:1/")
      .add_tracks("qobuz:playlist:111", &[])
      .await
      .unwrap();
  }

  #[tokio::test]
  async fn remove_tracks_reads_the_playlist_then_deletes_item_ids() {
    let (base, server) = serve(vec![("200 OK", PLAYLIST_ITEMS), ("200 OK", WRITE_OK)]).await;
    QobuzSource::with_base(base)
      .remove_tracks("qobuz:playlist:111", &["qobuz:track:5001".to_string()])
      .await
      .unwrap();
    let seen = server.await.unwrap();
    assert_eq!(seen.len(), 2);
    assert!(seen[0].0.starts_with("GET /playlist/get?"));
    assert!(seen[0].0.contains("playlist_id=111"));
    assert!(seen[0].0.contains("extra=tracks"));
    assert!(seen[1].0.contains("/playlist/deleteTracks"));
    assert!(sent(&seen[1]).contains("playlist_id=111"));
    assert!(sent(&seen[1]).contains("playlist_track_ids=90003"));
  }

  #[tokio::test]
  async fn sync_playlist_tracks_keeps_the_isrc_and_the_bare_title() {
    let (base, server) = serve(vec![("200 OK", SYNC_PLAYLIST)]).await;
    let tracks = QobuzSource::with_base(base)
      .sync_playlist_tracks("qobuz:playlist:111")
      .await
      .unwrap();
    let seen = server.await.unwrap();
    assert_eq!(seen.len(), 1);
    assert!(seen[0].0.starts_with("GET /playlist/get?"));
    assert!(seen[0].0.contains("playlist_id=111"));
    assert!(seen[0].0.contains("extra=tracks"));
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[0].key, "5001");
    assert_eq!(tracks[0].title, "Around the World");
    assert_eq!(tracks[0].artist, "Daft Punk");
    assert_eq!(tracks[0].isrc.as_deref(), Some("gb-aaa-00-00001"));
    assert_eq!(tracks[0].duration_ms, Some(429_000));
    assert_eq!(tracks[1].key, "5002");
    assert_eq!(tracks[1].artist, "Daft Punk");
    assert_eq!(tracks[1].isrc, None);
  }

  #[tokio::test]
  async fn sync_search_sends_the_limit_it_was_given() {
    let (base, server) = serve(vec![("200 OK", SYNC_SEARCH)]).await;
    let found = QobuzSource::with_base(base)
      .sync_search("daft punk around the world", 10)
      .await
      .unwrap();
    let seen = server.await.unwrap();
    assert_eq!(seen.len(), 1);
    assert!(seen[0].0.starts_with("GET /catalog/search?"));
    assert!(seen[0].0.contains("query=daft+punk+around+the+world"));
    assert!(seen[0].0.contains("limit=10"));
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].key, "5001");
    assert_eq!(found[0].isrc.as_deref(), Some("GBAAA0000001"));
    assert_eq!(found[0].duration_ms, Some(429_000));
  }

  #[test]
  fn a_track_with_no_duration_reports_an_unknown_one() {
    let playlist: types::Playlist = serde_json::from_str(PLAYLIST_ITEMS).unwrap();
    let items = playlist.tracks.unwrap().items;
    let track = track_to_sync_track(&items[1]);
    assert_eq!(track.key, "5002");
    assert_eq!(track.duration_ms, None);
    assert_eq!(track.isrc, None);
    assert_eq!(track.artist, "");
  }

  /// Live end to end: scrape the bundle, start a session, stream one track
  /// through the shared sink while it downloads, and seek ahead. Needs
  /// `SPOTATUI_QOBUZ_TOKEN` (and `SPOTATUI_QOBUZ_TEST_FORMAT`, default 27). Run:
  /// `cargo test --features qobuz -- --ignored live_qobuz --nocapture`
  #[tokio::test]
  #[ignore = "needs SPOTATUI_QOBUZ_TOKEN, the network, and an audio output device"]
  async fn live_qobuz_download_plays_through_sink() {
    use crate::infra::audio::LocalPlayer;
    use std::time::Instant;

    let Some(token) = auth::load_token() else {
      eprintln!("no token: set {}", auth::TOKEN_ENV);
      return;
    };
    let format: u8 = std::env::var("SPOTATUI_QOBUZ_TEST_FORMAT")
      .ok()
      .and_then(|s| s.parse().ok())
      .unwrap_or(27);

    let http = shared_qobuz_client();
    let constants = auth::resolve_constants(&http, None)
      .await
      .expect("bundle scrape");
    eprintln!(
      "bundle {} app_id {}",
      constants.bundle_version, constants.app_id
    );
    let source = QobuzSource::new(constants.app_id, constants.app_secret, token);

    let playlists = source.playlists().await.expect("playlists");
    eprintln!("{} sidebar rows", playlists.len());
    let results = source.search("Daft Punk").await.expect("search");
    let track = results.tracks.first().expect("a search hit");
    let track_id = track_id_from_uri(track.uri.as_deref().unwrap()).unwrap();
    eprintln!("track {} ({})", track.name, track_id);

    let tmp = tempfile::NamedTempFile::new().unwrap();
    let started = Instant::now();
    let (init, segments) = source
      .begin_stream(track_id, format)
      .await
      .expect("session start + file url + init segment");
    let total = init.total_bytes();
    eprintln!(
      "init after {:?}, total {} bytes, {}",
      started.elapsed(),
      total,
      init.quality().label()
    );
    let reader = stream::progressive::open(segments, &tmp)
      .await
      .expect("open stream");
    let mime = if init.quality().flac {
      "audio/flac"
    } else {
      "audio/mpeg"
    };
    let prepared = tokio::task::spawn_blocking(move || {
      LocalPlayer::prepare_stream(reader, Some(mime), Some(total))
    })
    .await
    .unwrap()
    .expect("decode the stream head");
    eprintln!("decoder ready after {:?}", started.elapsed());

    let player = LocalPlayer::new().expect("open default output device");
    player.play_prepared(prepared).unwrap();
    eprintln!("playing after {:?}", started.elapsed());
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert!(
      player.position() >= Duration::from_millis(200),
      "playback position should advance, got {:?}",
      player.position()
    );
    // A seek past the downloaded part restarts the stream at that segment.
    let seek_started = Instant::now();
    tokio::task::spawn_blocking(move || {
      player.seek(Duration::from_secs(90)).expect("seek ahead");
      eprintln!("seek to 90 s took {:?}", seek_started.elapsed());
      std::thread::sleep(Duration::from_millis(600));
      assert!(
        player.position() >= Duration::from_secs(90),
        "position after seek: {:?}",
        player.position()
      );
      player.stop();
    })
    .await
    .unwrap();

    if format != 5 {
      let bytes = std::fs::read(tmp.path()).unwrap();
      assert!(bytes.starts_with(b"fLaC"), "got {:02X?}", &bytes[..4]);
    }
  }
}
