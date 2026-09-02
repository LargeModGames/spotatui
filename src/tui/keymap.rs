//! The terminal's key surface: one table of help rows, each with the
//! [`Requirement`] it needs, filtered by the active source and the session so
//! the help menu never lists a key the session cannot serve.

use crate::core::app::App;
use crate::core::input::Key;
use crate::core::requirement::{Capability, Requirement};
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

#[cfg(test)]
mod tests {
  use super::*;

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
}
