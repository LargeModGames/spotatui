use std::sync::OnceLock;

/// The community invite the Welcome page links to.
pub const DISCORD_URL: &str = "https://spotatui.com/discord";

static CHANGELOG: OnceLock<String> = OnceLock::new();

/// The changelog this binary was built with, as the Home page shows it.
pub fn changelog() -> &'static str {
  CHANGELOG
    .get_or_init(|| clean_changelog(include_str!("../../CHANGELOG.md"), !cfg!(debug_assertions)))
    .as_str()
}

/// LF line endings; a release build drops the `[Unreleased]` heading.
fn clean_changelog(changelog: &str, release: bool) -> String {
  let changelog = changelog.replace("\r\n", "\n");
  if release {
    changelog.replace("\n## [Unreleased]\n", "")
  } else {
    changelog
  }
}

/// An https link the Welcome page may open: the Discord invite or a link target in the changelog.
pub fn opens_link(url: &str) -> bool {
  opens_link_in(url, changelog())
}

fn opens_link_in(url: &str, changelog: &str) -> bool {
  url.starts_with("https://")
    && !url.contains(char::is_whitespace)
    && (url == DISCORD_URL || changelog.contains(&format!("]({url})")))
}

#[cfg(test)]
mod tests {
  use super::*;

  const SAMPLE: &str = "# Changelog\n\n## [Unreleased]\n\n### Added\n\n- **New** ([#1](https://github.com/LargeModGames/spotatui/issues/1)).\n\n## [v1.0.0] 2026-01-01\n";

  #[test]
  fn a_release_build_drops_the_unreleased_heading_and_a_debug_build_keeps_it() {
    let release = clean_changelog(SAMPLE, true);
    assert!(!release.contains("[Unreleased]"));
    assert!(release.contains("## [v1.0.0] 2026-01-01"));
    assert_eq!(clean_changelog(SAMPLE, false), SAMPLE);
  }

  #[test]
  fn a_crlf_checkout_is_cleaned_like_an_lf_one() {
    let crlf = SAMPLE.replace('\n', "\r\n");
    assert_eq!(clean_changelog(&crlf, true), clean_changelog(SAMPLE, true));
    assert_eq!(clean_changelog(&crlf, false), SAMPLE);
  }

  #[test]
  fn the_embedded_changelog_starts_with_its_title() {
    assert!(changelog().starts_with("# Changelog"));
  }

  #[test]
  fn only_the_discord_invite_and_changelog_link_targets_open() {
    assert!(opens_link_in(DISCORD_URL, SAMPLE));
    assert!(opens_link_in(
      "https://github.com/LargeModGames/spotatui/issues/1",
      SAMPLE
    ));
    assert!(!opens_link_in(
      "https://github.com/LargeModGames/spotatui/issues/2",
      SAMPLE
    ));
    assert!(!opens_link_in("https://github.com", SAMPLE));
    assert!(!opens_link_in("file:///etc/passwd", SAMPLE));
    assert!(!opens_link_in("http://spotatui.com/discord", SAMPLE));
  }
}
