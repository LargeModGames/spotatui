use crate::core::app::App;
use crate::core::user_config::UserConfig;
use crate::infra::network::{IoEvent, Network};

use super::util::{Flag, Format, FormatType, JumpDirection, Type};

use anyhow::{anyhow, Result};
use rspotify::model::{
  context::CurrentPlaybackContext, idtypes::Id, playlist::FullPlaylist, PlayableItem,
};

pub struct CliApp {
  pub net: Network,
  pub config: UserConfig,
}

fn clear_sonos_selection_for_spotify_device(app: &mut App) -> Option<String> {
  let room_uuid = app.selected_sonos_room_uuid.take();
  app.sonos_now_playing = None;
  app.sonos_is_playing = None;
  app.sonos_volume = None;
  app.sonos_poll_error_room_uuid = None;
  room_uuid
}

// Non-concurrent functions
// I feel that async in a cli is not working
// I just .await all processes and directly interact
// by calling network.handle_network_event
impl CliApp {
  pub fn new(net: Network, config: UserConfig) -> Self {
    Self { net, config }
  }

  async fn is_a_saved_track(&mut self, id: &str) -> bool {
    // Update the liked_song_ids_set
    self
      .net
      .handle_network_event(IoEvent::CurrentUserSavedTracksContains(
        vec![id.to_string()],
      ))
      .await;
    self.net.app.lock().await.liked_song_ids_set.contains(id)
  }

  pub fn format_output(&self, mut format: String, values: Vec<Format>) -> String {
    for val in values {
      format = format.replace(val.get_placeholder(), &val.inner(self.config.clone()));
    }
    // Replace unsupported flags with 'None'
    for p in &["%a", "%b", "%t", "%p", "%h", "%u", "%d", "%v", "%f", "%s"] {
      format = format.replace(p, "None");
    }
    format.trim().to_string()
  }

  // spt playback -t
  pub async fn toggle_playback(&mut self) {
    let is_playing = {
      let app = self.net.app.lock().await;
      if app.sonos_owns_playback() {
        app.sonos_is_playing.unwrap_or(false)
      } else {
        app
          .current_playback_context
          .as_ref()
          .is_some_and(|context| context.is_playing)
      }
    };
    let event = if is_playing {
      IoEvent::PausePlayback
    } else {
      IoEvent::StartPlayback(None, None, None)
    };
    self.net.handle_network_event(event).await;
  }

  // spt pb --share-track (share the current playing song)
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

  // spt pb --share-album (share the current album)
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

  // spt ... -d ... (specify device to control)
  pub async fn set_device(&mut self, name: String) -> Result<()> {
    // Extract owned device data before issuing a direct Sonos pause; the CLI
    // invokes Network synchronously and does not run the TUI event pump.
    let (device_index, device_id, device_name, sonos_room_uuid) = {
      let app = self.net.app.lock().await;
      let devices = app
        .devices
        .as_ref()
        .ok_or_else(|| anyhow!("no device available"))?;
      let (device_index, device) = devices
        .devices
        .iter()
        .enumerate()
        .find(|(_, device)| device.name == name)
        .ok_or_else(|| anyhow!("no device named '{name}' is available"))?;
      let device_id = device
        .id
        .clone()
        .ok_or_else(|| anyhow!("device '{}' has no usable id", device.name))?;
      (
        device_index,
        device_id,
        device.name.clone(),
        app.selected_sonos_room_uuid.clone(),
      )
    };

    self
      .net
      .client_config
      .set_device_id(device_id)
      .map_err(|_| anyhow!("failed to use device with name '{device_name}'"))?;

    if let Some(room_uuid) = sonos_room_uuid {
      self
        .net
        .handle_network_event(IoEvent::PauseSonosRoom(room_uuid))
        .await;
    }

    let mut app = self.net.app.lock().await;
    clear_sonos_selection_for_spotify_device(&mut app);
    app.selected_device_index = Some(device_index);
    Ok(())
  }

  // spt query ... --limit LIMIT (set max search limit)
  pub async fn update_query_limits(&mut self, max: String) -> Result<()> {
    let num = max
      .parse::<u32>()
      .map_err(|_e| anyhow!("limit must be between 1 and 50"))?;

    // 50 seems to be the maximum limit
    if num > 50 || num == 0 {
      return Err(anyhow!("limit must be between 1 and 50"));
    };

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

  // spt playback --next / --previous
  pub async fn jump(&mut self, d: &JumpDirection) {
    match d {
      JumpDirection::Next => self.net.handle_network_event(IoEvent::NextTrack).await,
      JumpDirection::Previous => self.net.handle_network_event(IoEvent::PreviousTrack).await,
    }
  }

  // spt query -l ...
  pub async fn list(&mut self, item: Type, format: &str) -> String {
    match item {
      Type::Device => {
        if let Some(devices) = &self.net.app.lock().await.devices {
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
        if let Some(playlists) = &self.net.app.lock().await.playlists {
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

  // spt playback --transfer DEVICE
  pub async fn transfer_playback(&mut self, device: &str) -> Result<()> {
    // Get the device id by name
    let mut id = String::new();
    if let Some(devices) = &self.net.app.lock().await.devices {
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
      if app.sonos_owns_playback() {
        let duration = app
          .sonos_now_playing
          .as_ref()
          .and_then(|snapshot| snapshot.duration_ms)
          .ok_or_else(|| anyhow!("Sonos did not report a seekable duration"))?;
        (
          u32::try_from(app.song_progress_ms).unwrap_or(u32::MAX),
          duration,
        )
      } else if let Some(CurrentPlaybackContext {
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

  // spt playback --like / --dislike / --shuffle / --repeat
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
        if s && !self.is_a_saved_track(&id_string).await {
          self
            .net
            .handle_network_event(IoEvent::ToggleSaveTrack(id.uri()))
            .await;
        // Want to dislike but is already disliked -> do nothing
        // Want to dislike and is liked currently -> remove like
        } else if !s && self.is_a_saved_track(&id_string).await {
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

  // spt playback -s
  pub async fn get_status(&mut self, format: String) -> Result<String> {
    // Update info on current playback
    self
      .net
      .handle_network_event(IoEvent::GetCurrentPlayback)
      .await;
    let sonos_status = {
      let app = self.net.app.lock().await;
      if app.sonos_owns_playback() {
        let room_uuid = app.selected_sonos_room_uuid.as_deref();
        app
          .sonos_now_playing
          .as_ref()
          .filter(|snapshot| Some(snapshot.room_uuid.as_str()) == room_uuid)
          .cloned()
          .map(|snapshot| {
            let room_name = app
              .sonos_rooms
              .iter()
              .find(|room| room.uuid == snapshot.room_uuid)
              .map(|room| room.name.clone())
              .unwrap_or_else(|| "Sonos".to_string());
            let volume = app.sonos_volume.or(snapshot.volume_percent).unwrap_or(0);
            (snapshot, room_name, volume)
          })
      } else {
        None
      }
    };
    if let Some((snapshot, room_name, volume)) = sonos_status {
      let title = snapshot.title.ok_or_else(|| anyhow!("no track playing"))?;
      let hs = vec![
        Format::Track(title),
        Format::Artist(snapshot.artist.unwrap_or_default()),
        Format::Album(snapshot.album.unwrap_or_default()),
        Format::Uri(snapshot.track_uri.unwrap_or_default()),
        Format::Position((
          snapshot.position_ms,
          snapshot.duration_ms.unwrap_or_default(),
        )),
        Format::Flags((rspotify::model::RepeatState::Off, false, false)),
        Format::Device(room_name),
        Format::Volume(u32::from(volume)),
        Format::Playing(snapshot.is_playing),
      ];
      return Ok(self.format_output(format, hs));
    }

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
          self.is_a_saved_track(&id).await,
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

  // spt play -u URI
  pub async fn play_uri(&mut self, uri: String, queue: bool, random: bool) {
    let offset = if random {
      // Only works with playlists for now
      if uri.contains("spotify:playlist:") {
        let id_str = uri.split(':').next_back().unwrap();
        if let Ok(playlist_id) = rspotify::model::idtypes::PlaylistId::from_id(id_str) {
          match self
            .net
            .spotify_get_typed::<FullPlaylist>(&format!("playlists/{}", playlist_id.id()), &[])
            .await
          {
            Ok(p) => {
              let num = p.items.total;
              Some(rand::random_range(0..num) as usize)
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

  // spt play -n NAME ...
  pub async fn play(&mut self, name: String, item: Type, queue: bool, random: bool) -> Result<()> {
    self
      .net
      .handle_network_event(IoEvent::GetSearchResults(name.clone(), None))
      .await;
    // Get the uri of the first found
    // item + the offset or return an error message
    let uri = {
      let results = &self.net.app.lock().await.search_results;
      match item {
        Type::Track => {
          if let Some(r) = &results.tracks {
            if let Some(ref id) = r.items[0].id {
              format!("spotify:track:{}", id)
            } else {
              return Err(anyhow!("track has no id"));
            }
          } else {
            return Err(anyhow!("no tracks with name '{}'", name));
          }
        }
        Type::Album => {
          if let Some(r) = &results.albums {
            let album = &r.items[0];
            if let Some(ref id) = album.id {
              format!("spotify:album:{}", id)
            } else {
              return Err(anyhow!("album {} has no id", album.name));
            }
          } else {
            return Err(anyhow!("no albums with name '{}'", name));
          }
        }
        Type::Artist => {
          if let Some(r) = &results.artists {
            if let Some(ref id) = r.items[0].id {
              format!("spotify:artist:{}", id)
            } else {
              return Err(anyhow!("artist has no id"));
            }
          } else {
            return Err(anyhow!("no artists with name '{}'", name));
          }
        }
        Type::Show => {
          if let Some(r) = &results.shows {
            if let Some(ref id) = r.items[0].id {
              format!("spotify:show:{}", id)
            } else {
              return Err(anyhow!("show has no id"));
            }
          } else {
            return Err(anyhow!("no shows with name '{}'", name));
          }
        }
        Type::Playlist => {
          if let Some(r) = &results.playlists {
            let p = &r.items[0];
            if let Some(ref id) = p.id {
              format!("spotify:playlist:{}", id)
            } else {
              return Err(anyhow!("playlist has no id"));
            }
          } else {
            return Err(anyhow!("no playlists with name '{}'", name));
          }
        }
        _ => unreachable!(),
      }
    };

    // Play or queue the uri
    self.play_uri(uri, queue, random).await;

    Ok(())
  }

  // spt query -s SEARCH ...
  pub async fn query(&mut self, search: String, format: String, item: Type) -> String {
    self
      .net
      .handle_network_event(IoEvent::GetSearchResults(search.clone(), None))
      .await;

    let app = self.net.app.lock().await;
    match item {
      Type::Playlist => {
        if let Some(results) = &app.search_results.playlists {
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
        if let Some(results) = &app.search_results.tracks {
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
        if let Some(results) = &app.search_results.artists {
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
        if let Some(results) = &app.search_results.shows {
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
        if let Some(results) = &app.search_results.albums {
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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn spotify_cli_device_selection_clears_sonos_ownership_state() {
    let mut app = App::default();
    app.selected_sonos_room_uuid = Some("RINCON_KITCHEN".to_string());
    app.sonos_is_playing = Some(true);
    app.sonos_volume = Some(42);
    app.sonos_poll_error_room_uuid = Some("RINCON_KITCHEN".to_string());

    let room_uuid = clear_sonos_selection_for_spotify_device(&mut app);

    assert_eq!(room_uuid.as_deref(), Some("RINCON_KITCHEN"));
    assert!(app.selected_sonos_room_uuid.is_none());
    assert!(app.sonos_is_playing.is_none());
    assert!(app.sonos_volume.is_none());
    assert!(app.sonos_poll_error_room_uuid.is_none());
  }
}
