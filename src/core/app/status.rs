use super::*;

#[derive(Clone, Copy, PartialEq, Debug)]
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
        incoming.remove(0);
      }
    }

    self.pending_announcements.extend(incoming);
  }

  pub fn dismiss_active_announcement(&mut self) -> Option<String> {
    let dismissed_id = self
      .active_announcement
      .take()
      .map(|announcement| announcement.id);

    if let Some(next_announcement) = self.pending_announcements.first().cloned() {
      self.active_announcement = Some(next_announcement);
      self.pending_announcements.remove(0);
    }

    dismissed_id
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
  }

  /// Set an error status message. Errors always replace whatever is currently shown
  /// (including a previous error) and are styled distinctly in the UI.
  #[cfg_attr(not(feature = "scripting"), allow(dead_code))]
  pub fn set_error_status_message(&mut self, message: impl Into<String>, ttl_secs: u64) {
    self.status_message = Some(message.into());
    let ttl = self.scaled_status_ttl(ttl_secs);
    self.status_message_expires_at = Some(Instant::now() + Duration::from_secs(ttl));
    self.status_message_is_error = true;
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
    info!("error occurred: {}", e);
    self.push_navigation_stack(RouteId::Error, ActiveBlock::Error);
    self.api_error = e.to_string();
    self.api_error_expires_at = Some(Instant::now() + API_ERROR_TTL);
  }

  /// Dismiss the live error: the message, its lifetime, and every navigation
  /// frame still showing it. The single dismissal primitive, so a frontend
  /// with no route stack has one call to make. Dropping the frames is what
  /// stops a cleared message from leaving an error page with nothing on it.
  pub fn clear_api_error(&mut self) {
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

  // The search key rewrites the error frame in place instead of pushing, so
  // the frame can be holding text-input focus. Deleting it there would drop
  // the user's next keystrokes into the global bindings.
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
}
