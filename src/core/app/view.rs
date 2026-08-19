use super::*;

/// Presentation state of the terminal frontend, grouped on [`App::view`].
///
/// Everything in here is a cursor, a scroll offset, an edit buffer, a focus
/// marker, a popup flag or the terminal geometry: state that tells a frontend
/// *where the user is looking*, not what the player or the library holds. It
/// lives on `App` today so the existing handlers and draw functions keep one
/// argument, but nothing outside the TUI should depend on it. Producers that
/// replace a list (the network layer, a source dispatcher, a script effect)
/// still reset or clamp a few of these cursors directly; the
/// `view_writes_outside_tui` ratchet in `src/gates.rs` counts those writes and
/// exists to burn them down to zero.
///
/// Add a field here only when it is presentation state. A pending operation or
/// a value another frontend would also need belongs on `App` itself.
#[derive(Default)]
pub struct ViewState {
  /// Terminal geometry as of the last frame. The small/large search limits
  /// derive from it.
  pub size: Viewport,
  /// What the terminal can report about key presses, probed at startup and
  /// consulted when the rebind flow validates a capture.
  pub terminal_input_caps: TerminalInputCapabilities,

  // Inputs:
  // input is the string for input;
  // input_idx is the index of the cursor in terms of character;
  // input_cursor_position is the sum of the width of characters preceding the cursor.
  // Reason for this complication is due to non-ASCII characters, they may
  // take more than 1 bytes to store and more than 1 character width to display.
  pub input: Vec<char>,
  pub input_idx: usize,
  pub input_cursor_position: u16,
  pub input_context: InputContext,
  /// Horizontal scroll offset for the input box, computed during rendering.
  pub input_scroll_offset: Cell<u16>,

  // Sidebar, library and table cursors
  pub home_scroll: u16,
  pub selected_playlist_index: Option<usize>,
  pub active_playlist_index: Option<usize>,
  pub saved_album_tracks_index: usize,
  pub album_list_index: usize,
  pub artists_list_index: usize,
  pub shows_list_index: usize,
  pub episode_list_index: usize,
  pub queue_selected_index: usize,
  /// Cursor in the Local Files folder list.
  pub local_playlists_index: usize,

  // The `d` device/source picker
  pub selected_device_index: Option<usize>,
  /// Cursor within the Source panel of the `d` picker (index into [`Source::ALL`]).
  pub source_list_index: usize,
  /// Which panel of the `d` picker currently has focus.
  pub source_device_focus: SourceFocus,

  // Help menu
  pub help_docs_size: u32,
  pub help_menu_page: u32,
  pub help_menu_max_lines: u32,
  pub help_menu_offset: u32,
  /// Text filter applied to the rows in the Help menu.
  pub help_filter: String,
  /// Whether typed keys are currently editing [`Self::help_filter`].
  pub help_filter_editing: bool,
  /// Formatted Help rows for the current width/keys/filter; `None` until the
  /// Help menu is first prepared for rendering.
  pub help_menu_model: Option<HelpMenuModel>,

  // Modal dialogs
  /// Which modal dialog is open, if any.
  pub dialog: Option<String>,
  /// The yes/no cursor of the open dialog.
  pub confirm: bool,

  /// Scroll/browse state for the lyrics view.
  pub lyrics_view: LyricsViewState,

  // Settings screen state
  pub settings_category: SettingsCategory,
  pub settings_selected_index: usize,
  pub settings_edit_mode: bool,
  pub settings_edit_buffer: String,
  /// Fuzzy filter over the current category's rows. Survives a tab switch, so
  /// one query can be walked across categories.
  pub settings_filter: String,
  /// Whether typed keys are currently editing [`Self::settings_filter`].
  pub settings_filter_editing: bool,
  pub settings_unsaved_prompt_visible: bool,
  /// Yes/no cursor of the unsaved-changes prompt. Set to `true` whenever the
  /// prompt opens; only read while the prompt is visible.
  pub settings_unsaved_prompt_save_selected: bool,

  // Discover and Stats screens
  /// Selected index in the Discover view
  pub discover_selected_index: usize,
  /// Time range for Top Tracks
  pub discover_time_range: DiscoverTimeRange,
  /// Selected index in the Stats screen's Top Tracks list
  pub stats_selected_track: usize,

  // Sort menu state
  /// Whether the sort menu popup is visible
  pub sort_menu_visible: bool,
  /// Currently selected sort option in the menu
  pub sort_menu_selected: usize,
  /// Current sort context (what we're sorting)
  pub sort_context: Option<SortContext>,

  // Animation
  /// Animation frame counter for the "Liked" heart flash effect (0-10)
  pub liked_song_animation_frame: Option<u8>,
  /// Global animation tick counter, incremented every tick.
  pub animation_tick: u64,

  // Listening party inputs
  /// Input buffer for the party join code
  pub party_input: Vec<char>,
  /// Cursor position in party code input
  pub party_input_idx: usize,
  /// Input buffer for the required party guest name
  pub party_join_name: Vec<char>,

  // Add-to-playlist picker dialog
  /// Selected playlist index in the add-to-playlist picker dialog
  pub playlist_picker_selected_index: usize,
  /// Folder ID the add-to-playlist picker dialog is viewing (0 = root)
  pub playlist_picker_folder_id: usize,

  // Friends screen state
  /// Cursor position in the friends list
  pub friend_selected_index: usize,
  /// Active filter (All / Online)
  pub friend_filter: FriendFilter,
  /// Inline search / filter input on the Friends screen
  pub friend_search_input: Vec<char>,
  /// Whether the "Add Friend" overlay dialog is open
  pub friend_add_dialog_visible: bool,
  /// Which tab is active inside the add-friend dialog
  pub friend_add_mode: FriendAddMode,
  /// Input buffer for the "add by friend code" text field
  pub friend_add_input: Vec<char>,
  /// Input buffer for the "search by username" text field in the add dialog
  pub friend_user_search_input: Vec<char>,
  /// Selected row in the user-search results list
  pub friend_user_search_selected: usize,

  // Create Playlist form state
  pub create_playlist_name: Vec<char>,
  pub create_playlist_name_idx: usize,
  pub create_playlist_name_cursor: u16,
  pub create_playlist_stage: CreatePlaylistStage,
  pub create_playlist_search_input: Vec<char>,
  pub create_playlist_search_idx: usize,
  pub create_playlist_search_cursor: u16,
  pub create_playlist_selected_result: usize,
  pub create_playlist_focus: CreatePlaylistFocus,

  // Plugin surfaces
  /// Vertical scroll for the focused plugin screen.
  pub plugin_screen_scroll: u16,
  /// Scroll offset for the plugin popup.
  pub plugin_popup_scroll: u16,
}
