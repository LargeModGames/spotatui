//! Frontend-neutral, serializable UI messages. Add English fields first; each
//! translated instance inherits untranslated fields from its English instance.
pub mod gui;
pub mod help;
mod locale;
mod message;
pub(crate) use message::LocalizedMessage;
pub mod onboarding;
pub mod popups;
pub mod recap;
pub mod screens;
pub mod settings;
pub mod sidebar;
pub mod status_app;
pub mod status_infra;
pub mod status_runtime;
pub mod tools;
pub use locale::{system_language, Language};

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub struct Strings {
  pub recap: recap::RecapStrings,
  pub onboarding: onboarding::OnboardingStrings,
  pub popups: popups::PopupStrings,
  pub status: StatusStrings,
  pub gui: gui::GuiStrings,
  pub screens: screens::ScreenStrings,
  pub settings: settings::SettingsStrings,
  pub sidebar: sidebar::SidebarStrings,
  pub help: help::HelpStrings,
}

pub static EN: Strings = Strings {
  recap: recap::EN,
  onboarding: onboarding::EN,
  popups: popups::EN,
  status: StatusStrings {
    app: status_app::EN,
    infra: status_infra::EN,
    runtime: status_runtime::EN,
    tools: tools::EN,
  },
  gui: gui::EN,
  screens: screens::EN,
  settings: settings::EN,
  sidebar: sidebar::EN,
  help: help::EN,
};

// Keep ..EN so new English fields compile before their translations are added.
#[allow(clippy::needless_update)]
pub static ZH_HANS: Strings = Strings {
  recap: recap::ZH_HANS,
  onboarding: onboarding::ZH_HANS,
  popups: popups::ZH_HANS,
  status: StatusStrings {
    app: status_app::ZH_HANS,
    infra: status_infra::ZH_HANS,
    runtime: status_runtime::ZH_HANS,
    tools: tools::ZH_HANS,
  },
  gui: gui::ZH_HANS,
  screens: screens::ZH_HANS,
  settings: settings::ZH_HANS,
  sidebar: sidebar::ZH_HANS,
  help: help::ZH_HANS,
  ..EN
};

// Keep ..EN so new English fields compile before their translations are added.
#[allow(clippy::needless_update)]
pub static ZH_HANT: Strings = Strings {
  recap: recap::ZH_HANT,
  onboarding: onboarding::ZH_HANT,
  popups: popups::ZH_HANT,
  status: StatusStrings {
    app: status_app::ZH_HANT,
    infra: status_infra::ZH_HANT,
    runtime: status_runtime::ZH_HANT,
    tools: tools::ZH_HANT,
  },
  gui: gui::ZH_HANT,
  screens: screens::ZH_HANT,
  settings: settings::ZH_HANT,
  sidebar: sidebar::ZH_HANT,
  help: help::ZH_HANT,
  ..EN
};

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub struct StatusStrings {
  pub tools: tools::ToolStrings,
  pub app: status_app::AppStatusStrings,
  pub infra: status_infra::InfraStatusStrings,
  pub runtime: status_runtime::RuntimeStatusStrings,
}

impl Language {
  pub const fn strings(self) -> &'static Strings {
    match self {
      Self::ZhHans => &ZH_HANS,
      Self::ZhHant => &ZH_HANT,
      Self::Auto | Self::En => &EN,
    }
  }
}

/// Replace named placeholders once. Values are never reparsed, so a track
/// called "{count}" stays literal. Unknown placeholders remain visible, making
/// a contributor's typo inspectable; table tests validate placeholder parity.
pub fn format_template(template: &str, values: &[(&str, &str)]) -> String {
  let mut result = String::with_capacity(template.len());
  let mut tail = template;
  while let Some(start) = tail.find('{') {
    result.push_str(&tail[..start]);
    let Some(end) = tail[start + 1..].find('}').map(|end| start + 1 + end) else {
      result.push_str(&tail[start..]);
      return result;
    };
    let key = &tail[start + 1..end];
    match values.iter().find(|(name, _)| *name == key) {
      Some((_, value)) => result.push_str(value),
      None => result.push_str(&tail[start..=end]),
    }
    tail = &tail[end + 1..];
  }
  result.push_str(tail);
  result
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod template_tests;
