use super::*;

#[derive(Clone, Copy, PartialEq, Debug)]
#[cfg_attr(feature = "gui", derive(serde::Serialize))]
#[cfg_attr(feature = "gui", serde(rename_all = "snake_case"))]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub enum AnnouncementLevel {
  Info,
  Warning,
  Critical,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Announcement {
  pub id: String,
  pub title: String,
  pub body: String,
  pub level: AnnouncementLevel,
  pub url: Option<String>,
  pub received_at: Instant,
}

/// How long a recorded error stays current before the tick retires it. Long
/// enough not to snatch the page away from someone reading it; short enough
/// that a frontend with no dismissal gesture does not latch the first failure
/// for the life of the process.
const API_ERROR_TTL: Duration = Duration::from_secs(60);

/// How long the retired message lingers in the status bar afterwards, before
/// `status_message_ttl_percent` scaling.
const API_ERROR_HANDOFF_TTL_SECS: u64 = 8;

impl App {
  #[allow(dead_code)]
  pub fn enqueue_announcements(&mut self, announcements: Vec<Announcement>) {
    if announcements.is_empty() {
      return;
    }

    let mut existing_ids: HashSet<String> = self
      .pending_announcements
      .iter()
      .map(|announcement| announcement.id.clone())
      .collect();

    if let Some(active) = &self.active_announcement {
      existing_ids.insert(active.id.clone());
    }

    let mut incoming = announcements
      .into_iter()
      .filter(|announcement| existing_ids.insert(announcement.id.clone()))
      .collect::<Vec<Announcement>>();

    if self.active_announcement.is_none() {
      if let Some(first) = incoming.first().cloned() {
        self.active_announcement = Some(first);
        self.announcement_batch_dismissed = 0;
        incoming.remove(0);
        self.display_revisions.bump(DisplayDomain::Announcement);
      }
    }

    if !incoming.is_empty() {
      self.display_revisions.bump(DisplayDomain::Announcement);
    }
    self.pending_announcements.extend(incoming);
  }

  /// Keep the fetched feed for the history and queue its announcements not seen yet.
  pub(crate) fn receive_announcement_feed(&mut self, feed: Vec<Announcement>) {
    let same = |a: &Announcement, b: &Announcement| {
      (&a.id, &a.title, &a.body, a.level, &a.url) == (&b.id, &b.title, &b.body, b.level, &b.url)
    };
    let unchanged = self.announcement_feed.len() == feed.len()
      && self
        .announcement_feed
        .iter()
        .zip(&feed)
        .all(|(a, b)| same(a, b));
    if !unchanged {
      self.announcement_feed = feed.clone();
      self.display_revisions.bump(DisplayDomain::Announcement);
    }

    let seen = &self.runtime_state.seen_announcement_ids;
    let unseen = feed
      .into_iter()
      .filter(|announcement| !seen.contains(&announcement.id))
      .collect();
    let had_active = self.active_announcement.is_some();
    self.enqueue_announcements(unseen);
    if !had_active && self.active_announcement.is_some() {
      self.push_navigation_stack(RouteId::AnnouncementPrompt, ActiveBlock::AnnouncementPrompt);
    }
  }

  /// The active announcement's place in its batch and the batch size, both from 1.
  #[cfg(feature = "gui")]
  pub(crate) fn announcement_position(&self) -> Option<(usize, usize)> {
    self.active_announcement.as_ref().map(|_| {
      let position = self.announcement_batch_dismissed + 1;
      (position, position + self.pending_announcements.len())
    })
  }

  /// The seen announcements still in the feed, in feed order.
  #[cfg(feature = "gui")]
  pub(crate) fn announcement_history(&self) -> impl Iterator<Item = &Announcement> {
    let seen = &self.runtime_state.seen_announcement_ids;
    self
      .announcement_feed
      .iter()
      .filter(move |announcement| seen.contains(&announcement.id))
  }

  pub fn dismiss_active_announcement(&mut self) -> Option<String> {
    let dismissed_id = self
      .active_announcement
      .take()
      .map(|announcement| announcement.id);
    if dismissed_id.is_some() {
      self.announcement_batch_dismissed += 1;
    }

    if let Some(next_announcement) = self.pending_announcements.first().cloned() {
      self.active_announcement = Some(next_announcement);
      self.pending_announcements.remove(0);
    } else {
      self.announcement_batch_dismissed = 0;
    }

    self.display_revisions.bump(DisplayDomain::Announcement);
    dismissed_id
  }

  /// Dismiss the active announcement if it has this id, mark it seen in
  /// `state.yml`, and close the prompt when no announcement waits.
  pub(crate) fn dismiss_announcement(&mut self, id: &str) {
    let is_active = self
      .active_announcement
      .as_ref()
      .is_some_and(|announcement| announcement.id == id);
    if is_active {
      if let Some(dismissed_id) = self.dismiss_active_announcement() {
        self.runtime_state.mark_announcement_seen(dismissed_id);
        let patch = crate::core::state::PersistedRuntimeState::announcements(
          &self.runtime_state.seen_announcement_ids,
          &self.runtime_state.dismissed_announcements,
        );
        if let Err(error) = self.save_runtime_state(&patch) {
          let message = self
            .strings()
            .status
            .app
            .announcement_save_failed(&error.to_string());
          self.handle_error_with_message(
            anyhow!("Failed to persist dismissed announcement: {}", error),
            message,
          );
        }
      }
    }

    if self.active_announcement.is_none()
      && self.get_current_route().active_block == ActiveBlock::AnnouncementPrompt
    {
      self.pop_navigation_stack();
    }
  }

  /// Open the link of the active or a fetched announcement with this id.
  pub(crate) fn open_announcement_url(&mut self, id: &str) {
    let url = self
      .active_announcement
      .iter()
      .chain(&self.announcement_feed)
      .find(|announcement| announcement.id == id)
      .and_then(|announcement| announcement.url.clone());
    if let Some(url) = url {
      self.open_external_url(&url);
    }
  }

  /// Open a URL the caller checked in the system browser.
  pub(crate) fn open_external_url(&mut self, url: &str) {
    if let Err(e) = open::that_detached(url) {
      log::warn!("failed to open {} in the browser: {}", url, e);
      let message = self.strings().status.app.link_open_failed(url);
      self.set_error_status_message(message, 6);
    }
  }

  /// The status-bar message that is live now, if any.
  pub fn status_message(&self) -> Option<&str> {
    self.status_message.as_deref()
  }

  /// Whether the live status message is an error.
  pub fn status_message_is_error(&self) -> bool {
    self.status_message_is_error
  }

  /// The live error text, or empty when none is live. The CLI's exit gate.
  pub fn api_error(&self) -> &str {
    &self.api_error
  }

  /// Drop the live status message and its error priority.
  pub(crate) fn clear_status_message(&mut self) {
    if self.status_message.take().is_some() {
      self.display_revisions.bump(DisplayDomain::Status);
    }
    self.status_message_expires_at = None;
    self.status_message_is_error = false;
  }

  pub fn set_status_message(&mut self, message: impl Into<String>, ttl_secs: u64) {
    // A live error message blocks normal messages from overwriting it.
    if self.status_message_is_error {
      if let (Some(_), Some(expires_at)) = (&self.status_message, self.status_message_expires_at) {
        if Instant::now() < expires_at {
          return;
        }
      }
    }
    self.status_message = Some(message.into());
    let ttl = self.scaled_status_ttl(ttl_secs);
    self.status_message_expires_at = Some(Instant::now() + Duration::from_secs(ttl));
    self.status_message_is_error = false;
    self.display_revisions.bump(DisplayDomain::Status);
  }

  /// Set an error status message. Errors always replace whatever is currently shown
  /// (including a previous error) and are styled distinctly in the UI.
  #[cfg_attr(not(feature = "scripting"), allow(dead_code))]
  pub fn set_error_status_message(&mut self, message: impl Into<String>, ttl_secs: u64) {
    self.status_message = Some(message.into());
    let ttl = self.scaled_status_ttl(ttl_secs);
    self.status_message_expires_at = Some(Instant::now() + Duration::from_secs(ttl));
    self.status_message_is_error = true;
    self.display_revisions.bump(DisplayDomain::Status);
  }

  /// Scale a status-message TTL by `status_message_ttl_percent` (default 100
  /// == 1.0×). Applied here at the single sink so the ~66 call sites keep
  /// their relative per-severity TTLs.
  fn scaled_status_ttl(&self, ttl_secs: u64) -> u64 {
    let pct = self.user_config.behavior.status_message_ttl_percent as u64;
    // round to nearest, never zero.
    ((ttl_secs * pct + 50) / 100).max(1)
  }

  /// Record an error and raise the `RouteId::Error` frame the terminal
  /// frontend draws full-screen. That frame is a presentation hint, not the
  /// state: another frontend may render [`Self::api_error`] however it likes.
  /// Every frontend dismisses through [`Self::clear_api_error`], and one that
  /// never dismisses gets the [`API_ERROR_TTL`] backstop on the tick.
  pub fn handle_error(&mut self, e: anyhow::Error) {
    let message = e.to_string();
    self.handle_error_with_message(e, message);
  }

  /// Keep the original diagnostic in the log while presenting a localized
  /// message through the same error route, lifetime, and display revision.
  pub fn handle_error_with_message(&mut self, e: anyhow::Error, message: String) {
    info!("error occurred: {}", e);
    self.push_navigation_stack(RouteId::Error, ActiveBlock::Error);
    self.api_error = message;
    self.api_error_expires_at = Some(Instant::now() + API_ERROR_TTL);
    self.display_revisions.bump(DisplayDomain::Status);
  }

  /// Dismiss the live error: the message, its lifetime, and every navigation
  /// frame still showing it. The single dismissal primitive, so a frontend
  /// with no route stack has one call to make. Dropping the frames is what
  /// stops a cleared message from leaving an error page with nothing on it.
  pub fn clear_api_error(&mut self) {
    if !self.api_error.is_empty() || self.api_error_expires_at.is_some() {
      self.display_revisions.bump(DisplayDomain::Status);
    }
    self.api_error.clear();
    self.api_error_expires_at = None;
    self.drop_error_routes();
  }

  /// Retire an error whose lifetime has passed. When the error frame is the
  /// screen the user is on, the text moves to the status bar on the way out so
  /// the page does not vanish leaving nothing behind. A frame that expired
  /// buried under something else goes quietly: a toast about a minute-old
  /// failure the user already navigated away from is noise.
  pub(super) fn expire_api_error(&mut self) {
    let Some(expires_at) = self.api_error_expires_at else {
      return;
    };
    if Instant::now() < expires_at {
      return;
    }
    let showing = self.get_current_route().active_block == ActiveBlock::Error;
    let message = std::mem::take(&mut self.api_error);
    self.clear_api_error();
    if showing && !message.is_empty() {
      self.set_error_status_message(message, API_ERROR_HANDOFF_TTL_SECS);
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::test_support::*;

  // --- status message priority tests ---

  #[test]
  fn normal_message_does_not_overwrite_live_error() {
    let mut app = make_app_simple();
    app.set_error_status_message("plugin error", 6);
    assert!(app.status_message_is_error);

    app.set_status_message("now playing", 4);

    assert_eq!(app.status_message.as_deref(), Some("plugin error"));
    assert!(app.status_message_is_error);
  }

  #[test]
  fn error_overwrites_normal_message() {
    let mut app = make_app_simple();
    app.set_status_message("now playing", 4);
    assert!(!app.status_message_is_error);

    app.set_error_status_message("plugin error", 6);

    assert_eq!(app.status_message.as_deref(), Some("plugin error"));
    assert!(app.status_message_is_error);
  }

  #[test]
  fn error_overwrites_previous_error() {
    let mut app = make_app_simple();
    app.set_error_status_message("first error", 6);
    app.set_error_status_message("second error", 6);

    assert_eq!(app.status_message.as_deref(), Some("second error"));
    assert!(app.status_message_is_error);
  }

  // --- error lifecycle tests ---

  #[test]
  fn raising_an_error_stamps_a_lifetime_on_it() {
    let mut app = make_app_simple();

    app.handle_error(anyhow!("boom"));

    assert_eq!(app.api_error, "boom");
    assert!(app.api_error_expires_at.is_some());
    assert_eq!(app.get_current_route().id, RouteId::Error);
    assert_eq!(app.get_current_route().active_block, ActiveBlock::Error);
  }

  #[test]
  fn dismissing_the_error_route_clears_the_message() {
    let mut app = make_app_simple();
    app.handle_error(anyhow!("boom"));

    app.pop_navigation_stack();

    assert!(app.api_error.is_empty());
    assert!(app.api_error_expires_at.is_none());
    assert_ne!(app.get_current_route().id, RouteId::Error);
  }

  // The error page's own hint tells the user to press `d`. Coming back from
  // the device picker must still show the message, which is why the clear is
  // keyed on the frame that was popped rather than on the new top.
  #[test]
  fn popping_a_screen_opened_from_the_error_route_keeps_the_message() {
    let mut app = make_app_simple();
    app.handle_error(anyhow!("boom"));
    app.push_navigation_stack(RouteId::SelectedDevice, ActiveBlock::SelectDevice);

    app.pop_navigation_stack();

    assert_eq!(app.api_error, "boom");
    assert_eq!(app.get_current_route().id, RouteId::Error);
  }

  #[test]
  fn dismissing_an_error_drops_a_duplicate_error_frame_left_lower_in_the_stack() {
    let mut app = make_app_simple();
    app.handle_error(anyhow!("first"));
    app.push_navigation_stack(RouteId::SelectedDevice, ActiveBlock::SelectDevice);
    // Pushes a SECOND error frame: pushes dedupe only against the top frame.
    app.handle_error(anyhow!("second"));

    app.pop_navigation_stack();

    assert!(app.api_error.is_empty());
    assert_eq!(app.get_current_route().id, RouteId::SelectedDevice);
  }

  #[test]
  fn dismissing_an_error_never_empties_the_navigation_stack() {
    let mut app = make_app_simple();
    app.handle_error(anyhow!("boom"));

    app.pop_navigation_stack();

    assert_eq!(app.get_current_route().id, RouteId::Home);
  }

  // A producer that rewrites the error frame in place leaves it holding
  // text-input focus; deleting it would drop keystrokes into the global
  // bindings. No such producer exists today; this pins the invariant.
  #[test]
  fn clearing_an_error_leaves_a_frame_repurposed_as_a_search_input_alone() {
    let mut app = make_app_simple();
    app.handle_error(anyhow!("boom"));
    app.set_current_route_state(Some(ActiveBlock::Input), Some(ActiveBlock::Input));

    app.clear_api_error();

    assert!(app.api_error.is_empty());
    assert_eq!(app.get_current_route().active_block, ActiveBlock::Input);
  }

  #[test]
  fn normal_message_accepted_after_error_expires() {
    let mut app = make_app_simple();
    app.set_error_status_message("plugin error", 6);

    // Simulate expiry by backdating the timestamp.
    app.status_message_expires_at = Some(Instant::now() - Duration::from_secs(1));

    app.set_status_message("now playing", 4);

    assert_eq!(app.status_message.as_deref(), Some("now playing"));
    assert!(!app.status_message_is_error);
  }

  fn status_rev(app: &App) -> u64 {
    app.display_revisions().get(DisplayDomain::Status)
  }

  fn route_rev(app: &App) -> u64 {
    app.display_revisions().get(DisplayDomain::Route)
  }

  #[test]
  fn a_status_write_bumps_the_status_revision_unless_a_live_error_blocks_it() {
    let mut app = make_app_simple();
    let start = status_rev(&app);

    app.set_status_message("now playing", 4);
    assert_eq!(status_rev(&app), start + 1);
    app.set_error_status_message("plugin error", 6);
    assert_eq!(status_rev(&app), start + 2);
    app.set_status_message("blocked", 4);
    assert_eq!(status_rev(&app), start + 2);
  }

  #[test]
  fn clearing_a_live_status_message_bumps_the_status_revision_once() {
    let mut app = make_app_simple();
    app.set_status_message("x", 4);
    let rev = status_rev(&app);

    app.clear_status_message();
    assert_eq!(status_rev(&app), rev + 1);
    app.clear_status_message();
    assert_eq!(status_rev(&app), rev + 1);
  }

  #[test]
  fn raising_and_dismissing_an_error_bump_status_and_route_once_each() {
    let mut app = make_app_simple();
    let (s0, r0) = (status_rev(&app), route_rev(&app));

    app.handle_error(anyhow::anyhow!("boom"));
    assert_eq!((status_rev(&app), route_rev(&app)), (s0 + 1, r0 + 1));
    app.clear_api_error();
    assert_eq!((status_rev(&app), route_rev(&app)), (s0 + 2, r0 + 2));
    app.clear_api_error();
    assert_eq!((status_rev(&app), route_rev(&app)), (s0 + 2, r0 + 2));
  }

  #[test]
  fn an_expired_error_on_screen_bumps_status_for_the_clear_and_the_handoff() {
    let mut app = make_app_simple();
    app.handle_error(anyhow::anyhow!("boom"));
    let (s0, r0) = (status_rev(&app), route_rev(&app));

    app.expire_api_error();
    assert_eq!((status_rev(&app), route_rev(&app)), (s0, r0));

    app.api_error_expires_at = Some(Instant::now() - Duration::from_secs(1));
    app.expire_api_error();
    assert_eq!((status_rev(&app), route_rev(&app)), (s0 + 2, r0 + 1));
  }
  #[test]
  fn localized_errors_keep_the_same_route_lifetime_and_dismissal() {
    let mut app = App::default();
    let before = app.display_revisions().get(DisplayDomain::Status);
    app.handle_error_with_message(
      anyhow!("Original diagnostic"),
      "无法播放：Original diagnostic".into(),
    );
    assert_eq!(app.api_error(), "无法播放：Original diagnostic");
    assert_eq!(app.get_current_route().id, RouteId::Error);
    assert!(app.api_error_expires_at.is_some());
    assert!(app.display_revisions().get(DisplayDomain::Status) > before);
    app.clear_api_error();
    assert!(app.api_error().is_empty());
    assert!(app.api_error_expires_at.is_none());
    assert_ne!(app.get_current_route().id, RouteId::Error);
  }
}
