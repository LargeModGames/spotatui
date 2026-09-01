use super::*;

impl Default for App {
  fn default() -> Self {
    App {
      spectrum_data: None,
      audio_capture_active: false,
      view: ViewState::default(),
      album_table_context: AlbumTableContext::Full,
      discover_top_tracks: vec![],
      discover_artists_mix: vec![],
      discover_loading: false,
      stats_period: RecapPeriod::ThirtyDays,
      stats_loading: false,
      stats_data: None,
      listening_streaks: None,
      recap_prompt: None,
      community_pin_item: PlaylistFolderItem::CommunityPin,
      local_playlists: Vec::new(),
      subsonic_playlists: Vec::new(),
      qobuz_playlists: Vec::new(),
      radio_stations: Vec::new(),
      youtube_playlists: Vec::new(),
      youtube_open_playlist: None,
      active_source: Source::default(),
      artists: vec![],
      artist: None,
      user_config: UserConfig::new(),
      runtime_state: RuntimeState::default(),
      state_path: None,
      recently_played: Default::default(),
      selected_album_simplified: None,
      selected_album_full: None,
      library: Library {
        saved_tracks: ScrollableResultPages::new(),
        saved_albums: ScrollableResultPages::new(),
        saved_shows: ScrollableResultPages::new(),
        saved_artists: ScrollableResultPages::new(),
        show_episodes: ScrollableResultPages::new(),
        selected_index: 0,
      },
      liked_song_ids_set: HashSet::new(),
      liked_lookup_pending: HashSet::new(),
      liked_lookup_worker_running: false,
      liked_state_epoch: 0,
      followed_artist_ids_set: HashSet::new(),
      saved_album_ids_set: HashSet::new(),
      saved_show_ids_set: HashSet::new(),
      navigation_stack: vec![DEFAULT_ROUTE],
      small_search_limit: 4,
      api_error: String::new(),
      api_error_expires_at: None,
      current_playback_context: None,
      last_track_id: None,
      pending_stop_after_track: false,
      devices: None,
      queue: None,
      native_queue: Vec::new(),
      queue_suspended: None,
      #[cfg(any(
        feature = "streaming",
        feature = "local-files",
        feature = "subsonic",
        feature = "qobuz",
        feature = "youtube"
      ))]
      queue_now: None,
      #[cfg(feature = "streaming")]
      spotify_queue_guard_reloads: 0,
      #[cfg(any(
        feature = "streaming",
        feature = "local-files",
        feature = "subsonic",
        feature = "qobuz",
        feature = "youtube"
      ))]
      queue_slot_desired_playing: true,
      playlist_offset: 0,
      playlist_tracks: None,
      playlist_track_pages: ScrollableResultPages::new(),
      playlist_track_table_id: None,
      active_playlist_track_filter: None,
      pending_playlist_track_search: None,
      playlists: None,
      recommendations_context: None,
      recommendations_seed: "".to_string(),
      search_results: SearchResult {
        hovered_block: SearchResultBlock::SongSearch,
        selected_block: SearchResultBlock::Empty,
        albums: None,
        artists: None,
        playlists: None,
        shows: None,
        selected_album_index: None,
        selected_artists_index: None,
        selected_playlists_index: None,
        selected_tracks_index: None,
        selected_shows_index: None,
        tracks: None,
      },
      song_progress_ms: 0,
      last_tick_at: Instant::now(),
      seek_ms: None,
      #[cfg(feature = "streaming")]
      last_native_seek: None,
      #[cfg(feature = "streaming")]
      pending_native_seek: None,
      last_api_seek: None,
      pending_api_seek: None,
      last_source_seek: None,
      pending_source_seek: None,
      track_table: Default::default(),
      episode_table_context: EpisodeTableContext::Full,
      selected_show_simplified: None,
      selected_show_full: None,
      user: None,
      instant_since_last_current_playback_poll: Instant::now(),
      clipboard: Clipboard::new().ok(),
      is_loading: false,
      io_tx: None,
      is_fetching_current_playback: false,
      spotify_token_expiry: None,
      spotify_connected: false,
      auth_refresh_in_progress: false,
      pending_keybinding_persist: None,
      keybinding_runtime: KeybindingRuntimeState::default(),

      active_announcement: None,
      pending_announcements: Vec::new(),
      lyrics: None,
      lyrics_status: LyricsStatus::default(),
      desired_lyrics_identity: None,
      lyrics_synced: false,
      global_song_count: None,
      global_song_count_failed: false,
      // Settings defaults
      settings_items: Vec::new(),
      settings_saved_items: Vec::new(),
      native_track_info: None,
      is_streaming_active: false,
      native_device_id: None,
      pending_play_file: None,
      native_is_playing: None,
      native_playback_origin: None,
      #[cfg(feature = "streaming")]
      native_spotify_shuffle: None,
      #[cfg(feature = "streaming")]
      native_shuffle_generation: 0,
      keepawake: None,
      last_device_activation: None,
      native_activation_pending: false,
      // Sort menu defaults
      playlist_sort: SortState::new(),
      album_sort: SortState::new(),
      artist_sort: SortState::new(),
      recently_played_sort: SortState::new(),
      last_party_sync_at: Instant::now(),
      status_message: None,
      status_message_expires_at: None,
      status_message_is_error: false,
      party_status: PartyStatus::default(),
      party_session: None,
      pending_track_table_selection: None,
      playlist_track_positions: None,
      pending_playlist_track_add: None,
      pending_playlist_track_removal: None,
      all_playlists: Vec::new(),
      _playlist_folder_nodes: None,
      playlist_folder_items: Vec::new(),
      current_playlist_folder_id: 0,
      playlist_refresh_generation: 0,
      saved_tracks_prefetch_generation: 0,
      saved_tracks_prefetch_in_flight: HashSet::new(),
      playlist_tracks_prefetch_generation: 0,
      pending_playlist_open: None,
      playlist_tracks_prefetch_in_flight: HashSet::new(),
      playlist_sort_fetch_in_flight: HashSet::new(),
      is_volume_change_in_flight: false,
      state_save_due: None,
      pending_state_save_patch: PersistedRuntimeState::default(),
      state_save_error_reported: false,
      pending_volume: None,
      last_dispatched_volume: None,
      #[cfg(feature = "streaming")]
      streaming_player: None,
      decoded_repeat: RepeatMode::Off,
      decoded_shuffle: false,
      #[cfg(feature = "local-files")]
      local_playback: None,
      #[cfg(feature = "subsonic")]
      subsonic_playback: None,
      #[cfg(feature = "qobuz")]
      qobuz_playback: None,
      #[cfg(feature = "internet-radio")]
      radio_playback: None,
      #[cfg(feature = "youtube")]
      youtube_playback: None,
      #[cfg(feature = "streaming")]
      streaming_recovery_tx: None,
      #[cfg(feature = "streaming")]
      pending_start_playback: None,
      #[cfg(feature = "streaming")]
      native_backend_pending: false,
      #[cfg(feature = "streaming")]
      native_load_watchdog: None,
      #[cfg(feature = "streaming")]
      native_playback_recovery: None,
      #[cfg(feature = "streaming")]
      native_restore_pending: None,
      #[cfg(feature = "streaming")]
      native_playback_generation: 0,
      #[cfg(all(feature = "mpris", target_os = "linux"))]
      mpris_manager: None,
      #[cfg(feature = "art-decode")]
      cover_art: crate::core::art::CoverArtStore::default(),
      #[cfg(feature = "art-decode")]
      desired_cover_art_key: None,
      #[cfg(feature = "art-decode")]
      cover_art_palette: None,
      #[cfg(feature = "art-decode")]
      cover_theme_state: crate::core::cover_theme::CoverThemeState::default(),
      #[cfg(feature = "art-decode")]
      theme_transition: None,
      #[cfg(feature = "dj-core")]
      dj: crate::infra::dj::DjState::default(),
      friends: Vec::new(),
      friends_loading: false,
      friend_code: None,
      friend_user_search_results: Vec::new(),
      last_friends_refresh_at: Instant::now(),
      create_playlist_tracks: Vec::new(),
      create_playlist_search_results: Vec::new(),
      pending_plugin_commands: Vec::new(),
      plugin_data_generations: PluginDataGenerations::default(),
      plugin_screens: std::collections::BTreeMap::new(),
      pending_plugin_screen_keys: Vec::new(),
      plugin_playbar_segments: std::collections::BTreeMap::new(),
      plugin_popup: None,
      log_path: crate::core::paths::app_log_path().display().to_string(),
    }
  }
}

impl App {
  /// `App::default()` with a Spotify session, for tests that need one without
  /// an `IoEvent` channel.
  #[cfg(all(test, feature = "tui"))]
  pub(crate) fn default_connected() -> App {
    App {
      spotify_connected: true,
      ..App::default()
    }
  }

  #[cfg(test)]
  pub fn new(
    io_tx: Sender<IoEvent>,
    user_config: UserConfig,
    spotify_token_expiry: Option<SystemTime>,
  ) -> App {
    Self::new_with_state(
      io_tx,
      user_config,
      RuntimeState::default(),
      None,
      spotify_token_expiry,
    )
  }

  pub fn new_with_state(
    io_tx: Sender<IoEvent>,
    user_config: UserConfig,
    runtime_state: RuntimeState,
    state_path: Option<PathBuf>,
    spotify_token_expiry: Option<SystemTime>,
  ) -> App {
    // Read the persisted active source before moving runtime_state into the struct,
    // so the restored value overrides the Source::default() set by App::default().
    let active_source = runtime_state.active_source;
    // Same reason: read before the move. The config only seeds the DJ's filter;
    // the toggle owns it from then on.
    #[cfg(feature = "ai-dj")]
    let dj_avoid_library = user_config.behavior.dj_avoid_library;
    // Resolve configurable per-context default sort states. Config validation
    // already rejected invalid specs at load time, so parse failure here is a
    // defensive fallback to the built-in default sort.
    let parse_sort = |spec: &str, ctx: SortContext| -> SortState {
      SortState::parse(spec, ctx).unwrap_or_default()
    };
    let playlist_sort = parse_sort(
      &user_config.behavior.default_sort_playlist_tracks,
      SortContext::PlaylistTracks,
    );
    let album_sort = parse_sort(
      &user_config.behavior.default_sort_saved_albums,
      SortContext::SavedAlbums,
    );
    let artist_sort = parse_sort(
      &user_config.behavior.default_sort_saved_artists,
      SortContext::SavedArtists,
    );
    let recently_played_sort = parse_sort(
      &user_config.behavior.default_sort_recently_played,
      SortContext::RecentlyPlayed,
    );
    // Resolve the configurable startup route. Unknown / non-context-free values
    // degrade to Home + warn (precedent: StartupBehavior::from_name).
    let startup_route_id = match RouteId::from_config_str(&user_config.behavior.startup_route) {
      Some(id) => id,
      None => {
        log::warn!(
          "[config] startup_route '{}' is not a valid context-free route (valid: {}); using Home",
          user_config.behavior.startup_route,
          RouteId::STARTUP_OPTIONS
            .iter()
            .map(|r| r.to_config_str())
            .collect::<Vec<_>>()
            .join(", ")
        );
        RouteId::Home
      }
    };
    let startup_route = Route {
      id: startup_route_id,
      active_block: ActiveBlock::Empty,
      hovered_block: ActiveBlock::Library,
    };
    App {
      io_tx: Some(io_tx),
      user_config,
      runtime_state,
      state_path,
      // A token expiry means a Spotify session loaded at startup; a free-source
      // launch with no cached token passes `None`. In-TUI login flips both fields.
      spotify_connected: spotify_token_expiry.is_some(),
      spotify_token_expiry,
      active_source,
      navigation_stack: vec![startup_route],
      playlist_sort,
      album_sort,
      artist_sort,
      recently_played_sort,
      #[cfg(feature = "ai-dj")]
      dj: crate::infra::dj::DjState {
        avoid_library: dj_avoid_library,
        ..crate::infra::dj::DjState::default()
      },
      ..App::default()
    }
  }
}
