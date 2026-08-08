//! Network-lane handlers for the DJ.
//!
//! These run on the **serial** IoEvent lane, which is where the real Spotify
//! client lives. The service lane deliberately constructs its `Network` with
//! `None` for the client (`runtime.rs`), so anything that has to resolve a track
//! name to a URI belongs here rather than there.

use super::Network;
use crate::infra::dj::{library, DjLibrary, DjLine};

/// Helpers shared by both front doors.
impl Network {
  /// Serial lane: crawl the listener's own playlists for the avoid-library
  /// filter, and cache the result on `App`.
  ///
  /// Idempotent by way of `library_indexing`: the in-TUI toggle, the first turn,
  /// and an MCP `search_tracks` can all ask for this, and a second crawl would be
  /// pure cost.
  pub async fn dj_index_library(&mut self) {
    let already = {
      let mut app = self.app.lock().await;
      let already = app.dj.library_indexing || app.dj.library.is_some();
      if !already {
        app.dj.library_indexing = true;
      }
      already
    };
    if already {
      return;
    }

    let outcome = self.build_dj_library().await;
    let mut app = self.app.lock().await;
    app.dj.library_indexing = false;
    match outcome {
      Ok(library) => {
        let summary = library.summary();
        app.dj.library = Some(library);
        app.dj.push_line(DjLine::system(summary.clone()));
        app.set_status_message(format!("DJ: {summary}"), 5);
      }
      Err(e) => {
        log::warn!("DJ: could not index the library: {e}");
        let message = format!("DJ: could not read your playlists ({e}); recommending unfiltered");
        app.dj.push_line(DjLine::system(message.clone()));
        app.set_error_status_message(message, 8);
      }
    }
  }

  /// Run the crawl, resolving the listener's own account ID first.
  async fn build_dj_library(&self) -> anyhow::Result<DjLibrary> {
    if self.spotify.is_none() {
      anyhow::bail!("no Spotify session");
    }
    let cached = {
      self
        .app
        .lock()
        .await
        .user
        .as_ref()
        .map(|user| user.id.clone())
    };
    let owner_id = match cached {
      Some(id) => id,
      // Not fetched yet (or a source other than Spotify was browsed first). One
      // cheap call rather than filtering against nothing.
      None => {
        #[derive(serde::Deserialize)]
        struct Me {
          id: String,
        }
        self.spotify_get_typed::<Me>("me", &[]).await?.id
      }
    };
    library::build_index(self, &owner_id).await
  }
}
