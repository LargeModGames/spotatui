#![cfg(test)]

use crate::core::app::UserInfo;
use crate::core::plugin_api::PlaylistInfo;
use chrono::Duration;
use rspotify::model::{
  idtypes::{PlaylistId, UserId},
  playlist::PlaylistTracksRef,
  track::FullTrack,
  user::{PrivateUser, PublicUser},
  SimplifiedAlbum, SimplifiedArtist, SimplifiedPlaylist, TrackId,
};
use std::collections::HashMap;

/// Domain [`UserInfo`] for tests. `display_name` mirrors `private_user`.
pub fn user_info(id: &str) -> UserInfo {
  UserInfo {
    id: id.to_string(),
    display_name: Some("Test User".to_string()),
    country: None,
  }
}

/// Domain [`PlaylistInfo`] mirroring [`simplified_playlist`] (owner display name
/// equals the owner id, matching `PlaylistInfo::from_simplified` on that fixture).
pub fn playlist_info(id: &str, name: &str, owner_id: &str, collaborative: bool) -> PlaylistInfo {
  PlaylistInfo {
    uri: format!("spotify:playlist:{id}"),
    name: name.to_string(),
    owner: owner_id.to_string(),
    track_count: 5,
    id: Some(id.to_string()),
    owner_id: Some(owner_id.to_string()),
    collaborative,
    public: Some(false),
    image_url: None,
  }
}

// Superseded by `user_info` after the playlist slice migrated `App.user` to the
// domain `UserInfo`; kept until the remaining slices that may still reference it land.
#[allow(deprecated, dead_code)]
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

#[allow(deprecated)]
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

#[allow(deprecated)]
pub fn simplified_playlist(
  id: &str,
  name: &str,
  owner_id: &str,
  collaborative: bool,
) -> SimplifiedPlaylist {
  let tracks = PlaylistTracksRef {
    href: format!("https://api.spotify.com/v1/playlists/{id}/tracks"),
    total: 5,
  };
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
    tracks: tracks.clone(),
    items: tracks,
  }
}

#[allow(deprecated)]
pub fn full_track(id: &str, name: &str) -> FullTrack {
  FullTrack {
    album: SimplifiedAlbum {
      name: "Test Album".to_string(),
      ..Default::default()
    },
    artists: vec![SimplifiedArtist {
      name: "Test Artist".to_string(),
      ..Default::default()
    }],
    available_markets: Vec::new(),
    disc_number: 1,
    duration: Duration::milliseconds(180_000),
    explicit: false,
    external_ids: HashMap::new(),
    external_urls: HashMap::new(),
    href: None,
    id: Some(TrackId::from_id(id).unwrap().into_static()),
    is_local: false,
    is_playable: Some(true),
    linked_from: None,
    restrictions: None,
    name: name.to_string(),
    popularity: 50,
    preview_url: None,
    track_number: 1,
    r#type: rspotify::model::Type::Track,
  }
}

/// Scripted [`Onboarding`] double: hands out canned prompt answers in order
/// (each with the trailing newline `read_line` would produce) and records
/// everything shown.
pub struct ScriptedOnboarding {
  answers: std::sync::Mutex<std::collections::VecDeque<String>>,
  pub shown: std::sync::Mutex<Vec<String>>,
  pub asked: std::sync::Mutex<Vec<crate::core::onboarding::OnboardingPrompt>>,
  interactive: bool,
}

impl ScriptedOnboarding {
  pub fn with_answers(answers: &[&str]) -> Self {
    Self {
      answers: std::sync::Mutex::new(answers.iter().map(|answer| format!("{answer}\n")).collect()),
      shown: std::sync::Mutex::new(Vec::new()),
      asked: std::sync::Mutex::new(Vec::new()),
      interactive: true,
    }
  }

  /// A double for the non-interactive path (`is_interactive` reports false),
  /// matching what a windowed or piped frontend would see.
  pub fn non_interactive(answers: &[&str]) -> Self {
    let mut onboarding = Self::with_answers(answers);
    onboarding.interactive = false;
    onboarding
  }

  pub fn saw(&self, text: &str) -> bool {
    self.shown.lock().unwrap().iter().any(|shown| shown == text)
  }
}

impl crate::core::onboarding::Onboarding for ScriptedOnboarding {
  fn info(&self, text: &str) {
    self.shown.lock().unwrap().push(text.to_string());
  }

  fn progress(&self, text: &str) {
    self.shown.lock().unwrap().push(text.to_string());
  }

  fn prompt_line(&self, prompt: &str) -> anyhow::Result<String> {
    self.shown.lock().unwrap().push(prompt.to_string());
    self
      .answers
      .lock()
      .unwrap()
      .pop_front()
      .ok_or_else(|| anyhow::anyhow!("script ran out of answers for prompt {prompt:?}"))
  }

  fn pick_sources(
    &self,
    _options: &[crate::core::source::Source],
  ) -> anyhow::Result<Option<Vec<crate::core::source::Source>>> {
    Ok(None)
  }

  fn is_interactive(&self) -> bool {
    self.interactive
  }

  fn ask(
    &self,
    prompt: &crate::core::onboarding::OnboardingPrompt,
  ) -> anyhow::Result<crate::core::onboarding::OnboardingAnswer> {
    use crate::core::onboarding::{confirm_answer, OnboardingPrompt};
    self.asked.lock().unwrap().push(prompt.clone());
    // Record what the terminal impl would render, so `saw` can pin the text.
    let OnboardingPrompt::Confirm {
      title,
      body,
      question,
    } = prompt;
    {
      let mut shown = self.shown.lock().unwrap();
      shown.push(title.clone());
      shown.push(body.clone());
      shown.push(question.clone());
    }
    let answer = self
      .answers
      .lock()
      .unwrap()
      .pop_front()
      .ok_or_else(|| anyhow::anyhow!("script ran out of answers for prompt {prompt:?}"))?;
    Ok(confirm_answer(&answer))
  }
}
