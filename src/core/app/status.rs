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

  pub fn handle_error(&mut self, e: anyhow::Error) {
    info!("error occurred: {}", e);
    self.push_navigation_stack(RouteId::Error, ActiveBlock::Error);
    self.api_error = e.to_string();
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
