use crate::core::app::SearchResult;
use crate::core::user_config::UserConfig;
use crate::infra::network::{IoEvent, Network};

use super::util::{Flag, Format, FormatType, JumpDirection, Type};

use anyhow::{anyhow, Result};
use reqwest::Method;
use rspotify::model::{context::CurrentPlaybackContext, idtypes::Id, PlayableItem};

pub struct CliApp {
  pub net: Network,
  pub config: UserConfig,
}

// Non-concurrent functions
// I feel that async in a cli is not working
// I just .await all processes and directly interact
// by calling network.handle_network_event
impl CliApp {
  pub fn new(net: Network, config: UserConfig) -> Self {
    Self { net, config }
  }

  async fn is_a_saved_track(&mut self, id: &str) -> Result<bool> {
    // Tracks without an ID (e.g. local files) can't be saved; asking the API
    // about `spotify:track:` would only fail the whole status output.
    if id.is_empty() {
      return Ok(false);
    }
    // The IoEvent handler defers to a detached worker (it must not block the
    // TUI's serial pump); the CLI needs the answer before returning.
    self.net.resolve_liked_state_now(&[id.to_string()]).await?;
    Ok(self.net.app.lock().await.liked_song_ids_set().contains(id))
  }

  pub fn format_output(&self, format: String, values: Vec<Format>) -> String {
    super::util::render_format(&format, &values, &self.config)
  }

  // spotatui playback -t
  pub async fn toggle_playback(&mut self) {
    let context = self.net.app.lock().await.current_playback_context.clone();
    if let Some(c) = context {
      if c.is_playing {
        self.net.handle_network_event(IoEvent::PausePlayback).await;
        return;
      }
    }
    self
      .net
      .handle_network_event(IoEvent::StartPlayback(None, None, None))
      .await;
  }

  // spotatui pb --share-track (share the current playing song)
  // Basically copy-pasted the 'copy_song_url' function
  pub async fn share_track_or_episode(&mut self) -> Result<String> {
    let app = self.net.app.lock().await;
    if let Some(CurrentPlaybackContext {
      item: Some(item), ..
    }) = &app.current_playback_context
    {
      match item {
        PlayableItem::Track(track) => {
          if let Some(id) = &track.id {
            Ok(format!("https://open.spotify.com/track/{}", id.id()))
          } else {
            Err(anyhow!("track has no ID"))
          }
        }
        PlayableItem::Episode(episode) => Ok(format!(
          "https://open.spotify.com/episode/{}",
          episode.id.id()
        )),
        _ => Err(anyhow!("unknown playable item type")),
      }
    } else {
      Err(anyhow!(
        "failed to generate a shareable url for the current song"
      ))
    }
  }

  // spotatui pb --share-album (share the current album)
  // Basically copy-pasted the 'copy_album_url' function
  pub async fn share_album_or_show(&mut self) -> Result<String> {
    let app = self.net.app.lock().await;
    if let Some(CurrentPlaybackContext {
      item: Some(item), ..
    }) = &app.current_playback_context
    {
      match item {
        PlayableItem::Track(track) => {
          if let Some(id) = &track.album.id {
            Ok(format!("https://open.spotify.com/album/{}", id.id()))
          } else {
            Err(anyhow!("album has no ID"))
          }
        }
        PlayableItem::Episode(episode) => Ok(format!(
          "https://open.spotify.com/show/{}",
          episode.show.id.id()
        )),
        _ => Err(anyhow!("unknown playable item type")),
      }
    } else {
      Err(anyhow!(
        "failed to generate a shareable url for the current song"
      ))
    }
  }

  // spotatui ... -d ... (specify device to control)
  pub async fn set_device(&mut self, name: String) -> Result<()> {
    // Change the device if specified by user
    let app = self.net.app.lock().await;
    if let Some(dp) = app.devices() {
      for d in &dp.devices {
        if d.name == name {
          // Save the id of the device
          if let Some(id) = d.id.clone() {
            self
              .net
              .client_config
              .set_device_id(id)
              .map_err(|_e| anyhow!("failed to use device with name '{}'", d.name))?;
          }
        }
      }
    } else {
      // Error out if no device is available
      return Err(anyhow!("no device available"));
    }
    Ok(())
  }

  // spotatui list/search --limit LIMIT (set max search limit)
  pub async fn update_query_limits(&mut self, max: String, ceiling: u32) -> Result<()> {
    let num = parse_query_limit(&max, ceiling)?;

    self
      .net
      .handle_network_event(IoEvent::UpdateSearchLimits(num, num))
      .await;
    Ok(())
  }

  pub async fn volume(&mut self, vol: String) -> Result<()> {
    let num = vol
      .parse::<u32>()
      .map_err(|_e| anyhow!("volume must be between 0 and 100"))?;

    // Check if it's in range
    if num > 100 {
      return Err(anyhow!("volume must be between 0 and 100"));
    };

    self
      .net
      .handle_network_event(IoEvent::ChangeVolume(num as u8))
      .await;
    Ok(())
  }

  // spotatui playback --next / --previous
  pub async fn jump(&mut self, d: &JumpDirection) {
    match d {
      JumpDirection::Next => self.net.handle_network_event(IoEvent::NextTrack).await,
      JumpDirection::Previous => self.net.handle_network_event(IoEvent::PreviousTrack).await,
    }
  }

  // spotatui list ...
  pub async fn list(&mut self, item: Type, format: &str) -> String {
    match item {
      Type::Device => {
        if let Some(devices) = self.net.app.lock().await.devices() {
          devices
            .devices
            .iter()
            .map(|d| {
              self.format_output(
                format.to_string(),
                vec![
                  Format::Device(d.name.clone()),
                  Format::Volume(d.volume_percent.unwrap_or(0)),
                ],
              )
            })
            .collect::<Vec<String>>()
            .join("\n")
        } else {
          "No devices available".to_string()
        }
      }
      Type::Playlist => {
        self.net.handle_network_event(IoEvent::GetPlaylists).await;
        if let Some(playlists) = &self.net.app.lock().await.playlists() {
          playlists
            .items
            .iter()
            .map(|p| {
              self.format_output(
                format.to_string(),
                Format::from_type(FormatType::PlaylistInfo(Box::new(p.clone()))),
              )
            })
            .collect::<Vec<String>>()
            .join("\n")
        } else {
          "No playlists found".to_string()
        }
      }
      Type::Liked => {
        self
          .net
          .handle_network_event(IoEvent::GetCurrentSavedTracks(None))
          .await;
        let liked_songs = self
          .net
          .app
          .lock()
          .await
          .track_table
          .tracks
          .iter()
          .map(|t| {
            self.format_output(
              format.to_string(),
              Format::from_type(FormatType::TrackInfo(Box::new(t.clone()))),
            )
          })
          .collect::<Vec<String>>();
        // Check if there are any liked songs
        if liked_songs.is_empty() {
          "No liked songs found".to_string()
        } else {
          liked_songs.join("\n")
        }
      }
      // Enforced by clap
      _ => unreachable!(),
    }
  }

  // spotatui playback --transfer DEVICE
  pub async fn transfer_playback(&mut self, device: &str) -> Result<()> {
    // Get the device id by name
    let mut id = String::new();
    if let Some(devices) = self.net.app.lock().await.devices() {
      for d in &devices.devices {
        if d.name == device {
          if let Some(device_id) = &d.id {
            id.push_str(device_id);
            break;
          }
          break;
        }
      }
    };

    if id.is_empty() {
      Err(anyhow!("no device with name '{}'", device))
    } else {
      self
        .net
        .handle_network_event(IoEvent::TransferPlaybackToDevice(id.to_string(), true))
        .await;
      Ok(())
    }
  }

  pub async fn seek(&mut self, seconds_str: String) -> Result<()> {
    let seconds = match seconds_str.parse::<i32>() {
      Ok(s) => s.unsigned_abs(),
      Err(_) => return Err(anyhow!("failed to convert seconds to i32")),
    };

    let (current_pos, duration) = {
      self
        .net
        .handle_network_event(IoEvent::GetCurrentPlayback)
        .await;
      let app = self.net.app.lock().await;
      if let Some(CurrentPlaybackContext {
        progress: Some(ms),
        item: Some(item),
        ..
      }) = &app.current_playback_context
      {
        let duration = match item {
          PlayableItem::Track(track) => track.duration.num_milliseconds() as u32,
          PlayableItem::Episode(episode) => episode.duration.num_milliseconds() as u32,
          _ => return Err(anyhow!("unknown playable item type")),
        };

        (ms.num_milliseconds() as u32, duration)
      } else {
        return Err(anyhow!("no context available"));
      }
    };

    // Convert secs to ms
    let ms = seconds * 1000;
    // Calculate new positon
    let position_to_seek = if seconds_str.starts_with('+') {
      current_pos + ms
    } else if seconds_str.starts_with('-') {
      // Jump to the beginning if the position_to_seek would be
      // negative, must be checked before the calculation to avoid
      // an 'underflow'
      current_pos.saturating_sub(ms)
    } else {
      // Absolute value of the track
      seconds * 1000
    };

    // Check if position_to_seek is greater than duration (next track)
    if position_to_seek > duration {
      self.jump(&JumpDirection::Next).await;
    } else {
      // This seeks to a position in the current song
      self
        .net
        .handle_network_event(IoEvent::Seek(position_to_seek))
        .await;
    }

    Ok(())
  }

  // spotatui playback --like / --dislike / --shuffle / --repeat
  pub async fn mark(&mut self, flag: Flag) -> Result<()> {
    let c = {
      let app = self.net.app.lock().await;
      app
        .current_playback_context
        .clone()
        .ok_or_else(|| anyhow!("no context available"))?
    };

    match flag {
      Flag::Like(s) => {
        // Get the id of the current song
        let id = match c.item {
          Some(i) => match i {
            PlayableItem::Track(t) => t.id.ok_or_else(|| anyhow!("item has no id")),
            PlayableItem::Episode(_) => Err(anyhow!("saving episodes not yet implemented")),
            _ => Err(anyhow!("unknown playable item type")),
          },
          None => Err(anyhow!("no item playing")),
        }?;

        let id_string = id.id().to_string();
        // Want to like but is already liked -> do nothing
        // Want to like and is not liked yet -> like
        if s && !self.is_a_saved_track(&id_string).await? {
          self
            .net
            .handle_network_event(IoEvent::ToggleSaveTrack(id.uri()))
            .await;
        // Want to dislike but is already disliked -> do nothing
        // Want to dislike and is liked currently -> remove like
        } else if !s && self.is_a_saved_track(&id_string).await? {
          self
            .net
            .handle_network_event(IoEvent::ToggleSaveTrack(id.uri()))
            .await;
        }
      }
      Flag::Shuffle => {
        self
          .net
          .handle_network_event(IoEvent::Shuffle(!c.shuffle_state))
          .await
      }
      Flag::Repeat => {
        self
          .net
          .handle_network_event(IoEvent::Repeat(c.repeat_state))
          .await;
      }
    }

    Ok(())
  }

  // spotatui playback -s
  pub async fn get_status(&mut self, format: String) -> Result<String> {
    // Update info on current playback
    self
      .net
      .handle_network_event(IoEvent::GetCurrentPlayback)
      .await;
    self
      .net
      .handle_network_event(IoEvent::GetCurrentSavedTracks(None))
      .await;

    let context = self
      .net
      .app
      .lock()
      .await
      .current_playback_context
      .clone()
      .ok_or_else(|| anyhow!("no context available"))?;

    let playing_item = context.item.ok_or_else(|| anyhow!("no track playing"))?;

    let mut hs = match playing_item {
      PlayableItem::Track(track) => {
        let id = track
          .id
          .clone()
          .map(|track_id| track_id.id().to_string())
          .unwrap_or_default();
        let mut hs = Format::from_type(FormatType::Track(Box::new(track.clone())));
        if let Some(ms) = &context.progress {
          hs.push(Format::Position((
            ms.num_milliseconds() as u32,
            track.duration.num_milliseconds() as u32,
          )))
        }
        hs.push(Format::Flags((
          context.repeat_state,
          context.shuffle_state,
          self.is_a_saved_track(&id).await?,
        )));
        hs
      }
      PlayableItem::Episode(episode) => {
        let mut hs = Format::from_type(FormatType::Episode(Box::new(episode.clone())));
        if let Some(ms) = &context.progress {
          hs.push(Format::Position((
            ms.num_milliseconds() as u32,
            episode.duration.num_milliseconds() as u32,
          )))
        }
        hs.push(Format::Flags((
          context.repeat_state,
          context.shuffle_state,
          false,
        )));
        hs
      }
      _ => return Err(anyhow!("unknown playable item type")),
    };

    hs.push(Format::Device(context.device.name));
    hs.push(Format::Volume(context.device.volume_percent.unwrap_or(0)));
    hs.push(Format::Playing(context.is_playing));

    Ok(self.format_output(format, hs))
  }

  // spotatui play -u URI
  pub async fn play_uri(&mut self, uri: String, queue: bool, random: bool) {
    let offset = if random {
      // Only works with playlists for now
      if uri.contains("spotify:playlist:") {
        let id_str = uri.split(':').next_back().unwrap();
        if let Ok(playlist_id) = rspotify::model::idtypes::PlaylistId::from_id(id_str) {
          match self
            .net
            .spotify_api_request_json(
              Method::GET,
              &format!("playlists/{}", playlist_id.id()),
              &[],
              None,
            )
            .await
            .and_then(|payload| Ok(serde_json::from_value::<PlaylistItemsTotal>(payload)?))
          {
            Ok(p) => {
              // A playlist the user does not own gets no item page at all in
              // Development Mode, and that is not the same as an empty one.
              let Some(total) = playlist_items_total(&p) else {
                self
                  .net
                  .app
                  .lock()
                  .await
                  .handle_error(anyhow!(
                    "playlist track count is unavailable: Spotify sends no item page for playlists you do not own"
                  ));
                return;
              };
              let Some(offset) = random_offset(total) else {
                self
                  .net
                  .app
                  .lock()
                  .await
                  .handle_error(anyhow!("playlist has no tracks"));
                return;
              };
              Some(offset)
            }
            Err(e) => {
              self
                .net
                .app
                .lock()
                .await
                .handle_error(anyhow!(e.to_string()));
              return;
            }
          }
        } else {
          None
        }
      } else {
        None
      }
    } else {
      None
    };

    // The network boundary reconstructs the typed rspotify id from the URI.
    if uri.contains("spotify:track:") {
      if queue {
        self
          .net
          .handle_network_event(IoEvent::AddItemToQueue(uri))
          .await;
      } else {
        self
          .net
          .handle_network_event(IoEvent::StartPlayback(None, Some(vec![uri]), Some(0)))
          .await;
      }
    } else if uri.contains("spotify:playlist:")
      || uri.contains("spotify:album:")
      || uri.contains("spotify:artist:")
      || uri.contains("spotify:show:")
    {
      // Context URIs (playlist, album, artist, show)
      self
        .net
        .handle_network_event(IoEvent::StartPlayback(Some(uri), None, offset))
        .await;
    }
  }

  // spotatui play -n NAME ...
  pub async fn play(&mut self, name: String, item: Type, queue: bool, random: bool) -> Result<()> {
    self
      .net
      .handle_network_event(IoEvent::GetSearchResults(name.clone(), None))
      .await;
    // Get the uri of the first found
    // item + the offset or return an error message
    let uri = {
      let app = self.net.app.lock().await;
      first_result_uri(app.search_results(), &item, &name)?
    };

    // Play or queue the uri
    self.play_uri(uri, queue, random).await;

    Ok(())
  }

  // spotatui search SEARCH ...
  pub async fn query(&mut self, search: String, format: String, item: Type) -> String {
    self
      .net
      .handle_network_event(IoEvent::GetSearchResults(search.clone(), None))
      .await;

    let app = self.net.app.lock().await;
    match item {
      Type::Playlist => {
        if let Some(results) = app
          .search_results()
          .playlists
          .as_ref()
          .filter(|r| !r.items.is_empty())
        {
          results
            .items
            .iter()
            .map(|r| {
              self.format_output(
                format.clone(),
                Format::from_type(FormatType::PlaylistInfo(Box::new(r.clone()))),
              )
            })
            .collect::<Vec<String>>()
            .join("\n")
        } else {
          format!("no playlists with name '{}'", search)
        }
      }
      Type::Track => {
        if let Some(results) = app
          .search_results()
          .tracks
          .as_ref()
          .filter(|r| !r.items.is_empty())
        {
          results
            .items
            .iter()
            .map(|r| {
              self.format_output(
                format.clone(),
                Format::from_type(FormatType::TrackInfo(Box::new(r.clone()))),
              )
            })
            .collect::<Vec<String>>()
            .join("\n")
        } else {
          format!("no tracks with name '{}'", search)
        }
      }
      Type::Artist => {
        if let Some(results) = app
          .search_results()
          .artists
          .as_ref()
          .filter(|r| !r.items.is_empty())
        {
          results
            .items
            .iter()
            .map(|r| {
              self.format_output(
                format.clone(),
                Format::from_type(FormatType::ArtistInfo(Box::new(r.clone()))),
              )
            })
            .collect::<Vec<String>>()
            .join("\n")
        } else {
          format!("no artists with name '{}'", search)
        }
      }
      Type::Show => {
        if let Some(results) = app
          .search_results()
          .shows
          .as_ref()
          .filter(|r| !r.items.is_empty())
        {
          results
            .items
            .iter()
            .map(|r| {
              self.format_output(
                format.clone(),
                Format::from_type(FormatType::ShowInfo(Box::new(r.clone()))),
              )
            })
            .collect::<Vec<String>>()
            .join("\n")
        } else {
          format!("no shows with name '{}'", search)
        }
      }
      Type::Album => {
        if let Some(results) = app
          .search_results()
          .albums
          .as_ref()
          .filter(|r| !r.items.is_empty())
        {
          results
            .items
            .iter()
            .map(|r| {
              self.format_output(
                format.clone(),
                Format::from_type(FormatType::AlbumInfo(Box::new(r.clone()))),
              )
            })
            .collect::<Vec<String>>()
            .join("\n")
        } else {
          format!("no albums with name '{}'", search)
        }
      }
      // Enforced by clap
      _ => unreachable!(),
    }
  }
}

// The `--random` offset only needs the playlist's track count. The migrated
// Web API response no longer carries the item page in the shape rspotify's
// `FullPlaylist` expects, so decoding the whole playlist model fails. The
// raw response is read instead: it still names the total wherever it puts
// it, and when the user does not own the playlist in Development Mode no
// item page exists at all - which must stay distinguishable from a real zero.
#[derive(Debug, serde::Deserialize)]
struct PlaylistItemsTotal {
  #[serde(default)]
  items: Option<PlaylistTotalRef>,
  #[serde(default)]
  tracks: Option<PlaylistTotalRef>,
}

#[derive(Debug, serde::Deserialize)]
struct PlaylistTotalRef {
  #[serde(default)]
  total: Option<u32>,
}

/// The playlist's track count, or `None` when the response carries no item
/// page at all - the Development Mode shape for a playlist the user does not
/// own. `Some(0)` is a real empty playlist and means something different.
fn playlist_items_total(playlist: &PlaylistItemsTotal) -> Option<u32> {
  playlist
    .items
    .as_ref()
    .and_then(|ref_| ref_.total)
    .or_else(|| playlist.tracks.as_ref().and_then(|ref_| ref_.total))
}

fn first_result_uri(results: &SearchResult, item: &Type, name: &str) -> Result<String> {
  let (kind, id) = match item {
    Type::Track => {
      let track = results
        .tracks
        .as_ref()
        .and_then(|r| r.items.first())
        .ok_or_else(|| anyhow!("no tracks with name '{}'", name))?;
      (
        "track",
        track
          .id
          .as_ref()
          .ok_or_else(|| anyhow!("track has no id"))?,
      )
    }
    Type::Album => {
      let album = results
        .albums
        .as_ref()
        .and_then(|r| r.items.first())
        .ok_or_else(|| anyhow!("no albums with name '{}'", name))?;
      (
        "album",
        album
          .id
          .as_ref()
          .ok_or_else(|| anyhow!("album {} has no id", album.name))?,
      )
    }
    Type::Artist => {
      let artist = results
        .artists
        .as_ref()
        .and_then(|r| r.items.first())
        .ok_or_else(|| anyhow!("no artists with name '{}'", name))?;
      (
        "artist",
        artist
          .id
          .as_ref()
          .ok_or_else(|| anyhow!("artist has no id"))?,
      )
    }
    Type::Show => {
      let show = results
        .shows
        .as_ref()
        .and_then(|r| r.items.first())
        .ok_or_else(|| anyhow!("no shows with name '{}'", name))?;
      (
        "show",
        show.id.as_ref().ok_or_else(|| anyhow!("show has no id"))?,
      )
    }
    Type::Playlist => {
      let playlist = results
        .playlists
        .as_ref()
        .and_then(|r| r.items.first())
        .ok_or_else(|| anyhow!("no playlists with name '{}'", name))?;
      (
        "playlist",
        playlist
          .id
          .as_ref()
          .ok_or_else(|| anyhow!("playlist has no id"))?,
      )
    }
    // Enforced by clap.
    _ => unreachable!(),
  };
  Ok(format!("spotify:{kind}:{id}"))
}

fn random_offset(total: u32) -> Option<usize> {
  (total > 0).then(|| rand::random_range(0..total) as usize)
}

fn parse_query_limit(max: &str, ceiling: u32) -> Result<u32> {
  match max.parse::<u32>() {
    Ok(num) if (1..=ceiling).contains(&num) => Ok(num),
    _ => Err(anyhow!("limit must be between 1 and {ceiling}")),
  }
}

#[cfg(test)]
mod tests {
  use super::{
    first_result_uri, parse_query_limit, playlist_items_total, random_offset, SearchResult, Type,
  };
  use crate::core::pagination::Paged;
  use crate::core::plugin_api::{AlbumInfo, ArtistInfo, ShowInfo, TrackInfo};
  use crate::core::test_helpers::{full_track, playlist_info};

  fn search_results_with_hits() -> SearchResult {
    SearchResult {
      tracks: Some(Paged {
        items: vec![
          TrackInfo::from(&full_track("4uLU6hMCjMI75M1A2tKUQC", "A")),
          TrackInfo::from(&full_track("1301WleyT98MSxVHPZCA6M", "B")),
        ],
        ..Paged::default()
      }),
      albums: Some(Paged {
        items: vec![AlbumInfo {
          id: Some("album1".to_string()),
          name: "Album".to_string(),
          ..AlbumInfo::default()
        }],
        ..Paged::default()
      }),
      artists: Some(Paged {
        items: vec![ArtistInfo {
          id: Some("artist1".to_string()),
          ..ArtistInfo::default()
        }],
        ..Paged::default()
      }),
      shows: Some(Paged {
        items: vec![ShowInfo {
          id: Some("show1".to_string()),
          ..ShowInfo::default()
        }],
        ..Paged::default()
      }),
      playlists: Some(Paged {
        items: vec![
          playlist_info("pl1", "Mix", "owner", false),
          playlist_info("pl2", "Other", "owner", false),
        ],
        ..Paged::default()
      }),
    }
  }

  #[test]
  fn play_by_name_reports_no_tracks_when_the_search_page_is_empty() {
    let results = SearchResult {
      tracks: Some(Paged::default()),
      ..SearchResult::default()
    };
    assert_eq!(
      first_result_uri(&results, &Type::Track, "qwxz")
        .unwrap_err()
        .to_string(),
      "no tracks with name 'qwxz'"
    );
  }

  #[test]
  fn play_by_name_reports_no_playlists_when_the_search_page_is_empty() {
    let results = SearchResult {
      playlists: Some(Paged::default()),
      ..SearchResult::default()
    };
    assert_eq!(
      first_result_uri(&results, &Type::Playlist, "qwxz")
        .unwrap_err()
        .to_string(),
      "no playlists with name 'qwxz'"
    );
  }

  #[test]
  fn play_by_name_reports_empty_album_artist_and_show_pages() {
    let results = SearchResult {
      albums: Some(Paged::default()),
      artists: Some(Paged::default()),
      shows: Some(Paged::default()),
      ..SearchResult::default()
    };
    for (item, kind) in [
      (Type::Album, "albums"),
      (Type::Artist, "artists"),
      (Type::Show, "shows"),
    ] {
      assert_eq!(
        first_result_uri(&results, &item, "qwxz")
          .unwrap_err()
          .to_string(),
        format!("no {kind} with name 'qwxz'")
      );
    }
  }

  #[test]
  fn play_by_name_reports_a_missing_search_page_for_every_search_type() {
    for (item, kind) in [
      (Type::Track, "tracks"),
      (Type::Album, "albums"),
      (Type::Artist, "artists"),
      (Type::Show, "shows"),
      (Type::Playlist, "playlists"),
    ] {
      assert_eq!(
        first_result_uri(&SearchResult::default(), &item, "qwxz")
          .unwrap_err()
          .to_string(),
        format!("no {kind} with name 'qwxz'")
      );
    }
  }

  #[test]
  fn play_by_name_picks_the_first_track_hit() {
    assert_eq!(
      first_result_uri(&search_results_with_hits(), &Type::Track, "A").unwrap(),
      "spotify:track:4uLU6hMCjMI75M1A2tKUQC"
    );
  }

  #[test]
  fn play_by_name_picks_the_first_playlist_hit() {
    assert_eq!(
      first_result_uri(&search_results_with_hits(), &Type::Playlist, "Mix").unwrap(),
      "spotify:playlist:pl1"
    );
  }

  #[test]
  fn play_by_name_preserves_the_type_of_album_artist_and_show_hits() {
    let results = search_results_with_hits();
    for (item, expected) in [
      (Type::Album, "spotify:album:album1"),
      (Type::Artist, "spotify:artist:artist1"),
      (Type::Show, "spotify:show:show1"),
    ] {
      assert_eq!(first_result_uri(&results, &item, "name").unwrap(), expected);
    }
  }

  #[test]
  fn play_by_name_keeps_the_missing_id_errors_for_hits_without_ids() {
    let mut results = search_results_with_hits();
    results.tracks.as_mut().unwrap().items[0].id = None;
    results.albums.as_mut().unwrap().items[0].id = None;
    results.artists.as_mut().unwrap().items[0].id = None;
    results.shows.as_mut().unwrap().items[0].id = None;
    results.playlists.as_mut().unwrap().items[0].id = None;
    for (item, expected) in [
      (Type::Track, "track has no id"),
      (Type::Album, "album Album has no id"),
      (Type::Artist, "artist has no id"),
      (Type::Show, "show has no id"),
      (Type::Playlist, "playlist has no id"),
    ] {
      assert_eq!(
        first_result_uri(&results, &item, "name")
          .unwrap_err()
          .to_string(),
        expected
      );
    }
  }

  #[test]
  fn a_random_offset_into_an_empty_playlist_is_none() {
    assert_eq!(random_offset(0), None);
  }

  #[test]
  fn the_playlist_total_survives_a_migrated_playlist_response() {
    // After Spotify's API migration `playlists/{id}` no longer carries the
    // item page in the shape rspotify's `FullPlaylist` expects. The raw
    // response still names the total wherever it puts it.
    let payload = serde_json::json!({
      "collaborative": false,
      "description": "",
      "external_urls": { "spotify": "https://open.spotify.com/playlist/37i9dQZF1DXcBWIGoYBM5M" },
      "href": "",
      "id": "37i9dQZF1DXcBWIGoYBM5M",
      "images": [],
      "name": "Today's Top Hits",
      "owner": {
        "external_urls": { "spotify": "https://open.spotify.com/user/spotify" },
        "href": "",
        "id": "spotify",
        "type": "user",
        "uri": "spotify:user:spotify"
      },
      "public": false,
      "snapshot_id": "abc",
      "type": "playlist",
      "uri": "spotify:playlist:37i9dQZF1DXcBWIGoYBM5M",
      "tracks": { "href": "", "total": 50 }
    });
    let total = playlist_items_total(&serde_json::from_value(payload).unwrap());
    assert_eq!(total, Some(50));
  }

  #[test]
  fn a_playlist_without_an_item_page_is_not_an_empty_playlist() {
    // For a playlist the user does not own, Development Mode sends no item
    // page at all - there is no total to read. That must stay apart from a
    // real zero: the caller reports it as an unavailable count, not as an
    // empty playlist.
    let payload = serde_json::json!({
      "collaborative": false,
      "description": "The premier league of Japanese city pop",
      "external_urls": { "spotify": "https://open.spotify.com/playlist/37i9dQZF1DXcBWIGoYBM5M" },
      "href": "",
      "id": "37i9dQZF1DXcBWIGoYBM5M",
      "images": [],
      "name": "This Is City Pop",
      "owner": {
        "external_urls": { "spotify": "https://open.spotify.com/user/spotify" },
        "href": "",
        "id": "spotify",
        "type": "user",
        "uri": "spotify:user:spotify"
      },
      "public": false,
      "snapshot_id": "abc",
      "type": "playlist",
      "uri": "spotify:playlist:37i9dQZF1DXcBWIGoYBM5M"
    });
    let total = playlist_items_total(&serde_json::from_value(payload).unwrap());
    assert_eq!(total, None);
  }

  #[test]
  fn a_real_zero_total_is_not_an_unavailable_count() {
    let payload = serde_json::json!({ "items": { "total": 0 } });
    let total = playlist_items_total(&serde_json::from_value(payload).unwrap());
    assert_eq!(total, Some(0));
  }

  #[test]
  fn an_items_object_without_a_total_falls_back_to_tracks() {
    // Some shapes carry an empty items object next to a tracks ref with the
    // real total: the fallback must read tracks.total, not give up.
    let payload = serde_json::json!({ "items": {}, "tracks": { "total": 50 } });
    let total = playlist_items_total(&serde_json::from_value(payload).unwrap());
    assert_eq!(total, Some(50));
  }

  #[test]
  fn a_random_offset_stays_inside_the_playlist() {
    assert_eq!(random_offset(1), Some(0));
    for _ in 0..50 {
      assert!(random_offset(5).unwrap() < 5);
    }
  }

  #[test]
  fn a_query_limit_is_accepted_only_within_its_ceiling() {
    assert_eq!(parse_query_limit("50", 50).unwrap(), 50);
    assert_eq!(parse_query_limit("10", 10).unwrap(), 10);
    assert!(parse_query_limit("51", 50).is_err());
    assert!(parse_query_limit("11", 10).is_err());
    assert!(parse_query_limit("0", 10).is_err());
    assert!(parse_query_limit("ten", 10).is_err());
  }
}
