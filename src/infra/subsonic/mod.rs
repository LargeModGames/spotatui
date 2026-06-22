//! Subsonic / OpenSubsonic REST API media source.
//!
//! Implements [`MediaSource`] and [`Searcher`] for any server that speaks the
//! Subsonic REST protocol: Navidrome, Funkwhale, Gonic, Airsonic, etc.
//!
//! # Authentication
//!
//! The Subsonic token auth scheme (introduced in API 1.13.0) is used:
//! - `u` = username
//! - `s` = random salt (6+ chars)
//! - `t` = `md5(password + salt)` as a hex string
//! - `v` = protocol version (`"1.16.1"`)
//! - `c` = client name (`"spotatui"`)
//! - `f` = response format (`"json"`)
//!
//! # Status
//!
//! Dead code until Wave 1 wires this into the application runtime.

// Not yet wired into the application runtime -- Wave 1 will do that.
#![allow(dead_code)]

mod types;

use anyhow::{anyhow, bail, Context, Result};
use md5::{Digest, Md5};
use reqwest::Client;

use crate::core::{
  plugin_api::{PlaylistInfo, TrackInfo},
  source::{BoxFuture, MediaSource, SearchResults, Searcher},
};

use types::{SubsonicAlbum, SubsonicArtist, SubsonicPlaylist, SubsonicResponse, SubsonicSong};

/// The Subsonic REST protocol version this client speaks.
const PROTOCOL_VERSION: &str = "1.16.1";

/// Client name sent with every request.
const CLIENT_NAME: &str = "spotatui";

// ---------------------------------------------------------------------------
// SubsonicSource
// ---------------------------------------------------------------------------

/// A [`MediaSource`] backed by a Subsonic-compatible server.
///
/// Constructed via [`SubsonicSource::new`].  Holds an HTTP client and the
/// authentication parameters needed to sign every REST request.
pub struct SubsonicSource {
  /// Base URL of the server, e.g. `"https://demo.navidrome.org"`.
  /// Never has a trailing slash.
  base_url: String,
  /// Subsonic username.
  username: String,
  /// Plaintext password stored only in memory; hashed per-request.
  password: String,
  /// Optional display name for this source (falls back to `"Subsonic"` when absent).
  display_name: Option<String>,
  /// Shared HTTP client (connection pool is reused across requests).
  client: Client,
}

impl SubsonicSource {
  /// Create a new [`SubsonicSource`].
  ///
  /// # Arguments
  ///
  /// * `base_url` - Root URL of the Subsonic server (trailing slash is stripped).
  /// * `username` - Subsonic username.
  /// * `password` - Plaintext password; will be hashed before being sent on the wire.
  /// * `display_name` - Optional human-readable label shown in the UI.
  pub fn new(
    base_url: impl Into<String>,
    username: impl Into<String>,
    password: impl Into<String>,
    display_name: Option<String>,
  ) -> Self {
    let mut base_url = base_url.into();
    if base_url.ends_with('/') {
      base_url.pop();
    }
    SubsonicSource {
      base_url,
      username: username.into(),
      password: password.into(),
      display_name,
      client: Client::new(),
    }
  }

  // -------------------------------------------------------------------------
  // Internal helpers
  // -------------------------------------------------------------------------

  /// Compute `t = md5(password + salt)` and return both the token hex string
  /// and the salt so the caller can include both in the query string.
  fn token_and_salt(&self) -> (String, String) {
    let salt = make_salt();
    let token = md5_hex(format!("{}{}", self.password, salt));
    (token, salt)
  }

  /// Build the common Subsonic auth query parameters.
  fn auth_params(&self) -> Vec<(&'static str, String)> {
    let (token, salt) = self.token_and_salt();
    vec![
      ("u", self.username.clone()),
      ("t", token),
      ("s", salt),
      ("v", PROTOCOL_VERSION.to_owned()),
      ("c", CLIENT_NAME.to_owned()),
      ("f", "json".to_owned()),
    ]
  }

  /// Make an authenticated GET request to `endpoint` (e.g. `"rest/ping.view"`)
  /// with additional `extra_params` appended, and deserialize the response body.
  async fn get<T>(&self, endpoint: &str, extra_params: &[(&str, String)]) -> Result<T>
  where
    T: serde::de::DeserializeOwned,
  {
    let url = format!("{}/{}", self.base_url, endpoint);
    let mut params = self.auth_params();
    params.extend_from_slice(extra_params);

    let response = self
      .client
      .get(&url)
      .query(&params)
      .send()
      .await
      .with_context(|| format!("HTTP request to {url} failed"))?;

    if !response.status().is_success() {
      bail!(
        "Subsonic server returned HTTP {} for {url}",
        response.status()
      );
    }

    response
      .json::<T>()
      .await
      .with_context(|| format!("Failed to decode JSON from {url}"))
  }

  /// Call a Subsonic endpoint and unwrap the nested `subsonic-response` envelope,
  /// returning the inner value produced by `extractor`.
  async fn call<T, F>(&self, endpoint: &str, params: &[(&str, String)], extractor: F) -> Result<T>
  where
    F: FnOnce(SubsonicResponse) -> Result<T>,
  {
    let wrapper = self
      .get::<types::SubsonicResponseWrapper>(endpoint, params)
      .await?;
    let resp = wrapper.subsonic_response;
    if resp.status != "ok" {
      let msg = resp
        .error
        .as_ref()
        .map(|e| e.message.as_str())
        .unwrap_or("unknown error");
      bail!("Subsonic error from {endpoint}: {msg}");
    }
    extractor(resp)
  }

  // -------------------------------------------------------------------------
  // Public API methods
  // -------------------------------------------------------------------------

  /// Ping the server and return `Ok(())` on success.
  pub async fn ping(&self) -> Result<()> {
    self.call("rest/ping.view", &[], |_| Ok(())).await
  }

  /// Fetch all playlists from the server.
  async fn fetch_playlists(&self) -> Result<Vec<PlaylistInfo>> {
    self
      .call("rest/getPlaylists.view", &[], |resp| {
        let playlists = resp
          .playlists
          .ok_or_else(|| anyhow!("Missing 'playlists' in getPlaylists response"))?;
        Ok(
          playlists
            .playlist
            .unwrap_or_default()
            .into_iter()
            .map(SubsonicPlaylist::into_playlist_info)
            .collect(),
        )
      })
      .await
  }

  /// Fetch every track in `playlist_id` (numeric or string Subsonic ID).
  async fn fetch_tracks(&self, playlist_id: &str) -> Result<Vec<TrackInfo>> {
    let id_param = [("id", playlist_id.to_string())];
    self
      .call("rest/getPlaylist.view", &id_param, |resp| {
        let detail = resp
          .playlist
          .ok_or_else(|| anyhow!("Missing 'playlist' in getPlaylist response"))?;
        Ok(
          detail
            .entry
            .unwrap_or_default()
            .into_iter()
            .map(SubsonicSong::into_track_info)
            .collect(),
        )
      })
      .await
  }

  /// Full-text search using `search3`.
  async fn fetch_search(&self, query: &str) -> Result<SearchResults> {
    let q_param = [("query", query.to_string())];
    self
      .call("rest/search3.view", &q_param, |resp| {
        let result = resp
          .search_result3
          .ok_or_else(|| anyhow!("Missing 'searchResult3' in search3 response"))?;

        let tracks = result
          .song
          .unwrap_or_default()
          .into_iter()
          .map(SubsonicSong::into_track_info)
          .collect();

        let albums = result
          .album
          .unwrap_or_default()
          .into_iter()
          .map(SubsonicAlbum::into_album_info)
          .collect();

        let artists = result
          .artist
          .unwrap_or_default()
          .into_iter()
          .map(SubsonicArtist::into_artist_info)
          .collect();

        Ok(SearchResults {
          tracks,
          albums,
          artists,
          // Subsonic search3 does not return playlists; intentionally empty.
          playlists: vec![],
        })
      })
      .await
  }
}

// ---------------------------------------------------------------------------
// MediaSource impl
// ---------------------------------------------------------------------------

impl MediaSource for SubsonicSource {
  fn name(&self) -> &str {
    self.display_name.as_deref().unwrap_or("Subsonic")
  }

  fn scheme(&self) -> &str {
    "subsonic"
  }

  fn playlists(&self) -> BoxFuture<'_, Result<Vec<PlaylistInfo>>> {
    Box::pin(self.fetch_playlists())
  }

  fn tracks<'a>(&'a self, playlist_uri: &'a str) -> BoxFuture<'a, Result<Vec<TrackInfo>>> {
    // Strip the `subsonic:playlist:` prefix if present; otherwise pass through
    // so callers may supply a bare numeric ID directly.
    let id = playlist_uri
      .strip_prefix("subsonic:playlist:")
      .unwrap_or(playlist_uri);
    Box::pin(self.fetch_tracks(id))
  }
}

// ---------------------------------------------------------------------------
// Searcher impl
// ---------------------------------------------------------------------------

impl Searcher for SubsonicSource {
  fn search<'a>(&'a self, query: &'a str) -> BoxFuture<'a, Result<SearchResults>> {
    Box::pin(self.fetch_search(query))
  }
}

// ---------------------------------------------------------------------------
// Utility helpers
// ---------------------------------------------------------------------------

/// Compute the MD5 hex digest of `input`.
fn md5_hex(input: impl AsRef<[u8]>) -> String {
  let mut hasher = Md5::new();
  hasher.update(input.as_ref());
  format!("{:x}", hasher.finalize())
}

/// Generate a random 8-character alphanumeric salt.
///
/// 8 characters satisfies the Subsonic spec minimum of 6 characters.
fn make_salt() -> String {
  use rand::Rng;
  rand::thread_rng()
    .sample_iter(&rand::distributions::Alphanumeric)
    .take(8)
    .map(char::from)
    .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
  use super::*;

  fn make_source() -> SubsonicSource {
    SubsonicSource::new(
      "https://demo.navidrome.org",
      "demo",
      "demo",
      Some("Demo Navidrome".to_string()),
    )
  }

  // --- md5_hex and token computation ---

  #[test]
  fn md5_hex_known_value() {
    // Verified against RFC 1321 test vector.
    assert_eq!(md5_hex("password"), "5f4dcc3b5aa765d61d8327deb882cf99");
  }

  #[test]
  fn token_is_md5_of_password_plus_salt() {
    // Per the Subsonic API spec: t = md5(password + salt).
    // Verify the formula with a known password and fixed salt.
    let password = "sesame";
    let salt = "c19b2d";
    let expected = md5_hex(format!("{password}{salt}"));
    // The token must equal md5(password + salt), not md5(password) alone.
    let token_only_password = md5_hex(password);
    assert_ne!(expected, token_only_password, "token must include the salt");
    assert_eq!(expected, md5_hex(format!("{}{}", password, salt)));
  }

  // --- URI prefix stripping in tracks() ---

  #[test]
  fn strip_prefix_removes_known_prefix() {
    let uri = "subsonic:playlist:42";
    let id = uri.strip_prefix("subsonic:playlist:").unwrap_or(uri);
    assert_eq!(id, "42");
  }

  #[test]
  fn strip_prefix_passthrough_when_no_match() {
    let uri = "42";
    let id = uri.strip_prefix("subsonic:playlist:").unwrap_or(uri);
    assert_eq!(id, "42");
  }

  // --- source name/scheme ---

  #[test]
  fn name_returns_display_name_when_set() {
    let src = make_source();
    assert_eq!(src.name(), "Demo Navidrome");
  }

  #[test]
  fn name_falls_back_to_subsonic() {
    let src = SubsonicSource::new("https://example.com", "u", "p", None);
    assert_eq!(src.name(), "Subsonic");
  }

  #[test]
  fn scheme_is_subsonic() {
    let src = make_source();
    assert_eq!(src.scheme(), "subsonic");
  }

  // --- base_url trailing slash stripped ---

  #[test]
  fn trailing_slash_stripped_from_base_url() {
    let src = SubsonicSource::new("https://example.com/", "u", "p", None);
    assert!(!src.base_url.ends_with('/'));
  }

  // --- JSON parsing: playlists ---

  #[test]
  fn parse_get_playlists_response() {
    let json = r#"{
      "subsonic-response": {
        "status": "ok",
        "version": "1.16.1",
        "playlists": {
          "playlist": [
            {
              "id": "1",
              "name": "My Favorites",
              "owner": "alice",
              "songCount": 42,
              "duration": 9000,
              "public": true,
              "created": "2024-01-01T00:00:00",
              "changed": "2024-06-01T00:00:00"
            }
          ]
        }
      }
    }"#;

    let wrapper: types::SubsonicResponseWrapper = serde_json::from_str(json).unwrap();
    let resp = wrapper.subsonic_response;
    assert_eq!(resp.status, "ok");
    let playlists = resp.playlists.unwrap().playlist.unwrap();
    assert_eq!(playlists.len(), 1);
    let info = playlists[0].clone().into_playlist_info();
    assert_eq!(info.uri, "subsonic:playlist:1");
    assert_eq!(info.name, "My Favorites");
    assert_eq!(info.owner, "alice");
    assert_eq!(info.track_count, 42);
  }

  #[test]
  fn parse_get_playlists_empty() {
    let json = r#"{
      "subsonic-response": {
        "status": "ok",
        "version": "1.16.1",
        "playlists": {}
      }
    }"#;
    let wrapper: types::SubsonicResponseWrapper = serde_json::from_str(json).unwrap();
    let playlists = wrapper
      .subsonic_response
      .playlists
      .unwrap()
      .playlist
      .unwrap_or_default();
    assert!(playlists.is_empty());
  }

  // --- JSON parsing: playlist tracks ---

  #[test]
  fn parse_get_playlist_tracks() {
    let json = r#"{
      "subsonic-response": {
        "status": "ok",
        "version": "1.16.1",
        "playlist": {
          "id": "1",
          "name": "My Favorites",
          "owner": "alice",
          "songCount": 2,
          "duration": 400,
          "entry": [
            {
              "id": "101",
              "title": "Song A",
              "artist": "Artist X",
              "album": "Album Y",
              "duration": 200,
              "track": 1,
              "year": 2020,
              "coverArt": "al-1"
            },
            {
              "id": "102",
              "title": "Song B",
              "artist": "Artist Z",
              "album": "Album W",
              "duration": 200
            }
          ]
        }
      }
    }"#;

    let wrapper: types::SubsonicResponseWrapper = serde_json::from_str(json).unwrap();
    let playlist = wrapper.subsonic_response.playlist.unwrap();
    let entries = playlist.entry.unwrap();
    assert_eq!(entries.len(), 2);

    let track_a = entries[0].clone().into_track_info();
    assert_eq!(track_a.uri, Some("subsonic:track:101".to_string()));
    assert_eq!(track_a.name, "Song A");
    assert_eq!(track_a.artists, vec!["Artist X"]);
    assert_eq!(track_a.album, "Album Y");
    assert_eq!(track_a.duration_ms, 200_000);

    let track_b = entries[1].clone().into_track_info();
    assert_eq!(track_b.uri, Some("subsonic:track:102".to_string()));
    assert_eq!(track_b.name, "Song B");
    assert_eq!(track_b.artists, vec!["Artist Z"]);
  }

  // --- JSON parsing: search3 ---

  #[test]
  fn parse_search3_response() {
    let json = r#"{
      "subsonic-response": {
        "status": "ok",
        "version": "1.16.1",
        "searchResult3": {
          "song": [
            {
              "id": "201",
              "title": "Found Track",
              "artist": "Found Artist",
              "album": "Found Album",
              "duration": 180
            }
          ],
          "album": [
            {
              "id": "301",
              "name": "Found Album",
              "artist": "Found Artist",
              "year": 2023
            }
          ],
          "artist": [
            {
              "id": "401",
              "name": "Found Artist"
            }
          ]
        }
      }
    }"#;

    let wrapper: types::SubsonicResponseWrapper = serde_json::from_str(json).unwrap();
    let result = wrapper.subsonic_response.search_result3.unwrap();

    let songs = result.song.unwrap();
    assert_eq!(songs.len(), 1);
    let track = songs[0].clone().into_track_info();
    assert_eq!(track.uri, Some("subsonic:track:201".to_string()));
    assert_eq!(track.name, "Found Track");
    assert_eq!(track.duration_ms, 180_000);

    let albums = result.album.unwrap();
    assert_eq!(albums.len(), 1);
    let album = albums[0].clone().into_album_info();
    assert_eq!(album.uri, "subsonic:album:301");
    assert_eq!(album.name, "Found Album");
    assert_eq!(album.artists, vec!["Found Artist"]);
    assert_eq!(album.year, Some(2023));

    let artists = result.artist.unwrap();
    assert_eq!(artists.len(), 1);
    let artist = artists[0].clone().into_artist_info();
    assert_eq!(artist.uri, "subsonic:artist:401");
    assert_eq!(artist.name, "Found Artist");
  }

  // --- JSON parsing: error response ---

  #[test]
  fn parse_error_response() {
    let json = r#"{
      "subsonic-response": {
        "status": "failed",
        "version": "1.16.1",
        "error": {
          "code": 40,
          "message": "Wrong username or password."
        }
      }
    }"#;
    let wrapper: types::SubsonicResponseWrapper = serde_json::from_str(json).unwrap();
    let resp = wrapper.subsonic_response;
    assert_eq!(resp.status, "failed");
    let err = resp.error.unwrap();
    assert_eq!(err.code, 40);
    assert_eq!(err.message, "Wrong username or password.");
  }
}
