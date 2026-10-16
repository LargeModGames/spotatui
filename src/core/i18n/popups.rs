//! Popup and dialog copy. User names, titles, paths, codes and URLs are inserted literally.

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub struct PopupStrings {
  pub resume_template: &'static str,
  pub now_playing: &'static str,
  pub queue_empty_template: &'static str,
  pub queue_up_next: &'static str,
  pub queue_title_template: &'static str,
  pub source_local: &'static str,
  pub api_response: &'static str,
  pub playback_check: &'static str,
  pub check_premium: &'static str,
  pub check_device: &'static str,
  pub check_spotifyd: &'static str,
  pub playback_hint: &'static str,
  pub error_back: &'static str,
  pub error_title: &'static str,
  pub log_hint: &'static str,
  pub log_file: &'static str,
  pub delete_playlist_question: &'static str,
  pub question_mark: &'static str,
  pub confirm: &'static str,
  pub remove_track_question: &'static str,
  pub track_template: &'static str,
  pub playlist_template: &'static str,
  pub remove_track: &'static str,
  pub fallback_terminal: &'static str,
  pub fallback_question: &'static str,
  pub save_shortcut_template: &'static str,
  pub save_shortcut_fallback: &'static str,
  pub remove_link_question: &'static str,
  pub mirrors_stay: &'static str,
  pub remove_link: &'static str,
  pub ok: &'static str,
  pub cancel: &'static str,
  pub add_track_title: &'static str,
  pub selected_track: &'static str,
  pub choose_playlist_template: &'static str,
  pub no_editable_playlists: &'static str,
  pub collaborative_playlist_template: &'static str,
  pub playlist_picker_controls_template: &'static str,
  pub mirror_playlist: &'static str,
  pub selected_playlist: &'static str,
  pub mirror_onto_template: &'static str,
  pub no_mirror_source: &'static str,
  pub mirror_picker_controls_template: &'static str,
  pub announcement_info: &'static str,
  pub announcement_warning: &'static str,
  pub announcement_critical: &'static str,
  pub announcement_more_template: &'static str,
  pub announcement_dismiss: &'static str,
  pub announcement_title: &'static str,
  pub recap_ready: &'static str,
  pub recap_listens_template: &'static str,
  pub recap_browser: &'static str,
  pub recap_controls: &'static str,
  pub recap_title: &'static str,
  pub community_pinned: &'static str,
  pub community_details: &'static str,
  pub community_add: &'static str,
  pub community_choice: &'static str,
  pub community_controls: &'static str,
  pub community_title: &'static str,
  pub exit_question: &'static str,
  pub exit_yes_no: &'static str,
  pub exit_controls: &'static str,
  pub exit_title: &'static str,
  pub sort_tracks: &'static str,
  pub sort_albums: &'static str,
  pub sort_artists: &'static str,
  pub sort_title: &'static str,
  pub sort_default: &'static str,
  pub sort_name: &'static str,
  pub sort_date_added: &'static str,
  pub sort_artist: &'static str,
  pub sort_duration: &'static str,
  pub sort_album: &'static str,
  pub sort_release_date: &'static str,
  pub party_code_prompt: &'static str,
  pub party_name_prompt: &'static str,
  pub party_join_enter: &'static str,
  pub party_name_required: &'static str,
  pub party_code_length_template: &'static str,
  pub party_name_length_template: &'static str,
  pub party_code_then_name: &'static str,
  pub esc_cancel: &'static str,
  pub listening_party: &'static str,
  pub connecting: &'static str,
  pub host_party: &'static str,
  pub join_party: &'static str,
  pub esc_close: &'static str,
  pub party_hosting: &'static str,
  pub generating: &'static str,
  pub share_code: &'static str,
  pub control_label: &'static str,
  pub control_host_only: &'static str,
  pub control_shared: &'static str,
  pub waiting_guests: &'static str,
  pub listener_one: &'static str,
  pub listeners_template: &'static str,
  pub party_toggle_control: &'static str,
  pub party_leave: &'static str,
  pub party_close_menu: &'static str,
  pub party_guest: &'static str,
  pub host_label: &'static str,
  pub room_label: &'static str,
  pub mode_label: &'static str,
  pub following_host: &'static str,
  pub party_hosting_title: &'static str,
  pub party_joined_title: &'static str,
  pub party_title: &'static str,
  pub plugin_close_hint: &'static str,
  pub fallback_session_only: &'static str,
  pub create_playlist_title: &'static str,
  pub playlist_name_label: &'static str,
  pub create_continue_hint: &'static str,
  pub add_tracks_title_template: &'static str,
  pub create_search_title: &'static str,
  pub unknown_artist: &'static str,
  pub create_results_title: &'static str,
  pub added_tracks_title_template: &'static str,
}

pub const EN: PopupStrings = PopupStrings {
  resume_template: "Resumes: {station}",
  now_playing: "Now playing: ",
  queue_empty_template: "Queue is empty — press {key} on a track to add it",
  queue_up_next: "Up next from context:",
  queue_title_template: "Queue  ({key} remove · J/K move · Enter play · Esc back)",
  source_local: "Local",
  api_response: "Api response: ",
  playback_check: "If you are trying to play a track, please check that",
  check_premium: " 1. You have a Spotify Premium Account",
  check_device: " 2. Your playback device is active and selected - press `d` to go to device selection menu",
  check_spotifyd: " 3. If you're using spotifyd as a playback device, your device name must not contain spaces",
  playback_hint: "Hint: a playback device must be either an official spotify client or a light weight alternative such as spotifyd",
  error_back: "\nPress <Esc> to return",
  error_title: "Error",
  log_hint: "Rerun with --debug for more detail. Read it before posting publicly.",
  log_file: "Log file: ",
  delete_playlist_question: "Are you sure you want to delete the playlist: ",
  question_mark: "?",
  confirm: "Confirm",
  remove_track_question: "Remove this track from playlist?",
  track_template: "Track: {name}",
  playlist_template: "Playlist: {name}",
  remove_track: "Remove Track",
  fallback_terminal: "Ctrl+, is not reported by this terminal stack.",
  fallback_question: "Use fallback shortcut for Open Settings?",
  save_shortcut_template: "Save as: {key}",
  save_shortcut_fallback: "Save Shortcut Fallback",
  remove_link_question: "Remove the playlist link for:",
  mirrors_stay: "The mirror playlists stay.",
  remove_link: "Remove Link",
  ok: "Ok",
  cancel: "Cancel",
  add_track_title: "Add Track To Playlist",
  selected_track: "Selected track",
  choose_playlist_template: "Choose a playlist for: {track}",
  no_editable_playlists: "No editable playlists available",
  collaborative_playlist_template: "{name} - {owner} (collab)",
  playlist_picker_controls_template: "Enter add/open | q cancel | {down}/{up} or arrows move | H/M/L jump",
  mirror_playlist: "Mirror Playlist",
  selected_playlist: "Selected playlist",
  mirror_onto_template: "Mirror \"{name}\" onto:",
  no_mirror_source: "No other source can take a mirror",
  mirror_picker_controls_template: "Enter mirror | q cancel | {down}/{up} or arrows move",
  announcement_info: "INFO",
  announcement_warning: "WARNING",
  announcement_critical: "CRITICAL",
  announcement_more_template: "More: {url}",
  announcement_dismiss: "[Press ENTER or ESC to dismiss]",
  announcement_title: " Announcement ",
  recap_ready: "Your monthly listening recap is ready! 🎉",
  recap_listens_template: "{count} listens made it into your 30-day recap.",
  recap_browser: "Open it in the browser to view and download your share card.",
  recap_controls: "[ENTER] Open   [ESC] Later   [d] Don't show this again",
  recap_title: " Monthly Recap ",
  community_pinned: "📌 spotatui community playlist pinned",
  community_details: "The \"spotatui community\" playlist is now pinned to the top of your Spotify playlists.",
  community_add: "Add songs to it any time with the /queue command in the spotatui Discord.",
  community_choice: "Keep it pinned, or hide it (you can re-enable it later in Settings).",
  community_controls: "[Enter] keep   [h] hide   (Esc keeps it)",
  community_title: " Community Playlist ",
  exit_question: "Exit spotatui?",
  exit_yes_no: "Press Y for Yes or N for No",
  exit_controls: "[ENTER = Yes, ESC = No]",
  exit_title: " Confirm Exit ",
  sort_tracks: "Sort Tracks",
  sort_albums: "Sort Albums",
  sort_artists: "Sort Artists",
  sort_title: "Sort",
  sort_default: "Default",
  sort_name: "Name",
  sort_date_added: "Date Added",
  sort_artist: "Artist",
  sort_duration: "Duration",
  sort_album: "Album",
  sort_release_date: "Release Date",
  party_code_prompt: "Enter 6-character party code:",
  party_name_prompt: "Enter your name:",
  party_join_enter: "Press Enter to join",
  party_name_required: "Type a display name to continue",
  party_code_length_template: "{count}/6 characters",
  party_name_length_template: "Name length: {count}/32",
  party_code_then_name: "Code fills first, then name input",
  esc_cancel: "Esc to cancel",
  listening_party: "Listening Party",
  connecting: "Connecting...",
  host_party: "Host a Party",
  join_party: "Join a Party",
  esc_close: "Esc to close",
  party_hosting: "Hosting Listening Party",
  generating: "Generating...",
  share_code: "Share this code: ",
  control_label: "Control: ",
  control_host_only: "Host Only",
  control_shared: "Shared Control",
  waiting_guests: "Waiting for guests...",
  listener_one: "1 listener:",
  listeners_template: "{count} listeners:",
  party_toggle_control: "c - toggle control mode",
  party_leave: "l - leave party",
  party_close_menu: "Esc to close menu",
  party_guest: "Listening Party (Guest)",
  host_label: "Host: ",
  room_label: "Room: ",
  mode_label: "Mode: ",
  following_host: "Following host playback",
  party_hosting_title: "Party (Hosting)",
  party_joined_title: "Party (Joined)",
  party_title: "Party",
  plugin_close_hint: "(Esc to close)",
  fallback_session_only: "Using Alt+, for this session only",
  create_playlist_title: "Create Playlist (Esc to cancel)",
  playlist_name_label: "Playlist name:",
  create_continue_hint: "Press Enter to continue, Esc to cancel",
  add_tracks_title_template: "Add Tracks to \"{name}\" (Enter=create, Tab=switch panel, Esc=cancel)",
  create_search_title: "Search (Enter to search)",
  unknown_artist: "Unknown",
  create_results_title: "Results (Enter to add)",
  added_tracks_title_template: "Added ({count}) — d=remove, Enter=create",
};

// Keep ..EN so new English fields compile before their translations are added.
#[allow(clippy::needless_update)]
pub const ZH_HANS: PopupStrings = PopupStrings { ..EN };

// Keep ..EN so new English fields compile before their translations are added.
#[allow(clippy::needless_update)]
pub const ZH_HANT: PopupStrings = PopupStrings { ..EN };

#[cfg(any(feature = "tui", test))]
impl PopupStrings {
  #[cfg(any(feature = "internet-radio", test))]
  pub fn resume(&self, station: &str) -> String {
    super::format_template(self.resume_template, &[("station", station)])
  }

  pub fn queue_empty(&self, key: &str) -> String {
    super::format_template(self.queue_empty_template, &[("key", key)])
  }

  pub fn queue_title(&self, key: &str) -> String {
    super::format_template(self.queue_title_template, &[("key", key)])
  }

  pub fn track(&self, name: &str) -> String {
    super::format_template(self.track_template, &[("name", name)])
  }

  pub fn playlist(&self, name: &str) -> String {
    super::format_template(self.playlist_template, &[("name", name)])
  }

  pub fn save_shortcut(&self, key: &str) -> String {
    super::format_template(self.save_shortcut_template, &[("key", key)])
  }

  pub fn choose_playlist(&self, track: &str) -> String {
    super::format_template(self.choose_playlist_template, &[("track", track)])
  }

  pub fn collaborative_playlist(&self, name: &str, owner: &str) -> String {
    super::format_template(
      self.collaborative_playlist_template,
      &[("name", name), ("owner", owner)],
    )
  }

  pub fn playlist_picker_controls(&self, down: &str, up: &str) -> String {
    super::format_template(
      self.playlist_picker_controls_template,
      &[("down", down), ("up", up)],
    )
  }

  pub fn mirror_onto(&self, name: &str) -> String {
    super::format_template(self.mirror_onto_template, &[("name", name)])
  }

  pub fn mirror_picker_controls(&self, down: &str, up: &str) -> String {
    super::format_template(
      self.mirror_picker_controls_template,
      &[("down", down), ("up", up)],
    )
  }

  pub fn announcement_more(&self, url: &str) -> String {
    super::format_template(self.announcement_more_template, &[("url", url)])
  }

  pub fn recap_listens(&self, count: usize) -> String {
    super::format_template(
      self.recap_listens_template,
      &[("count", &count.to_string())],
    )
  }

  pub fn party_code_length(&self, count: usize) -> String {
    super::format_template(
      self.party_code_length_template,
      &[("count", &count.to_string())],
    )
  }

  pub fn party_name_length(&self, count: usize) -> String {
    super::format_template(
      self.party_name_length_template,
      &[("count", &count.to_string())],
    )
  }

  pub fn listeners(&self, count: usize) -> String {
    super::format_template(self.listeners_template, &[("count", &count.to_string())])
  }

  pub fn add_tracks_title(&self, name: &str) -> String {
    super::format_template(self.add_tracks_title_template, &[("name", name)])
  }

  pub fn added_tracks_title(&self, count: usize) -> String {
    super::format_template(
      self.added_tracks_title_template,
      &[("count", &count.to_string())],
    )
  }

  pub fn sort_field(&self, field: crate::core::sort::SortField) -> &'static str {
    use crate::core::sort::SortField;
    match field {
      SortField::Default => self.sort_default,
      SortField::Name => self.sort_name,
      SortField::DateAdded => self.sort_date_added,
      SortField::Artist => self.sort_artist,
      SortField::Duration => self.sort_duration,
      SortField::Album => self.sort_album,
      SortField::ReleaseDate => self.sort_release_date,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn popup_templates_keep_english_and_literal_user_titles() {
    assert_eq!(EN.mirror_onto("{name}"), "Mirror \"{name}\" onto:");
    assert_eq!(
      EN.collaborative_playlist("{owner}", "{name}"),
      "{owner} - {name} (collab)"
    );

    assert_eq!(
      EN.queue_empty("a"),
      "Queue is empty — press a on a track to add it"
    );
    assert_eq!(
      EN.recap_listens(42),
      "42 listens made it into your 30-day recap."
    );
    assert_eq!(
      EN.added_tracks_title(3),
      "Added (3) — d=remove, Enter=create"
    );
  }
}
