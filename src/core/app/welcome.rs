use super::*;

impl App {
  /// The listening streak and its note for the Home page; `None` without a streak.
  pub fn welcome_streak(&self) -> Option<(String, String)> {
    let streaks = self.listening_streaks.as_ref()?;
    if streaks.current_days == 0 {
      return None;
    }
    let screens = &self.strings().screens;
    let note = if streaks.today_ms > 0 {
      screens.listened_today(&crate::infra::history::format_duration_with(
        streaks.today_ms,
        self.strings(),
      ))
    } else {
      screens.streak_keep.to_string()
    };
    Some((screens.listening_streak(streaks.current_days), note))
  }

  /// The global song counter line in its current state.
  pub fn global_counter_line(&self) -> String {
    let screens = &self.strings().screens;
    if !cfg!(feature = "telemetry") {
      return screens.global_counter_build_disabled.to_string();
    }
    if !self.user_config.behavior.enable_global_song_count {
      return screens.global_counter_disabled.to_string();
    }
    match self.global_song_count {
      Some(count) => screens.global_counter(&count.to_string()),
      None if self.global_song_count_failed => screens.global_counter_unavailable.to_string(),
      None => screens.global_counter_loading.to_string(),
    }
  }

  /// Open a Welcome page link in the system browser; any other target is ignored.
  pub(crate) fn open_changelog_link(&mut self, url: &str) {
    if !crate::core::changelog::opens_link(url) {
      log::warn!("ignored a Welcome page link that is not in the changelog");
      return;
    }
    self.open_external_url(url);
  }
}

#[cfg(test)]
mod tests {
  use crate::core::app::test_support::*;
  use crate::infra::history::StreakSummary;

  #[test]
  fn the_welcome_streak_names_today_or_asks_to_keep_it() {
    let mut app = make_app_simple();
    assert_eq!(app.welcome_streak(), None);
    app.listening_streaks = Some(StreakSummary {
      current_days: 3,
      longest_days: 5,
      today_ms: 0,
    });
    assert_eq!(
      app.welcome_streak(),
      Some((
        "3-day listening streak".to_string(),
        " · listen today to keep it!".to_string()
      ))
    );
    app.listening_streaks = Some(StreakSummary {
      current_days: 0,
      longest_days: 5,
      today_ms: 0,
    });
    assert_eq!(app.welcome_streak(), None);
  }

  #[test]
  fn the_global_counter_line_follows_the_setting_and_the_fetch() {
    let mut app = make_app_simple();
    app.user_config.behavior.enable_global_song_count = false;
    let disabled = app.global_counter_line();
    app.user_config.behavior.enable_global_song_count = true;
    let loading = app.global_counter_line();
    app.global_song_count_failed = true;
    let failed = app.global_counter_line();
    app.global_song_count = Some(42);
    let counted = app.global_counter_line();
    if cfg!(feature = "telemetry") {
      assert_eq!(
        disabled,
        "Global song counter disabled (Settings -> Behavior)."
      );
      assert_eq!(loading, "Loading global song count...");
      assert_eq!(failed, "Global song counter unavailable right now.");
      assert_eq!(counted, "Global songs played with spotatui: 42");
    } else {
      assert_eq!(counted, app.strings().screens.global_counter_build_disabled);
    }
  }
}
