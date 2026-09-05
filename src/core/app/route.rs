use super::*;

pub(super) const DEFAULT_ROUTE: Route = Route {
  id: RouteId::Home,
  active_block: ActiveBlock::Empty,
  hovered_block: ActiveBlock::Library,
};

#[derive(PartialEq, Debug, Default)]
pub enum SearchResultBlock {
  AlbumSearch,
  SongSearch,
  ArtistSearch,
  PlaylistSearch,
  ShowSearch,
  #[default]
  Empty,
}

#[derive(PartialEq, Debug, Clone)]
pub enum ArtistBlock {
  TopTracks,
  Albums,
  RelatedArtists,
  Empty,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum DialogContext {
  PlaylistWindow,
  PlaylistSearch,
  AddTrackToPlaylistPicker,
  RemoveTrackFromPlaylistConfirm,
  PersistKeybindingFallback,
  /// Confirm deleting a local YouTube playlist (sidebar `D` under the
  /// YouTube source).
  YouTubePlaylistWindow,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ActiveBlock {
  Analysis,
  PlayBar,
  AlbumTracks,
  AlbumList,
  ArtistBlock,
  Empty,
  Error,
  HelpMenu,
  Home,
  Input,
  Library,
  MyPlaylists,
  Podcasts,
  EpisodeTable,
  RecentlyPlayed,
  SearchResultBlock,
  SelectDevice,
  TrackTable,
  Discover,
  Artists,
  LyricsView,
  CoverArtView,
  MiniPlayer,
  Dialog(DialogContext),

  AnnouncementPrompt,
  RecapPrompt,
  CommunityPinPrompt,
  ExitPrompt,
  Settings,
  SortMenu,
  Queue,
  Party,
  CreatePlaylistForm,
  Friends,
  LocalBrowser,
  Stats,
  /// The AI DJ screen's prompt + transcript.
  #[cfg(feature = "ai-dj")]
  AiDj,
  /// A plugin-registered custom screen (the screen name lives in
  /// [`RouteId::PluginScreen`]; `ActiveBlock` is `Copy` and can't carry it).
  /// Only script effects construct it.
  #[cfg_attr(not(feature = "scripting"), allow(dead_code))]
  PluginScreen,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum InputContext {
  #[default]
  GlobalSearch,
  PlaylistTrackSearch,
}

#[derive(Clone, PartialEq, Debug)]
pub enum RouteId {
  Analysis,
  AlbumTracks,
  AlbumList,
  Artist,
  LyricsView,
  CoverArtView,
  MiniPlayer,
  Error,
  Home,
  RecentlyPlayed,
  Search,
  SelectedDevice,
  TrackTable,
  Discover,
  Artists,
  Podcasts,
  PodcastEpisodes,
  Recommendations,
  Dialog,

  AnnouncementPrompt,
  RecapPrompt,
  CommunityPinPrompt,
  ExitPrompt,
  Settings,
  HelpMenu,
  Queue,
  Party,
  CreatePlaylist,
  Friends,
  /// Only reachable when `local-files` is built in (the sidebar row that opens it
  /// is gated on that feature).
  #[cfg_attr(not(feature = "local-files"), allow(dead_code))]
  LocalBrowser,
  Stats,
  #[cfg(feature = "ai-dj")]
  AiDj,
  /// A plugin-registered custom screen, keyed by its registered name.
  /// Only script effects construct it.
  #[cfg_attr(not(feature = "scripting"), allow(dead_code))]
  PluginScreen(String),
}

impl RouteId {
  /// Routes that can be shown at startup with no extra context (no album/artist
  /// id, search query, etc.). These are the only routes `startup_route` may
  /// select.
  pub const STARTUP_OPTIONS: &'static [RouteId] = &[
    RouteId::Home,
    RouteId::RecentlyPlayed,
    RouteId::Podcasts,
    RouteId::Discover,
    RouteId::Artists,
    RouteId::AlbumList,
    RouteId::Stats,
  ];

  /// What a startup route needs before its screen opens populated.
  pub(crate) fn startup_requirement(&self) -> Requirement {
    match self {
      RouteId::Home | RouteId::Stats => Requirement::None,
      _ => Requirement::SpotifySession,
    }
  }

  /// Parse a `startup_route` config token. Unknown / non-context-free strings
  /// return `None` (the caller logs a warning and falls back to Home).
  pub fn from_config_str(s: &str) -> Option<RouteId> {
    match s.trim().to_ascii_lowercase().as_str() {
      "home" => Some(RouteId::Home),
      "recently_played" | "recent" => Some(RouteId::RecentlyPlayed),
      "podcasts" => Some(RouteId::Podcasts),
      "discover" => Some(RouteId::Discover),
      "artists" | "library" => Some(RouteId::Artists),
      "album_list" | "albums" => Some(RouteId::AlbumList),
      "stats" => Some(RouteId::Stats),
      _ => None,
    }
  }

  /// The config-file token for this route (inverse of `from_config_str`).
  pub fn to_config_str(&self) -> &'static str {
    match self {
      RouteId::Home => "home",
      RouteId::RecentlyPlayed => "recently_played",
      RouteId::Podcasts => "podcasts",
      RouteId::Discover => "discover",
      RouteId::Artists => "artists",
      RouteId::AlbumList => "album_list",
      RouteId::Stats => "stats",
      _ => "home",
    }
  }
}

#[derive(Debug)]
pub struct Route {
  pub id: RouteId,
  pub active_block: ActiveBlock,
  pub hovered_block: ActiveBlock,
}

/// Which panel of the combined Source & Device picker (the `d` screen) has
/// keyboard focus.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum SourceFocus {
  Source,
  #[default]
  Devices,
}

impl App {
  /// Open the combined Source & Device picker. The picker opens immediately so
  /// it is reachable offline or when Local is the active source. Initial focus
  /// is the Source panel unless Spotify is active (devices are Spotify Connect
  /// only), and only Spotify needs a `me/player/devices` fetch: an
  /// unauthenticated or offline session must not surface a spurious error.
  pub(crate) fn open_source_device_picker(&mut self) {
    self.view.source_list_index = Source::ALL
      .iter()
      .position(|s| *s == self.active_source)
      .unwrap_or(0);
    self.view.source_device_focus = if self.active_source == Source::Spotify {
      SourceFocus::Devices
    } else {
      SourceFocus::Source
    };
    self.push_navigation_stack(RouteId::SelectedDevice, ActiveBlock::SelectDevice);
    if self.active_source == Source::Spotify {
      self.dispatch(IoEvent::GetDevices);
    }
  }

  /// Switch the active browse source and mirror + persist the choice so the
  /// selection survives restarts. Browse scope only: never interrupts
  /// playback (routing goes by URI scheme). Sidebar data loading and any
  /// view resets stay with the caller.
  pub fn set_active_source(&mut self, source: Source) {
    self.active_source = source;
    self.runtime_state.active_source = source;
    // The Spotify scope reaches disk when the login succeeds (see
    // `persist_active_source`), so a cancelled login never forces a browser at
    // the next boot.
    if source == Source::Spotify && !self.spotify_connected {
      return;
    }
    self.persist_active_source();
  }

  /// Whether the active browse scope and the session can serve a row or key.
  pub(crate) fn availability(&self, requirement: Requirement) -> Availability {
    availability(requirement, self.active_source, self.spotify_connected)
  }

  /// Write the current browse scope to `state.yml`.
  pub(crate) fn persist_active_source(&mut self) {
    if let Err(e) = self.save_runtime_state(
      &crate::core::state::PersistedRuntimeState::active_source(self.runtime_state.active_source),
    ) {
      log::warn!("[source] failed to persist active_source: {e}");
    }
  }

  /// Fetch the sidebar data a browse source needs; Spotify's playlists arrive
  /// with the session.
  pub(crate) fn load_source_sidebar(&mut self, source: Source) {
    let event = match source {
      Source::Local => IoEvent::GetLocalPlaylists,
      Source::Subsonic => IoEvent::GetSubsonicPlaylists,
      Source::Radio => IoEvent::GetRadioStations,
      Source::YouTube => IoEvent::GetYouTubePlaylists,
      Source::Qobuz => IoEvent::GetQobuzPlaylists,
      Source::Spotify => return,
    };
    self.dispatch(event);
  }

  // The navigation_stack actually only controls the large block to the right of `library` and
  // `playlists`
  pub fn push_navigation_stack(&mut self, next_route_id: RouteId, next_active_block: ActiveBlock) {
    info!("navigating to {:?}", next_route_id);
    if !self
      .navigation_stack
      .last()
      .map(|last_route| last_route.id == next_route_id)
      .unwrap_or(false)
    {
      self.navigation_stack.push(Route {
        id: next_route_id,
        active_block: next_active_block,
        hovered_block: next_active_block,
      });
    }
  }

  pub fn pop_navigation_stack(&mut self) -> Option<Route> {
    info!("navigating back");
    if self.navigation_stack.len() == 1 {
      None
    } else {
      let popped = self.navigation_stack.pop();
      // Leaving the error screen dismisses the error. Done here rather than in
      // a key handler so every back path clears it (the escape key, the
      // configurable back key, a script's `Back`), and so a frontend with no
      // back key at all is not the odd one out. Keyed on the frame that was
      // POPPED, not on the new top: pressing `d` from the error page stacks the
      // device picker over it, and coming back must still show the message.
      if popped
        .as_ref()
        .is_some_and(|route| route.id == RouteId::Error)
      {
        self.clear_api_error();
      }
      popped
    }
  }

  /// Remove every frame still serving as the error screen.
  ///
  /// Matched on the id AND the block: a producer that rewrites the current
  /// frame in place could leave a `{ Error, Input }` frame holding text-input
  /// focus, and an id-only match would drop its keystrokes into the global
  /// bindings. No such producer exists today; such a frame no longer draws
  /// the error screen, so leaving it is harmless.
  ///
  /// Both `push_navigation_stack` and the frames below the top are covered:
  /// pushes dedupe only against the top frame, so navigating away and failing
  /// again buries a second error frame that would otherwise resurface later
  /// rendering an unrelated message, or an empty one.
  ///
  /// Cannot empty the stack: the bottom frame is `DEFAULT_ROUTE` (Home) or a
  /// `RouteId::STARTUP_OPTIONS` route, and `Error` is in neither.
  pub(super) fn drop_error_routes(&mut self) {
    self
      .navigation_stack
      .retain(|route| !(route.id == RouteId::Error && route.active_block == ActiveBlock::Error));
  }

  pub fn get_current_route(&self) -> &Route {
    // if for some reason there is no route return the default
    self.navigation_stack.last().unwrap_or(&DEFAULT_ROUTE)
  }

  fn get_current_route_mut(&mut self) -> &mut Route {
    self.navigation_stack.last_mut().unwrap()
  }

  pub fn set_current_route_state(
    &mut self,
    active_block: Option<ActiveBlock>,
    hovered_block: Option<ActiveBlock>,
  ) {
    let current_route = self.get_current_route_mut();
    if let Some(active_block) = active_block {
      current_route.active_block = active_block;
    }
    if let Some(hovered_block) = hovered_block {
      current_route.hovered_block = hovered_block;
    }
  }

  /// Toggle the audio analysis visualization view
  /// This now uses local FFT analysis instead of the deprecated Spotify API
  pub fn get_audio_analysis(&mut self) {
    info!("entering audio analysis view");
    if self.get_current_route().id != RouteId::Analysis {
      // Enter visualization mode
      self.push_navigation_stack(RouteId::Analysis, ActiveBlock::Analysis);
    }
    // Spectrum data will be updated by the audio capture system on each tick
  }

  /// Open the album page of the item playing now, through the ownership
  /// order (episodes open their show). A queued Spotify track resolves through
  /// the slot; any other owner answers with a status message.
  pub(crate) fn jump_to_album(&mut self) {
    // Build the event under the borrow so only the payload is cloned, never
    // the whole playback context.
    let event = match self.playing_item() {
      PlayingItem::Spotify(PlayableItem::Track(track)) => {
        Ok(IoEvent::GetAlbumTracks(Box::new(track.album.clone())))
      }
      PlayingItem::Spotify(PlayableItem::Episode(episode)) => Ok(IoEvent::GetShowEpisodes(
        Box::new(crate::core::plugin_api::ShowInfo::from(&episode.show)),
      )),
      PlayingItem::Spotify(_) => return,
      // A track listed from an album page carries no album id of its own; the
      // track lookup finds the album from the track instead.
      PlayingItem::QueuedSpotify(track) => {
        match (&track.album_id, track.id.as_ref().or(track.uri.as_ref())) {
          (Some(album_id), _) => Ok(IoEvent::GetAlbum(album_id.clone())),
          (None, Some(track_id)) => Ok(IoEvent::GetAlbumForTrack(track_id.clone())),
          (None, None) => Err("The queued track has no album id"),
        }
      }
      PlayingItem::NotSpotify => Err("Jump to album needs a Spotify track playing"),
      PlayingItem::Nothing => Err(NOTHING_PLAYING_STATUS),
    };
    match event {
      Ok(event) => self.dispatch(event),
      Err(status) => self.set_status_message(status, 4),
    }
  }

  /// Open the album list of the first artist of the track playing now,
  /// through the ownership order. Episodes have no artist page.
  pub(crate) fn jump_to_artist_album(&mut self) {
    let artist = match self.playing_item() {
      PlayingItem::Spotify(PlayableItem::Track(track)) => track
        .artists
        .first()
        .and_then(|artist| {
          artist
            .id
            .as_ref()
            .map(|id| (id.id().to_string(), artist.name.clone()))
        })
        .ok_or("The playing track has no artist id"),
      PlayingItem::Spotify(_) => return,
      PlayingItem::QueuedSpotify(track) => track
        .artist_refs
        .first()
        .and_then(|artist| artist.id.clone().map(|id| (id, artist.name.clone())))
        .ok_or("The queued track has no artist id"),
      PlayingItem::NotSpotify => Err("Jump to artist needs a Spotify track playing"),
      PlayingItem::Nothing => Err(NOTHING_PLAYING_STATUS),
    };
    match artist {
      Ok((artist_id, artist_name)) => self.get_artist(artist_id, artist_name),
      Err(status) => self.set_status_message(status, 4),
    }
  }

  /// Open the context (album/artist/playlist) that playback runs in, through
  /// the ownership order. The queue slot plays outside any context, so a
  /// queued track answers with a status message. A context with no item (an
  /// ad plays) still opens.
  pub(crate) fn jump_to_context(&mut self) {
    match self.playing_item() {
      PlayingItem::Spotify(_) | PlayingItem::Nothing => {}
      PlayingItem::QueuedSpotify(_) => {
        self.set_status_message("The queue slot has no play context", 4);
        return;
      }
      PlayingItem::NotSpotify => {
        self.set_status_message("Jump to context needs a Spotify track playing", 4);
        return;
      }
    }
    let context = self
      .current_playback_context
      .as_ref()
      .and_then(|playback| playback.context.as_ref())
      .map(|context| {
        (
          context._type.clone(),
          crate::infra::network::ids::playlist_id(&context.uri),
        )
      });
    let Some((context_type, playlist)) = context else {
      self.set_status_message(NOTHING_PLAYING_STATUS, 4);
      return;
    };
    match context_type {
      rspotify::model::enums::Type::Album => self.jump_to_album(),
      rspotify::model::enums::Type::Artist => self.jump_to_artist_album(),
      rspotify::model::enums::Type::Playlist => {
        if let Some(playlist_id) = playlist {
          self.open_playlist_tracks(playlist_id, TrackTableContext::MyPlaylists);
        }
      }
      _ => {}
    }
  }

  /// Generate a listening recap: the selected period on the Stats screen
  /// when that screen is current, 30 days anywhere else.
  pub(crate) fn generate_recap(&mut self) {
    let period = if self.get_current_route().active_block == ActiveBlock::Stats {
      self.stats_period
    } else {
      RecapPeriod::ThirtyDays
    };
    self.dispatch(IoEvent::GenerateRecap(period));
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::test_support::*;

  /// A Spotify session whose cached context names a track with an artist id.
  #[cfg(feature = "streaming")]
  fn app_with_suspended_context() -> (App, std::sync::mpsc::Receiver<IoEvent>) {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    let mut track = full_track("0000000000000000000001", "Suspended");
    track.artists[0].id = Some(
      rspotify::model::idtypes::ArtistId::from_id("0OdUWJ0sBjDrqHygGUXeCF")
        .unwrap()
        .into_static(),
    );
    app.current_playback_context = Some(playing_track_context(track));
    (app, rx)
  }

  #[cfg(feature = "streaming")]
  fn queue_spotify_track(app: &mut App, track: TrackInfo) {
    app.queue_now = Some(crate::infra::queue::QueueNowPlaying::Spotify { track });
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn jump_to_album_opens_the_queued_tracks_album_not_the_suspended_one() {
    let (mut app, rx) = app_with_suspended_context();
    let mut track = queue_track(Some("spotify:track:queued"), "Queued");
    track.album_id = Some("queuedalbum".to_string());
    queue_spotify_track(&mut app, track);

    app.jump_to_album();

    assert!(matches!(rx.try_recv(), Ok(IoEvent::GetAlbum(id)) if id == "queuedalbum"));
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn jump_to_album_reports_a_queued_track_without_an_album_id() {
    let (mut app, rx) = app_with_suspended_context();
    queue_spotify_track(&mut app, queue_track(None, "Queued"));

    app.jump_to_album();

    assert!(rx.try_recv().is_err());
    assert_eq!(
      app.status_message(),
      Some("The queued track has no album id")
    );
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn jump_to_artist_album_opens_the_queued_tracks_artist() {
    let (mut app, rx) = app_with_suspended_context();
    let mut track = queue_track(Some("spotify:track:queued"), "Queued");
    track.artist_refs = vec![crate::core::plugin_api::ArtistRef {
      id: Some("queuedartist".to_string()),
      name: "Queued Artist".to_string(),
    }];
    queue_spotify_track(&mut app, track);

    app.jump_to_artist_album();

    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::GetArtist(id, name, _)) if id == "queuedartist" && name == "Queued Artist"
    ));
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn jump_to_context_refuses_the_queue_slot() {
    let (mut app, rx) = app_with_suspended_context();
    queue_spotify_track(
      &mut app,
      queue_track(Some("spotify:track:queued"), "Queued"),
    );

    app.jump_to_context();

    assert!(rx.try_recv().is_err());
    assert_eq!(
      app.status_message(),
      Some("The queue slot has no play context")
    );
  }

  #[test]
  fn jump_to_album_reports_nothing_playing_without_a_session() {
    let (mut app, rx) = session_free_app();

    app.jump_to_album();

    assert!(rx.try_recv().is_err());
    assert_eq!(app.status_message(), Some(NOTHING_PLAYING_STATUS));
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn jump_to_album_looks_the_album_up_from_a_queued_track_without_an_album_id() {
    let (mut app, rx) = app_with_suspended_context();
    let mut track = queue_track(Some("spotify:track:queued"), "Queued");
    track.id = Some("queuedtrack".to_string());
    queue_spotify_track(&mut app, track);

    app.jump_to_album();

    assert!(matches!(rx.try_recv(), Ok(IoEvent::GetAlbumForTrack(id)) if id == "queuedtrack"));
  }

  #[test]
  fn jump_to_context_reports_nothing_playing_without_a_context() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.current_playback_context = Some(make_external_context());

    app.jump_to_context();

    assert!(rx.try_recv().is_err());
    assert_eq!(app.status_message(), Some(NOTHING_PLAYING_STATUS));
  }

  #[test]
  fn jump_to_context_opens_the_context_when_no_item_plays() {
    use rspotify::model::{context::Context, enums::Type};
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    let mut context = make_external_context();
    context.context = Some(Context {
      uri: "spotify:playlist:37i9dQZF1DX4WYpdgoIcn6".to_string(),
      href: String::new(),
      external_urls: std::collections::HashMap::new(),
      _type: Type::Playlist,
    });
    app.current_playback_context = Some(context);

    app.jump_to_context();

    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::GetPlaylistItems(id, _)) if id == "37i9dQZF1DX4WYpdgoIcn6"
    ));
  }
}
