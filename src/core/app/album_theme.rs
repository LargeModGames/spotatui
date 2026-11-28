use super::*;

impl App {
  /// The user's own theme: the restore target while an album-derived theme is
  /// applied (or fading out), otherwise the live theme. Settings rows must be
  /// built from this, never from the live theme, or album accents would leak
  /// into `custom_theme` and persist on the next settings save.
  #[cfg(feature = "art-decode")]
  pub fn user_theme(&self) -> crate::core::user_config::Theme {
    use crate::core::cover_theme::CoverThemeState;
    match self.cover_theme_state {
      CoverThemeState::Active { base } | CoverThemeState::Restoring { base } => base,
      CoverThemeState::Inactive => self.user_config.theme,
    }
  }

  #[cfg(not(feature = "art-decode"))]
  pub fn user_theme(&self) -> crate::core::user_config::Theme {
    self.user_config.theme
  }

  /// Whether an adaptive-theme fade is animating (drives the fast tick rate).
  #[cfg(feature = "art-decode")]
  pub fn theme_fade_active(&self) -> bool {
    self.theme_transition.is_some()
  }

  #[cfg(not(feature = "art-decode"))]
  pub fn theme_fade_active(&self) -> bool {
    false
  }

  /// Store freshly decoded cover art together with the palette extracted from
  /// it. The single entry point that keeps the adaptive theme in sync with the
  /// art: `None` for the palette fades back to the user's own theme.
  #[cfg(feature = "art-decode")]
  pub fn store_cover_art(
    &mut self,
    key: String,
    img: impl Into<std::sync::Arc<image::DynamicImage>>,
    palette: Option<crate::core::cover_theme::AlbumPalette>,
  ) {
    self.cover_art.note_fetch_finished(true);
    self.cover_art.store_decoded(key, img);
    match palette {
      Some(palette) => self.set_cover_art_palette(palette),
      None => self.clear_cover_art_palette(),
    }
  }

  /// Drop the stored cover art together with its palette; the adaptive theme
  /// follows the art and fades back to the user's own theme.
  #[cfg(feature = "art-decode")]
  pub fn clear_cover_art(&mut self) {
    self.cover_art.begin_clear();
    self.cover_art.clear();
    self.clear_cover_art_palette();
  }

  #[cfg(feature = "cover-art")]
  pub(crate) fn browse_preview_cover_art(&self) -> &crate::core::art::CoverArtStore {
    &self.browse_preview_cover_art
  }

  #[cfg(all(test, feature = "cover-art"))]
  pub(crate) fn browse_preview_cover_art_mut(&mut self) -> &mut crate::core::art::CoverArtStore {
    &mut self.browse_preview_cover_art
  }

  #[cfg(feature = "cover-art")]
  pub(crate) fn browse_preview_cover_art_available(&self) -> bool {
    self.browse_preview_cover_art.available()
  }

  #[cfg(feature = "art-decode")]
  pub(crate) fn cover_art_generation_is(&self, generation: u64) -> bool {
    self.desired_cover_art_generation == generation
  }

  #[cfg(feature = "art-decode")]
  pub(crate) fn begin_cover_art_fetch(
    &mut self,
    request: &crate::core::art::CoverArtRequest,
  ) -> u64 {
    use crate::core::art::CoverArtStatus;
    self.desired_cover_art_generation = self.desired_cover_art_generation.wrapping_add(1);
    self.desired_cover_art_key = Some(request.key().to_string());
    self.cover_art.begin_fetch(request.key());
    self.cover_art.status = CoverArtStatus::Loading;
    self.desired_cover_art_generation
  }

  #[cfg(feature = "art-decode")]
  pub(crate) fn keep_cover_art_request(&mut self, key: Option<String>) {
    self.desired_cover_art_key = key;
  }

  #[cfg(feature = "art-decode")]
  pub(crate) fn drop_cover_art_request(
    &mut self,
    clear: bool,
    status: crate::core::art::CoverArtStatus,
  ) {
    self.desired_cover_art_generation = self.desired_cover_art_generation.wrapping_add(1);
    self.desired_cover_art_key = None;
    if clear {
      self.clear_cover_art();
    }
    self.cover_art.status = status;
  }

  #[cfg(feature = "cover-art")]
  pub(crate) fn browse_preview_art_key_is_desired(&self, key: &str) -> bool {
    self.desired_browse_preview_cover_art_key.as_deref() == Some(key)
  }

  #[cfg(feature = "cover-art")]
  pub(crate) fn browse_preview_art_generation_is(&self, generation: u64) -> bool {
    self.desired_browse_preview_cover_art_generation == generation
  }

  /// A new song was highlighted: start the fade now, but leave the image in
  /// the store until the debounce has settled. The store's transition keeps
  /// the selection instant so the renderer never waits for the load.
  #[cfg(feature = "cover-art")]
  pub(crate) fn note_browse_preview_art_selection(
    &mut self,
    request: Option<&crate::core::art::CoverArtRequest>,
  ) {
    let key = request.map(|request| request.key());
    self.desired_browse_preview_cover_art_generation = self
      .desired_browse_preview_cover_art_generation
      .wrapping_add(1);
    self.desired_browse_preview_cover_art_key = key.map(str::to_string);
    if self.user_config.behavior.fade_song_selection_cover_art {
      if let Some(key) = key {
        self
          .browse_preview_cover_art
          .begin_transition_at(key, std::time::Instant::now());
      }
    }
  }

  #[cfg(feature = "cover-art")]
  pub(crate) fn note_browse_preview_art_key_changed(
    &mut self,
    key: Option<&str>,
    now: std::time::Instant,
  ) {
    self.desired_browse_preview_cover_art_generation = self
      .desired_browse_preview_cover_art_generation
      .wrapping_add(1);
    if self.user_config.behavior.fade_song_selection_cover_art {
      if let Some(key) = key {
        self.browse_preview_cover_art.begin_transition_at(key, now);
      }
    } else if !key.is_some_and(|key| self.browse_preview_cover_art.key() == Some(key)) {
      self.browse_preview_cover_art.note_fetch_finished(false);
      self.browse_preview_cover_art.snap();
      self.browse_preview_cover_art.clear();
    }
  }

  /// Load the settled preview selection. Unlike [`Self::note_browse_preview_art_selection`]
  /// this marks the fetch as loading but keeps any
  /// transition already started so the fade is not restarted by the load.
  #[cfg(feature = "cover-art")]
  pub(crate) fn select_browse_preview_art(
    &mut self,
    request: Option<&crate::core::art::CoverArtRequest>,
  ) {
    use crate::core::art::CoverArtStatus;
    self.desired_browse_preview_cover_art_generation = self
      .desired_browse_preview_cover_art_generation
      .wrapping_add(1);
    match request {
      None => {
        self.desired_browse_preview_cover_art_key = None;
        self.browse_preview_cover_art.status = CoverArtStatus::Unavailable;
      }
      Some(request) => {
        let key = request.key().to_string();
        self.desired_browse_preview_cover_art_key = Some(key.clone());
        if self.user_config.behavior.fade_song_selection_cover_art {
          self.browse_preview_cover_art.begin_fetch(&key);
        } else {
          self.browse_preview_cover_art.begin_clear();
          self.browse_preview_cover_art.clear();
        }
        self.browse_preview_cover_art.status = CoverArtStatus::Loading;
      }
    }
  }

  #[cfg(feature = "cover-art")]
  pub(crate) fn begin_browse_preview_art_fetch(
    &mut self,
    request: &crate::core::art::CoverArtRequest,
  ) -> u64 {
    use crate::core::art::CoverArtStatus;
    self.desired_browse_preview_cover_art_generation = self
      .desired_browse_preview_cover_art_generation
      .wrapping_add(1);
    self.desired_browse_preview_cover_art_key = Some(request.key().to_string());
    if !self.user_config.behavior.fade_song_selection_cover_art {
      self.browse_preview_cover_art.begin_clear();
      self.browse_preview_cover_art.clear();
    }
    self.browse_preview_cover_art.begin_fetch(request.key());
    self.browse_preview_cover_art.status = CoverArtStatus::Loading;
    self.desired_browse_preview_cover_art_generation
  }

  #[cfg(feature = "cover-art")]
  pub(crate) fn keep_browse_preview_art_request(&mut self, key: Option<String>) {
    self.desired_browse_preview_cover_art_key = key;
  }

  #[cfg(feature = "cover-art")]
  pub(crate) fn drop_browse_preview_art(&mut self, status: crate::core::art::CoverArtStatus) {
    self.desired_browse_preview_cover_art_generation = self
      .desired_browse_preview_cover_art_generation
      .wrapping_add(1);
    self.desired_browse_preview_cover_art_key = None;
    self.browse_preview_cover_art.begin_clear();
    self.browse_preview_cover_art.clear();
    self.browse_preview_cover_art.status = status;
  }

  #[cfg(feature = "cover-art")]
  pub(crate) fn store_browse_preview_art(
    &mut self,
    key: String,
    img: impl Into<std::sync::Arc<image::DynamicImage>>,
  ) {
    self.browse_preview_cover_art.note_fetch_finished(true);
    self.browse_preview_cover_art.store_decoded(key, img);
    self.browse_preview_cover_art.status = crate::core::art::CoverArtStatus::Loaded;
  }

  #[cfg(feature = "cover-art")]
  pub(crate) fn fail_browse_preview_art(&mut self) {
    use crate::core::art::CoverArtStatus;
    self.browse_preview_cover_art.note_fetch_finished(false);
    self.browse_preview_cover_art.begin_clear();
    self.browse_preview_cover_art.clear();
    self.browse_preview_cover_art.status = CoverArtStatus::Failed;
  }

  /// Store the palette extracted from freshly loaded cover art and, when
  /// Adaptive Theme is on, fade the UI accents toward it.
  #[cfg(feature = "art-decode")]
  fn set_cover_art_palette(&mut self, palette: crate::core::cover_theme::AlbumPalette) {
    self.cover_art_palette = Some(palette);
    self.apply_cover_theme();
  }

  /// Drop the stored palette and fade back to the user's own theme. A cheap
  /// no-op when nothing is applied, so it is safe on every tick of the
  /// "no art" path.
  #[cfg(feature = "art-decode")]
  fn clear_cover_art_palette(&mut self) {
    self.cover_art_palette = None;
    self.restore_cover_theme();
  }

  /// Fade toward the theme derived from the stored palette, capturing the
  /// user's theme as the restore base on first application.
  #[cfg(feature = "art-decode")]
  fn apply_cover_theme(&mut self) {
    use crate::core::cover_theme::{derive_theme, CoverThemeState};
    if !self.user_config.behavior.cover_art_theme {
      return;
    }
    let Some(palette) = self.cover_art_palette else {
      return;
    };
    // Re-applying while active or mid-restore keeps the original base; only
    // from Inactive is the live theme really the user's own (a fade-out in
    // flight leaves a blend in `user_config.theme` that must not be captured).
    let base = self.user_theme();
    self.cover_theme_state = CoverThemeState::Active { base };
    let target = derive_theme(&base, &palette);
    self.begin_theme_transition(target);
  }

  /// Fade back to the user's own theme, if an album theme is applied.
  #[cfg(feature = "art-decode")]
  fn restore_cover_theme(&mut self) {
    use crate::core::cover_theme::CoverThemeState;
    if let CoverThemeState::Active { base } = self.cover_theme_state {
      self.cover_theme_state = if self.begin_theme_transition(base) {
        CoverThemeState::Restoring { base }
      } else {
        CoverThemeState::Inactive
      };
    }
  }

  /// Start fading the live theme toward `target`. Returns false when the
  /// theme is already there (nothing to animate). A transition already headed
  /// to the same target is left to finish rather than restarted.
  #[cfg(feature = "art-decode")]
  fn begin_theme_transition(&mut self, target: crate::core::user_config::Theme) -> bool {
    use crate::core::cover_theme::ThemeTransition;
    if let Some(transition) = &self.theme_transition {
      if transition.target() == target {
        return true;
      }
    }
    if self.user_config.theme == target {
      self.theme_transition = None;
      return false;
    }
    self.theme_transition = Some(ThemeTransition::new(self.user_config.theme, target));
    true
  }

  /// Reconcile the adaptive theme after `apply_settings_changes` rewrote
  /// `user_config.theme` from the settings rows (which every save does, for
  /// every row). The rows carry the user's own colors, so that rewrite is the
  /// new base; what was actually on screen (`live_before`) is put back so the
  /// fade continues from there. `enabled_before` is the Adaptive Theme flag
  /// before the save, so a flip re-applies or restores.
  #[cfg(feature = "art-decode")]
  pub(super) fn reconcile_cover_theme_after_settings(
    &mut self,
    live_before: crate::core::user_config::Theme,
    enabled_before: bool,
  ) {
    use crate::core::cover_theme::CoverThemeState;
    match self.cover_theme_state {
      CoverThemeState::Inactive => {}
      previous => {
        let base = self.user_config.theme;
        self.cover_theme_state = CoverThemeState::Active { base };
        self.user_config.theme = live_before;
        self.theme_transition = None;
        if matches!(previous, CoverThemeState::Restoring { .. }) {
          self.restore_cover_theme();
        } else {
          self.apply_cover_theme();
        }
      }
    }
    if self.user_config.behavior.cover_art_theme != enabled_before {
      if self.user_config.behavior.cover_art_theme {
        self.apply_cover_theme();
      } else {
        self.restore_cover_theme();
      }
    }
  }
}

#[cfg(all(test, feature = "art-decode"))]
mod tests {
  use super::*;

  #[cfg(feature = "cover-art")]
  #[test]
  fn browse_preview_art_is_separate_from_playing_art_and_theme() {
    use crate::core::art::CoverArtStatus;
    use crate::core::cover_theme::AlbumPalette;

    let palette = AlbumPalette {
      primary: (200, 30, 40),
      secondary: (30, 60, 220),
    };
    let mut app = App::default();
    app.store_cover_art(
      "playing".into(),
      image::DynamicImage::new_rgb8(1, 1),
      Some(palette),
    );
    app.store_browse_preview_art("preview".into(), image::DynamicImage::new_rgb8(2, 2));
    app
      .browse_preview_cover_art_mut()
      .begin_fetch("https://example.test/missing.jpg");

    assert_eq!(app.cover_art.key(), Some("playing"));
    assert_eq!(app.browse_preview_cover_art().key(), Some("preview"));
    assert_eq!(app.cover_art_palette, Some(palette));
    assert!(app.browse_preview_cover_art().transition().is_some());

    app.fail_browse_preview_art();

    assert_eq!(app.cover_art.key(), Some("playing"));
    assert_eq!(app.cover_art_palette, Some(palette));
    assert_eq!(app.browse_preview_cover_art().key(), None);
    assert_eq!(
      app
        .browse_preview_cover_art()
        .transition()
        .map(|(key, _)| key),
      Some("")
    );
    assert_eq!(
      app.browse_preview_cover_art().status,
      CoverArtStatus::Failed
    );
  }

  #[cfg(feature = "cover-art")]
  #[test]
  fn clearing_playing_art_ends_the_transition() {
    let mut app = App::default();
    app.store_cover_art("playing".into(), image::DynamicImage::new_rgb8(2, 2), None);
    app
      .cover_art
      .begin_fetch("https://example.test/missing.jpg");
    assert!(app.cover_art.transition().is_some());

    app.clear_cover_art();

    assert_eq!(app.cover_art.key(), None);
    assert_eq!(app.cover_art.transition().map(|(key, _)| key), Some(""));
  }

  #[cfg(feature = "cover-art")]
  #[test]
  fn changing_the_preview_key_keeps_the_previous_art_while_loading() {
    use crate::core::art::CoverArtStatus;

    let mut app = App::default();
    app.store_browse_preview_art(
      "https://example.test/old.jpg".into(),
      image::DynamicImage::new_rgb8(2, 2),
    );
    assert_eq!(
      app.browse_preview_cover_art().key(),
      Some("https://example.test/old.jpg")
    );

    let request = crate::core::art::CoverArtRequest::Url("https://example.test/new.jpg".into());
    app.begin_browse_preview_art_fetch(&request);

    assert_eq!(
      app.browse_preview_cover_art().key(),
      Some("https://example.test/old.jpg")
    );
    assert_eq!(
      app.browse_preview_cover_art().status,
      CoverArtStatus::Loading
    );
  }

  #[cfg(feature = "cover-art")]
  #[test]
  fn selecting_uncached_preview_art_starts_the_transition_before_the_fetch() {
    use crate::core::art::CoverArtStatus;

    let mut app = App::default();
    app.store_browse_preview_art("old".into(), image::DynamicImage::new_rgb8(1, 1));
    let request =
      crate::core::art::CoverArtRequest::Url("https://example.test/select-uncached.jpg".into());
    let key = request.key();

    app.note_browse_preview_art_selection(Some(&request));

    assert_eq!(app.browse_preview_cover_art().key(), Some("old"));
    assert_eq!(app.browse_preview_cover_art().transition().unwrap().0, key);
    assert_eq!(
      app.browse_preview_cover_art().status,
      CoverArtStatus::Loaded
    );

    app.select_browse_preview_art(Some(&request));

    assert_eq!(app.browse_preview_cover_art().key(), Some("old"));
    assert_eq!(app.browse_preview_cover_art().transition().unwrap().0, key);
    assert_eq!(
      app.browse_preview_cover_art().status,
      CoverArtStatus::Loading
    );
  }

  #[cfg(feature = "cover-art")]
  #[test]
  fn a_changed_preview_key_invalidates_the_in_flight_fetch_without_snapping_the_fade() {
    let mut app = App::default();
    app.store_browse_preview_art("old".into(), image::DynamicImage::new_rgb8(1, 1));
    let request = crate::core::art::CoverArtRequest::Url("https://example.test/stale.jpg".into());
    let generation = app.begin_browse_preview_art_fetch(&request);
    let snap_generation = app.browse_preview_cover_art().snap_generation();

    app.note_browse_preview_art_selection(Some(&crate::core::art::CoverArtRequest::Url(
      "https://example.test/new.jpg".into(),
    )));

    assert!(!app.browse_preview_art_generation_is(generation));
    assert_eq!(app.browse_preview_cover_art().key(), Some("old"));
    assert_eq!(
      app.browse_preview_cover_art().transition().unwrap().0,
      "https://example.test/new.jpg"
    );
    assert_eq!(
      app.browse_preview_cover_art().snap_generation(),
      snap_generation
    );
  }

  #[cfg(feature = "cover-art")]
  #[test]
  fn a_changed_preview_key_snaps_when_fading_is_off() {
    let mut app = App::default();
    app.user_config.behavior.fade_song_selection_cover_art = false;
    app.store_browse_preview_art("old".into(), image::DynamicImage::new_rgb8(1, 1));
    let request = crate::core::art::CoverArtRequest::Url("https://example.test/stale.jpg".into());
    let generation = app.begin_browse_preview_art_fetch(&request);
    let snap_generation = app.browse_preview_cover_art().snap_generation();

    app.note_browse_preview_art_key_changed(
      Some("https://example.test/new.jpg"),
      std::time::Instant::now(),
    );

    assert!(!app.browse_preview_art_generation_is(generation));
    assert_eq!(app.browse_preview_cover_art().key(), None);
    assert!(app.browse_preview_cover_art().transition().is_none());
    assert_ne!(
      app.browse_preview_cover_art().snap_generation(),
      snap_generation
    );
  }

  #[cfg(feature = "cover-art")]
  #[test]
  fn a_newer_preview_fetch_supersedes_the_old_generation() {
    let mut app = App::default();
    let request = crate::core::art::CoverArtRequest::Url("https://example.test/same.jpg".into());
    let old_generation = app.begin_browse_preview_art_fetch(&request);
    let new_generation = app.begin_browse_preview_art_fetch(&request);

    assert_ne!(old_generation, new_generation);
    assert!(app.browse_preview_art_generation_is(new_generation));
    assert!(!app.browse_preview_art_generation_is(old_generation));
  }

  #[cfg(feature = "art-decode")]
  mod cover_theme_tests {
    use super::*;
    use crate::core::cover_theme::{derive_theme, AlbumPalette, CoverThemeState};
    use crate::core::user_config::ThemePreset;

    const PALETTE: AlbumPalette = AlbumPalette {
      primary: (200, 30, 40),
      secondary: (30, 60, 220),
    };

    fn app_with_adaptive_on() -> App {
      let mut app = App::default();
      app.user_config.behavior.cover_art_theme = true;
      app
    }

    #[test]
    fn palette_application_fades_in_and_clear_restores_the_user_theme() {
      let mut app = app_with_adaptive_on();
      let user = app.user_config.theme;

      app.set_cover_art_palette(PALETTE);
      assert!(matches!(
        app.cover_theme_state,
        CoverThemeState::Active { .. }
      ));
      assert!(app.theme_transition.is_some());

      // A tick longer than the fade completes it: accents changed, the
      // structural colors kept.
      app.update_on_tick(Duration::from_secs(2));
      assert!(app.theme_transition.is_none());
      assert_ne!(app.user_config.theme.active, user.active);
      assert_eq!(app.user_config.theme.text, user.text);
      assert_eq!(app.user_config.theme.background, user.background);

      app.clear_cover_art_palette();
      assert!(matches!(
        app.cover_theme_state,
        CoverThemeState::Restoring { .. }
      ));
      app.update_on_tick(Duration::from_secs(2));
      assert_eq!(app.user_config.theme, user);
      assert_eq!(app.cover_theme_state, CoverThemeState::Inactive);
    }

    #[test]
    fn palette_is_stored_but_not_applied_while_disabled() {
      let mut app = App::default();
      assert!(!app.user_config.behavior.cover_art_theme);
      let user = app.user_config.theme;

      app.set_cover_art_palette(PALETTE);

      assert_eq!(app.cover_theme_state, CoverThemeState::Inactive);
      assert!(app.theme_transition.is_none());
      assert_eq!(app.user_config.theme, user);
      assert_eq!(app.cover_art_palette, Some(PALETTE));
    }

    #[test]
    fn user_theme_reports_the_base_while_an_album_theme_is_applied() {
      let mut app = app_with_adaptive_on();
      let user = app.user_config.theme;
      app.set_cover_art_palette(PALETTE);
      app.update_on_tick(Duration::from_secs(2));

      assert_ne!(app.user_config.theme, user);
      assert_eq!(app.user_theme(), user);
    }

    #[test]
    fn settings_rewrite_becomes_the_new_base_and_rederives() {
      let mut app = app_with_adaptive_on();
      app.set_cover_art_palette(PALETTE);
      app.update_on_tick(Duration::from_secs(2));
      let displayed = app.user_config.theme;

      // What apply_settings_changes does on save: the arms rewrite the live
      // theme with the user's own colors (here: a new preset), then the
      // reconciliation runs.
      let new_user = ThemePreset::Dracula.to_theme();
      app.user_config.theme = new_user;
      app.reconcile_cover_theme_after_settings(displayed, true);

      assert!(
        matches!(app.cover_theme_state, CoverThemeState::Active { base } if base == new_user)
      );
      app.update_on_tick(Duration::from_secs(2));
      assert_eq!(app.user_config.theme, derive_theme(&new_user, &PALETTE));
      assert_eq!(app.user_theme(), new_user);
    }

    #[test]
    fn toggle_off_restores_and_toggle_on_reapplies_the_stored_palette() {
      let mut app = app_with_adaptive_on();
      let user = app.user_config.theme;
      app.set_cover_art_palette(PALETTE);
      app.update_on_tick(Duration::from_secs(2));

      // Toggle off the way a settings save does: through
      // apply_settings_changes, which resets the live theme to the base
      // before the arms run.
      app.settings_items = vec![SettingItem {
        id: "behavior.cover_art_theme".to_string(),
        name: "Adaptive Theme".to_string(),
        description: String::new(),
        value: SettingValue::Bool(false),
      }];
      app.apply_settings_changes();
      app.update_on_tick(Duration::from_secs(2));
      assert_eq!(app.user_config.theme, user);
      assert_eq!(app.cover_theme_state, CoverThemeState::Inactive);

      // Toggling back on recolors immediately from the stored palette.
      app.settings_items[0].value = SettingValue::Bool(true);
      app.apply_settings_changes();
      assert!(matches!(
        app.cover_theme_state,
        CoverThemeState::Active { .. }
      ));
      assert!(app.theme_transition.is_some());
    }

    #[test]
    fn a_theme_fade_bumps_the_theme_revision_on_each_changing_tick_until_it_completes() {
      let mut app = app_with_adaptive_on();
      app.set_cover_art_palette(PALETTE);
      let rev = app.display_revisions().get(DisplayDomain::Theme);

      app.update_on_tick(Duration::from_millis(400));
      assert_eq!(app.display_revisions().get(DisplayDomain::Theme), rev + 1);
      app.update_on_tick(Duration::from_secs(2));
      assert_eq!(app.display_revisions().get(DisplayDomain::Theme), rev + 2);
      app.update_on_tick(Duration::from_secs(2));
      assert_eq!(app.display_revisions().get(DisplayDomain::Theme), rev + 2);
    }

    #[test]
    fn save_from_another_category_keeps_the_base_theme() {
      let mut app = app_with_adaptive_on();
      let user = app.user_config.theme;
      app.set_cover_art_palette(PALETTE);
      app.update_on_tick(Duration::from_secs(2));
      assert_ne!(app.user_config.theme, user);

      // A save from the Behavior category: settings_items holds only that
      // category's rows, so no theme.* arm rewrites `user_config.theme` with
      // the user's own colors. The base must not drift to the blend.
      app.settings_items = vec![SettingItem {
        id: "behavior.seek_milliseconds".to_string(),
        name: "Seek Duration (ms)".to_string(),
        description: String::new(),
        value: SettingValue::Number(app.user_config.behavior.seek_milliseconds as i64),
      }];
      let theme_rev = app.display_revisions().get(DisplayDomain::Theme);
      app.apply_settings_changes();
      assert_eq!(app.display_revisions().get(DisplayDomain::Theme), theme_rev);

      assert_eq!(app.user_theme(), user, "base drifted to the blended theme");

      app.clear_cover_art_palette();
      app.update_on_tick(Duration::from_secs(2));
      assert_eq!(
        app.user_config.theme, user,
        "did not restore the user theme"
      );
    }

    #[test]
    fn track_change_during_restore_keeps_the_original_base() {
      let mut app = app_with_adaptive_on();
      let user = app.user_config.theme;
      app.set_cover_art_palette(PALETTE);
      app.update_on_tick(Duration::from_secs(2));

      // Art clears (fade-out starts) and new art arrives mid-fade: the base
      // captured must be the user's theme, not the half-restored blend.
      app.clear_cover_art_palette();
      app.update_on_tick(Duration::from_millis(100));
      assert!(matches!(
        app.cover_theme_state,
        CoverThemeState::Restoring { .. }
      ));
      app.set_cover_art_palette(AlbumPalette {
        primary: (30, 200, 90),
        secondary: (200, 30, 40),
      });
      match app.cover_theme_state {
        CoverThemeState::Active { base } => assert_eq!(base, user),
        other => panic!("expected Active, got {other:?}"),
      }
    }

    #[test]
    fn custom_theme_save_does_not_leak_album_analysis_bar_into_base() {
      let mut app = app_with_adaptive_on();
      app.user_config.current_preset = ThemePreset::Custom;
      app.user_config.custom_theme = app.user_config.theme;
      let user = app.user_config.theme;

      app.set_cover_art_palette(PALETTE);
      app.update_on_tick(Duration::from_secs(2));
      // analysis_bar is album-colored while applied, and has no settings row
      // in any category, so a save must not treat the live value as the
      // user's choice.
      assert_ne!(app.user_config.theme.analysis_bar, user.analysis_bar);

      // Settings -> Theme, save without changing anything.
      app.view.settings_category = crate::core::app::SettingsCategory::Theme;
      app.load_settings_for_category();
      app.apply_settings_changes();
      assert_eq!(app.user_theme().analysis_bar, user.analysis_bar);

      // Turning adaptive theming off (a real settings save) restores the
      // user's analysis_bar.
      app.settings_items = vec![SettingItem {
        id: "behavior.cover_art_theme".to_string(),
        name: "Adaptive Theme".to_string(),
        description: String::new(),
        value: SettingValue::Bool(false),
      }];
      app.apply_settings_changes();
      app.update_on_tick(Duration::from_secs(2));
      assert_eq!(
        app.user_config.theme.analysis_bar, user.analysis_bar,
        "album accent stuck in analysis_bar after adaptive theme off"
      );
    }
  }
}
