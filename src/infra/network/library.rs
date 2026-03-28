use super::requests::{spotify_api_request_json_for, spotify_get_typed_compat_for};
use super::{IoEvent, Network};
use crate::core::app::{
  ActiveBlock, App, PlaylistFolder, PlaylistFolderItem, PlaylistFolderNode, PlaylistFolderNodeType,
  RouteId,
};
use anyhow::anyhow;
use reqwest::Method;
use rspotify::model::{
  idtypes::{AlbumId, LibraryId, PlaylistId, ShowId, TrackId, UserId},
  page::Page,
  playlist::{PlaylistItem, SimplifiedPlaylist},
  track::SavedTrack,
  PlayableItem,
};
use rspotify::{prelude::*, AuthCodePkceSpotify};
use serde_json::json;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

#[cfg(feature = "streaming")]
use crate::infra::player::StreamingPlayer;

const LIBRARY_CONTAINS_MAX_URIS: usize = 50;

#[cfg(test)]
fn next_saved_tracks_offset(page: &Page<SavedTrack>) -> Option<u32> {
  page.next.as_ref().map(|_| page.offset + page.limit)
}

fn uri_batches(uris: &[String]) -> impl Iterator<Item = &[String]> {
  uris.chunks(LIBRARY_CONTAINS_MAX_URIS)
}

fn populate_liked_song_ids_from_saved_tracks(
  liked_song_ids_set: &mut std::collections::HashSet<String>,
  page: &Page<SavedTrack>,
) {
  for item in &page.items {
    if let Some(track_id) = &item.track.id {
      liked_song_ids_set.insert(track_id.id().to_string());
    }
  }
}

pub async fn prefetch_saved_tracks_page_task(
  spotify: AuthCodePkceSpotify,
  app: Arc<Mutex<App>>,
  limit: u32,
  offset: u32,
  generation: u64,
) {
  let should_fetch = {
    let app = app.lock().await;
    app.saved_tracks_prefetch_generation == generation
      && app
        .library
        .saved_tracks
        .page_index_for_offset(offset)
        .is_none()
  };

  if !should_fetch {
    return;
  }

  let query = vec![("limit", limit.to_string()), ("offset", offset.to_string())];
  let Ok(page) = spotify_get_typed_compat_for::<Page<rspotify::model::SavedTrack>>(
    &spotify,
    "me/tracks",
    &query,
  )
  .await
  else {
    return;
  };

  if page.items.is_empty() {
    return;
  }

  let mut app_guard = app.lock().await;
  if app_guard.saved_tracks_prefetch_generation != generation {
    return;
  }

  populate_liked_song_ids_from_saved_tracks(&mut app_guard.liked_song_ids_set, &page);
  app_guard.library.saved_tracks.upsert_page_by_offset(page);
}

pub async fn prefetch_playlist_tracks_page_task(
  spotify: AuthCodePkceSpotify,
  app: Arc<Mutex<App>>,
  limit: u32,
  playlist_id: PlaylistId<'static>,
  offset: u32,
  generation: u64,
) {
  let should_fetch = {
    let app = app.lock().await;
    app.playlist_tracks_prefetch_generation == generation
      && app.is_playlist_track_table_active_for(&playlist_id)
      && app
        .playlist_track_pages
        .page_index_for_offset(offset)
        .is_none()
  };

  if !should_fetch {
    return;
  }

  let path = format!("playlists/{}/items", playlist_id.id());
  let query = vec![("limit", limit.to_string()), ("offset", offset.to_string())];
  let Ok(page) = spotify_get_typed_compat_for::<Page<PlaylistItem>>(&spotify, &path, &query).await
  else {
    return;
  };

  if page.items.is_empty() {
    return;
  }

  let mut app_guard = app.lock().await;
  if app_guard.playlist_tracks_prefetch_generation != generation
    || !app_guard.is_playlist_track_table_active_for(&playlist_id)
  {
    return;
  }

  app_guard.playlist_track_pages.upsert_page_by_offset(page);
}

pub trait LibraryNetwork {
  async fn get_current_user_playlists(&mut self);
  async fn get_playlist_tracks(&mut self, playlist_id: PlaylistId<'static>, playlist_offset: u32);
  async fn get_current_user_saved_tracks(&mut self, offset: Option<u32>);
  async fn get_current_user_saved_albums(&mut self, offset: Option<u32>);
  async fn current_user_saved_albums_contains(&mut self, album_ids: Vec<AlbumId<'static>>);
  async fn current_user_saved_album_delete(&mut self, album_id: AlbumId<'static>);
  async fn current_user_saved_album_add(&mut self, album_id: AlbumId<'static>);
  async fn current_user_saved_shows_contains(&mut self, show_ids: Vec<ShowId<'static>>);
  async fn current_user_saved_shows_delete(&mut self, show_id: ShowId<'static>);
  async fn current_user_saved_shows_add(&mut self, show_id: ShowId<'static>);
  async fn get_current_user_saved_shows(&mut self, offset: Option<u32>);
  async fn user_follow_playlist(
    &mut self,
    playlist_owner_id: UserId<'static>,
    playlist_id: PlaylistId<'static>,
    is_public: Option<bool>,
  );
  async fn user_unfollow_playlist(
    &mut self,
    user_id: UserId<'static>,
    playlist_id: PlaylistId<'static>,
  );
  async fn add_track_to_playlist(
    &mut self,
    playlist_id: PlaylistId<'static>,
    track_id: TrackId<'static>,
  );
  async fn remove_track_from_playlist_at_position(
    &mut self,
    playlist_id: PlaylistId<'static>,
    track_id: TrackId<'static>,
    position: usize,
  );
  async fn toggle_save_track(&mut self, track_id: rspotify::model::idtypes::PlayableId<'static>);
  async fn current_user_saved_tracks_contains(&mut self, ids: Vec<TrackId<'static>>);
  async fn fetch_all_playlist_tracks_and_sort(&mut self, playlist_id: PlaylistId<'static>);
  async fn create_new_playlist(&mut self, name: String, track_ids: Vec<TrackId<'static>>);
}

// Private helper methods
impl Network {
  async fn library_contains_uris(&self, uris: &[String]) -> anyhow::Result<Vec<bool>> {
    if uris.is_empty() {
      return Ok(Vec::new());
    }

    let mut all_results = Vec::with_capacity(uris.len());
    for batch in uri_batches(uris) {
      let batch_results = spotify_get_typed_compat_for::<Vec<bool>>(
        &self.spotify,
        "me/library/contains",
        &[("uris", batch.join(","))],
      )
      .await?;
      all_results.extend(batch_results);
    }

    Ok(all_results)
  }

  async fn library_save_uris(&self, uris: &[String]) -> anyhow::Result<()> {
    if uris.is_empty() {
      return Ok(());
    }

    let query = vec![("uris", uris.join(","))];
    spotify_api_request_json_for(
      &self.spotify,
      Method::PUT,
      "me/library",
      &query,
      Some(json!({ "uris": uris })),
    )
    .await?;
    Ok(())
  }

  async fn library_remove_uris(&self, uris: &[String]) -> anyhow::Result<()> {
    if uris.is_empty() {
      return Ok(());
    }

    let query = vec![("uris", uris.join(","))];
    spotify_api_request_json_for(
      &self.spotify,
      Method::DELETE,
      "me/library",
      &query,
      Some(json!({ "uris": uris })),
    )
    .await?;
    Ok(())
  }

  pub fn spawn_saved_tracks_prefetch(&self, offset: u32, generation: u64) {
    let spotify = self.spotify.clone();
    let app = self.app.clone();
    let large_search_limit = self.large_search_limit;
    tokio::spawn(async move {
      prefetch_saved_tracks_page_task(spotify, app, large_search_limit, offset, generation).await;
    });
  }

  pub fn spawn_playlist_tracks_prefetch(
    &self,
    playlist_id: PlaylistId<'static>,
    offset: u32,
    generation: u64,
  ) {
    let spotify = self.spotify.clone();
    let app = self.app.clone();
    let large_search_limit = self.large_search_limit;
    tokio::spawn(async move {
      prefetch_playlist_tracks_page_task(
        spotify,
        app,
        large_search_limit,
        playlist_id,
        offset,
        generation,
      )
      .await;
    });
  }
}

impl LibraryNetwork for Network {
  async fn get_current_user_playlists(&mut self) {
    let (preferred_playlist_id, preferred_folder_id, preferred_selected_index) = {
      let app = self.app.lock().await;
      (
        app.get_selected_playlist_id(),
        app.current_playlist_folder_id,
        app.selected_playlist_index,
      )
    };

    let limit = 50u32;
    let mut offset = 0u32;
    let mut all_playlists = Vec::new();
    let mut first_page = None;

    loop {
      // Always use the compat path: parses raw JSON to serde_json::Value first,
      // which silently deduplicates keys (last-wins). This handles the known Spotify
      // API bug where "items" appears twice in the same JSON object.
      let page = match super::requests::spotify_get_typed_compat_for::<Page<SimplifiedPlaylist>>(
        &self.spotify,
        "me/playlists",
        &[
          ("limit", limit.to_string()),
          ("offset", offset.to_string()),
        ],
      )
      .await
      {
        Ok(page) => page,
        Err(e) => {
          self.handle_error(anyhow!(e)).await;
          return;
        }
      };

      if offset == 0 {
        first_page = Some(page.clone());
      }

      if page.items.is_empty() {
        break;
      }

      all_playlists.extend(page.items);

      if page.next.is_none() {
        break;
      }
      offset += limit;
    }

    #[cfg(feature = "streaming")]
    let streaming_player = {
      let app = self.app.lock().await;
      app.streaming_player.clone()
    };
    #[cfg(feature = "streaming")]
    let folder_nodes = fetch_rootlist_folders(streaming_player).await;
    #[cfg(not(feature = "streaming"))]
    let folder_nodes: Option<Vec<PlaylistFolderNode>> = None;

    let folder_items = if let Some(ref nodes) = folder_nodes {
      structurize_playlist_folders(nodes, &all_playlists)
    } else {
      build_flat_playlist_items(&all_playlists)
    };

    let mut app = self.app.lock().await;
    app.playlists = first_page;
    app.all_playlists = all_playlists;
    app._playlist_folder_nodes = folder_nodes;
    app.playlist_folder_items = folder_items;

    reconcile_playlist_selection(
      &mut app,
      preferred_playlist_id.as_deref(),
      preferred_folder_id,
      preferred_selected_index,
    );
  }

  async fn get_playlist_tracks(&mut self, playlist_id: PlaylistId<'static>, playlist_offset: u32) {
    let generation = {
      let app = self.app.lock().await;
      app.playlist_tracks_prefetch_generation
    };

    let path = format!("playlists/{}/items", playlist_id.id());
    match spotify_get_typed_compat_for::<Page<PlaylistItem>>(
      &self.spotify,
      &path,
      &[
        ("limit", self.large_search_limit.to_string()),
        ("offset", playlist_offset.to_string()),
      ],
    )
    .await
    {
      Ok(playlist_tracks) => {
        let mut app = self.app.lock().await;
        if app.playlist_tracks_prefetch_generation != generation
          || !app.is_playlist_track_table_active_for(&playlist_id)
        {
          return;
        }

        let playlist_tracks_index = app
          .playlist_track_pages
          .upsert_page_by_offset(playlist_tracks);
        app.show_playlist_tracks_page_at_index(playlist_tracks_index);

        let next_offset = app.next_missing_playlist_tracks_offset(playlist_tracks_index);
        let generation = app.playlist_tracks_prefetch_generation;
        app.push_navigation_stack(RouteId::TrackTable, ActiveBlock::TrackTable);
        drop(app);

        if let Some(next_offset) = next_offset {
          self.spawn_playlist_tracks_prefetch(playlist_id, next_offset, generation);
        }
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn get_current_user_saved_tracks(&mut self, offset: Option<u32>) {
    let generation = {
      let app = self.app.lock().await;
      app.saved_tracks_prefetch_generation
    };

    let mut query = vec![("limit", self.large_search_limit.to_string())];
    if let Some(offset) = offset {
      query.push(("offset", offset.to_string()));
    }

    match spotify_get_typed_compat_for::<Page<rspotify::model::SavedTrack>>(
      &self.spotify,
      "me/tracks",
      &query,
    )
    .await
    {
      Ok(saved_tracks) => {
        let mut app = self.app.lock().await;
        if app.saved_tracks_prefetch_generation != generation {
          return;
        }

        populate_liked_song_ids_from_saved_tracks(&mut app.liked_song_ids_set, &saved_tracks);
        let saved_tracks_index = app.library.saved_tracks.upsert_page_by_offset(saved_tracks);
        app.show_saved_tracks_page_at_index(saved_tracks_index);

        let next_offset = app.next_missing_saved_tracks_offset(saved_tracks_index);
        let generation = app.saved_tracks_prefetch_generation;
        drop(app);

        if let Some(next_offset) = next_offset {
          self.spawn_saved_tracks_prefetch(next_offset, generation);
        }
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn get_current_user_saved_albums(&mut self, offset: Option<u32>) {
    let mut query = vec![("limit", self.large_search_limit.to_string())];
    if let Some(offset) = offset {
      query.push(("offset", offset.to_string()));
    }

    match spotify_get_typed_compat_for::<Page<rspotify::model::SavedAlbum>>(
      &self.spotify,
      "me/albums",
      &query,
    )
    .await
    {
      Ok(saved_albums) => {
        if !saved_albums.items.is_empty() {
          let mut app = self.app.lock().await;
          app.library.saved_albums.add_pages(saved_albums);
        }
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn current_user_saved_albums_contains(&mut self, album_ids: Vec<AlbumId<'static>>) {
    let uris: Vec<String> = album_ids
      .iter()
      .map(|id| format!("spotify:album:{}", id.id()))
      .collect();

    match self.library_contains_uris(&uris).await {
      Ok(is_saved_vec) => {
        let mut app = self.app.lock().await;
        for (i, id) in album_ids.iter().enumerate() {
          if let Some(is_saved) = is_saved_vec.get(i) {
            if *is_saved {
              app.saved_album_ids_set.insert(id.id().to_string());
            } else if app.saved_album_ids_set.contains(id.id()) {
              app.saved_album_ids_set.remove(id.id());
            }
          };
        }
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn current_user_saved_album_delete(&mut self, album_id: AlbumId<'static>) {
    let uris = vec![format!("spotify:album:{}", album_id.id())];
    match self.library_remove_uris(&uris).await {
      Ok(_) => {
        let mut app = self.app.lock().await;
        app.saved_album_ids_set.remove(album_id.id());
        // Reload saved albums to refresh UI
        // dispatching event would require loop access, but we can't from here easily unless we return IoEvent
        // For now, assume optimistic update is handled or manually remove
      }
      Err(e) => self.handle_error(anyhow!(e)).await,
    }
  }

  async fn current_user_saved_album_add(&mut self, album_id: AlbumId<'static>) {
    let uris = vec![format!("spotify:album:{}", album_id.id())];
    match self.library_save_uris(&uris).await {
      Ok(_) => {
        let mut app = self.app.lock().await;
        app.saved_album_ids_set.insert(album_id.id().to_string());
      }
      Err(e) => self.handle_error(anyhow!(e)).await,
    }
  }

  async fn current_user_saved_shows_contains(&mut self, show_ids: Vec<ShowId<'static>>) {
    let uris: Vec<String> = show_ids
      .iter()
      .map(|id| format!("spotify:show:{}", id.id()))
      .collect();
    match self.library_contains_uris(&uris).await {
      Ok(is_saved_vec) => {
        let mut app = self.app.lock().await;
        for (i, id) in show_ids.iter().enumerate() {
          if let Some(is_saved) = is_saved_vec.get(i) {
            if *is_saved {
              app.saved_show_ids_set.insert(id.id().to_string());
            } else if app.saved_show_ids_set.contains(id.id()) {
              app.saved_show_ids_set.remove(id.id());
            }
          };
        }
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn current_user_saved_shows_delete(&mut self, show_id: ShowId<'static>) {
    let uris = vec![format!("spotify:show:{}", show_id.id())];
    match self.library_remove_uris(&uris).await {
      Ok(_) => {
        let mut app = self.app.lock().await;
        app.saved_show_ids_set.remove(show_id.id());
      }
      Err(e) => self.handle_error(anyhow!(e)).await,
    }
  }

  async fn current_user_saved_shows_add(&mut self, show_id: ShowId<'static>) {
    let uris = vec![format!("spotify:show:{}", show_id.id())];
    match self.library_save_uris(&uris).await {
      Ok(_) => {
        let mut app = self.app.lock().await;
        app.saved_show_ids_set.insert(show_id.id().to_string());
      }
      Err(e) => self.handle_error(anyhow!(e)).await,
    }
  }

  async fn get_current_user_saved_shows(&mut self, offset: Option<u32>) {
    let mut query = vec![("limit", self.large_search_limit.to_string())];
    if let Some(offset) = offset {
      query.push(("offset", offset.to_string()));
    }

    match spotify_get_typed_compat_for::<Page<rspotify::model::show::Show>>(
      &self.spotify,
      "me/shows",
      &query,
    )
    .await
    {
      Ok(saved_shows) => {
        if !saved_shows.items.is_empty() {
          let mut app = self.app.lock().await;
          app.library.saved_shows.add_pages(saved_shows);
        }
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn user_follow_playlist(
    &mut self,
    _playlist_owner_id: UserId<'static>,
    playlist_id: PlaylistId<'static>,
    _is_public: Option<bool>,
  ) {
    match self
      .spotify
      .library_add([LibraryId::Playlist(playlist_id)])
      .await
    {
      Ok(_) => {
        // Optimistic update handled in handler or next refresh
      }
      Err(e) => self.handle_error(anyhow!(e)).await,
    }
  }

  async fn user_unfollow_playlist(
    &mut self,
    _user_id: UserId<'static>,
    playlist_id: PlaylistId<'static>,
  ) {
    match self
      .spotify
      .library_remove([LibraryId::Playlist(playlist_id)])
      .await
    {
      Ok(_) => {
        // Handled
      }
      Err(e) => self.handle_error(anyhow!(e)).await,
    }
  }

  async fn add_track_to_playlist(
    &mut self,
    playlist_id: PlaylistId<'static>,
    track_id: TrackId<'static>,
  ) {
    match self
      .spotify
      .playlist_add_items(playlist_id.clone(), vec![PlayableId::Track(track_id)], None)
      .await
    {
      Ok(_) => {
        let status_message = {
          let mut app = self.app.lock().await;
          let playlist_name = app
            .all_playlists
            .iter()
            .find(|playlist| playlist.id.id() == playlist_id.id())
            .map(|playlist| playlist.name.clone());

          if app.is_current_route_playlist_track_table_for(&playlist_id) {
            let playlist_offset = app.playlist_offset;
            app.dispatch(IoEvent::GetPlaylistItems(
              playlist_id.clone(),
              playlist_offset,
            ));
          }

          playlist_name
            .map(|name| format!("Added to {}", name))
            .unwrap_or_else(|| "Added to playlist".to_string())
        };
        self.show_status_message(status_message, 3).await;
      }
      Err(e) => self.handle_error(anyhow!(e)).await,
    }
  }

  async fn remove_track_from_playlist_at_position(
    &mut self,
    playlist_id: PlaylistId<'static>,
    track_id: TrackId<'static>,
    position: usize,
  ) {
    let body = json!({
        "tracks": [{
            "uri": format!("spotify:track:{}", track_id.id()),
            "positions": [position]
        }]
    });

    match spotify_api_request_json_for(
      &self.spotify,
      Method::DELETE,
      &format!("playlists/{}/tracks", playlist_id.id()),
      &[],
      Some(body),
    )
    .await
    {
      Ok(_) => {
        self
          .show_status_message("Removed from playlist".to_string(), 3)
          .await;
      }
      Err(e) => self.handle_error(anyhow!(e)).await,
    }
  }

  async fn toggle_save_track(&mut self, track_id: rspotify::model::idtypes::PlayableId<'static>) {
    let id_str = match &track_id {
      PlayableId::Track(id) => id.id(),
      PlayableId::Episode(id) => id.id(),
    };
    let uri = match &track_id {
      PlayableId::Track(id) => format!("spotify:track:{}", id.id()),
      PlayableId::Episode(id) => format!("spotify:episode:{}", id.id()),
    };

    let is_liked = {
      let app = self.app.lock().await;
      app.liked_song_ids_set.contains(id_str)
    };

    if is_liked {
      if let Err(e) = self.library_remove_uris(&[uri]).await {
        self.handle_error(anyhow!(e)).await;
      } else {
        let mut app = self.app.lock().await;
        app.liked_song_ids_set.remove(id_str);
      }
    } else if let Err(e) = self.library_save_uris(&[uri]).await {
      self.handle_error(anyhow!(e)).await;
    } else {
      let mut app = self.app.lock().await;
      app.liked_song_ids_set.insert(id_str.to_string());
    }
  }

  async fn current_user_saved_tracks_contains(&mut self, ids: Vec<TrackId<'static>>) {
    let uris: Vec<String> = ids
      .iter()
      .map(|id| format!("spotify:track:{}", id.id()))
      .collect();

    match self.library_contains_uris(&uris).await {
      Ok(is_saved_vec) => {
        let mut app = self.app.lock().await;
        for (i, id) in ids.iter().enumerate() {
          if let Some(is_liked) = is_saved_vec.get(i) {
            if *is_liked {
              app.liked_song_ids_set.insert(id.id().to_string());
            } else if app.liked_song_ids_set.contains(id.id()) {
              app.liked_song_ids_set.remove(id.id());
            }
          };
        }
      }
      Err(e) => {
        let mut app = self.app.lock().await;
        app.status_message = Some(format!("Could not check liked track state: {}", e));
        app.status_message_expires_at = Some(Instant::now() + Duration::from_secs(5));
      }
    }
  }

  async fn fetch_all_playlist_tracks_and_sort(&mut self, playlist_id: PlaylistId<'static>) {
    let mut all_tracks = Vec::new();
    let mut offset = 0u32;
    let limit = 50u32;
    let path = format!("playlists/{}/items", playlist_id.id());

    loop {
      let query = vec![("limit", limit.to_string()), ("offset", offset.to_string())];
      match spotify_get_typed_compat_for::<Page<PlaylistItem>>(&self.spotify, &path, &query).await {
        Ok(page) => {
          if page.items.is_empty() {
            break;
          }

          for item in page.items {
            if let Some(PlayableItem::Track(full_track)) = item.item {
              all_tracks.push(full_track);
            }
          }

          if page.next.is_none() {
            break;
          }
          offset += limit;
        }
        Err(e) => {
          self.handle_error(anyhow!(e)).await;
          return;
        }
      }
    }

    // Apply sort if any
    let mut app = self.app.lock().await;

    use crate::core::sort::Sorter;
    let sorter = Sorter::new(app.playlist_sort);
    sorter.sort_tracks(&mut all_tracks);
    let _ = app.apply_sorted_playlist_tracks_if_current(&playlist_id, all_tracks);
  }

  async fn create_new_playlist(&mut self, name: String, track_ids: Vec<TrackId<'static>>) {
    let user_id = {
      let app = self.app.lock().await;
      app.user.as_ref().and_then(|u| Some(u.id.clone()))
    };

    let user_id = match user_id {
      Some(id) => id,
      None => {
        self
          .show_status_message("Cannot create playlist: not logged in".to_string(), 4)
          .await;
        return;
      }
    };

    // Use raw API call to avoid rspotify deserializing FullPlaylist, which crashes when
    // Spotify returns a duplicate "items" key in the response (known API migration bug).
    let user_id_str = user_id.id().to_string();
    let create_path = format!("users/{}/playlists", user_id_str);
    let create_body = json!({
      "name": name,
      "public": false,
      "collaborative": false,
      "description": "Created with spotatui"
    });
    let playlist_value = match spotify_api_request_json_for(
      &self.spotify,
      Method::POST,
      &create_path,
      &[],
      Some(create_body),
    )
    .await
    {
      Ok(v) => v,
      Err(e) => {
        self.handle_error(e).await;
        return;
      }
    };

    let playlist_id_str = match playlist_value.get("id").and_then(|v| v.as_str()) {
      Some(id) => id.to_string(),
      None => {
        self
          .show_status_message("Playlist created but could not get its ID".to_string(), 4)
          .await;
        return;
      }
    };

    let playlist_id = match PlaylistId::from_id(playlist_id_str) {
      Ok(id) => id.into_static(),
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
        return;
      }
    };

    if !track_ids.is_empty() {
      let items: Vec<rspotify::model::idtypes::PlayableId> = track_ids
        .iter()
        .map(|id| rspotify::model::idtypes::PlayableId::Track(id.clone()))
        .collect();
      if let Err(e) = self
        .spotify
        .playlist_add_items(playlist_id, items, None)
        .await
      {
        self.handle_error(anyhow!(e)).await;
        return;
      }
    }

    // Refresh playlists
    {
      let mut app = self.app.lock().await;
      app.dispatch(IoEvent::GetPlaylists);
    }

    let status = format!("Playlist \"{}\" created!", name);
    self.show_status_message(status, 4).await;
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use chrono::{Duration as ChronoDuration, Utc};
  use rspotify::model::{artist::SimplifiedArtist, track::FullTrack};
  use std::collections::{HashMap, HashSet};

  #[allow(deprecated)]
  fn full_track(id: &str) -> FullTrack {
    FullTrack {
      album: rspotify::model::album::SimplifiedAlbum {
        name: "Album".to_string(),
        ..Default::default()
      },
      artists: vec![SimplifiedArtist {
        name: "Artist".to_string(),
        ..Default::default()
      }],
      available_markets: Vec::new(),
      disc_number: 1,
      duration: ChronoDuration::milliseconds(180_000),
      explicit: false,
      external_ids: HashMap::new(),
      external_urls: HashMap::new(),
      href: None,
      id: Some(TrackId::from_id(id).unwrap().into_static()),
      is_local: false,
      is_playable: Some(true),
      linked_from: None,
      restrictions: None,
      name: format!("Track {id}"),
      popularity: 50,
      preview_url: None,
      track_number: 1,
      r#type: rspotify::model::Type::Track,
    }
  }

  fn saved_track(id: &str) -> SavedTrack {
    SavedTrack {
      added_at: Utc::now(),
      track: full_track(id),
    }
  }

  fn saved_tracks_page(offset: u32, limit: u32, has_next: bool) -> Page<SavedTrack> {
    let ids = match offset {
      0 => vec!["0000000000000000000001", "0000000000000000000002"],
      20 => vec!["0000000000000000000003", "0000000000000000000004"],
      40 => vec!["0000000000000000000005", "0000000000000000000006"],
      _ => vec!["0000000000000000000007", "0000000000000000000008"],
    };

    Page {
      href: "https://example.com/me/tracks".to_string(),
      items: ids.into_iter().map(saved_track).collect(),
      limit,
      next: has_next.then(|| "https://example.com/me/tracks?next".to_string()),
      offset,
      previous: None,
      total: 60,
    }
  }

  #[test]
  fn next_saved_tracks_offset_uses_page_limit() {
    let page = saved_tracks_page(20, 20, true);
    assert_eq!(next_saved_tracks_offset(&page), Some(40));
  }

  #[test]
  fn next_saved_tracks_offset_returns_none_without_next_link() {
    let page = saved_tracks_page(20, 20, false);
    assert_eq!(next_saved_tracks_offset(&page), None);
  }

  #[test]
  fn uri_batches_split_large_contains_requests() {
    let uris = (0..120)
      .map(|index| format!("spotify:track:{index:022}"))
      .collect::<Vec<_>>();
    let batches = uri_batches(&uris)
      .map(|batch| batch.len())
      .collect::<Vec<_>>();

    assert_eq!(batches, vec![50, 50, 20]);
  }

  #[test]
  fn populate_liked_song_ids_from_saved_tracks_uses_raw_track_ids() {
    let page = saved_tracks_page(0, 20, false);
    let mut liked_song_ids_set = HashSet::new();

    populate_liked_song_ids_from_saved_tracks(&mut liked_song_ids_set, &page);

    assert!(liked_song_ids_set.contains("0000000000000000000001"));
    assert!(liked_song_ids_set.contains("0000000000000000000002"));
    assert!(!liked_song_ids_set.contains("spotify:track:0000000000000000000001"));
  }
}

#[cfg(feature = "streaming")]
async fn fetch_rootlist_folders(
  streaming_player: Option<Arc<StreamingPlayer>>,
) -> Option<Vec<PlaylistFolderNode>> {
  let player = streaming_player?;
  let session = player.session();

  let bytes = match session.spclient().get_rootlist(0, Some(100_000)).await {
    Ok(bytes) => bytes,
    Err(_) => return None,
  };

  use protobuf::Message;
  let selected: librespot_protocol::playlist4_external::SelectedListContent =
    Message::parse_from_bytes(&bytes).ok()?;

  let contents = selected.contents.as_ref()?;
  Some(parse_rootlist_items(&contents.items))
}

fn build_flat_playlist_items(
  playlists: &[rspotify::model::playlist::SimplifiedPlaylist],
) -> Vec<PlaylistFolderItem> {
  playlists
    .iter()
    .enumerate()
    .map(|(index, _)| PlaylistFolderItem::Playlist {
      index,
      current_id: 0,
    })
    .collect()
}

fn reconcile_playlist_selection(
  app: &mut App,
  preferred_playlist_id: Option<&str>,
  preferred_folder_id: usize,
  preferred_selected_index: Option<usize>,
) {
  if app.playlist_folder_items.is_empty() {
    app.current_playlist_folder_id = 0;
    app.selected_playlist_index = None;
    return;
  }

  let folder_has_visible = |folder_id: usize, app: &App| {
    app.playlist_folder_items.iter().any(|item| match item {
      PlaylistFolderItem::Folder(folder) => folder.current_id == folder_id,
      PlaylistFolderItem::Playlist { current_id, .. } => *current_id == folder_id,
    })
  };

  app.current_playlist_folder_id = if folder_has_visible(preferred_folder_id, app) {
    preferred_folder_id
  } else {
    0
  };

  if let Some(playlist_id) = preferred_playlist_id {
    let visible_playlist_index = app
      .playlist_folder_items
      .iter()
      .filter(|item| app.is_playlist_item_visible_in_current_folder(item))
      .enumerate()
      .find_map(|(display_idx, item)| match item {
        PlaylistFolderItem::Playlist { index, .. } => app
          .all_playlists
          .get(*index)
          .filter(|playlist| playlist.id.id() == playlist_id)
          .map(|_| display_idx),
        PlaylistFolderItem::Folder(_) => None,
      });

    if let Some(display_idx) = visible_playlist_index {
      app.selected_playlist_index = Some(display_idx);
      return;
    }

    let mut target_folder: Option<usize> = None;
    for item in &app.playlist_folder_items {
      if let PlaylistFolderItem::Playlist { index, current_id } = item {
        if let Some(playlist) = app.all_playlists.get(*index) {
          if playlist.id.id() == playlist_id {
            target_folder = Some(*current_id);
            break;
          }
        }
      }
    }

    if let Some(folder_id) = target_folder {
      app.current_playlist_folder_id = folder_id;
      let display_idx = app
        .playlist_folder_items
        .iter()
        .filter(|item| app.is_playlist_item_visible_in_current_folder(item))
        .enumerate()
        .find_map(|(idx, item)| match item {
          PlaylistFolderItem::Playlist { index, .. } => app
            .all_playlists
            .get(*index)
            .filter(|playlist| playlist.id.id() == playlist_id)
            .map(|_| idx),
          PlaylistFolderItem::Folder(_) => None,
        });
      if let Some(idx) = display_idx {
        app.selected_playlist_index = Some(idx);
        return;
      }
    }
  }

  let visible_count = app.get_playlist_display_count();
  if visible_count == 0 {
    app.current_playlist_folder_id = 0;
    let root_count = app.get_playlist_display_count();
    app.selected_playlist_index = if root_count == 0 {
      None
    } else {
      Some(preferred_selected_index.unwrap_or(0).min(root_count - 1))
    };
    return;
  }

  app.selected_playlist_index = Some(preferred_selected_index.unwrap_or(0).min(visible_count - 1));
}

#[cfg(feature = "streaming")]
fn parse_rootlist_items(
  items: &[librespot_protocol::playlist4_external::Item],
) -> Vec<PlaylistFolderNode> {
  let mut root: Vec<PlaylistFolderNode> = Vec::new();
  let mut stack: Vec<Vec<PlaylistFolderNode>> = Vec::new();
  let mut name_stack: Vec<(String, String)> = Vec::new();

  for item in items {
    let uri = item.uri();

    if let Some(rest) = uri.strip_prefix("spotify:start-group:") {
      let (group_id, name) = match rest.find(':') {
        Some(pos) => (rest[..pos].to_string(), rest[pos + 1..].to_string()),
        None => (rest.to_string(), String::new()),
      };
      name_stack.push((group_id, name));
      stack.push(std::mem::take(&mut root));
      root = Vec::new();
    } else if uri.starts_with("spotify:end-group:") {
      if let Some((group_id, name)) = name_stack.pop() {
        let children = std::mem::take(&mut root);
        root = stack.pop().unwrap_or_default();
        root.push(PlaylistFolderNode {
          name: Some(name),
          node_type: PlaylistFolderNodeType::Folder,
          uri: format!("spotify:folder:{}", group_id),
          children,
        });
      }
    } else {
      root.push(PlaylistFolderNode {
        name: None,
        node_type: PlaylistFolderNodeType::Playlist,
        uri: uri.to_string(),
        children: Vec::new(),
      });
    }
  }

  while let Some((group_id, name)) = name_stack.pop() {
    let children = std::mem::take(&mut root);
    root = stack.pop().unwrap_or_default();
    root.push(PlaylistFolderNode {
      name: Some(name),
      node_type: PlaylistFolderNodeType::Folder,
      uri: format!("spotify:folder:{}", group_id),
      children,
    });
  }

  root
}

fn structurize_playlist_folders(
  nodes: &[PlaylistFolderNode],
  playlists: &[rspotify::model::playlist::SimplifiedPlaylist],
) -> Vec<PlaylistFolderItem> {
  use std::collections::{HashMap, HashSet};

  let playlist_map: HashMap<String, usize> = playlists
    .iter()
    .enumerate()
    .map(|(idx, playlist)| (playlist.id.id().to_string(), idx))
    .collect();

  let mut items: Vec<PlaylistFolderItem> = Vec::new();
  let mut next_folder_id: usize = 1;
  let mut used_playlist_indices: HashSet<usize> = HashSet::new();

  fn walk(
    nodes: &[PlaylistFolderNode],
    current_folder_id: usize,
    items: &mut Vec<PlaylistFolderItem>,
    next_folder_id: &mut usize,
    playlist_map: &std::collections::HashMap<String, usize>,
    used_playlist_indices: &mut std::collections::HashSet<usize>,
  ) {
    for node in nodes {
      match node.node_type {
        PlaylistFolderNodeType::Folder => {
          let folder_id = *next_folder_id;
          *next_folder_id += 1;

          let name = node.name.as_deref().unwrap_or("Unnamed Folder").to_string();

          items.push(PlaylistFolderItem::Folder(PlaylistFolder {
            name: name.clone(),
            current_id: current_folder_id,
            target_id: folder_id,
          }));

          items.push(PlaylistFolderItem::Folder(PlaylistFolder {
            name: format!("\u{2190} {}", name),
            current_id: folder_id,
            target_id: current_folder_id,
          }));

          walk(
            &node.children,
            folder_id,
            items,
            next_folder_id,
            playlist_map,
            used_playlist_indices,
          );
        }
        PlaylistFolderNodeType::Playlist => {
          let playlist_id = node
            .uri
            .strip_prefix("spotify:playlist:")
            .unwrap_or(&node.uri);

          if let Some(&index) = playlist_map.get(playlist_id) {
            items.push(PlaylistFolderItem::Playlist {
              index,
              current_id: current_folder_id,
            });
            used_playlist_indices.insert(index);
          }
        }
      }
    }
  }

  walk(
    nodes,
    0,
    &mut items,
    &mut next_folder_id,
    &playlist_map,
    &mut used_playlist_indices,
  );

  for (index, _) in playlists.iter().enumerate() {
    if !used_playlist_indices.contains(&index) {
      items.push(PlaylistFolderItem::Playlist {
        index,
        current_id: 0,
      });
    }
  }

  items
}
