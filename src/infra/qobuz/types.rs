//! Serde types for the Qobuz API responses, private to the module.

use serde::{Deserialize, Deserializer};

/// A Qobuz id: integers for tracks, playlists and artists, strings for albums.
fn de_id<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
  #[derive(Deserialize)]
  #[serde(untagged)]
  enum RawId {
    Num(u64),
    Str(String),
  }
  Ok(match RawId::deserialize(d)? {
    RawId::Num(n) => n.to_string(),
    RawId::Str(s) => s,
  })
}

fn de_opt_id<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
  #[derive(Deserialize)]
  struct Wrap(#[serde(deserialize_with = "de_id")] String);
  Ok(Option::<Wrap>::deserialize(d)?.map(|w| w.0))
}

fn default_true() -> bool {
  true
}

/// `session/start`.
#[derive(Debug, Deserialize)]
pub struct SessionStart {
  pub session_id: String,
  #[serde(default)]
  pub expires_at: u64,
  /// `"<salt>.<info>"`, both base64url.
  pub infos: String,
}

/// `file/url` with `intent=stream`; the segment count comes from the init table.
#[derive(Debug, Deserialize)]
pub struct FileUrl {
  pub url_template: String,
  /// `"<prefix>.<wrapped>.<iv>"`, base64url.
  pub key: String,
}

/// `oauth/callback`.
#[derive(Debug, Deserialize)]
pub struct OauthCallback {
  pub token: Option<String>,
  pub user_auth_token: Option<String>,
  #[serde(default, deserialize_with = "de_opt_id")]
  pub user_id: Option<String>,
  pub user: Option<UserRef>,
}

#[derive(Debug, Deserialize)]
pub struct UserRef {
  #[serde(default, deserialize_with = "de_opt_id")]
  pub id: Option<String>,
}

/// One page of a paginated list: `{ offset, limit, total, items }`.
#[derive(Debug, Deserialize)]
pub struct Page<T> {
  #[serde(default)]
  pub total: u32,
  #[serde(default = "Vec::new")]
  pub items: Vec<T>,
}

impl<T> Default for Page<T> {
  fn default() -> Self {
    Page {
      total: 0,
      items: Vec::new(),
    }
  }
}

#[derive(Debug, Deserialize)]
pub struct Named {
  #[serde(default, deserialize_with = "de_opt_id")]
  pub id: Option<String>,
  #[serde(default)]
  pub name: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct Image {
  pub large: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Album {
  #[serde(deserialize_with = "de_id")]
  pub id: String,
  #[serde(default)]
  pub title: String,
  pub artist: Option<Named>,
  #[serde(default)]
  pub image: Image,
  #[serde(default)]
  pub tracks_count: u32,
  pub tracks: Option<Page<Track>>,
}

#[derive(Debug, Deserialize)]
pub struct Track {
  #[serde(deserialize_with = "de_id")]
  pub id: String,
  #[serde(default)]
  pub title: String,
  pub version: Option<String>,
  /// Seconds.
  #[serde(default)]
  pub duration: u64,
  #[serde(default)]
  pub track_number: u32,
  pub performer: Option<Named>,
  /// Absent inside `album/get`, where the parent album is the context.
  pub album: Option<Album>,
  #[serde(default = "default_true")]
  pub streamable: bool,
  #[serde(default)]
  pub parental_warning: bool,
}

#[derive(Debug, Deserialize)]
pub struct Playlist {
  #[serde(deserialize_with = "de_id")]
  pub id: String,
  #[serde(default)]
  pub name: String,
  pub owner: Option<Named>,
  #[serde(default)]
  pub tracks_count: u32,
  pub is_public: Option<bool>,
  #[serde(default)]
  pub images300: Vec<String>,
  pub tracks: Option<Page<Track>>,
}

/// `playlist/getUserPlaylists`.
#[derive(Debug, Deserialize)]
pub struct UserPlaylists {
  #[serde(default)]
  pub playlists: Page<Playlist>,
}

/// `favorite/getUserFavorites`.
#[derive(Debug, Deserialize)]
pub struct Favorites {
  pub tracks: Option<Page<Track>>,
  pub albums: Option<Page<Album>>,
}

/// `catalog/search`.
#[derive(Debug, Deserialize)]
pub struct Search {
  pub tracks: Option<Page<Track>>,
}
