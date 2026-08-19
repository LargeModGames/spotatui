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
  /// Matched on the id AND the block, not on the id alone. The search key
  /// rewrites the error frame in place to `{ id: Error, active_block: Input }`
  /// (it does not push), so an id-only match would delete a frame that is
  /// holding live text-input focus and drop the user's next keystrokes into
  /// the global bindings. Such a frame no longer draws the error screen, so
  /// leaving it is harmless.
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
}
