use super::*;

impl App {
  /// One table per app; neither another instance nor a test can change it.
  pub fn strings(&self) -> &'static crate::core::i18n::Strings {
    self.strings
  }

  pub(crate) fn language(&self) -> crate::core::i18n::Language {
    if self.localize_messages {
      self
        .user_config
        .behavior
        .language
        .resolve(self.system_language)
    } else {
      crate::core::i18n::Language::En
    }
  }

  /// Re-select after a settings change. Persisted values remain locale-neutral.
  pub(crate) fn refresh_language(&mut self) {
    let strings = self.language().strings();
    if !std::ptr::eq(self.strings, strings) {
      self.strings = strings;
      self.view.help_menu_model = None;
      // Every derived payload can contain labels. A language change must also
      // resend cached pages such as playlist-sync results to another frontend.
      for domain in DisplayDomain::ALL {
        self.display_revisions.bump(domain);
      }
      // Stats contain generated count/duration prose. Rebuild it from local
      // history; raw track titles and artist names remain untouched.
      if self.stats_data.is_some() || self.stats_loading {
        self.stats_data = None;
        self.reload_stats();
      }
    }
  }

  /// Shared core/network diagnostics keep the CLI's English output contract.
  pub(crate) fn use_english_messages(&mut self) {
    self.localize_messages = false;
    self.refresh_language();
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::i18n::{Language, EN, ZH_HANS, ZH_HANT};

  #[test]
  fn language_changes_are_per_app_and_invalidate_the_settings_revision() {
    let mut app = App::default();
    let other = App::default();
    let before = app.display_revisions().get(DisplayDomain::Settings);
    app.user_config.behavior.language = Language::ZhHans;
    app.refresh_language();
    assert!(std::ptr::eq(app.strings(), &ZH_HANS));
    assert!(std::ptr::eq(other.strings(), &EN));
    assert!(app.display_revisions().get(DisplayDomain::Settings) > before);
    app.user_config.behavior.language = Language::ZhHant;
    app.refresh_language();
    assert!(std::ptr::eq(app.strings(), &ZH_HANT));
  }

  #[test]
  fn switching_language_refreshes_every_display_domain_only_when_it_changes() {
    let mut app = App::default();
    let before = app.display_revisions();
    app.user_config.behavior.language = Language::ZhHant;
    app.refresh_language();
    let after = app.display_revisions();
    for domain in DisplayDomain::ALL {
      assert_eq!(after.get(domain), before.get(domain) + 1);
    }
    app.refresh_language();
    for domain in DisplayDomain::ALL {
      assert_eq!(app.display_revisions().get(domain), after.get(domain));
    }
  }

  #[test]
  fn switching_language_reloads_pending_stats_in_the_new_language() {
    let (mut app, rx) = crate::core::app::test_support::session_free_app();
    app.reload_stats();
    assert!(matches!(rx.try_recv(), Ok(IoEvent::LoadListeningStats(_))));
    app.user_config.behavior.language = Language::ZhHans;
    app.refresh_language();
    assert!(matches!(rx.try_recv(), Ok(IoEvent::LoadListeningStats(_))));
    assert!(app.stats_loading());
  }

  #[test]
  fn auto_resolves_the_startup_locale_and_cli_keeps_english() {
    let mut app = App {
      system_language: Language::ZhHant,
      ..Default::default()
    };
    app.refresh_language();
    assert_eq!(app.language(), Language::ZhHant);
    app.use_english_messages();
    assert_eq!(app.language(), Language::En);
    assert_eq!(app.user_config.behavior.language, Language::Auto);
    app.user_config.behavior.language = Language::ZhHans;
    app.refresh_language();
    assert_eq!(app.language(), Language::En);
  }

  #[test]
  fn the_language_setting_applies_immediately_and_persists_its_raw_value() {
    use crate::core::action::{Action, ActionOutcome, SettingInput};
    use crate::core::user_config::UserConfigPaths;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yml");
    let mut app = App::default();
    app.user_config.path_to_config = Some(UserConfigPaths {
      config_file_path: path.clone(),
    });
    let result = app.apply(Action::SetSetting {
      id: "behavior.language".into(),
      value: SettingInput::Text("zh-Hant".into()),
    });
    assert!(matches!(
      result,
      ActionOutcome::SettingsSaved { saved: true }
    ));
    assert_eq!(app.language(), Language::ZhHant);
    let saved: serde_yaml::Value =
      serde_yaml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(saved["behavior"]["language"].as_str(), Some("zh-Hant"));
    let result = app.apply(Action::SetSetting {
      id: "behavior.language".into(),
      value: SettingInput::Text("fr".into()),
    });
    assert!(matches!(
      result,
      ActionOutcome::SettingsSaved { saved: false }
    ));
    assert_eq!(app.language(), Language::ZhHant);
  }
}
