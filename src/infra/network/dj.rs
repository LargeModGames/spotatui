//! Network-lane handlers for the DJ.
//!
//! These run on the **serial** IoEvent lane, which is where the real Spotify
//! client lives. The service lane deliberately constructs its `Network` with
//! `None` for the client (`runtime/`), so anything that has to resolve a track
//! name to a URI belongs here rather than there.

use super::Network;
#[cfg(any(feature = "mcp-server", feature = "ai-dj"))]
use crate::core::action::{Action, ActionOutcome};
use crate::core::plugin_api::TrackInfo;
use crate::infra::dj::resolve::{self, ResolveReport};
#[cfg(any(feature = "mcp-server", feature = "ai-dj"))]
use crate::infra::dj::tools::{DjToolCall, QueueItem, ToolOutcome};
#[cfg(any(feature = "mcp-server", feature = "ai-dj"))]
use crate::infra::dj::MAX_BATCH;
use crate::infra::dj::{library, DjLibrary, DjLine, DjSuggestion};
use crate::infra::network::IoEvent;
use rspotify::model::track::FullTrack;
use serde::Deserialize;
#[cfg(any(feature = "mcp-server", feature = "ai-dj"))]
use serde_json::json;
use std::collections::HashSet;
#[cfg(any(feature = "mcp-server", feature = "ai-dj"))]
use tokio::sync::oneshot;

#[derive(Deserialize, Debug)]
struct TracksResponse {
  tracks: Vec<FullTrack>,
}

/// The tool handlers behind `DjToolCall`, shared by both front doors.
#[cfg(any(feature = "mcp-server", feature = "ai-dj"))]
impl Network {
  /// Run a DJ tool call that needs the catalogue, and answer the waiting caller.
  ///
  /// The responder is a `oneshot`, so every path must send exactly once —
  /// dropping it silently would leave an MCP client waiting out its timeout with
  /// no idea why.
  pub async fn run_dj_tool_call(
    &mut self,
    call: DjToolCall,
    responder: oneshot::Sender<ToolOutcome>,
  ) {
    let outcome = self.dj_tool_outcome(call).await;
    // The receiver is gone if the MCP client hung up mid-call; that is normal,
    // not an error worth surfacing.
    let _ = responder.send(outcome);
  }

  async fn dj_tool_outcome(&mut self, call: DjToolCall) -> ToolOutcome {
    // This variant bypasses the generic auth gate so we can say something
    // useful here instead of the caller seeing a dropped channel.
    if self.spotify.is_none() {
      return ToolOutcome::error(
        "spotatui has no Spotify session, so the catalogue is unavailable. Log in from the \
         spotatui UI, then try again.",
      );
    }

    match call {
      DjToolCall::SearchTracks { query, limit } => self.dj_search(&query, limit).await,
      DjToolCall::QueueTracks {
        items,
        exclude_owned,
        extra_skip_keys,
      } => self.dj_queue(items, exclude_owned, extra_skip_keys).await,
      DjToolCall::PlayNow { uri } => self.dj_play_now(uri).await,
      // Everything else is answered without the network, before the event is
      // ever dispatched (`tools::execute_app_only`).
      other => ToolOutcome::error(format!(
        "{} does not need the catalogue and should not have been routed here",
        other.tool_name()
      )),
    }
  }

  async fn dj_search(&self, query: &str, limit: usize) -> ToolOutcome {
    let params = vec![
      ("q", query.to_string()),
      ("type", "track".to_string()),
      ("limit", limit.to_string()),
      ("offset", "0".to_string()),
    ];
    let response = match self
      .spotify_get_typed::<crate::infra::network::search::TrackSearchResponse>("search", &params)
      .await
    {
      Ok(response) => response,
      // Reported as an execution error, not the modal error page: an agent can
      // act on this, and a failed search must not hijack the user's screen.
      Err(e) => return ToolOutcome::error(format!("Search failed: {e}")),
    };

    let tracks: Vec<TrackInfo> = response
      .tracks
      .items
      .iter()
      .filter(|track| track.id.is_some())
      .map(TrackInfo::from)
      .collect();

    if tracks.is_empty() {
      return ToolOutcome::ok(format!("No tracks found for \"{query}\""));
    }

    // Ownership rides along with the results rather than behind a separate tool:
    // an agent already searches to confirm a track exists before queueing it, so
    // this arrives exactly when it is choosing, with no extra round trip. Marking
    // beats filtering here — the agent can still queue something the listener
    // owns when that is genuinely the right answer.
    let ownership = self.track_ownership(&tracks).await;
    let owned = |track: &TrackInfo| {
      track
        .id
        .as_ref()
        .is_some_and(|id| ownership.owned.contains(id))
    };

    let mut rendered = tracks
      .iter()
      .map(|track| {
        format!(
          "{} — {} [{}] [{}]",
          track.name,
          track.artists.join(", "),
          track.uri.as_deref().unwrap_or("no uri"),
          if owned(track) { "owned" } else { "new" }
        )
      })
      .collect::<Vec<_>>()
      .join("\n");
    if !ownership.complete {
      // Said out loud rather than passed off as complete: a silently partial
      // answer is how this feature would look broken.
      rendered.push_str(
        "\n\nNote: the playlist index is still being built, so [owned] reflects Liked Songs only \
         for this search. Search again for the complete answer.",
      );
    }

    let structured = json!({
      "ownership_complete": ownership.complete,
      "tracks": tracks.iter().map(|track| json!({
        "title": track.name,
        "artist": track.artists.join(", "),
        "album": track.album,
        "uri": track.uri,
        "duration_ms": track.duration_ms,
        "owned": owned(track),
      })).collect::<Vec<_>>()
    });
    ToolOutcome::with_data(rendered, structured)
  }

  async fn dj_queue(
    &mut self,
    items: Vec<QueueItem>,
    exclude_owned: bool,
    extra_skip_keys: Vec<String>,
  ) -> ToolOutcome {
    // Split the two input shapes: exact URIs are looked up as a batch, names go
    // through the fuzzy resolver that drops anything it cannot confidently match.
    let mut uris = Vec::new();
    let mut named: Vec<DjSuggestion> = Vec::new();
    for item in items {
      match item {
        QueueItem::Uri(uri) => uris.push(uri),
        QueueItem::Named(suggestion) => named.push(suggestion),
      }
    }

    // The one path that crawls inline. `exclude_owned` is a guarantee the caller
    // opted into, so a cold index has to be paid for here rather than deferred
    // the way `search_tracks` defers it — and a crawl that fails is refused
    // outright, because queueing unfiltered would silently break the contract
    // while looking like success.
    //
    // Deliberately before the resolve rather than after it: the name gate lives
    // *inside* `resolve_suggestions` and needs these keys to skip a search it can
    // already rule out. The cost of ordering it this way is one wasted crawl in
    // the case where nothing resolves at all, once per process.
    let library_keys = if exclude_owned {
      match self.dj_library_index().await {
        Some(library) => library.keys,
        None => {
          return ToolOutcome::error(
            "Could not read your playlists, so exclude_owned cannot be honoured and nothing was \
             queued. Try again in a moment, or call queue_tracks without exclude_owned to queue \
             these anyway.",
          )
        }
      }
    } else {
      HashSet::new()
    };

    let mut report = ResolveReport::default();
    if !uris.is_empty() {
      let (resolved, missing) = self.tracks_for_uris(&uris).await;
      report.resolved.extend(resolved);
      report.unresolved.extend(missing);
    }

    if !named.is_empty() {
      let skip_keys = {
        let app = self.app.lock().await;
        let mut keys = app.dj_skip_keys();
        keys.extend(extra_skip_keys);
        keys
      };
      let youtube = self.dj_youtube_resolver().await;
      // The library set is empty unless the caller asked for `exclude_owned`: an
      // agent told to queue a named track was told to queue *that* track, and
      // dropping it because the listener owns it would answer a question nobody
      // asked. When it did ask, this is the cheap gate — it rejects on the name
      // before a catalogue search is paid for.
      let named_report = resolve::resolve_suggestions(
        self,
        &named,
        &resolve::SkipSets {
          session: &skip_keys,
          library: &library_keys,
        },
        MAX_BATCH.saturating_sub(report.resolved.len()),
        youtube.as_ref(),
      )
      .await;
      report.resolved.extend(named_report.resolved);
      report.unresolved.extend(named_report.unresolved);
      report.duplicates.extend(named_report.duplicates);
      report.in_library.extend(named_report.in_library);
    }

    // The exact gate, on the resolved ID. It catches what the name gate cannot —
    // a title written differently enough to normalise apart — and it is the only
    // gate that sees the `uri` entries at all, which never went through a name
    // lookup. Nothing is substituted for a rejected track: the caller named
    // these, so coming back short is the honest answer.
    if exclude_owned {
      self.reject_owned_tracks(&mut report).await;
    }

    if report.resolved.is_empty() {
      return ToolOutcome::error(format!(
        "Nothing was queued.\n{}",
        nothing_queued_detail(&report)
      ));
    }

    let queued_labels: Vec<String> = report
      .resolved
      .iter()
      .map(|track| format!("{} — {}", track.name, track.artists.join(", ")))
      .collect();

    // Read before the take: `summary()` counts `resolved`, and taking it first is
    // what made every queue report "queued 0".
    let summary = report.summary();
    let accepted = {
      let mut app = self.app.lock().await;
      match app.apply(Action::QueueTracks(std::mem::take(&mut report.resolved))) {
        ActionOutcome::Queued { accepted } => accepted,
        ActionOutcome::Applied | ActionOutcome::SettingsSaved { .. } => 0,
      }
    };
    // After the block, not inside it: the network layer's helper takes the lock
    // itself. "DJ:", not "MCP:", because this handler serves both front doors and
    // `DjToolCall` does not carry which one asked.
    self.show_status_message(format!("DJ: {summary}"), 5).await;

    let mut text = format!("Queued {accepted} track(s):\n{}", queued_labels.join("\n"));
    // Report the misses explicitly: MCP clients feed execution detail back to
    // the model, which is how it learns not to invent that track again.
    if !report.unresolved.is_empty() {
      text.push_str(&format!(
        "\n\nNot found (skipped): {}",
        report.unresolved.join("; ")
      ));
    }
    if !report.in_library.is_empty() {
      // Its own line, and its own words. "Already queued" is false for a track
      // they own and never queued, and "not found" would tell the model a real
      // track does not exist — which is how it learns the wrong lesson.
      text.push_str(&format!(
        "\nAlready in your library (skipped by exclude_owned): {}",
        report.in_library.join("; ")
      ));
    }
    if !report.duplicates.is_empty() {
      text.push_str(&format!(
        "\nAlready queued, playing, or recently played (skipped): {}",
        report.duplicates.join("; ")
      ));
    }
    ToolOutcome::with_data(
      text,
      json!({
        "queued": queued_labels,
        "not_found": report.unresolved,
        "in_library": report.in_library,
        "duplicates": report.duplicates,
      }),
    )
  }

  /// Interrupt playback with one track, after confirming it exists.
  ///
  /// On the network lane rather than the app-only lane purely for that
  /// confirmation. Dispatching straight to `StartPlayback` is what the caller
  /// ultimately wants, but doing it blind meant a URI the catalogue does not
  /// have stopped playback while the tool reported it was playing — the one
  /// answer an agent cannot recover from, because it has no reason to re-check.
  ///
  /// Deliberately not filtered by `exclude_owned`: the caller asked for this
  /// exact track right now, and owning it is not a reason to refuse.
  async fn dj_play_now(&self, uri: String) -> ToolOutcome {
    // Only Spotify URIs can be checked from here. The opaque schemes are minted
    // by their own sources and have no catalogue to look them up in, so they go
    // through with wording that does not overclaim.
    if !uri.starts_with("spotify:track:") {
      // Well-formed is not the same as playable *here*: the router that would
      // consume this scheme is gated on its own feature, and without it the event
      // reaches nothing while the caller is told playback started.
      if let Some(feature) = crate::core::queue::missing_source_feature(&uri) {
        return ToolOutcome::error(format!(
          "Nothing is playing that: this spotatui was built without the `{feature}` feature, so it \
           has no source that can play {uri}. Playback was left alone."
        ));
      }
      self.dj_start_playback(&uri).await;
      return ToolOutcome::ok(format!("Sent {uri} to the player"));
    }

    let (resolved, _missing) = self.tracks_for_uris(std::slice::from_ref(&uri)).await;
    let Some(track) = resolved.into_iter().next() else {
      return ToolOutcome::error(format!(
        "Nothing is playing that: {uri} is not in the catalogue, so playback was left alone. \
         Use search_tracks to get a URI that exists."
      ));
    };

    let label = format!("{} — {}", track.name, track.artists.join(", "));
    self.dj_start_playback(&uri).await;
    ToolOutcome::with_data(
      format!("Playing {label}"),
      json!({"uri": uri, "title": track.name, "artist": track.artists.join(", ")}),
    )
  }

  async fn dj_start_playback(&self, uri: &str) {
    let mut app = self.app.lock().await;
    app.apply(Action::PlayUris {
      uris: vec![uri.to_string()],
      offset: Some(0),
    });
  }

  /// Whether the listener already has each of these search results.
  ///
  /// The two halves cost wildly different amounts, so they are treated
  /// differently. Liked Songs is one exact `me/tracks/contains` for the whole
  /// page, so it is always checked. Playlists have no such lookup — the only way
  /// to know is the crawl — so this reads the cached index and, when it is cold,
  /// *asks* for the crawl as its own event instead of running it here.
  ///
  /// That deferral is the point. `search_tracks` is what an agent calls between
  /// deciding and queueing, and the crawl is seconds of pagination on the serial
  /// lane; running it inline would stall every other event behind an agent's
  /// search, which is exactly the bug `ai_dj::open` exists to avoid in the TUI. So
  /// the first search of a cold session answers from Liked Songs alone and says
  /// so, and every search after it is complete.
  async fn track_ownership(&self, tracks: &[TrackInfo]) -> Ownership {
    let ids: Vec<String> = tracks.iter().filter_map(|track| track.id.clone()).collect();
    if ids.is_empty() {
      return Ownership {
        owned: HashSet::new(),
        complete: true,
      };
    }

    let mut owned = library::liked_among(self, &ids).await;
    let playlist_ids = {
      let app = self.app.lock().await;
      app.dj.library.as_ref().map(|library| library.ids.clone())
    };

    match playlist_ids {
      Some(playlist_ids) => {
        owned.extend(ids.into_iter().filter(|id| playlist_ids.contains(id)));
        Ownership {
          owned,
          complete: true,
        }
      }
      None => {
        self.request_library_index().await;
        Ownership {
          owned,
          complete: false,
        }
      }
    }
  }

  /// Ask for the playlist crawl on the serial lane, unless it has already run or
  /// is running.
  ///
  /// A channel send, never a call. This runs *on* the serial lane, so the crawl
  /// queues behind the tool call that asked for it rather than extending it; the
  /// channel is unbounded, so the send cannot block the handler it is inside.
  async fn request_library_index(&self) {
    let app = self.app.lock().await;
    if app.dj.library.is_some() || app.dj.library_indexing {
      return;
    }
    match app.io_tx_clone() {
      Some(sender) => {
        let _ = sender.send(IoEvent::DjIndexLibrary);
      }
      // Shutdown only, and unreachable in practice: `App::new` always sets the
      // channel, `close_io_channel` is the sole path that clears it, and
      // `mcp::spawn_listener` refuses to start without it — so an MCP call cannot
      // arrive before it exists.
      None => log::debug!("DJ: no event channel, so the library index was not requested"),
    }
  }
}

/// Why a `queue_tracks` call queued nothing at all.
///
/// Every bucket gets named, and each in its own words. Folding "already yours"
/// into "could not find" would teach the model that a real track does not exist,
/// and folding it into "already queued" describes something that never happened.
/// One reason per line, matching the success path's layout. Deliberately not
/// `"; "` between reasons: that is already the separator *within* a bucket, and a
/// track label is `"Title — Artist"`, so neither semicolons nor dashes can
/// separate the groups unambiguously.
#[cfg(any(feature = "mcp-server", feature = "ai-dj"))]
fn nothing_queued_detail(report: &ResolveReport) -> String {
  let mut reasons = Vec::new();
  if !report.unresolved.is_empty() {
    reasons.push(format!("Could not find: {}", report.unresolved.join("; ")));
  }
  if !report.in_library.is_empty() {
    reasons.push(format!(
      "Already in your library: {}",
      report.in_library.join("; ")
    ));
  }
  if !report.duplicates.is_empty() {
    reasons.push(format!(
      "Already queued, playing, or recently played: {}",
      report.duplicates.join("; ")
    ));
  }
  if reasons.is_empty() {
    return "Every track was already queued or playing.".to_string();
  }
  reasons.join("\n")
}

/// What `search_tracks` was able to work out about ownership for one page of
/// results.
#[cfg(any(feature = "mcp-server", feature = "ai-dj"))]
struct Ownership {
  /// Track IDs the listener already has.
  owned: HashSet<String>,
  /// Whether both halves were consulted. `false` means the playlist index was
  /// still cold and only Liked Songs was checked — reported to the caller rather
  /// than passed off as a complete answer.
  complete: bool,
}

/// Helpers shared by both front doors.
impl Network {
  /// Look up exact Spotify URIs so the queue shows real titles rather than raw
  /// URIs. Non-Spotify URIs are queued as-is, since only their own source can
  /// describe them.
  #[cfg_attr(not(any(feature = "mcp-server", feature = "ai-dj")), allow(dead_code))]
  async fn tracks_for_uris(&self, uris: &[String]) -> (Vec<TrackInfo>, Vec<String>) {
    let mut resolved = Vec::new();
    let mut missing = Vec::new();

    let spotify_ids: Vec<&str> = uris
      .iter()
      .filter_map(|uri| uri.strip_prefix("spotify:track:"))
      .collect();

    if !spotify_ids.is_empty() {
      let ids = spotify_ids.join(",");
      match self
        .spotify_get_typed::<TracksResponse>("tracks", &[("ids", ids)])
        .await
      {
        Ok(response) => resolved.extend(response.tracks.iter().map(TrackInfo::from)),
        Err(e) => {
          log::debug!("DJ: batch track lookup failed: {e}");
          missing.extend(spotify_ids.iter().map(|id| format!("spotify:track:{id}")));
        }
      }
    }

    for uri in uris.iter().filter(|uri| !uri.starts_with("spotify:track:")) {
      if uri.starts_with("radio:") {
        // A live stream is not a finite track; `add_track_to_native_queue`
        // rejects these, so report rather than silently dropping.
        missing.push(format!("{uri} (a radio stream, not a track)"));
        continue;
      }
      // Only the schemes we can actually play get through. The rest — free
      // text, `https://open.spotify.com/...` links, `spotify:album:` and
      // friends — used to be fabricated into a `TrackInfo` named after the raw
      // string and reported as queued, which told the caller its request had
      // succeeded and left an unplayable row in the user's queue.
      //
      // Reported with the reason attached rather than a bare "could not find":
      // these are the wrong *kind* of URI, so a retry with different spelling
      // cannot help, and the caller should be able to tell that apart from a
      // track that genuinely is not in the catalogue.
      if !crate::core::queue::is_playable_track_uri(uri) {
        missing.push(format!("{uri} (not a playable track URI)"));
        continue;
      }
      // Same silent-drop class as `play_now`: the scheme is right, but this build
      // has no router that would consume it, so queueing it would report success
      // for a row that can never play.
      if let Some(feature) = crate::core::queue::missing_source_feature(uri) {
        missing.push(format!(
          "{uri} (this spotatui was built without the `{feature}` feature)"
        ));
        continue;
      }
      resolved.push(TrackInfo {
        uri: Some(uri.clone()),
        name: uri.clone(),
        artists: Vec::new(),
        album: String::new(),
        duration_ms: 0,
        id: None,
        album_id: None,
        artist_refs: Vec::new(),
        is_playable: true,
        is_local: uri.starts_with("file:"),
        track_number: 0,
        explicit: false,
        image_url: None,
      });
    }

    (resolved, missing)
  }

  /// Build the YouTube fallback resolver, when that source is compiled in.
  async fn dj_youtube_resolver(&self) -> Option<resolve::YouTubeResolver> {
    #[cfg(feature = "youtube")]
    {
      Some(resolve::YouTubeResolver::new(&self.app).await)
    }
    #[cfg(not(feature = "youtube"))]
    {
      None
    }
  }

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

  /// The avoid-library index, crawling for it inline if it is not cached yet.
  ///
  /// `None` means the crawl itself failed, and the two callers want opposite
  /// things from that: the in-TUI DJ carries on unfiltered, because handing back
  /// an empty queue is worse than handing back a track you own, while
  /// `queue_tracks(exclude_owned)` refuses, because it was asked for a guarantee.
  ///
  /// Costs the caller a few seconds of pagination when it is cold, which is why
  /// every path that can warm it earlier does: `ai_dj::open`, the in-TUI toggle,
  /// and `search_tracks` all dispatch `DjIndexLibrary` instead of waiting for
  /// this.
  async fn dj_library_index(&mut self) -> Option<DjLibrary> {
    if let Some(library) = { self.app.lock().await.dj.library.clone() } {
      return Some(library);
    }
    self.dj_index_library().await;
    self.app.lock().await.dj.library.clone()
  }

  /// Drop resolved tracks the listener already owns, moving them to the
  /// `in_library` bucket.
  ///
  /// This is the exact gate, and it catches what the name-based one cannot: the
  /// title was written differently enough to normalise apart, but search landed
  /// on the very copy sitting in their playlist. Two sources of truth: the
  /// crawled playlist IDs, and one `me/tracks/contains` call for Liked Songs,
  /// which needs no index at all.
  async fn reject_owned_tracks(&mut self, report: &mut ResolveReport) {
    let ids: Vec<String> = report
      .resolved
      .iter()
      .filter_map(|track| track.id.clone())
      .collect();
    if ids.is_empty() {
      return;
    }

    let playlist_ids = {
      let app = self.app.lock().await;
      app
        .dj
        .library
        .as_ref()
        .map(|library| library.ids.clone())
        .unwrap_or_default()
    };
    let liked = library::liked_among(self, &ids).await;

    let mut kept = Vec::with_capacity(report.resolved.len());
    for track in std::mem::take(&mut report.resolved) {
      let owned = track
        .id
        .as_ref()
        .is_some_and(|id| liked.contains(id) || playlist_ids.contains(id));
      if owned {
        report
          .in_library
          .push(format!("{} — {}", track.name, track.artists.join(", ")));
      } else {
        kept.push(track);
      }
    }
    report.resolved = kept;
  }
}

/// Tests for the shared tool handlers, which exist under either front door.
#[cfg(all(test, any(feature = "mcp-server", feature = "ai-dj")))]
mod tests {
  use super::*;
  use crate::core::app::App;
  use crate::core::config::ClientConfig;
  use std::sync::Arc;
  use tokio::sync::Mutex;

  fn temp_token_cache_path() -> std::path::PathBuf {
    std::env::temp_dir().join("spotatui-dj-test-token-cache.json")
  }

  async fn unauthenticated_network() -> Network {
    let app = Arc::new(Mutex::new(App::default()));
    Network::new(None, ClientConfig::new(), &app, temp_token_cache_path())
  }

  #[tokio::test]
  async fn an_unauthenticated_call_answers_instead_of_dropping_the_channel() {
    // The whole reason this event bypasses the auth gate: the caller is an MCP
    // client waiting on a oneshot, and a dropped channel tells it nothing.
    let mut network = unauthenticated_network().await;
    let (tx, rx) = oneshot::channel();
    network
      .run_dj_tool_call(
        DjToolCall::SearchTracks {
          query: "nude".into(),
          limit: 5,
        },
        tx,
      )
      .await;
    let outcome = rx.await.expect("the responder must always be used");
    assert!(outcome.is_error);
    assert!(outcome.text.contains("no Spotify session"));
  }

  #[tokio::test]
  async fn app_only_calls_routed_here_by_mistake_are_reported_not_ignored() {
    let mut network = unauthenticated_network().await;
    let (tx, rx) = oneshot::channel();
    // Reaches the auth check first in this fixture, which still answers; the
    // point is that the responder is always consumed.
    network.run_dj_tool_call(DjToolCall::SkipTrack, tx).await;
    assert!(rx.await.is_ok());
  }

  #[tokio::test]
  async fn uris_that_are_not_tracks_are_reported_rather_than_fabricated() {
    // These reach no network: the point is that they never become a TrackInfo.
    // They used to be turned into one named after the raw string, pushed into
    // the user's queue as unplayable rows, and counted as queued — so the
    // caller was told its request had succeeded.
    let network = unauthenticated_network().await;
    let uris: Vec<String> = [
      "not-a-uri",
      "spotify:album:1DFixLWuPkv3KT3TnV35m3",
      "spotify:playlist:37i9dQZF1DXcBWIGoYBM5M",
      "https://open.spotify.com/track/7o2AeQZzfCERsRmOM86EcB",
    ]
    .iter()
    .map(|uri| uri.to_string())
    .collect();

    let (resolved, missing) = network.tracks_for_uris(&uris).await;
    assert!(
      resolved.is_empty(),
      "nothing here names a track: {resolved:?}"
    );
    assert_eq!(missing.len(), uris.len());
    for report in &missing {
      // Says *why*, so the model does not retry a spelling fix that cannot
      // work: these are the wrong kind of URI, not a track that is missing.
      assert!(
        report.contains("not a playable track URI"),
        "the reason has to travel with the report: {report}"
      );
    }

    // A radio stream is a finite-track failure of its own kind, and says so.
    let (_, missing) = network
      .tracks_for_uris(&["radio:https://example.com/s.aac".to_string()])
      .await;
    assert!(missing[0].contains("radio stream"), "{missing:?}");
  }

  #[tokio::test]
  async fn opaque_source_uris_pass_through_when_their_source_is_compiled_in() {
    // The pass-through exists for sources with no catalogue to check against;
    // tightening the gate must not break them. But "no catalogue to check" is not
    // "always playable": the router for each of these schemes is `#[cfg]`-gated,
    // and a build without it would queue a row that can never play while telling
    // the agent it succeeded.
    let network = unauthenticated_network().await;
    let uris: Vec<String> = ["file:/music/a.flac", "subsonic:track:1", "youtube:abc"]
      .iter()
      .map(|uri| uri.to_string())
      .collect();
    let (resolved, missing) = network.tracks_for_uris(&uris).await;
    let compiled_in = uris
      .iter()
      .filter(|uri| crate::core::queue::missing_source_feature(uri).is_none())
      .count();
    assert_eq!(resolved.len(), compiled_in, "{resolved:?}");
    assert_eq!(missing.len(), uris.len() - compiled_in, "{missing:?}");
    // Rejections name the feature, so the agent can tell "this build cannot" from
    // "this track does not exist".
    assert!(
      missing.iter().all(|m| m.contains("built without the")),
      "{missing:?}"
    );
  }

  #[tokio::test]
  async fn exclude_owned_refuses_rather_than_queueing_unfiltered() {
    // The guarantee is the whole reason the flag exists. With no Spotify session
    // the playlist crawl cannot run, and queueing the batch anyway would look
    // like success while doing the opposite of what was asked.
    let mut network = unauthenticated_network().await;
    let outcome = network
      .dj_queue(
        vec![QueueItem::Uri("spotify:track:abc".to_string())],
        /* exclude_owned */ true,
        Vec::new(),
      )
      .await;
    assert!(outcome.is_error);
    assert!(
      outcome.text.contains("exclude_owned cannot be honoured"),
      "the caller has to learn the filter did not run: {}",
      outcome.text
    );
    // Names the way out, so an agent can recover without guessing.
    assert!(outcome.text.contains("without exclude_owned"));
  }

  #[test]
  fn nothing_queued_names_every_bucket_in_its_own_words() {
    let owned = ResolveReport {
      in_library: vec!["Firestone — Kygo".into()],
      ..ResolveReport::default()
    };
    let detail = nothing_queued_detail(&owned);
    assert!(detail.contains("Already in your library: Firestone — Kygo"));
    assert!(
      !detail.contains("Could not find"),
      "a track they own exists; saying otherwise teaches the model something false"
    );
    assert!(
      !detail.contains("Already queued"),
      "it was never queued, so that explains nothing"
    );

    let mixed = ResolveReport {
      unresolved: vec!["Ghost Song".into(), "Second Ghost".into()],
      in_library: vec!["Firestone — Kygo".into()],
      duplicates: vec!["Nightcall".into()],
      ..ResolveReport::default()
    };
    let detail = nothing_queued_detail(&mixed);
    for expected in [
      "Could not find: Ghost Song; Second Ghost",
      "Already in your library: Firestone — Kygo",
      "Already queued, playing, or recently played: Nightcall",
    ] {
      assert!(
        detail.contains(expected),
        "{expected} missing from {detail}"
      );
    }
    // One reason per line. `; ` already separates entries *inside* a bucket and
    // `—` sits inside every track label, so reusing either as the group separator
    // would make a multi-entry bucket unreadable.
    assert_eq!(
      detail.lines().count(),
      3,
      "each reason needs its own line: {detail}"
    );

    // The fallback still says something, rather than trailing off after the dash.
    assert!(!nothing_queued_detail(&ResolveReport::default()).is_empty());
  }

  #[tokio::test]
  async fn a_radio_uri_is_reported_as_a_stream_rather_than_queued_as_a_track() {
    // A live stream is not a finite track, and `add_track_to_native_queue`
    // rejects it — so queueing one would leave a row that can never advance.
    // The reason has to say *stream*: told "not found", the model learns the
    // station does not exist and goes looking for another spelling.
    let network = unauthenticated_network().await;
    let (resolved, missing) = network.tracks_for_uris(&["radio:somafm".to_string()]).await;
    assert!(resolved.is_empty(), "{resolved:?}");
    assert_eq!(missing.len(), 1);
    assert!(
      missing[0].contains("a radio stream, not a track"),
      "{missing:?}"
    );
  }

  /// `play_now`'s URI gate lives in `tools::parse_call`, which is the only
  /// producer of `DjToolCall::PlayNow`, so `dj_play_now` never sees a URI that
  /// is not a single playable track. This pins that the two stay joined up:
  /// widening the parse gate without widening the lane's build-capability check
  /// is what would let a URI through to a router that does not exist.
  #[test]
  fn play_now_only_ever_reaches_the_lane_with_a_playable_track_uri() {
    for uri in [
      "not-a-uri",
      "spotify:album:1DFixLWuPkv3KT3TnV35m3",
      "https://open.spotify.com/track/7o2AeQZzfCERsRmOM86EcB",
      "radio:somafm",
    ] {
      assert!(
        crate::infra::dj::tools::parse_call("play_now", &json!({"uri": uri})).is_err(),
        "{uri} must not reach the network lane at all"
      );
    }
  }
}

// ---------------------------------------------------------------------------
// The in-TUI DJ
// ---------------------------------------------------------------------------

#[cfg(feature = "ai-dj")]
mod in_tui {
  use super::*;
  use crate::infra::dj::agent::Turn;
  use crate::infra::dj::brain::DjBrain;
  use crate::infra::dj::exec::AppExecutor;
  use crate::infra::dj::session;
  use crate::infra::dj::{AskDjRequest, DjState, QUEUE_LOW_WATER};
  use std::sync::Arc;

  impl Network {
    /// Service lane: run one DJ turn.
    ///
    /// The turn loop lives in `dj::agent`; this assembles what it needs and
    /// reports how it ended. Tool calls that need the catalogue go back down the
    /// serial lane from inside the loop, because this lane's `Network` has no
    /// Spotify client.
    ///
    /// Writes no `you` line: the handler already pushed what the listener typed,
    /// synchronously, so it is on screen while this lane is still waiting on the
    /// brain. See `AskDjRequest::extra_instruction`.
    pub async fn ask_dj(&mut self, request: AskDjRequest) {
      let outcome = self.run_turn(&request).await;
      let mut app = self.app.lock().await;
      // Whether this turn is still the current one. Read before `finish_turn`
      // purely for clarity — that call does not touch `turn_seq`.
      let owned = app.dj.turn_seq == request.turn_seq;
      // Only if this turn still owns the flag: an abandoned turn finishing would
      // otherwise clear the flag its replacement had already set, and the refill
      // tick gates on exactly that flag.
      app.dj.finish_turn(request.turn_seq);
      match outcome {
        Ok(turn) => {
          // The listener's words only become the standing auto-queue direction
          // when the turn actually did something. Otherwise "what did I play last
          // week?" would silently steer every later refill. A vibe the model set
          // itself is more considered than the raw sentence, so it wins.
          if turn.acted && !turn.abandoned && !turn.vibe_set {
            if let Some(vibe) = request.vibe_on_success {
              app.dj.vibe = Some(vibe);
            }
          }
          // A refill happens while the listener is on another screen, so the
          // transcript is not a surface they can see. The interactive case is
          // deliberately left transcript-only.
          if request.must_act && !turn.abandoned {
            if turn.acted {
              app.set_status_message("DJ: topped up the queue".to_string(), 5);
            } else {
              // A turn that had to queue and did not is a failure, not a quiet
              // success. A vibe shift has just *emptied* the queue, and with
              // auto-queue off nothing retries — so saying nothing leaves the
              // listener with silence and no explanation.
              let message = "DJ: nothing was queued that time. Ask again, or try a different \
                             direction.";
              app.dj.push_line(DjLine::system(message));
              app.set_error_status_message(message.to_string(), 6);
            }
          }
        }
        Err(e) => {
          log::warn!("DJ: turn failed: {e}");
          // Silent when the turn has been superseded. An agent CLI hits its
          // timeout a minute and a half after the listener moved on, and a
          // failure toast for a turn they abandoned would land over the
          // conversation they are now having.
          if !owned {
            return;
          }
          // Naming the backend matters when several are configurable: "DJ error"
          // alone does not say whether to check the CLI or the key.
          let backend = session::build_brain(&app.user_config.behavior)
            .map(|brain| brain.label())
            .unwrap_or("DJ");
          let message = format!("DJ error ({backend}): {e}");
          app.dj.push_line(DjLine::system(message.clone()));
          // A toast, never `handle_error` — that pushes a modal error page over
          // whatever the user was doing.
          app.set_error_status_message(message, 8);
        }
      }
    }

    async fn run_turn(
      &mut self,
      request: &AskDjRequest,
    ) -> anyhow::Result<crate::infra::dj::agent::TurnOutcome> {
      let (brain, io_tx): (DjBrain, _) = {
        let app = self.app.lock().await;
        (
          session::build_brain(&app.user_config.behavior)?,
          app.io_tx_clone(),
        )
      };
      let Some(io_tx) = io_tx else {
        anyhow::bail!("no event channel available");
      };
      let context = session::turn_context(&self.app, request.extra_instruction.as_deref()).await?;
      // Silent: the DJ transcript already shows every call, so the MCP front
      // door's status-bar announcements would be duplicated and misattributed.
      let executor = AppExecutor::silent(Arc::clone(&self.app), io_tx);

      Turn {
        app: &self.app,
        brain: &brain,
        context: &context,
        executor: &executor,
        generation: request.generation,
        must_act: request.must_act,
      }
      .run()
      .await
    }

    /// Service lane: refill the queue because it is running low.
    pub async fn dj_top_up(&mut self, generation: u64, turn_seq: u64) {
      // Cheap early bail before spending a brain call on a stale request.
      {
        let mut app = self.app.lock().await;
        if app.dj.generation != generation || !app.dj.auto_queue {
          log::debug!("DJ: dropping a stale top-up (generation {generation})");
          app.dj.finish_turn(turn_seq);
          return;
        }
      }
      self
        .ask_dj(AskDjRequest {
          extra_instruction: None,
          generation,
          // Nobody is watching a refill, so it may not stop to ask anything.
          must_act: true,
          vibe_on_success: None,
          turn_seq,
        })
        .await;
    }
  }

  /// Whether the queue is low enough to want a refill.
  ///
  /// Free function so the tick can ask without constructing a `Network`.
  ///
  /// `external_spotify_device` gates the whole thing, because `queue_len` is
  /// only a meaningful measure of runway when queued tracks actually land in
  /// the native queue. On an external Connect device
  /// `App::add_track_to_native_queue` diverts every Spotify track to the Web
  /// API queue instead, so `native_queue` stays empty no matter how much the DJ
  /// queues — and refilling on it would add a fresh batch per brain call,
  /// forever. The remote queue's depth is not cheaply observable, so the DJ
  /// waits until the native queue feeds playback again.
  pub fn wants_top_up(queue_len: usize, dj: &DjState, external_spotify_device: bool) -> bool {
    !external_spotify_device && dj.auto_queue && !dj.thinking && queue_len <= QUEUE_LOW_WATER
  }

  #[cfg(test)]
  mod tests {
    use super::*;
    use crate::core::app::App;

    /// A `Network` with no Spotify client, plus the `App` it shares.
    async fn network_with_app() -> (Network, std::sync::Arc<tokio::sync::Mutex<App>>) {
      let (tx, rx) = std::sync::mpsc::channel();
      // Leaked deliberately: dropping the receiver would make every dispatch fail
      // as "shutting down", which is not the state under test.
      std::mem::forget(rx);
      let app = std::sync::Arc::new(tokio::sync::Mutex::new(App::new(
        tx,
        crate::core::user_config::UserConfig::new(),
        Some(std::time::SystemTime::now()),
      )));
      let network = Network::new(
        None,
        crate::core::config::ClientConfig::new(),
        &app,
        std::env::temp_dir().join("spotatui-dj-turn-test-cache.json"),
      );
      (network, app)
    }

    /// A stub agent CLI that answers with words and never calls a tool.
    #[cfg(unix)]
    fn say_only_stub() -> std::path::PathBuf {
      use std::os::unix::fs::PermissionsExt;
      static STUB: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
      STUB
        .get_or_init(|| {
          let dir =
            std::env::temp_dir().join(format!("spotatui-turn-stub-{}", std::process::id()));
          std::fs::create_dir_all(&dir).unwrap();
          let path = dir.join("say-only");
          std::fs::write(
            &path,
            "#!/bin/sh\ncat >/dev/null\necho '{\"say\":\"What sort of mood?\",\"tool_calls\":[]}'\n",
          )
          .unwrap();
          std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
          path
        })
        .clone()
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_refill_that_queues_nothing_says_so_rather_than_going_quiet() {
      // A vibe shift drops the DJ's queued tracks *first*, so a turn that then
      // queues nothing leaves the listener in silence. With auto-queue off there
      // is no tick to retry it either, so this message is the only signal.
      let (mut network, app) = network_with_app().await;
      let turn_seq = {
        let mut app = app.lock().await;
        app.user_config.behavior.dj_agent_command =
          vec![say_only_stub().to_string_lossy().to_string()];
        app.user_config.behavior.dj_agent_prompt_via = Some("stdin".to_string());
        app.dj.begin_turn(crate::infra::dj::TurnKind::Ask)
      };

      network
        .ask_dj(AskDjRequest {
          extra_instruction: None,
          generation: 0,
          must_act: true,
          vibe_on_success: None,
          turn_seq,
        })
        .await;

      let app = app.lock().await;
      assert!(!app.dj.thinking, "the flag has to be released either way");
      let transcript: Vec<&str> = app
        .dj
        .transcript
        .iter()
        .map(|line| line.text.as_str())
        .collect();
      assert!(
        transcript.iter().any(|t| t.contains("nothing was queued")),
        "{transcript:?}"
      );
      assert!(
        app.status_message_is_error,
        "and it is reported as a failure"
      );
    }

    #[test]
    fn top_up_is_wanted_only_when_enabled_idle_and_low() {
      let mut dj = DjState {
        auto_queue: true,
        ..DjState::default()
      };
      assert!(wants_top_up(0, &dj, false), "an empty queue wants a refill");
      assert!(
        wants_top_up(QUEUE_LOW_WATER, &dj, false),
        "at the watermark"
      );
      assert!(!wants_top_up(QUEUE_LOW_WATER + 1, &dj, false), "above it");

      dj.thinking = true;
      assert!(
        !wants_top_up(0, &dj, false),
        "must not stack refills while one is in flight"
      );

      dj.thinking = false;
      dj.auto_queue = false;
      assert!(!wants_top_up(0, &dj, false), "DJ off means no refill");
    }

    #[test]
    fn no_refill_while_an_external_spotify_device_owns_playback() {
      // On an external Connect device, queued Spotify tracks are diverted to the
      // Web API queue and `native_queue` stays empty — so a length of 0 is not
      // "out of runway", and refilling on it queues a batch per brain call
      // forever.
      let dj = DjState {
        auto_queue: true,
        ..DjState::default()
      };
      assert!(
        !wants_top_up(0, &dj, true),
        "an empty native queue proves nothing on a remote device"
      );
      assert!(
        wants_top_up(0, &dj, false),
        "and refills resume once playback is back on this device"
      );
    }

    #[tokio::test]
    async fn a_stale_top_up_releases_the_progress_flag_before_bailing() {
      // The `finish_turn` in that bail is load-bearing, not tidiness: the
      // dispatcher already set `thinking`, and `wants_top_up` gates on it, so a
      // bail that skipped it would stop auto-queue for the rest of the session
      // with nothing on screen to explain why.
      let (mut network, app) = network_with_app().await;
      let turn_seq = {
        let mut app = app.lock().await;
        app.dj.auto_queue = true;
        let turn_seq = app.dj.begin_turn(crate::infra::dj::TurnKind::Refill);
        // The listener moved on while the refill was in flight.
        app.dj.bump_generation();
        turn_seq
      };

      network.dj_top_up(0, turn_seq).await;

      let app = app.lock().await;
      assert!(
        !app.dj.thinking,
        "a dropped refill must not wedge auto-queue"
      );
      assert!(wants_top_up(0, &app.dj, false));
    }
  }
}

#[cfg(feature = "ai-dj")]
pub use in_tui::wants_top_up;
