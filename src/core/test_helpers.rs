#![cfg(test)]

use rspotify::model::{
  idtypes::{PlaylistId, UserId},
  playlist::PlaylistTracksRef,
  user::{PrivateUser, PublicUser},
  SimplifiedPlaylist,
};
use std::collections::HashMap;

pub fn private_user(id: &str) -> PrivateUser {
  PrivateUser {
    country: None,
    display_name: Some("Test User".to_string()),
    email: None,
    explicit_content: None,
    external_urls: HashMap::new(),
    followers: None,
    href: "https://api.spotify.com/v1/me".to_string(),
    id: UserId::from_id(id).unwrap().into_static(),
    images: None,
    product: None,
  }
}

pub fn public_user(id: &str, display_name: &str) -> PublicUser {
  PublicUser {
    display_name: Some(display_name.to_string()),
    external_urls: HashMap::new(),
    followers: None,
    href: format!("https://api.spotify.com/v1/users/{id}"),
    id: UserId::from_id(id).unwrap().into_static(),
    images: Vec::new(),
  }
}

pub fn simplified_playlist(
  id: &str,
  name: &str,
  owner_id: &str,
  collaborative: bool,
) -> SimplifiedPlaylist {
  SimplifiedPlaylist {
    collaborative,
    external_urls: HashMap::new(),
    href: format!("https://api.spotify.com/v1/playlists/{id}"),
    id: PlaylistId::from_id(id).unwrap().into_static(),
    images: Vec::new(),
    name: name.to_string(),
    owner: public_user(owner_id, owner_id),
    public: Some(false),
    snapshot_id: "snapshot".to_string(),
    tracks: PlaylistTracksRef {
      href: format!("https://api.spotify.com/v1/playlists/{id}/tracks"),
      total: 5,
    },
  }
}
