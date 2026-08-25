use super::*;

pub(super) const DEFAULT_ROUTE: Route = Route {
  id: RouteId::Home,
  active_block: ActiveBlock::Empty,
  hovered_block: ActiveBlock::Library,
};

#[derive(PartialEq, Debug)]
pub enum SearchResultBlock {
  AlbumSearch,
  SongSearch,
  ArtistSearch,
  PlaylistSearch,
  ShowSearch,
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

  /// Open the album page of the item that is playing now (episodes open
  /// their show). A no-op without a playback context.
  pub(crate) fn jump_to_album(&mut self) {
    // Build the event under the borrow so only the payload is cloned, never
    // the whole playback context.
    let event = match self
      .current_playback_context
      .as_ref()
      .and_then(|playback| playback.item.as_ref())
    {
      Some(PlayableItem::Track(track)) => {
        Some(IoEvent::GetAlbumTracks(Box::new(track.album.clone())))
      }
      Some(PlayableItem::Episode(episode)) => Some(IoEvent::GetShowEpisodes(Box::new(
        crate::core::plugin_api::ShowInfo::from(&episode.show),
      ))),
      _ => None,
    };
    if let Some(event) = event {
      self.dispatch(event);
    }
  }

  // NOTE: this only finds the first artist of the song and jumps to their albums
  pub(crate) fn jump_to_artist_album(&mut self) {
    let artist = match self
      .current_playback_context
      .as_ref()
      .and_then(|playback| playback.item.as_ref())
    {
      Some(PlayableItem::Track(track)) => track.artists.first().and_then(|artist| {
        artist
          .id
          .as_ref()
          .map(|id| (id.id().to_string(), artist.name.clone()))
      }),
      // Episodes have no artist page to jump to (yet!)
      _ => None,
    };
    if let Some((artist_id, artist_name)) = artist {
      self.get_artist(artist_id, artist_name);
    }
  }

  /// Open the context (album/artist/playlist) that playback runs in. A no-op
  /// without a playback context.
  pub(crate) fn jump_to_context(&mut self) {
    let Some((context_type, playlist)) = self
      .current_playback_context
      .as_ref()
      .and_then(|playback| playback.context.as_ref())
      .map(|context| {
        (
          context._type.clone(),
          crate::infra::network::ids::playlist_id(&context.uri),
        )
      })
    else {
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
