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

/// Every help row in display order, before the availability filter.
pub fn help_entries() -> Vec<HelpEntry> {
  use HelpKey::{Binding, Custom, Literal};
  vec![
    row(
      "Scroll down to next result page",
      Binding(|k| k.next_page),
      "Pagination",
    ),
    row(
      "Scroll up to previous result page",
      Binding(|k| k.previous_page),
      "Pagination",
    ),
    row(
      "Jump to start of playlist",
      Binding(|k| k.jump_to_start),
      "Pagination",
    ),
    row(
      "Jump to end of playlist",
      Binding(|k| k.jump_to_end),
      "Pagination",
    ),
    row(
      "Jump to currently playing album",
      Binding(|k| k.jump_to_album),
      "General",
    )
    .needs(SESSION),
    row(
      "Jump to currently playing artist's album list",
      Binding(|k| k.jump_to_artist_album),
      "General",
    )
    .needs(SESSION),
    row(
      "Jump to current play context",
      Binding(|k| k.jump_to_context),
      "General",
    )
    .needs(SESSION),
    row(
      "Increase volume by 10%",
      Binding(|k| k.increase_volume),
      "General",
    ),
    row(
      "Decrease volume by 10%",
      Binding(|k| k.decrease_volume),
      "General",
    ),
    row("Skip to next track", Binding(|k| k.next_track), "General"),
    row(
      "Skip to previous track",
      Binding(|k| k.previous_track),
      "General",
    ),
    row(
      "Force skip to previous track",
      Binding(|k| k.force_previous_track),
      "General",
    ),
    row(
      "Seek backwards 5 seconds",
      Binding(|k| k.seek_backwards),
      "General",
    ),
    row(
      "Seek forwards 5 seconds",
      Binding(|k| k.seek_forwards),
      "General",
    ),
    row("Toggle shuffle", Binding(|k| k.shuffle), "General"),
    row(
      "Copy url to currently playing song/episode",
      Binding(|k| k.copy_song_url),
      "General",
    )
    .needs(SESSION),
    row(
      "Copy url to currently playing album/show",
      Binding(|k| k.copy_album_url),
      "General",
    )
    .needs(SESSION),
    row("Cycle repeat mode", Binding(|k| k.repeat), "General"),
    row(
      "Move selection left",
      Custom(|app| {
        format!(
          "{} | <Left Arrow Key> | <Ctrl+b>",
          app.user_config.keys.move_left
        )
      }),
      "General",
    ),
    row(
      "Move selection down",
      Custom(|app| {
        format!(
          "{} | <Down Arrow Key> | <Ctrl+n>",
          app.user_config.keys.move_down
        )
      }),
      "General",
    ),
    row(
      "Move selection up",
      Custom(|app| {
        format!(
          "{} | <Up Arrow Key> | <Ctrl+p>",
          app.user_config.keys.move_up
        )
      }),
      "General",
    ),
    row(
      "Move selection right",
      Custom(|app| {
        format!(
          "{} | <Right Arrow Key> | <Ctrl+f>",
          app.user_config.keys.move_right
        )
      }),
      "General (Ctrl+f searches inside playlist track tables)",
    ),
    row("Move selection to top of list", Literal("H"), "General"),
    row("Move selection to middle of list", Literal("M"), "General"),
    row("Move selection to bottom of list", Literal("L"), "General"),
    row("Enter input for search", Binding(|k| k.search), "General").needs(SEARCH),
    row("Filter help rows", Binding(|k| k.search), "Help menu"),
    row("Filter settings rows", Binding(|k| k.search), "Settings"),
    row(
      "Pause/Resume playback",
      Binding(|k| k.toggle_playback),
      "General",
    ),
    row("Enter active mode", Literal("<Enter>"), "General"),
    row(
      "Go to audio analysis screen",
      Binding(|k| k.audio_analysis),
      "General",
    ),
    row(
      "Cycle visualizer style (in audio analysis)",
      Literal("V"),
      "General",
    ),
    row("Go to lyrics view", Binding(|k| k.lyrics_view), "General"),
    row(
      "Scroll lyrics (pauses auto-follow)",
      Custom(|app| {
        format!(
          "{}/{} | <Up>/<Down> | <Ctrl+p>/<Ctrl+n>",
          app.user_config.keys.move_up, app.user_config.keys.move_down
        )
      }),
      "Lyrics view",
    ),
    row(
      "Resume following the current lyric line",
      Literal("f or <Esc>"),
      "Lyrics view",
    ),
    row(
      "Nudge lyric timing earlier/later",
      Custom(|app| {
        format!(
          "{}/{} | <Right>/<Left> | <Ctrl+f>/<Ctrl+b>",
          app.user_config.keys.move_right, app.user_config.keys.move_left
        )
      }),
      "Lyrics view",
    ),
    row(
      "Toggle miniplayer view",
      Binding(|k| k.miniplayer_view),
      "General",
    ),
    #[cfg(feature = "cover-art")]
    row(
      "Go to cover art view",
      Binding(|k| k.cover_art_view),
      "General",
    ),
    row(
      "Go back or exit when nowhere left to back to",
      Binding(|k| k.back),
      "General",
    ),
    row(
      "Switch music source / select playback device",
      Binding(|k| k.manage_devices),
      "General",
    ),
    row(
      "Open settings",
      Custom(|app| app.effective_open_settings_key().to_string()),
      "General",
    ),
    row(
      "Save settings",
      Custom(|app| app.effective_save_settings_key().to_string()),
      "Settings",
    ),
    row("Enter hover mode", Literal("<Esc>"), "Selected block"),
    row(
      "Save track in list or table",
      Literal("s"),
      "Selected block",
    )
    .needs(LIKE),
    row(
      "Add selected track to playlist",
      Literal("w"),
      "Track table / search songs / artist top tracks / recently played",
    )
    .needs(PLAYLIST_WRITE),
    row(
      "Add currently playing track to playlist",
      Literal("w"),
      "Playbar",
    )
    .needs(SESSION),
    row(
      "Quick-add currently playing track to playlist",
      Literal("W"),
      "Global",
    )
    .needs(SESSION),
    row("Decrease sidebar width", Literal("{"), "Layout"),
    row("Increase sidebar width", Literal("}"), "Layout"),
    row("Decrease playbar or library height", Literal("("), "Layout"),
    row("Increase playbar or library height", Literal(")"), "Layout"),
    row("Reset layout to defaults", Literal("|"), "Layout"),
    row(
      "Remove selected track from current playlist",
      Literal("x"),
      "Track table (playlist views)",
    )
    .needs(PLAYLIST_WRITE),
    row(
      "Search tracks in current playlist",
      Literal("<Ctrl+f>"),
      "Track table (playlist views)",
    )
    .needs(SPOTIFY),
    row(
      "Clear playlist track search filter",
      Binding(|k| k.back),
      "Track table (filtered playlist views)",
    )
    .needs(SPOTIFY),
    row(
      "Start playback or enter album/artist/playlist",
      Binding(|k| k.submit),
      "Selected block",
    ),
    row(
      "Play recommendations for song/artist",
      Literal("r"),
      "Selected block",
    )
    .needs(SPOTIFY),
    row(
      "Play all tracks for artist",
      Literal("e"),
      "Library -> Artists",
    )
    .needs(SPOTIFY),
    row("Search with input text", Literal("<Enter>"), "Search input").needs(SEARCH),
    row(
      "Move cursor one space left",
      Literal("<Left Arrow Key>"),
      "Search input",
    ),
    row(
      "Move cursor one space right",
      Literal("<Right Arrow Key>"),
      "Search input",
    ),
    row("Delete entire input", Literal("<Ctrl+l>"), "Search input"),
    row(
      "Delete text from cursor to start of input",
      Literal("<Ctrl+u>"),
      "Search input",
    ),
    row(
      "Delete text from cursor to end of input",
      Literal("<Ctrl+k>"),
      "Search input",
    ),
    row("Delete previous word", Literal("<Ctrl+w>"), "Search input"),
    row(
      "Jump to start of input",
      Literal("<Ctrl+a>"),
      "Search input",
    ),
    row("Jump to end of input", Literal("<Ctrl+e>"), "Search input"),
    row(
      "Escape from the input back to hovered block",
      Literal("<Esc>"),
      "Search input",
    ),
    row("Delete saved album", Literal("D"), "Library -> Albums").needs(SPOTIFY),
    row("Delete saved playlist", Literal("D"), "Playlist").needs(PLAYLIST_WRITE),
    row("Remove favorite radio station", Literal("D"), "Radio").needs(RADIO),
    row("Follow an artist/playlist", Literal("w"), "Search result").needs(SPOTIFY),
    row(
      "Save (like) album to library",
      Literal("w"),
      "Search result",
    )
    .needs(SPOTIFY),
    row(
      "Play random song in playlist",
      Literal("S"),
      "Selected Playlist",
    ),
    row(
      "Toggle sort order of podcast episodes",
      Literal("S"),
      "Selected Show",
    )
    .needs(SPOTIFY),
    row(
      "Add track to queue",
      Binding(|k| k.add_item_to_queue),
      "Hovered over track",
    ),
    row("Show queue", Binding(|k| k.show_queue), "General"),
    row(
      "Remove selected track from queue",
      Binding(|k| k.remove_from_queue),
      "Queue",
    ),
    row(
      "Move selected queue item down / up",
      Literal("J / K"),
      "Queue",
    ),
    row(
      "Play selected queue item (skip ahead to it)",
      Literal("<Enter>"),
      "Queue",
    ),
    row(
      "Toggle saved state for currently playing track/episode",
      Binding(|k| k.like_track),
      "General",
    )
    .needs(LIKE),
    row(
      "Favorite highlighted/playing radio station",
      Binding(|k| k.like_track),
      "Radio",
    )
    .needs(RADIO),
    row(
      "Generate listening recap card (selected period on Stats, 30 days elsewhere)",
      Binding(|k| k.generate_recap),
      "General",
    ),
    row("Open Stats screen", Literal("Library sidebar"), "Stats"),
    row("Cycle stats period", Literal("[ / ]"), "Stats"),
    row("Play selected top track", Literal("<Enter>"), "Stats").needs(SESSION),
    row("Open sort menu", Literal(","), "Track/Album/Artist list"),
    row(
      "Open Listening Party menu",
      Binding(|k| k.listening_party),
      "General",
    )
    .needs(SESSION),
    #[cfg(feature = "ai-dj")]
    row("Open the AI DJ screen", Binding(|k| k.dj_open), "AI DJ"),
    #[cfg(feature = "ai-dj")]
    row(
      "Toggle DJ auto-queue (keep the queue topped up)",
      Binding(|k| k.dj_toggle_auto_queue),
      "AI DJ",
    ),
    #[cfg(feature = "ai-dj")]
    row(
      "Shift the vibe (drop the DJ's queued tracks and re-ask)",
      Binding(|k| k.dj_vibe_shift),
      "AI DJ",
    ),
    #[cfg(feature = "ai-dj")]
    row(
      "Toggle \"only tracks I do not already have\"",
      Binding(|k| k.dj_toggle_fresh_only),
      "AI DJ",
    ),
    #[cfg(feature = "ai-dj")]
    row(
      "Choose which AI and model the DJ uses",
      Binding(|k| k.dj_pick_model),
      "AI DJ",
    ),
  ]
}

/// The help rows the app can serve now, as `[description, key, context]`;
/// an unmet rebindable row stays, marked with the reason.
pub fn help_rows(app: &App) -> Vec<Vec<String>> {
  let keys = &app.user_config.keys;
  help_entries()
    .into_iter()
    .filter_map(|entry| {
      let availability = app.availability(entry.needs);
      let description = if availability.is_available() {
        entry.description.to_string()
      } else if matches!(entry.key, HelpKey::Literal(_)) {
        return None;
      } else {
        format!("{} ({})", entry.description, availability.hint()?)
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
    Action::OpenDiscover(_) => literal(ENTER, "Discover"),
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
  }
}

#[cfg(test)]
mod tests {
  use std::collections::BTreeSet;
  use std::path::{Path, PathBuf};

  use super::*;
  use crate::core::action::{DiscoverTarget, ListTarget, OpenTarget, RepeatSetting};
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
  fn a_connected_spotify_session_lists_every_row_it_can_serve_unmarked() {
    let app = App::default_connected();
    let rows = descriptions(&app);
    for entry in help_entries() {
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

  /// The variants no terminal gesture produces, in every build.
  const UNBOUND: &[&str] = &[
    "AddToQueue",
    "Back",
    "CloseScreen",
    "Notify",
    "NotifyError",
    "Pause",
    "Play",
    "QueueTracks",
    "Search",
    "SetDjVibe",
    "SetPlaybarSegment",
    "SetRepeat",
    "SetScreenContent",
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
      Action::OpenDiscover(DiscoverTarget::ArtistsMix),
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
      Action::CycleVisualizerStyle,
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
