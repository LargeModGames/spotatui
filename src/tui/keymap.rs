//! The terminal's key surface: the help rows, each with the [`Requirement`]
//! it needs and filtered by the active source and the session, and the
//! registry naming the key or gesture that produces each [`Action`].

#![deny(
  clippy::wildcard_enum_match_arm,
  clippy::match_wildcard_for_single_variants
)]

use crate::core::action::{Action, CopyTarget, LibraryTarget, NavTarget};
use crate::core::app::App;
use crate::core::input::Key;
use crate::core::requirement::{Capability, Requirement};
use crate::core::sort::SortContext;
use crate::core::source::Source;
use crate::core::user_config::KeyBindings;

/// How a help row names its key.
pub enum HelpKey {
  /// A rebindable key from `config.yml`; an unmet row stays listed, marked.
  Binding(fn(&KeyBindings) -> Key),
  /// A key the screen hard-codes; an unmet row is hidden.
  Literal(&'static str),
  /// A rebindable key expression built from the running app; treated like
  /// `Binding`.
  Custom(fn(&App) -> String),
}

pub struct HelpEntry {
  pub description: &'static str,
  pub key: HelpKey,
  pub context: &'static str,
  pub needs: Requirement,
}

fn row(description: &'static str, key: HelpKey, context: &'static str) -> HelpEntry {
  HelpEntry {
    description,
    key,
    context,
    needs: Requirement::None,
  }
}

impl HelpEntry {
  fn needs(mut self, needs: Requirement) -> Self {
    self.needs = needs;
    self
  }
}

const SPOTIFY: Requirement = Requirement::Source(Source::Spotify);
const SESSION: Requirement = Requirement::SpotifySession;
const RADIO: Requirement = Requirement::Source(Source::Radio);
const LIKE: Requirement = Requirement::Capability(Capability::Like);
const PLAYLIST_WRITE: Requirement = Requirement::Capability(Capability::PlaylistWrite);
const SEARCH: Requirement = Requirement::Capability(Capability::Search);
const PLAYLIST_SYNC: Requirement = Requirement::Capability(Capability::PlaylistSync);

/// Every help row using the selected language, before the availability filter.
pub fn help_entries_for(strings: &crate::core::i18n::Strings) -> Vec<HelpEntry> {
  let help = &strings.help;
  use HelpKey::{Binding, Custom, Literal};
  vec![
    row(
      help.next_page,
      Binding(|k| k.next_page),
      help.context_pagination,
    ),
    row(
      help.previous_page,
      Binding(|k| k.previous_page),
      help.context_pagination,
    ),
    row(
      help.playlist_start,
      Binding(|k| k.jump_to_start),
      help.context_pagination,
    ),
    row(
      help.playlist_end,
      Binding(|k| k.jump_to_end),
      help.context_pagination,
    ),
    row(
      help.current_album,
      Binding(|k| k.jump_to_album),
      help.context_general,
    )
    .needs(SESSION),
    row(
      help.current_artist_albums,
      Binding(|k| k.jump_to_artist_album),
      help.context_general,
    )
    .needs(SESSION),
    row(
      help.current_context,
      Binding(|k| k.jump_to_context),
      help.context_general,
    )
    .needs(SESSION),
    row(
      help.volume_up,
      Binding(|k| k.increase_volume),
      help.context_general,
    ),
    row(
      help.volume_down,
      Binding(|k| k.decrease_volume),
      help.context_general,
    ),
    row(
      help.next_track,
      Binding(|k| k.next_track),
      help.context_general,
    ),
    row(
      help.previous_track,
      Binding(|k| k.previous_track),
      help.context_general,
    ),
    row(
      help.force_previous_track,
      Binding(|k| k.force_previous_track),
      help.context_general,
    ),
    row(
      help.seek_backward,
      Binding(|k| k.seek_backwards),
      help.context_general,
    ),
    row(
      help.seek_forward,
      Binding(|k| k.seek_forwards),
      help.context_general,
    ),
    row(
      help.toggle_shuffle,
      Binding(|k| k.shuffle),
      help.context_general,
    ),
    row(
      help.copy_song_url,
      Binding(|k| k.copy_song_url),
      help.context_general,
    )
    .needs(SESSION),
    row(
      help.copy_album_url,
      Binding(|k| k.copy_album_url),
      help.context_general,
    )
    .needs(SESSION),
    row(
      help.cycle_repeat,
      Binding(|k| k.repeat),
      help.context_general,
    ),
    row(
      help.move_left,
      Custom(|app| {
        format!(
          "{} | <{}> | <Ctrl+b>",
          app.user_config.keys.move_left,
          app.strings().help.arrow_key("Left")
        )
      }),
      help.context_general,
    ),
    row(
      help.move_down,
      Custom(|app| {
        format!(
          "{} | <{}> | <Ctrl+n>",
          app.user_config.keys.move_down,
          app.strings().help.arrow_key("Down")
        )
      }),
      help.context_general,
    ),
    row(
      help.move_up,
      Custom(|app| {
        format!(
          "{} | <{}>",
          app.user_config.keys.move_up,
          app.strings().help.arrow_key("Up")
        )
      }),
      help.context_general,
    ),
    row(
      help.move_right,
      Custom(|app| {
        format!(
          "{} | <{}> | <Ctrl+f>",
          app.user_config.keys.move_right,
          app.strings().help.arrow_key("Right")
        )
      }),
      help.context_general_playlist_search,
    ),
    row(help.list_top, Literal("H"), help.context_general),
    row(help.list_middle, Literal("M"), help.context_general),
    row(help.list_bottom, Literal("L"), help.context_general),
    row(help.search, Binding(|k| k.search), help.context_general).needs(SEARCH),
    row(help.filter_help, Binding(|k| k.search), help.context_help),
    row(
      help.filter_settings,
      Binding(|k| k.search),
      help.context_settings,
    ),
    row(
      help.toggle_playback,
      Binding(|k| k.toggle_playback),
      help.context_general,
    ),
    row(help.active_mode, Literal("<Enter>"), help.context_general),
    row(
      help.audio_analysis,
      Binding(|k| k.audio_analysis),
      help.context_general,
    ),
    row(help.visualizer_style, Literal("V"), help.context_general),
    row(
      help.lyrics_view,
      Binding(|k| k.lyrics_view),
      help.context_general,
    ),
    row(
      help.lyrics_scroll,
      Custom(|app| {
        format!(
          "{}/{} | <Up>/<Down> | <Ctrl+n>",
          app.user_config.keys.move_up, app.user_config.keys.move_down
        )
      }),
      help.context_lyrics,
    ),
    row(
      help.lyrics_follow,
      Custom(|app| app.strings().help.key_alternative("f", "<Esc>")),
      help.context_lyrics,
    ),
    row(
      help.lyrics_timing,
      Custom(|app| {
        format!(
          "{}/{} | <Right>/<Left> | <Ctrl+f>/<Ctrl+b>",
          app.user_config.keys.move_right, app.user_config.keys.move_left
        )
      }),
      help.context_lyrics,
    ),
    row(
      help.miniplayer,
      Binding(|k| k.miniplayer_view),
      help.context_general,
    ),
    #[cfg(feature = "cover-art")]
    row(
      help.cover_art,
      Binding(|k| k.cover_art_view),
      help.context_general,
    ),
    row(help.back, Binding(|k| k.back), help.context_general),
    row(
      help.source_device,
      Binding(|k| k.manage_devices),
      help.context_general,
    ),
    row(
      help.settings_open,
      Custom(|app| app.effective_open_settings_key().to_string()),
      help.context_general,
    ),
    row(
      help.settings_save,
      Custom(|app| app.effective_save_settings_key().to_string()),
      help.context_settings,
    ),
    row(
      help.hover_mode,
      Literal("<Esc>"),
      help.context_selected_block,
    ),
    row(help.save_track, Literal("s"), help.context_selected_block).needs(LIKE),
    row(
      help.add_selected_to_playlist,
      Literal("w"),
      help.context_add_track,
    )
    .needs(PLAYLIST_WRITE),
    row(
      help.add_playing_to_playlist,
      Literal("w"),
      help.context_playbar,
    )
    .needs(SESSION),
    row(help.quick_add_playing, Literal("W"), help.context_global).needs(SESSION),
    row(help.sidebar_narrower, Literal("{"), help.context_layout),
    row(help.sidebar_wider, Literal("}"), help.context_layout),
    row(help.panels_shorter, Literal("("), help.context_layout),
    row(help.panels_taller, Literal(")"), help.context_layout),
    row(help.layout_reset, Literal("|"), help.context_layout),
    row(
      help.remove_playlist_track,
      Literal("x"),
      help.context_playlist_tracks,
    )
    .needs(PLAYLIST_WRITE),
    row(
      help.search_playlist,
      Literal("<Ctrl+f>"),
      help.context_playlist_tracks,
    )
    .needs(SPOTIFY),
    row(
      help.clear_playlist_filter,
      Binding(|k| k.back),
      help.context_filtered_playlist_tracks,
    )
    .needs(SPOTIFY),
    row(
      help.play_selection,
      Binding(|k| k.submit),
      help.context_selected_block,
    ),
    row(
      help.recommendations,
      Literal("r"),
      help.context_selected_block,
    )
    .needs(SPOTIFY),
    row(
      help.artist_tracks,
      Literal("e"),
      help.context_library_artists,
    )
    .needs(SPOTIFY),
    row(
      help.search_submit,
      Literal("<Enter>"),
      help.context_search_input,
    )
    .needs(SEARCH),
    row(
      help.cursor_left,
      Custom(|app| format!("<{}>", app.strings().help.arrow_key("Left"))),
      help.context_search_input,
    ),
    row(
      help.cursor_right,
      Custom(|app| format!("<{}>", app.strings().help.arrow_key("Right"))),
      help.context_search_input,
    ),
    row(
      help.input_clear,
      Literal("<Ctrl+l>"),
      help.context_search_input,
    ),
    row(
      help.input_delete_before,
      Literal("<Ctrl+u>"),
      help.context_search_input,
    ),
    row(
      help.input_delete_after,
      Literal("<Ctrl+k>"),
      help.context_search_input,
    ),
    row(
      help.input_delete_word,
      Literal("<Ctrl+w>"),
      help.context_search_input,
    ),
    row(
      help.input_start,
      Literal("<Ctrl+a>"),
      help.context_search_input,
    ),
    row(
      help.input_end,
      Literal("<Ctrl+e>"),
      help.context_search_input,
    ),
    row(
      help.input_escape,
      Literal("<Esc>"),
      help.context_search_input,
    ),
    row(help.album_delete, Literal("D"), help.context_library_albums).needs(SPOTIFY),
    row(
      help.artist_unfollow,
      Literal("D"),
      help.context_artist_unfollow,
    )
    .needs(SPOTIFY),
    row(
      help.show_remove,
      Literal("D"),
      help.context_library_podcasts,
    )
    .needs(SPOTIFY),
    row(help.playlist_delete, Literal("D"), help.context_playlist).needs(PLAYLIST_WRITE),
    row(help.playlist_mirror, Literal("m"), help.context_playlist).needs(PLAYLIST_SYNC),
    row(help.radio_remove, Literal("D"), help.context_radio).needs(RADIO),
    row(
      help.follow_artist_playlist,
      Literal("w"),
      help.context_search_results,
    )
    .needs(SPOTIFY),
    row(help.album_save, Literal("w"), help.context_save_album).needs(SPOTIFY),
    row(
      help.playlist_random,
      Literal("S"),
      help.context_selected_playlist,
    ),
    row(help.episode_sort, Literal("S"), help.context_selected_show).needs(SPOTIFY),
    row(help.show_follow, Literal("s"), help.context_selected_show).needs(SPOTIFY),
    row(help.show_unfollow, Literal("D"), help.context_selected_show).needs(SPOTIFY),
    row(
      help.queue_add,
      Binding(|k| k.add_item_to_queue),
      help.context_hovered_track,
    ),
    row(
      help.queue_show,
      Binding(|k| k.show_queue),
      help.context_general,
    ),
    row(
      help.queue_remove,
      Binding(|k| k.remove_from_queue),
      help.context_queue,
    ),
    row(help.queue_move, Literal("J / K"), help.context_queue),
    row(help.queue_play, Literal("<Enter>"), help.context_queue),
    row(
      help.toggle_saved,
      Binding(|k| k.like_track),
      help.context_general,
    )
    .needs(LIKE),
    row(
      help.radio_favorite,
      Binding(|k| k.like_track),
      help.context_radio,
    )
    .needs(RADIO),
    row(
      help.listening_recap,
      Binding(|k| k.generate_recap),
      help.context_general,
    ),
    row(
      help.stats_open,
      Literal(help.library_sidebar),
      help.context_stats,
    ),
    row(help.stats_period, Literal("[ / ]"), help.context_stats),
    row(help.stats_play, Literal("<Enter>"), help.context_stats).needs(SESSION),
    row(
      help.playlist_sync_open,
      Literal(help.library_sidebar),
      help.context_playlist_sync,
    ),
    row(
      help.playlist_sync_run,
      Literal("s"),
      help.context_playlist_sync,
    ),
    row(
      help.playlist_sync_remove,
      Literal("D"),
      help.context_playlist_sync,
    ),
    row(help.sort_open, Literal(","), help.context_sortable_lists),
    row(
      help.party_open,
      Binding(|k| k.listening_party),
      help.context_general,
    )
    .needs(SESSION),
    #[cfg(feature = "ai-dj")]
    row(help.dj_open, Binding(|k| k.dj_open), help.context_ai_dj),
    #[cfg(feature = "ai-dj")]
    row(
      help.dj_auto_queue,
      Binding(|k| k.dj_toggle_auto_queue),
      help.context_ai_dj,
    ),
    #[cfg(feature = "ai-dj")]
    row(
      help.dj_vibe,
      Binding(|k| k.dj_vibe_shift),
      help.context_ai_dj,
    ),
    #[cfg(feature = "ai-dj")]
    row(
      help.dj_fresh,
      Binding(|k| k.dj_toggle_fresh_only),
      help.context_ai_dj,
    ),
    #[cfg(feature = "ai-dj")]
    row(
      help.dj_model,
      Binding(|k| k.dj_pick_model),
      help.context_ai_dj,
    ),
    // Last, so the pinned English table keeps its indices.
    row(help.settings_button, Literal("o"), help.context_settings),
  ]
}

/// The help rows the app can serve now, as `[description, key, context]`;
/// an unmet rebindable row stays, marked with the reason.
pub fn help_rows(app: &App) -> Vec<Vec<String>> {
  let keys = &app.user_config.keys;
  help_entries_for(app.strings())
    .into_iter()
    .filter_map(|entry| {
      let availability = app.availability(entry.needs);
      let description = if availability.is_available() {
        entry.description.to_string()
      } else if matches!(entry.key, HelpKey::Literal(_)) {
        return None;
      } else {
        app.strings().help.unavailable_description(
          entry.description,
          &availability.localized_hint(app.strings())?,
        )
      };
      let key = match entry.key {
        HelpKey::Binding(read) => read(keys).to_string(),
        HelpKey::Literal(text) => text.to_string(),
        HelpKey::Custom(build) => build(app),
      };
      Some(vec![description, key, entry.context.to_string()])
    })
    .collect()
}

/// Where the terminal offers a shared [`Action`].
// Read by the meta-test only, until the GUI affordance table lands.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub enum TuiSurface {
  /// A rebindable key from `config.yml`, read like [`HelpKey::Binding`].
  Binding(fn(&KeyBindings) -> Key),
  /// A key one screen hard-codes; `context` names the screen like the help table.
  Literal {
    key: &'static str,
    context: &'static str,
  },
  /// A pointer gesture with no key at all.
  Mouse(&'static str),
}

/// Whether a frontend offers an [`Action`] at all.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub enum Exposure<S> {
  Bound(S),
  /// No producer in this build, and why.
  Unbound(&'static str),
}

const ENTER: &str = "<Enter>";
const ESC: &str = "<Esc>";
const PLUGIN_ONLY: &str = "plugin-only: the scripting engine is its only producer";
#[cfg(not(feature = "ai-dj"))]
const NO_AI_DJ: Exposure<TuiSurface> =
  Exposure::Unbound("the AI DJ keys exist only in ai-dj builds");

fn binding(read: fn(&KeyBindings) -> Key) -> Exposure<TuiSurface> {
  Exposure::Bound(TuiSurface::Binding(read))
}

fn literal(key: &'static str, context: &'static str) -> Exposure<TuiSurface> {
  Exposure::Bound(TuiSurface::Literal { key, context })
}

/// Each [`Action`]'s primary terminal surface; `Bound` means reachable in this build.
// No production caller yet: the meta-test below and the GUI affordance table read it.
#[allow(dead_code)]
pub fn default_binding(action: &Action) -> Exposure<TuiSurface> {
  use Exposure::Unbound;
  match action {
    Action::Play => Unbound("the terminal binds the play/pause toggle, not the play intent"),
    Action::Pause => Unbound("the terminal binds the play/pause toggle, not the pause intent"),
    Action::TogglePlayback => binding(|k| k.toggle_playback),
    Action::NextTrack => binding(|k| k.next_track),
    Action::PreviousTrack => binding(|k| k.previous_track),
    Action::ForcePreviousTrack => binding(|k| k.force_previous_track),
    Action::SeekTo(_) => Exposure::Bound(TuiSurface::Mouse(
      "a click or drag on the playbar progress line",
    )),
    Action::SeekForward => binding(|k| k.seek_forwards),
    Action::SeekBackward => binding(|k| k.seek_backwards),
    Action::SetVolume(_) => Unbound("the terminal steps the volume, it never sets it"),
    Action::VolumeUp => binding(|k| k.increase_volume),
    Action::VolumeDown => binding(|k| k.decrease_volume),
    Action::SetShuffle(_) => Unbound("the terminal toggles shuffle, it never sets it"),
    Action::ToggleShuffle => binding(|k| k.shuffle),
    Action::CycleRepeat => binding(|k| k.repeat),
    Action::SetRepeat(_) => Unbound("the terminal cycles repeat, it never sets it"),
    Action::PlayUris { .. } => literal(
      ENTER,
      "Track table / search songs / artist top tracks / recently played",
    ),
    Action::PlayContext { .. } => literal(ENTER, "Album tracks"),
    Action::PlayTrackInContext { .. } => literal(ENTER, "Track table (playlist views)"),
    Action::TransferPlayback { .. } => literal(ENTER, "Source and device picker"),
    Action::AddToQueue(_) => {
      Unbound("the queue key feeds the native queue through QueueTrack, not the Web API queue")
    }
    Action::QueueTrack(_) => binding(|k| k.add_item_to_queue),
    Action::PlayQueueItem { .. } => literal(ENTER, "Queue"),
    Action::RemoveFromQueue { .. } => binding(|k| k.remove_from_queue),
    Action::MoveQueueItem { .. } => literal("J / K", "Queue"),
    Action::ClearQueue => Unbound("the terminal queue removes one item at a time"),
    Action::RefreshQueue => Unbound("the terminal refreshes the queue by opening it"),
    Action::RefreshDevices => Unbound("the terminal fetches devices by opening the device picker"),
    Action::Search(_) => Unbound("the search box is source-scoped and produces SearchActiveSource"),
    Action::SearchActiveSource(_) => literal(ENTER, "Search input"),
    Action::SearchPlaylistTracks { .. } => {
      literal("<Ctrl+f>, then <Enter>", "Track table (playlist views)")
    }
    Action::CreatePlaylist { .. } => literal(ENTER, "Create playlist form"),
    Action::CreateYouTubePlaylist(_) => literal(ENTER, "Create playlist form"),
    Action::SearchTracksForPlaylist(_) => literal(ENTER, "Create playlist form"),
    Action::AddTrackToPlaylist { .. } => literal(ENTER, "Add to playlist picker"),
    Action::RemoveTrackFromPlaylist { .. } => literal(ENTER, "Remove track confirmation"),
    Action::FollowPlaylist(_) => literal("w", "Search result"),
    Action::UnfollowPlaylist(_) => literal("D", "Playlist"),
    Action::DeletePlaylist(_) => literal("D", "Playlist"),
    Action::OpenPlaylistSyncPicker => literal("m", "Playlist"),
    Action::LinkPlaylistTo(_) => literal(ENTER, "Mirror playlist picker"),
    Action::RunPlaylistSync => literal("s", "Playlist sync"),
    Action::RemovePlaylistSyncLink(_) => literal(ENTER, "Remove link confirmation"),
    Action::ToggleSaveTrack(_) => literal("s", "Selected block"),
    Action::ToggleSaveCurrentItem => binding(|k| k.like_track),
    Action::SaveAlbum(_) => literal("w", "Search result"),
    Action::UnsaveAlbum(_) => literal("D", "Library -> Albums"),
    Action::SaveShow(_) => literal("w", "Search result"),
    Action::UnsaveShow(_) => literal("D", "Library -> Podcasts"),
    Action::FollowArtist(_) => literal("w", "Search result"),
    Action::UnfollowArtist(_) => literal("D", "Library -> Artists"),
    Action::AddFriendByCode(_) => literal(ENTER, "Add friend dialog"),
    Action::AddFriendById(_) => literal(ENTER, "Add friend dialog"),
    Action::UnfollowFriend(_) => literal("u", "Friends"),
    Action::SearchFriendUsers(_) => literal("typing", "Add friend dialog"),
    Action::FavoriteRadioStation(_) => binding(|k| k.like_track),
    Action::RemoveRadioStation(_) => literal("D", "Radio"),
    Action::Notify(..) => Unbound("a status message is a consequence, never a gesture"),
    Action::NotifyError(..) => Unbound("a status message is a consequence, never a gesture"),
    Action::Navigate(NavTarget::Home) => {
      Unbound("the terminal reaches Home by popping the stack, never by a key")
    }
    Action::Navigate(NavTarget::Queue) => binding(|k| k.show_queue),
    Action::Navigate(NavTarget::Settings) => binding(|k| k.open_settings),
    Action::Navigate(NavTarget::Devices) => binding(|k| k.manage_devices),
    Action::Navigate(NavTarget::Help) => binding(|k| k.help),
    Action::Navigate(NavTarget::Lyrics) => binding(|k| k.lyrics_view),
    Action::Navigate(NavTarget::RecentlyPlayed) => {
      Unbound("the sidebar opens Recently Played through OpenLibrary")
    }
    Action::Navigate(NavTarget::Party) => binding(|k| k.listening_party),
    Action::Navigate(NavTarget::Analysis) => binding(|k| k.audio_analysis),
    Action::Navigate(NavTarget::MiniPlayer) => binding(|k| k.miniplayer_view),
    Action::Back => Unbound(
      "the back key runs the runner's richer path: filter clear, settings prompt, announcement dismissal, search double-pop, exit prompt",
    ),
    Action::LoadMore(_) => binding(|k| k.next_page),
    Action::Sort {
      context: SortContext::PlaylistTracks | SortContext::SavedAlbums | SortContext::SavedArtists,
      ..
    } => literal(ENTER, "Sort menu"),
    Action::Sort {
      context: SortContext::RecentlyPlayed,
      ..
    } => Unbound("no sort menu opens on Recently Played"),
    Action::ToggleSortOrder(
      SortContext::PlaylistTracks | SortContext::SavedAlbums | SortContext::SavedArtists,
    ) => literal("uppercase field shortcut", "Sort menu"),
    Action::ToggleSortOrder(SortContext::RecentlyPlayed) => {
      Unbound("no sort menu opens on Recently Played")
    }
    Action::Open(_) => literal(ENTER, "Selected block"),
    Action::OpenShowEpisodes(_) => literal(ENTER, "Library -> Podcasts"),
    Action::OpenLibrary(
      LibraryTarget::Discover
      | LibraryTarget::RecentlyPlayed
      | LibraryTarget::Friends
      | LibraryTarget::Stats
      | LibraryTarget::PlaylistSync
      | LibraryTarget::LikedSongs
      | LibraryTarget::Albums
      | LibraryTarget::Artists
      | LibraryTarget::Podcasts,
    ) => literal(ENTER, "Library sidebar"),
    Action::OpenLibrary(LibraryTarget::LocalFiles) => {
      #[cfg(feature = "local-files")]
      {
        literal(ENTER, "Library sidebar")
      }
      #[cfg(not(feature = "local-files"))]
      {
        Unbound("the Local Files row exists only in local-files builds")
      }
    }
    Action::OpenLibrary(LibraryTarget::AiDj) => {
      #[cfg(feature = "ai-dj")]
      {
        binding(|k| k.dj_open)
      }
      #[cfg(not(feature = "ai-dj"))]
      {
        NO_AI_DJ
      }
    }
    Action::SelectSource(_) => literal(ENTER, "Source and device picker"),
    Action::OpenAddTrackDialog => literal("w", "Track table"),
    Action::OpenAddTrackDialogFor { .. } => literal(
      "w",
      "Track table / search songs / artist top tracks / recently played",
    ),
    Action::OpenAddPlayingTrackDialog => literal("W", "Global"),
    Action::OpenRemoveTrackDialog => literal("x", "Track table (playlist views)"),
    Action::JumpToAlbum => binding(|k| k.jump_to_album),
    Action::JumpToArtist => binding(|k| k.jump_to_artist_album),
    Action::JumpToContext => binding(|k| k.jump_to_context),
    Action::CopyUrl(CopyTarget::CurrentSong) => binding(|k| k.copy_song_url),
    Action::CopyUrl(CopyTarget::CurrentAlbum) => binding(|k| k.copy_album_url),
    Action::GenerateRecap => binding(|k| k.generate_recap),
    Action::GenerateStatsRecap => Unbound("the terminal's recap key reads the Stats route"),
    Action::CycleStatsPeriod { .. } => literal("[ / ]", "Stats"),
    Action::RecommendFromTrack(_) => literal("r", "Selected block"),
    Action::RecommendFromArtist { .. } => literal("r", "Selected block"),
    Action::RecommendFromTrackId { .. } => literal("r", "Selected block"),
    Action::StartParty => literal("h", "Listening Party menu"),
    Action::JoinParty { .. } => literal(ENTER, "Listening Party menu"),
    Action::LeaveParty => literal("l", "Listening Party menu"),
    Action::TogglePartyControlMode => literal("c", "Listening Party menu"),
    Action::SetPlaybarSegment { .. } => Unbound(PLUGIN_ONLY),
    Action::ShowPopup(_) => Unbound(PLUGIN_ONLY),
    Action::ClosePopup => literal("<Esc>", "Plugin popup"),
    Action::SetTheme(_) => Unbound(PLUGIN_ONLY),
    Action::SaveSettings => binding(|k| k.save_settings),
    Action::SetSetting { .. } => Unbound("the GUI's settings page sets one row"),
    Action::PressSettingButton { .. } => literal("o", "Settings"),
    Action::CycleVisualizerStyle => literal("V", "Audio analysis"),
    Action::SetScreenContent { .. } => Unbound(PLUGIN_ONLY),
    Action::ShowScreen(_) => Unbound(PLUGIN_ONLY),
    Action::CloseScreen(_) => Unbound(PLUGIN_ONLY),
    Action::QueueTracks(_) => Unbound("the DJ and MCP queue tools have no gesture"),
    Action::SetDjVibe(_) => Unbound("the DJ and MCP vibe tools have no gesture"),
    Action::AskDj(_) => {
      #[cfg(feature = "ai-dj")]
      {
        literal(ENTER, "AI DJ")
      }
      #[cfg(not(feature = "ai-dj"))]
      {
        NO_AI_DJ
      }
    }
    Action::DjVibeShift => {
      #[cfg(feature = "ai-dj")]
      {
        binding(|k| k.dj_vibe_shift)
      }
      #[cfg(not(feature = "ai-dj"))]
      {
        NO_AI_DJ
      }
    }
    Action::ToggleDjAutoQueue => {
      #[cfg(feature = "ai-dj")]
      {
        binding(|k| k.dj_toggle_auto_queue)
      }
      #[cfg(not(feature = "ai-dj"))]
      {
        NO_AI_DJ
      }
    }
    Action::ToggleDjFreshOnly => {
      #[cfg(feature = "ai-dj")]
      {
        binding(|k| k.dj_toggle_fresh_only)
      }
      #[cfg(not(feature = "ai-dj"))]
      {
        NO_AI_DJ
      }
    }
    Action::OpenDjSetup => {
      #[cfg(feature = "ai-dj")]
      {
        binding(|k| k.dj_pick_model)
      }
      #[cfg(not(feature = "ai-dj"))]
      {
        NO_AI_DJ
      }
    }
    Action::DismissDjSetup => {
      #[cfg(feature = "ai-dj")]
      {
        literal(ESC, "AI DJ")
      }
      #[cfg(not(feature = "ai-dj"))]
      {
        NO_AI_DJ
      }
    }
    Action::CommitDjSetup => {
      #[cfg(feature = "ai-dj")]
      {
        literal(ENTER, "AI DJ")
      }
      #[cfg(not(feature = "ai-dj"))]
      {
        NO_AI_DJ
      }
    }
    Action::AnswerCommunityPinPrompt { keep: false } => literal("h", "Community pin prompt"),
    Action::AnswerCommunityPinPrompt { keep: true } => literal(ENTER, "Community pin prompt"),
    Action::OpenRecap => literal(ENTER, "Recap prompt"),
    Action::DismissRecapPrompt => literal(ESC, "Recap prompt"),
    Action::DisableRecapPrompt => literal("d", "Recap prompt"),
    Action::DismissAnnouncement { .. } => literal(ENTER, "Announcement prompt"),
    Action::OpenAnnouncementUrl { .. } => {
      Unbound("the announcement prompt shows its link as text; the GUI strip opens it")
    }
    Action::OpenChangelogLink { .. } => {
      Unbound("the terminal Home shows its links as text; the GUI Welcome page opens them")
    }
  }
}

#[cfg(test)]
mod tests {
  use std::collections::BTreeSet;
  use std::path::{Path, PathBuf};

  use super::*;
  use crate::core::action::{ListTarget, OpenTarget, RepeatSetting, SettingInput};
  use crate::core::plugin_api::{PluginPopup, PluginScreenContent, ShowInfo, TrackInfo};
  use crate::core::sort::SortField;
  use crate::core::test_helpers::full_track;
  use crate::core::theme::{Color, ThemeField};

  fn descriptions(app: &App) -> Vec<String> {
    help_rows(app)
      .into_iter()
      .map(|mut row| row.swap_remove(0))
      .collect()
  }

  #[test]
  fn every_english_help_event_and_description_matches_the_pre_localization_table() {
    // Captured by running help_entries/help_rows at 5d66123b with cover-art + ai-dj.
    // Keep all source/session views as well as rows hidden by availability.
    let baseline: serde_json::Value =
      serde_json::from_str(include_str!("help_english_5d66123b.json")).unwrap();
    let mut entries: Vec<Vec<String>> =
      serde_json::from_value(baseline["entries"].clone()).unwrap();
    // The fixture was captured on macOS; Open settings defaults to Alt+, elsewhere.
    let open_settings = crate::core::user_config::UserConfig::new()
      .keys
      .open_settings
      .to_string();
    for row in entries.iter_mut().filter(|row| row[0] == "Open settings") {
      row[1] = open_settings.clone();
    }
    let enabled = |row: &Vec<String>| {
      (cfg!(feature = "cover-art") || row[0] != "Go to cover art view")
        && (cfg!(feature = "ai-dj") || row[2] != "AI DJ")
    };
    let app = App::default_connected();
    let actual: Vec<Vec<String>> = help_entries_for(&crate::core::i18n::EN)
      .into_iter()
      .map(|entry| {
        let event = match entry.key {
          HelpKey::Binding(read) => read(&app.user_config.keys).to_string(),
          HelpKey::Literal(text) => text.to_string(),
          HelpKey::Custom(build) => build(&app),
        };
        vec![
          entry.description.to_string(),
          event,
          entry.context.to_string(),
        ]
      })
      .collect();
    assert_eq!(
      actual,
      entries
        .iter()
        .filter(|row| enabled(row))
        .cloned()
        .collect::<Vec<_>>()
    );
    for view in baseline["views"].as_array().unwrap() {
      let mut app = if view[1].as_bool().unwrap() {
        App::default_connected()
      } else {
        App::default()
      };
      app.active_source = serde_json::from_value(view[0].clone()).unwrap();
      let expected = view[2]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| {
          let index = item.as_u64().unwrap_or_else(|| item[0].as_u64().unwrap()) as usize;
          if !enabled(&entries[index]) {
            return None;
          }
          let mut row = entries[index].clone();
          if item.is_array() {
            row[0] = item[1].as_str().unwrap().to_string();
          }
          Some(row)
        })
        .collect::<Vec<_>>();
      assert_eq!(
        help_rows(&app),
        expected,
        "{} connected={}",
        view[0],
        view[1]
      );
      use ratatui::{backend::TestBackend, Terminal};
      let width = 600u16;
      let height = expected.len() as u16 + 12;
      app.view.size = crate::core::geometry::Viewport { width, height };
      crate::tui::ui::popups::ensure_help_menu_model(&mut app);
      let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
      terminal
        .draw(|frame| crate::tui::ui::popups::draw_help_menu(frame, &app))
        .unwrap();
      let buffer = terminal.backend().buffer();
      let description_width = ((width - 4) as f32 * 0.40) as u16;
      let event_width = ((width - 4) as f32 * 0.30) as u16;
      for (index, row) in expected.iter().enumerate() {
        let y = index as u16 + 4;
        let cell_text = |x, length| {
          (x..x + length)
            .map(|x| buffer[(x, y)].symbol())
            .collect::<String>()
        };
        assert_eq!(
          cell_text(3, description_width).trim_end(),
          row[0],
          "rendered description {index}"
        );
        assert_eq!(
          cell_text(3 + description_width + 2, event_width).trim_end(),
          row[1],
          "rendered Event {index}"
        );
      }
    }
  }

  #[test]
  fn help_events_translate_words_without_changing_literal_keys() {
    use crate::core::i18n::Language;
    let mut app = App::default();
    for language in [Language::En, Language::ZhHans, Language::ZhHant] {
      app.user_config.behavior.language = language;
      app.refresh_language();
      let help = &app.strings().help;
      let rows = help_rows(&app);
      let event = |description| rows.iter().find(|row| row[0] == description).unwrap()[1].clone();
      assert_eq!(
        event(help.move_left),
        format!(
          "{} | <{}> | <Ctrl+b>",
          app.user_config.keys.move_left,
          help.arrow_key("Left")
        )
      );
      assert_eq!(
        event(help.move_down),
        format!(
          "{} | <{}> | <Ctrl+n>",
          app.user_config.keys.move_down,
          help.arrow_key("Down")
        )
      );
      assert_eq!(
        event(help.move_up),
        format!(
          "{} | <{}>",
          app.user_config.keys.move_up,
          help.arrow_key("Up")
        )
      );
      assert_eq!(
        event(help.move_right),
        format!(
          "{} | <{}> | <Ctrl+f>",
          app.user_config.keys.move_right,
          help.arrow_key("Right")
        )
      );
      assert_eq!(
        event(help.lyrics_follow),
        help.key_alternative("f", "<Esc>")
      );
    }
  }

  #[test]
  fn a_connected_spotify_session_lists_every_row_it_can_serve_unmarked() {
    let app = App::default_connected();
    let rows = descriptions(&app);
    for entry in help_entries_for(&crate::core::i18n::EN) {
      if app.availability(entry.needs).is_available() {
        assert!(
          rows.contains(&entry.description.to_string()),
          "{} is missing",
          entry.description
        );
      }
    }
    assert!(!rows.iter().any(|row| row.ends_with("(needs Spotify)")));
    // The radio favorite shares a configured key, so it stays, marked.
    assert!(rows
      .contains(&"Favorite highlighted/playing radio station (Internet Radio only)".to_string()));
    assert!(!rows
      .iter()
      .any(|row| row == "Remove favorite radio station"));
  }

  #[test]
  fn without_a_session_configured_keys_stay_marked_and_fixed_keys_go() {
    let app = App::default();
    let rows = descriptions(&app);
    assert!(rows.contains(&"Open Listening Party menu (needs Spotify)".to_string()));
    assert!(rows.contains(
      &"Toggle saved state for currently playing track/episode (needs Spotify)".to_string()
    ));
    assert!(!rows.iter().any(|row| row == "Delete saved album"));
    assert!(!rows
      .iter()
      .any(|row| row == "Play recommendations for song/artist"));
    assert!(rows.contains(&"Open Stats screen".to_string()));
  }

  #[test]
  fn a_free_source_names_the_source_in_the_hint() {
    let mut app = App::default_connected();
    app.active_source = Source::Local;
    let rows = descriptions(&app);
    assert!(rows.contains(&"Enter input for search (not for Local Files)".to_string()));
    // The copy keys read the Spotify playback, whatever the browse scope.
    assert!(rows.contains(&"Copy url to currently playing song/episode".to_string()));
    assert!(!rows.iter().any(|row| row == "Search with input text"));
    assert!(rows.contains(&"Play random song in playlist".to_string()));
  }

  #[test]
  fn radio_rows_appear_only_under_the_radio_scope() {
    let mut app = App::default_connected();
    assert!(!descriptions(&app)
      .iter()
      .any(|row| row == "Remove favorite radio station"));
    app.active_source = Source::Radio;
    let rows = descriptions(&app);
    assert!(rows.contains(&"Remove favorite radio station".to_string()));
    assert!(rows.contains(&"Favorite highlighted/playing radio station".to_string()));
    assert!(rows.contains(
      &"Toggle saved state for currently playing track/episode (not for Internet Radio)"
        .to_string()
    ));
  }

  #[test]
  fn the_key_column_follows_the_configured_binding() {
    let mut app = App::default_connected();
    app.user_config.keys.next_track = Key::Char('N');
    let row = help_rows(&app)
      .into_iter()
      .find(|row| row[0] == "Skip to next track")
      .expect("the next-track row");
    assert_eq!(row[1], "N");
    assert_eq!(row[2], "General");
  }

  #[test]
  fn the_move_up_row_does_not_offer_the_listening_party_key() {
    let app = App::default_connected();
    let party = app.user_config.keys.listening_party.to_string();
    let row = help_rows(&app)
      .into_iter()
      .find(|row| row[0] == "Move selection up")
      .expect("the move-up row");
    assert!(!row[1].contains(&party), "{}", row[1]);
    assert!(row[1].contains("<Up Arrow Key>"));
  }

  #[test]
  fn the_lyrics_scroll_row_does_not_offer_the_listening_party_key() {
    let app = App::default_connected();
    let party = app.user_config.keys.listening_party.to_string();
    let row = help_rows(&app)
      .into_iter()
      .find(|row| row[0] == "Scroll lyrics (pauses auto-follow)")
      .expect("the lyrics scroll row");
    assert!(!row[1].contains(&party), "{}", row[1]);
    assert!(row[1].contains("<Ctrl+n>"));
  }

  /// The variants no terminal gesture produces, in every build.
  const UNBOUND: &[&str] = &[
    "AddToQueue",
    "Back",
    "ClearQueue",
    "CloseScreen",
    "GenerateStatsRecap",
    "Notify",
    "NotifyError",
    "OpenAnnouncementUrl",
    "OpenChangelogLink",
    "Pause",
    "Play",
    "QueueTracks",
    "RefreshDevices",
    "RefreshQueue",
    "Search",
    "SetDjVibe",
    "SetPlaybarSegment",
    "SetRepeat",
    "SetScreenContent",
    "SetSetting",
    "SetShuffle",
    "SetTheme",
    "SetVolume",
    "ShowPopup",
    "ShowScreen",
  ];

  /// Bound only with `ai-dj`: their producers are on disk but compiled out here.
  #[cfg(not(feature = "ai-dj"))]
  const FEATURE_UNBOUND: &[&str] = &[
    "AskDj",
    "CommitDjSetup",
    "DismissDjSetup",
    "DjVibeShift",
    "OpenDjSetup",
    "ToggleDjAutoQueue",
    "ToggleDjFreshOnly",
  ];
  #[cfg(feature = "ai-dj")]
  const FEATURE_UNBOUND: &[&str] = &[];

  fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
  }

  fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
      return;
    };
    for entry in entries.flatten() {
      let path = entry.path();
      if path.is_dir() {
        rs_files(&path, out);
      } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
        out.push(path);
      }
    }
  }

  /// `source` without its test items and comment lines: a gated block item
  /// runs to its column-0 `}` (rustfmt's shape), a braceless one to its `;`.
  fn production_half(source: &str) -> String {
    let mut out = String::new();
    let mut gated = false;
    let mut in_block = false;
    let mut in_item = false;
    for line in source.lines() {
      if in_block {
        in_block = line != "}";
        continue;
      }
      if in_item {
        in_item = !line.ends_with(';');
        continue;
      }
      if gated {
        gated = line.starts_with("#[");
        if !gated {
          in_block = line.ends_with('{');
          in_item = !in_block && !line.ends_with(';') && !line.ends_with('}');
        }
        continue;
      }
      if line.starts_with("#[cfg(test)]") || line.starts_with("#[cfg(all(test") {
        gated = true;
        continue;
      }
      if line.trim_start().starts_with("//") {
        continue;
      }
      out.push_str(line);
      out.push('\n');
    }
    out
  }

  fn leading_ident(text: &str) -> &str {
    let end = text
      .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
      .unwrap_or(text.len());
    &text[..end]
  }

  /// The variant names of `pub enum Action`, read from its source: the lines
  /// at exactly two spaces whose identifier is followed by `,`, `(` or ` {`.
  fn action_variant_names() -> BTreeSet<String> {
    let source = std::fs::read_to_string(repo_root().join("src/core/action/mod.rs")).unwrap();
    source
      .lines()
      .skip_while(|line| *line != "pub enum Action {")
      .skip(1)
      .take_while(|line| *line != "}")
      .filter_map(|line| {
        let rest = line.strip_prefix("  ")?;
        let name = leading_ident(rest);
        let tail = &rest[name.len()..];
        let is_variant = !name.is_empty()
          && (tail.starts_with(',') || tail.starts_with('(') || tail.starts_with(" {"));
        is_variant.then(|| name.to_string())
      })
      .collect()
  }

  /// `Action::<Name>` in `source`, bounded on both sides so `PlaybackAction::`
  /// is skipped and `Action::Play` is not found inside `Action::PlayUris`.
  fn producers_in(source: &str) -> BTreeSet<String> {
    let production = production_half(source);
    let mut found = BTreeSet::new();
    for (start, needle) in production.match_indices("Action::") {
      let before = start.checked_sub(1).map(|i| production.as_bytes()[i]);
      if before.is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_') {
        continue;
      }
      let name = leading_ident(&production[start + needle.len()..]);
      if !name.is_empty() {
        found.insert(name.to_string());
      }
    }
    found
  }

  /// Every variant some production file under `src/tui/` builds. The
  /// registry itself is skipped: it names every variant and produces none.
  fn tui_producers() -> BTreeSet<String> {
    let mut files = Vec::new();
    rs_files(&repo_root().join("src/tui"), &mut files);
    let registry = repo_root().join("src/tui/keymap.rs");
    files
      .iter()
      .filter(|path| **path != registry)
      .flat_map(|path| producers_in(&std::fs::read_to_string(path).unwrap()))
      .collect()
  }

  /// Derived `Debug` starts with the variant name: `Play`, `SeekTo(0)`,
  /// `PlayUris { .. }`.
  fn variant_name(action: &Action) -> String {
    format!("{action:?}")
      .split(['(', ' '])
      .next()
      .unwrap()
      .to_string()
  }

  /// One value per `Action` variant, with a reachable payload where the
  /// registry answers per payload.
  fn sample_actions() -> Vec<Action> {
    let text = String::new;
    let track = || TrackInfo::from(&full_track("4uLU6hMCjMI75M1A2tKUQC", "T"));
    vec![
      Action::Play,
      Action::Pause,
      Action::TogglePlayback,
      Action::NextTrack,
      Action::PreviousTrack,
      Action::ForcePreviousTrack,
      Action::SeekTo(0),
      Action::SeekForward,
      Action::SeekBackward,
      Action::SetVolume(0),
      Action::VolumeUp,
      Action::VolumeDown,
      Action::SetShuffle(false),
      Action::ToggleShuffle,
      Action::CycleRepeat,
      Action::SetRepeat(RepeatSetting::Off),
      Action::PlayUris {
        uris: vec![],
        offset: None,
      },
      Action::PlayContext {
        uri: text(),
        offset: None,
      },
      Action::PlayTrackInContext {
        context: text(),
        track: text(),
      },
      Action::TransferPlayback {
        device_id: text(),
        persist: false,
      },
      Action::AddToQueue(text()),
      Action::QueueTrack(track()),
      Action::PlayQueueItem {
        uri: text(),
        position: 0,
      },
      Action::RemoveFromQueue {
        uri: text(),
        position: 0,
      },
      Action::MoveQueueItem {
        uri: text(),
        from: 0,
        to: 0,
      },
      Action::ClearQueue,
      Action::RefreshQueue,
      Action::RefreshDevices,
      Action::Search(text()),
      Action::SearchActiveSource(text()),
      Action::SearchPlaylistTracks {
        playlist_id: text(),
        query: text(),
      },
      Action::CreatePlaylist {
        name: text(),
        track_uris: vec![],
      },
      Action::CreateYouTubePlaylist(text()),
      Action::SearchTracksForPlaylist(text()),
      Action::AddTrackToPlaylist {
        playlist: text(),
        track: text(),
      },
      Action::RemoveTrackFromPlaylist {
        playlist: text(),
        track: text(),
        position: 0,
      },
      Action::FollowPlaylist(text()),
      Action::UnfollowPlaylist(text()),
      Action::DeletePlaylist(text()),
      Action::OpenPlaylistSyncPicker,
      Action::LinkPlaylistTo(Source::Qobuz),
      Action::RunPlaylistSync,
      Action::RemovePlaylistSyncLink(text()),
      Action::ToggleSaveTrack(text()),
      Action::ToggleSaveCurrentItem,
      Action::SaveAlbum(text()),
      Action::UnsaveAlbum(text()),
      Action::SaveShow(text()),
      Action::UnsaveShow(text()),
      Action::FollowArtist(text()),
      Action::UnfollowArtist(text()),
      Action::AddFriendByCode(text()),
      Action::AddFriendById(text()),
      Action::UnfollowFriend(text()),
      Action::SearchFriendUsers(text()),
      Action::FavoriteRadioStation(track()),
      Action::RemoveRadioStation(text()),
      Action::Notify(text(), 0),
      Action::NotifyError(text(), 0),
      Action::Navigate(NavTarget::Queue),
      Action::Back,
      Action::LoadMore(ListTarget::PlaylistTracks),
      Action::Sort {
        context: SortContext::PlaylistTracks,
        field: SortField::default(),
      },
      Action::ToggleSortOrder(SortContext::PlaylistTracks),
      Action::Open(OpenTarget::SavedAlbum(text())),
      Action::OpenShowEpisodes(ShowInfo::default()),
      Action::OpenLibrary(LibraryTarget::Discover),
      Action::SelectSource(Source::Spotify),
      Action::OpenAddTrackDialog,
      Action::OpenAddTrackDialogFor {
        track_id: None,
        track_name: text(),
      },
      Action::OpenAddPlayingTrackDialog,
      Action::OpenRemoveTrackDialog,
      Action::JumpToAlbum,
      Action::JumpToArtist,
      Action::JumpToContext,
      Action::CopyUrl(CopyTarget::CurrentSong),
      Action::GenerateRecap,
      Action::GenerateStatsRecap,
      Action::CycleStatsPeriod { forward: true },
      Action::RecommendFromTrack(track()),
      Action::RecommendFromArtist {
        id: text(),
        name: text(),
      },
      Action::RecommendFromTrackId {
        id: text(),
        name: text(),
      },
      Action::StartParty,
      Action::JoinParty {
        code: text(),
        name: text(),
      },
      Action::LeaveParty,
      Action::TogglePartyControlMode,
      Action::SetPlaybarSegment {
        plugin: text(),
        text: None,
      },
      Action::ShowPopup(PluginPopup {
        title: text(),
        lines: vec![],
      }),
      Action::ClosePopup,
      Action::SetTheme(vec![(ThemeField::Active, Color::Reset)]),
      Action::SaveSettings,
      Action::SetSetting {
        id: text(),
        value: SettingInput::Bool(true),
      },
      Action::PressSettingButton { id: text() },
      Action::CycleVisualizerStyle,
      Action::AnswerCommunityPinPrompt { keep: false },
      Action::OpenRecap,
      Action::DismissRecapPrompt,
      Action::DisableRecapPrompt,
      Action::DismissAnnouncement { id: text() },
      Action::OpenAnnouncementUrl { id: text() },
      Action::OpenChangelogLink { url: text() },
      Action::SetScreenContent {
        name: text(),
        content: PluginScreenContent::default(),
      },
      Action::ShowScreen(text()),
      Action::CloseScreen(text()),
      Action::QueueTracks(vec![]),
      Action::SetDjVibe(None),
      Action::AskDj(text()),
      Action::DjVibeShift,
      Action::ToggleDjAutoQueue,
      Action::ToggleDjFreshOnly,
      Action::OpenDjSetup,
      Action::DismissDjSetup,
      Action::CommitDjSetup,
    ]
  }

  #[test]
  fn every_action_variant_has_exactly_one_sample() {
    let names: Vec<String> = sample_actions().iter().map(variant_name).collect();
    let sampled: BTreeSet<String> = names.iter().cloned().collect();
    assert_eq!(sampled.len(), names.len(), "a variant is sampled twice");
    assert_eq!(
      sampled,
      action_variant_names(),
      "sample_actions() and `pub enum Action` disagree"
    );
  }

  #[test]
  fn the_unbound_set_is_pinned_by_name() {
    let measured: BTreeSet<String> = sample_actions()
      .iter()
      .filter(|action| matches!(default_binding(action), Exposure::Unbound(_)))
      .map(variant_name)
      .collect();
    let pinned: BTreeSet<String> = UNBOUND
      .iter()
      .chain(FEATURE_UNBOUND)
      .map(|name| name.to_string())
      .collect();
    assert_eq!(measured, pinned, "move UNBOUND together with the registry");
  }

  #[test]
  fn bound_variants_have_a_terminal_producer_and_unbound_ones_have_none() {
    let producers = tui_producers();
    for action in sample_actions() {
      let name = variant_name(&action);
      match default_binding(&action) {
        Exposure::Bound(_) => assert!(
          producers.contains(&name),
          "Action::{name} claims a surface but nothing under src/tui/ builds it"
        ),
        Exposure::Unbound(reason) => assert!(
          FEATURE_UNBOUND.contains(&name.as_str()) || !producers.contains(&name),
          "Action::{name} is Unbound ({reason}) but src/tui/ builds it"
        ),
      }
    }
  }

  #[test]
  fn the_payloads_no_gesture_reaches_are_pinned_by_hand() {
    let unreached = [
      Action::Navigate(NavTarget::Home),
      Action::Navigate(NavTarget::RecentlyPlayed),
      Action::Sort {
        context: SortContext::RecentlyPlayed,
        field: SortField::default(),
      },
      Action::ToggleSortOrder(SortContext::RecentlyPlayed),
    ];
    for action in &unreached {
      assert!(
        matches!(default_binding(action), Exposure::Unbound(_)),
        "{action:?}"
      );
    }
    for target in NavTarget::ALL {
      if target != NavTarget::Home && target != NavTarget::RecentlyPlayed {
        assert!(
          matches!(
            default_binding(&Action::Navigate(target)),
            Exposure::Bound(_)
          ),
          "{target:?}"
        );
      }
    }
    let bound = |target| {
      matches!(
        default_binding(&Action::OpenLibrary(target)),
        Exposure::Bound(_)
      )
    };
    assert_eq!(bound(LibraryTarget::AiDj), cfg!(feature = "ai-dj"));
    assert_eq!(
      bound(LibraryTarget::LocalFiles),
      cfg!(feature = "local-files")
    );
  }

  #[test]
  fn the_enum_scanner_keeps_variants_and_drops_docs_attributes_and_fields() {
    let names = action_variant_names();
    assert!(
      names.contains("QueueTracks"),
      "a variant after a cfg_attr line"
    );
    assert!(names.contains("DjVibeShift"), "a variant with no doc line");
    assert!(names.contains("PlayUris"), "a struct variant");
    assert!(!names.contains("position"), "a struct field");
    assert!(!names.contains("cfg_attr"));
  }

  #[test]
  fn the_producer_scan_is_identifier_bounded_and_skips_tests_and_comments() {
    let source = "app.apply(Action::PlayUris { uris: vec![] });\n\
      PlaybackAction::Play;\n\
      /// through the shared `Action::Pause` vocabulary\n\
      #[cfg(test)]\n\
      mod tests {\n  Action::Back\n}\n\
      #[cfg(test)]\n\
      const HIDDEN: Option<Action> = Some(\n  Action::Notify(String::new(), 0),\n);\n\
      #[cfg(test)]\n\
      #[allow(dead_code)]\n\
      fn probe() -> Action { Action::Play }\n\
      let seek = Action::Search(query);\n";
    let expected: BTreeSet<String> = ["PlayUris", "Search"]
      .into_iter()
      .map(str::to_string)
      .collect();
    assert_eq!(producers_in(source), expected);
  }
}
