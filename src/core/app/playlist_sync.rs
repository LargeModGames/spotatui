use super::*;

impl App {
  /// Claim the single sync slot; `false` when a run already owns it.
  pub fn begin_playlist_sync(&mut self) -> bool {
    if self.playlist_sync_in_flight {
      return false;
    }
    self.playlist_sync_in_flight = true;
    true
  }

  /// Release the slot and keep the run's report.
  pub fn finish_playlist_sync(&mut self, report: crate::core::playlist_sync::SyncReport) {
    self.playlist_sync_in_flight = false;
    self.playlist_sync_last_report = Some(report);
  }

  /// Replace the link snapshot the sync screen reads.
  pub fn set_playlist_sync_links(&mut self, links: Vec<crate::core::playlist_sync::Link>) {
    self.playlist_sync_links = links;
    self.view.playlist_sync_selected_link = self
      .view
      .playlist_sync_selected_link
      .min(self.playlist_sync_links.len().saturating_sub(1));
  }

  /// The links as the last run loaded them.
  pub fn playlist_sync_links(&self) -> &[crate::core::playlist_sync::Link] {
    &self.playlist_sync_links
  }

  /// The last finished run.
  pub fn playlist_sync_last_report(&self) -> Option<&crate::core::playlist_sync::SyncReport> {
    self.playlist_sync_last_report.as_ref()
  }

  /// Whether a run owns the slot right now.
  pub fn playlist_sync_in_flight(&self) -> bool {
    self.playlist_sync_in_flight
  }

  /// The link under the sync screen's cursor.
  pub fn selected_playlist_sync_link(&self) -> Option<&crate::core::playlist_sync::Link> {
    self
      .playlist_sync_links
      .get(self.view.playlist_sync_selected_link)
  }

  /// The playlist the open mirror picker is for.
  pub fn pending_playlist_sync_master(&self) -> Option<&crate::core::playlist_sync::Endpoint> {
    self.pending_playlist_sync_master.as_ref()
  }

  /// The sources the mirror picker offers: compiled in, able to sync, reachable
  /// from this session, not the master's own, and not already mirrored.
  pub fn playlist_sync_picker_sources(&self) -> Vec<Source> {
    let Some(master) = self.pending_playlist_sync_master.as_ref() else {
      return Vec::new();
    };
    let mirrored: Vec<Source> = self
      .playlist_sync_links
      .iter()
      .find(|link| {
        link.master.source == master.source && link.master.playlist_uri == master.playlist_uri
      })
      .map(|link| link.mirrors.iter().map(|m| m.endpoint.source).collect())
      .unwrap_or_default();
    let subsonic_configured = self
      .user_config
      .behavior
      .subsonic_url
      .as_deref()
      .is_some_and(|url| !url.trim().is_empty());
    Source::ALL
      .into_iter()
      .filter(|source| *source != master.source && source.supports_playlist_sync())
      .filter(|source| !mirrored.contains(source))
      .filter(|source| crate::infra::playlist_sync::missing_sync_feature(*source).is_none())
      .filter(|source| match source {
        Source::Spotify => self.spotify_connected,
        Source::Subsonic => subsonic_configured,
        Source::Qobuz | Source::YouTube | Source::Local | Source::Radio => true,
      })
      .collect()
  }

  /// The id the open remove-link confirm is for.
  pub fn pending_playlist_sync_remove(&self) -> Option<&str> {
    self.pending_playlist_sync_remove.as_deref()
  }

  /// `D` on the sync screen: confirm removing the highlighted link.
  pub fn begin_remove_playlist_sync_link(&mut self) {
    let Some(link) = self.selected_playlist_sync_link() else {
      self.set_status_message("No playlist link is highlighted", 4);
      return;
    };
    let (id, name) = (link.id.clone(), link.master.name.clone());
    self.clear_dialog_state();
    self.pending_playlist_sync_remove = Some(id);
    self.view.dialog = Some(name);
    self.push_navigation_stack(
      RouteId::Dialog,
      ActiveBlock::Dialog(DialogContext::RemovePlaylistSyncLinkConfirm),
    );
    self.set_current_route_state(
      Some(ActiveBlock::Dialog(
        DialogContext::RemovePlaylistSyncLinkConfirm,
      )),
      None,
    );
  }

  /// `m` on a sidebar playlist: open the mirror picker for it.
  pub fn begin_playlist_sync_picker(&mut self) {
    let Some(master) = self.selected_sidebar_playlist_endpoint() else {
      self.set_status_message("Highlight a playlist to mirror it", 4);
      return;
    };
    let availability = self.availability(Requirement::Capability(Capability::PlaylistSync));
    if let Some(hint) = availability.hint() {
      self.set_status_message(format!("Playlist sync: {hint}"), 4);
      return;
    }
    self.clear_dialog_state();
    self.pending_playlist_sync_master = Some(master);
    if self.playlist_sync_picker_sources().is_empty() {
      self.pending_playlist_sync_master = None;
      self.set_status_message("No other source can take a mirror of this playlist", 4);
      return;
    }
    self.push_navigation_stack(
      RouteId::Dialog,
      ActiveBlock::Dialog(DialogContext::PlaylistSyncPicker),
    );
    self.set_current_route_state(
      Some(ActiveBlock::Dialog(DialogContext::PlaylistSyncPicker)),
      None,
    );
  }

  /// The picker's Enter: hand the master and the chosen source to the runner.
  pub fn link_playlist_to(&mut self, mirror: Source) {
    let Some(master) = self.pending_playlist_sync_master.take() else {
      return;
    };
    self.set_status_message(
      format!("Mirroring {} onto {}", master.name, mirror.label()),
      8,
    );
    self.dispatch(IoEvent::LinkPlaylist(master, mirror));
  }
}

#[cfg(test)]
mod tests {
  use crate::core::app::test_support::*;
  use crate::core::playlist_sync::{Endpoint, Link, LinkReport, SyncReport};
  use crate::core::source::Source;

  fn endpoint(source: Source, uri: &str) -> Endpoint {
    Endpoint {
      source,
      playlist_uri: uri.to_string(),
      name: "Mine".to_string(),
    }
  }

  fn link(id: &str, uri: &str) -> Link {
    Link {
      id: id.to_string(),
      master: endpoint(Source::Spotify, uri),
      mirrors: Vec::new(),
    }
  }

  #[test]
  fn a_second_playlist_sync_cannot_begin_until_the_first_finishes() {
    let mut app = make_app_simple();

    assert!(app.begin_playlist_sync());
    assert!(!app.begin_playlist_sync());
    assert!(!app.begin_playlist_sync());

    app.finish_playlist_sync(SyncReport::default());
    assert!(app.begin_playlist_sync());
  }

  #[test]
  fn finishing_a_run_keeps_its_report() {
    let mut app = make_app_simple();
    assert!(app.playlist_sync_last_report().is_none());
    assert!(app.playlist_sync_links().is_empty());

    let report = SyncReport {
      links: vec![LinkReport::new("abc123", "Road Trip")],
      error: Some("429 rate limited".to_string()),
      dry_run: false,
    };
    assert!(app.begin_playlist_sync());
    app.finish_playlist_sync(report.clone());

    assert_eq!(app.playlist_sync_last_report(), Some(&report));
  }

  #[test]
  fn the_picker_offers_every_other_reachable_source() {
    let mut app = make_app_simple();
    app.spotify_connected = true;
    app.user_config.behavior.subsonic_url = Some("http://x".to_string());
    app.pending_playlist_sync_master = Some(endpoint(Source::Qobuz, "qobuz:playlist:9"));

    let sources = app.playlist_sync_picker_sources();

    assert!(!sources.contains(&Source::Qobuz));
    assert!(!sources.contains(&Source::Local));
    assert!(!sources.contains(&Source::Radio));
    assert!(sources.contains(&Source::Spotify));
    assert_eq!(
      sources.contains(&Source::Subsonic),
      cfg!(feature = "subsonic")
    );
    assert_eq!(
      sources.contains(&Source::YouTube),
      cfg!(feature = "youtube")
    );
  }

  #[test]
  fn the_picker_drops_spotify_without_a_session_and_subsonic_without_a_url() {
    let mut app = make_app_simple();
    app.spotify_connected = false;
    app.user_config.behavior.subsonic_url = None;
    app.pending_playlist_sync_master = Some(endpoint(Source::Qobuz, "qobuz:playlist:9"));

    let sources = app.playlist_sync_picker_sources();

    assert!(!sources.contains(&Source::Spotify));
    assert!(!sources.contains(&Source::Subsonic));
  }

  #[test]
  fn the_selected_link_follows_the_view_cursor() {
    let mut app = make_app_simple();
    app.set_playlist_sync_links(vec![
      link("aaa", "spotify:playlist:a"),
      link("bbb", "spotify:playlist:b"),
    ]);

    app.view.playlist_sync_selected_link = 1;
    assert_eq!(
      app
        .selected_playlist_sync_link()
        .map(|link| link.id.as_str()),
      Some("bbb")
    );

    app.view.playlist_sync_selected_link = 5;
    assert!(app.selected_playlist_sync_link().is_none());
  }
}
