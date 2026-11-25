//! Network-lane handlers for the DJ.
//!
//! These run on the **Spotify** lane, with a clone of the pump's Spotify client,
//! so a crawl or a lookup never holds the serial pump. The service lane
//! deliberately constructs its `Network` with `None` for the client
//! (`runtime/`), so anything that has to resolve a track name to a URI belongs
//! here rather than there.

//! Tool schemas and replies intentionally stay English: they are model-facing
//! protocol text, independent of the user's interface language.

use super::Network;
#[cfg(any(feature = "mcp-server", feature = "ai-dj"))]
use crate::core::action::{Action, ActionOutcome};
use crate::core::plugin_api::TrackInfo;
use crate::infra::dj::resolve::{self, ResolveReport};
#[cfg(any(feature = "mcp-server", feature = "ai-dj"))]
use crate::infra::dj::tools::{DjToolCall, QueueItem, ToolOutcome};
use crate::infra::dj::{library, DjLibrary, DjLine, DjSuggestion};
#[cfg(any(feature = "mcp-server", feature = "ai-dj"))]
use crate::infra::dj::{DjTurn, RefillProgress, TurnKind, MAX_BATCH};
use crate::infra::network::IoEvent;
use rspotify::model::track::FullTrack;
#[cfg(any(feature = "mcp-server", feature = "ai-dj"))]
use serde_json::{json, Value};
use std::collections::HashSet;
#[cfg(any(feature = "mcp-server", feature = "ai-dj"))]
use tokio::sync::oneshot;

#[cfg(any(feature = "mcp-server", feature = "ai-dj"))]
fn artist_names(artists: &[rspotify::model::SimplifiedArtist]) -> String {
  artists
    .iter()
    .map(|artist| artist.name.as_str())
    .collect::<Vec<_>>()
    .join(", ")
}

/// `1. title [uri]` lines for tool text.
#[cfg(any(feature = "mcp-server", feature = "ai-dj"))]
fn numbered(items: &[serde_json::Value]) -> String {
  items
    .iter()
    .enumerate()
    .map(|(index, item)| {
      format!(
        "{}. {} [{}]",
        index + 1,
        item["title"].as_str().unwrap_or_default(),
        item["uri"].as_str().unwrap_or("no uri")
      )
    })
    .collect::<Vec<_>>()
    .join("\n")
}

/// The device a transfer goes to: an exact id, else a name in any case.
#[cfg(any(feature = "mcp-server", feature = "ai-dj"))]
fn pick_device(
  devices: &[rspotify::model::device::Device],
  wanted: &str,
) -> Option<(String, String)> {
  let found = devices
    .iter()
    .find(|device| device.id.as_deref() == Some(wanted))
    .or_else(|| {
      devices
        .iter()
        .find(|device| device.name.eq_ignore_ascii_case(wanted))
    })?;
  Some((found.id.clone()?, found.name.clone()))
}

/// Reject known source failures before a start can interrupt current playback.
#[cfg(any(feature = "mcp-server", feature = "ai-dj"))]
fn play_now_source_error(
  uri: &str,
  subsonic_configured: bool,
  qobuz_logged_in: bool,
) -> Option<ToolOutcome> {
  if let Some(feature) = crate::core::queue::missing_source_feature(uri) {
    return Some(ToolOutcome::error_localized(|strings| {
      strings.status.tools.missing_feature(feature, uri)
    }));
  }
  #[cfg(feature = "local-files")]
  if uri.starts_with("file:") {
    if let Err(error) = crate::infra::local::file_uri_to_path(uri) {
      return Some(ToolOutcome::error_localized(|strings| {
        strings.status.infra.local_uri_invalid(&error.to_string())
      }));
    }
  }
  #[cfg(feature = "youtube")]
  if uri.starts_with("youtube:") && crate::infra::youtube::video_id_from_uri(uri).is_err() {
    return Some(ToolOutcome::error_localized(|strings| {
      strings.status.infra.youtube_uri_invalid
    }));
  }
  if uri.starts_with("subsonic:") {
    #[cfg(feature = "subsonic")]
    if crate::infra::subsonic::track_id_from_uri(uri).map_or(true, str::is_empty) {
      return Some(ToolOutcome::error_localized(|strings| {
        strings.status.infra.subsonic_uri_invalid
      }));
    }
    if !subsonic_configured {
      return Some(ToolOutcome::error_localized(|strings| {
        strings.status.infra.subsonic_no_server
      }));
    }
  }
  if uri.starts_with("qobuz:") {
    #[cfg(feature = "qobuz")]
    if crate::infra::qobuz::track_id_from_uri(uri).map_or(true, str::is_empty) {
      return Some(ToolOutcome::error_localized(|strings| {
        strings.status.infra.qobuz_uri_invalid
      }));
    }
    if !qobuz_logged_in {
      return Some(ToolOutcome::error_localized(|strings| {
        strings.status.infra.qobuz_login_expired
      }));
    }
  }
  None
}

/// The tool handlers behind `DjToolCall`, shared by both front doors.
#[cfg(any(feature = "mcp-server", feature = "ai-dj"))]
impl Network {
  async fn queue_resolved_tracks(
    &self,
    report: &mut resolve::ResolveReport,
    turn: Option<DjTurn>,
  ) -> Option<Vec<String>> {
    let mut app = self.app.lock().await;
    if turn.is_some_and(|turn| !turn.is_current(&app)) {
      return None;
    }
    use crate::core::queue::{source_available, QueueItemSource};
    let spotify_playable =
      source_available(QueueItemSource::Spotify) || app.spotify_external_device_active();
    // Compare against the live queue under the commit lock. MCP callers can
    // intentionally request repeats, so only DJ turns build this set.
    let mut seen = turn.map(|_| {
      let mut uris: HashSet<_> = app
        .native_queue
        .iter()
        .filter_map(|t| t.uri.clone())
        .collect();
      uris.extend(
        crate::infra::media_metadata::tracking_playback_snapshot(&app).and_then(|now| now.item_uri),
      );
      uris
    });
    report.resolved.retain(|track| {
      let Some(uri) = &track.uri else {
        return false;
      };
      if !spotify_playable && uri.starts_with("spotify:track:") {
        report.unavailable.push(track_label(track));
        return false;
      }
      if seen.as_mut().is_some_and(|seen| !seen.insert(uri.clone())) {
        report.duplicates.push(track_label(track));
        return false;
      }
      true
    });
    if report.resolved.is_empty() {
      return Some(Vec::new());
    }
    let labels: Vec<_> = report.resolved.iter().map(track_label).collect();
    let queue_len = app.native_queue.len();
    let ActionOutcome::Queued { accepted } =
      app.apply(Action::QueueTracks(std::mem::take(&mut report.resolved)))
    else {
      return Some(Vec::new());
    };
    let mut queued_labels = Vec::new();
    for (label, accepted) in labels.into_iter().zip(accepted) {
      if accepted {
        queued_labels.push(label);
      } else {
        report.refused.push(label);
      }
    }
    if turn.is_some() && app.dj.turn_kind == TurnKind::Refill {
      if app.native_queue.len() > queue_len {
        app.dj.refill_progress = RefillProgress::Queued;
      }
    } else if !queued_labels.is_empty() {
      let summary = report.summary_with(app.strings(), queued_labels.len());
      let message = app.strings().status.infra.dj_status(&summary);
      app.set_status_message(message, 5);
    }
    Some(queued_labels)
  }

  /// Answers a call with no Spotify item (`!call.needs_network()`) at once,
  /// with no Spotify client, so it never waits behind Spotify work.
  pub async fn answer_dj_tool_call_without_spotify(
    app: &std::sync::Arc<tokio::sync::Mutex<crate::core::app::App>>,
    call: DjToolCall,
    turn: Option<DjTurn>,
  ) -> ToolOutcome {
    let mut network = Network::new(None, app);
    network.dj_tool_outcome(call, turn).await
  }

  /// Run a DJ tool call that needs the catalogue, and answer the waiting caller.
  ///
  /// The responder is a `oneshot`, so every path that runs the call must send
  /// exactly once — dropping it silently would leave an MCP client waiting out
  /// its timeout with no idea why. A caller that already gave up runs nothing.
  pub async fn run_dj_tool_call(
    &mut self,
    call: DjToolCall,
    responder: oneshot::Sender<ToolOutcome>,
    turn: Option<DjTurn>,
  ) {
    // The caller timed out while the call waited: running it now would queue or
    // play behind its back, and a retry would do it twice.
    if responder.is_closed() {
      log::debug!("DJ: {} dropped, its caller gave up", call.tool_name());
      return;
    }
    let outcome = self.dj_tool_outcome(call, turn).await;
    // The receiver is gone if the MCP client hung up mid-call; that is normal,
    // not an error worth surfacing.
    let _ = responder.send(outcome);
  }

  async fn dj_tool_outcome(&mut self, call: DjToolCall, turn: Option<DjTurn>) -> ToolOutcome {
    // This variant bypasses the generic auth gate so we can say something
    // useful here instead of the caller seeing a dropped channel.
    if call.needs_spotify_session() && self.spotify.is_none() {
      return ToolOutcome::error_localized(|strings| strings.status.tools.catalogue_unavailable);
    }

    match call {
      DjToolCall::SearchTracks { query, limit } => self.dj_search(&query, limit, turn).await,
      DjToolCall::QueueTracks {
        items,
        exclude_owned,
        extra_skip_keys,
      } => {
        self
          .dj_queue(items, exclude_owned, extra_skip_keys, turn)
          .await
      }
      DjToolCall::PlayNow { uri } => self.dj_play_now(uri, turn).await,
      DjToolCall::GetAlbum { album } => self.dj_album(&album).await,
      DjToolCall::GetArtist { artist } => self.dj_artist(&artist).await,
      DjToolCall::ListDevices => self.dj_devices().await,
      DjToolCall::TransferPlayback { device } => self.dj_transfer(&device).await,
      DjToolCall::LikeTracks { uris, liked } => self.dj_like(uris, liked).await,
      // Everything else is answered without the network, before the event is
      // ever dispatched (`tools::execute_app_only`).
      other => ToolOutcome::error(format!(
        "{} does not need the catalogue and should not have been routed here",
        other.tool_name()
      )),
    }
  }

  async fn dj_album(&self, id: &str) -> ToolOutcome {
    let album = match self
      .spotify_get_typed::<rspotify::model::album::FullAlbum>(&format!("albums/{id}"), &[])
      .await
    {
      Ok(album) => album,
      Err(e) => return ToolOutcome::error(format!("Could not read the album: {e}")),
    };
    let artists = artist_names(&album.artists);
    let year = album.release_date.get(..4).unwrap_or_default();
    let tracks: Vec<Value> = album
      .tracks
      .items
      .iter()
      .map(|track| {
        json!({
          "title": track.name,
          "uri": track.id.as_ref().map(rspotify::prelude::Id::uri),
          "duration_ms": track.duration.num_milliseconds(),
        })
      })
      .collect();
    let text = format!(
      "{} by {artists} ({year}), {} tracks:\n{}",
      album.name,
      tracks.len(),
      numbered(&tracks)
    );
    ToolOutcome::with_data(
      text,
      json!({ "name": album.name, "artists": artists, "year": year, "tracks": tracks }),
    )
  }

  async fn dj_artist(&self, id: &str) -> ToolOutcome {
    use rspotify::model::{artist::FullArtist, page::Page, SimplifiedAlbum};
    #[derive(serde::Deserialize)]
    struct TopTracks {
      tracks: Vec<FullTrack>,
    }
    let artist = match self
      .spotify_get_typed::<FullArtist>(&format!("artists/{id}"), &[])
      .await
    {
      Ok(artist) => artist,
      Err(e) => return ToolOutcome::error(format!("Could not read the artist: {e}")),
    };
    let top = self
      .spotify_get_typed::<TopTracks>(&format!("artists/{id}/top-tracks"), &[])
      .await
      .map(|top| top.tracks)
      .unwrap_or_default();
    let albums = self
      .spotify_get_typed::<Page<SimplifiedAlbum>>(
        &format!("artists/{id}/albums"),
        &[("limit", "20".to_string())],
      )
      .await
      .map(|page| page.items)
      .unwrap_or_default();
    let top: Vec<Value> = top
      .iter()
      .map(|track| json!({ "title": track.name, "uri": track.id.as_ref().map(rspotify::prelude::Id::uri) }))
      .collect();
    let albums: Vec<Value> = albums
      .iter()
      .map(|album| {
        json!({
          "title": album.name,
          "uri": album.id.as_ref().map(rspotify::prelude::Id::uri),
          "year": album.release_date.as_deref().and_then(|date| date.get(..4)),
        })
      })
      .collect();
    let text = format!(
      "{}\nTop tracks:\n{}\nAlbums:\n{}",
      artist.name,
      numbered(&top),
      numbered(&albums)
    );
    ToolOutcome::with_data(
      text,
      json!({ "name": artist.name, "top_tracks": top, "albums": albums }),
    )
  }

  async fn dj_devices(&self) -> ToolOutcome {
    match self
      .spotify_get_typed::<rspotify::model::device::DevicePayload>("me/player/devices", &[])
      .await
    {
      Ok(payload) => {
        let devices: Vec<Value> = payload
          .devices
          .iter()
          .map(|device| {
            json!({
              "id": device.id,
              "name": device.name,
              "type": format!("{:?}", device._type),
              "active": device.is_active,
              "volume_percent": device.volume_percent,
            })
          })
          .collect();
        let text = if devices.is_empty() {
          "No Spotify Connect devices are online".to_string()
        } else {
          payload
            .devices
            .iter()
            .map(|device| {
              format!(
                "{} [{}]{}",
                device.name,
                device.id.as_deref().unwrap_or("no id"),
                if device.is_active { " (active)" } else { "" }
              )
            })
            .collect::<Vec<_>>()
            .join("\n")
        };
        ToolOutcome::with_data(text, json!({ "devices": devices }))
      }
      Err(e) => ToolOutcome::error(format!("Could not list the devices: {e}")),
    }
  }

  async fn dj_transfer(&self, device: &str) -> ToolOutcome {
    let devices = match self
      .spotify_get_typed::<rspotify::model::device::DevicePayload>("me/player/devices", &[])
      .await
    {
      Ok(payload) => payload.devices,
      Err(e) => return ToolOutcome::error(format!("Could not list the devices: {e}")),
    };
    let Some((id, name)) = pick_device(&devices, device) else {
      let names: Vec<&str> = devices.iter().map(|device| device.name.as_str()).collect();
      return ToolOutcome::error(format!(
        "No device matches \"{device}\". Online: {}",
        if names.is_empty() {
          "none".to_string()
        } else {
          names.join(", ")
        }
      ));
    };
    self.app.lock().await.apply(Action::TransferPlayback {
      device_id: id,
      persist: false,
    });
    ToolOutcome::ok_localized(|strings| strings.status.tools.transfer(&name))
  }

  /// Toggles only the tracks whose liked state differs, after reading it.
  async fn dj_like(&self, uris: Vec<String>, liked: bool) -> ToolOutcome {
    let states = match self.library_contains_uris(&uris).await {
      Ok(states) if states.len() == uris.len() => states,
      Ok(states) => {
        return ToolOutcome::error(format!(
          "Could not read Liked Songs: Spotify answered for {} of {} track(s)",
          states.len(),
          uris.len()
        ))
      }
      Err(e) => return ToolOutcome::error(format!("Could not read Liked Songs: {e}")),
    };
    let mut app = self.app.lock().await;
    let mut changed = 0;
    for (uri, is_liked) in uris.iter().zip(states) {
      let id = uri.trim_start_matches("spotify:track:").to_string();
      // The toggle reads this set, so it must match Spotify before the toggle.
      if is_liked {
        app.liked_song_ids_set_mut().insert(id);
      } else {
        app.liked_song_ids_set_mut().remove(&id);
      }
      if is_liked != liked {
        app.apply(Action::ToggleSaveTrack(uri.clone()));
        changed += 1;
      }
    }
    let unchanged = uris.len() - changed;
    ToolOutcome::ok_localized(|strings| {
      if liked {
        strings.status.tools.liking(changed, unchanged)
      } else {
        strings.status.tools.unliking(changed, unchanged)
      }
    })
  }

  async fn dj_search(&self, query: &str, limit: usize, turn: Option<DjTurn>) -> ToolOutcome {
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
      Err(e) => {
        return ToolOutcome::error_localized(|strings| {
          strings.status.tools.search_failed(&e.to_string())
        })
      }
    };

    let tracks: Vec<TrackInfo> = response
      .tracks
      .items
      .iter()
      .filter(|track| track.id.is_some())
      .map(TrackInfo::from)
      .collect();

    if tracks.is_empty() {
      return ToolOutcome::ok_localized(|strings| strings.status.tools.no_tracks(query));
    }

    // Ownership rides along with the results rather than behind a separate tool:
    // an agent already searches to confirm a track exists before queueing it, so
    // this arrives exactly when it is choosing, with no extra round trip. Marking
    // beats filtering here — the agent can still queue something the listener
    // owns when that is genuinely the right answer.
    let ownership = self.track_ownership(&tracks, turn).await;
    let owned = |track: &TrackInfo| {
      track
        .id
        .as_ref()
        .is_some_and(|id| ownership.owned.contains(id))
    };

    let structured = json!({
      "ownership_complete": matches!(ownership.playlists, PlaylistOwnership::Checked),
      "tracks": tracks.iter().map(|track| json!({
        "title": track.name,
        "artist": track.artists.join(", "),
        "album": track.album,
        "uri": track.uri,
        "duration_ms": track.duration_ms,
        "owned": owned(track),
      })).collect::<Vec<_>>()
    });
    ToolOutcome::data_localized(
      |strings| {
        let s = &strings.status.tools;
        let mut rendered = tracks
          .iter()
          .map(|track| {
            s.found_track(
              &track.name,
              &track.artists.join(", "),
              track.uri.as_deref().unwrap_or("no uri"),
              if owned(track) { s.owned } else { s.new_track },
            )
          })
          .collect::<Vec<_>>()
          .join("\n");
        rendered.push_str(match ownership.playlists {
          PlaylistOwnership::Checked => "",
          PlaylistOwnership::IndexPending => s.ownership_incomplete,
          PlaylistOwnership::Unchecked => s.ownership_without_playlist_index,
        });
        rendered
      },
      structured,
    )
  }

  async fn dj_queue(
    &mut self,
    items: Vec<QueueItem>,
    exclude_owned: bool,
    extra_skip_keys: Vec<String>,
    turn: Option<DjTurn>,
  ) -> ToolOutcome {
    // Ownership is a Spotify notion: a batch with no Spotify item has nothing the
    // filter could skip, so it pays for no crawl. Without a session, names
    // resolve through YouTube, which the filter cannot judge either.
    let filter_applies = exclude_owned
      && items.iter().any(|item| match item {
        QueueItem::Uri(uri) => uri.starts_with("spotify:track:"),
        QueueItem::Named(_) => self.spotify.is_some(),
      });
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
    let library_keys = if filter_applies {
      match self.dj_library_index(true).await {
        Ok(library) => library.keys,
        Err(error) => {
          return ToolOutcome::error_localized(|strings| {
            let mut message = strings.status.tools.queue_filter_failed(&error.to_string());
            if turn.is_none() {
              message.push_str(strings.status.tools.queue_filter_retry_without);
            }
            message
          });
        }
      }
    } else {
      HashSet::new()
    };

    let mut report = self.tracks_for_uris(&uris).await;

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
      report.lookup_errors.extend(named_report.lookup_errors);
    }

    // The exact gate, on the resolved ID. It catches what the name gate cannot —
    // a title written differently enough to normalise apart — and it is the only
    // gate that sees the `uri` entries at all, which never went through a name
    // lookup. Nothing is substituted for a rejected track: the caller named
    // these, so coming back short is the honest answer.
    if filter_applies {
      self.reject_owned_tracks(&mut report).await;
    }

    let Some(queued_labels) = self.queue_resolved_tracks(&mut report, turn).await else {
      return ToolOutcome::dropped();
    };
    let accepted = queued_labels.len();
    if accepted == 0 {
      return ToolOutcome::error_localized(|strings| {
        strings
          .status
          .tools
          .nothing_queued(&nothing_queued_detail(&report, strings))
      });
    }

    ToolOutcome::data_localized(
      |strings| {
        let s = &strings.status.tools;
        let mut text = format!("{}\n{}", s.queued(accepted), queued_labels.join("\n"));
        if !report.unresolved.is_empty() {
          text.push_str(&s.queue_not_found(&report.unresolved.join("; ")));
        }
        if !report.in_library.is_empty() {
          text.push_str(&s.queue_owned(&report.in_library.join("; ")));
        }
        if !report.duplicates.is_empty() {
          text.push_str(&s.queue_duplicate(&report.duplicates.join("; ")));
        }
        for error in queue_errors(&report, strings) {
          text.push('\n');
          text.push_str(&error);
        }
        if exclude_owned && !filter_applies {
          text.push_str(s.queue_filter_spotify_only);
        }
        text
      },
      json!({
        "queued": queued_labels,
        "not_found": report.unresolved,
        "in_library": report.in_library,
        "duplicates": report.duplicates,
        "lookup_errors": report.lookup_errors,
        "unavailable": report.unavailable,
        "refused": report.refused,
      }),
    )
  }

  /// Request playback of one track, after checking known failures.
  ///
  /// A Spotify track goes to the Spotify lane purely for that confirmation;
  /// another source's track is answered without a Spotify client. Catalogue
  /// resolution and dispatch do not confirm that a player started: source
  /// downloads, decoding and device selection can still fail after we answer.
  /// Waiting here for those results would require a per-start acknowledgement;
  /// the shared playback state can still describe the previous track.
  ///
  /// Deliberately not filtered by `exclude_owned`: the caller asked for this
  /// exact track right now, and owning it is not a reason to refuse.
  async fn dj_play_now(&self, uri: String, turn: Option<DjTurn>) -> ToolOutcome {
    // Only Spotify URIs can be checked from here. The opaque schemes are minted
    // by their own sources and have no catalogue to look them up in, so they go
    // through with wording that does not overclaim.
    if !uri.starts_with("spotify:track:") {
      if let Some(error) = self.dj_start_playback(&uri, turn).await {
        return error;
      }
      return ToolOutcome::ok_localized(|strings| strings.status.tools.sent(&uri));
    }

    let report = self.tracks_for_uris(std::slice::from_ref(&uri)).await;
    if let Some(error) = report.lookup_errors.first() {
      return ToolOutcome::error_localized(|strings| strings.status.tools.search_failed(error));
    }
    let Some(track) = report.resolved.into_iter().next() else {
      return ToolOutcome::error_localized(|strings| strings.status.tools.missing_track(&uri));
    };

    let label = track_label(&track);
    if let Some(error) = self.dj_start_playback(&uri, turn).await {
      return error;
    }
    ToolOutcome::data_localized(
      |strings| strings.status.tools.play_started(&label),
      json!({"uri": uri, "title": track.name, "artist": track.artists.join(", ")}),
    )
  }

  async fn dj_start_playback(&self, uri: &str, turn: Option<DjTurn>) -> Option<ToolOutcome> {
    #[cfg(feature = "qobuz")]
    let qobuz_logged_in =
      !uri.starts_with("qobuz:") || crate::infra::qobuz::auth::current_token().is_some();
    #[cfg(not(feature = "qobuz"))]
    let qobuz_logged_in = false;
    let mut app = self.app.lock().await;
    if turn.is_some_and(|turn| !turn.is_current(&app)) {
      return Some(ToolOutcome::dropped());
    }
    if let Some(error) = play_now_source_error(
      uri,
      app.user_config.behavior.subsonic_url.is_some(),
      qobuz_logged_in,
    ) {
      return Some(error);
    }
    #[cfg(feature = "ai-dj")]
    if turn.is_some() && app.dj.turn_kind == TurnKind::Refill {
      let event = IoEvent::StartPlayback(None, Some(vec![uri.to_string()]), Some(0));
      if !app.io_tx_clone().is_some_and(|tx| tx.send(event).is_ok()) {
        return Some(ToolOutcome::error_localized(|strings| {
          strings.status.tools.shutting_down
        }));
      }
      if app.dj.refill_progress == RefillProgress::Waiting {
        app.dj.refill_progress = RefillProgress::PlaybackRequested;
      }
      return None;
    }
    app.apply(Action::PlayUris {
      uris: vec![uri.to_string()],
      offset: Some(0),
    });
    None
  }

  /// Whether the listener already has each of these search results.
  ///
  /// The two halves cost wildly different amounts, so they are treated
  /// differently. Liked Songs is one exact `me/library/contains` for the whole
  /// page, so it is always checked. Playlists have no such lookup — the only way
  /// to know is the crawl — so this reads the cached index and, when it is cold,
  /// *asks* for the crawl as its own event instead of running it here.
  ///
  /// That deferral is the point. `search_tracks` is what an agent calls between
  /// deciding and queueing, and the crawl is seconds of pagination on the
  /// Spotify lane; running it inline would hold every tool call behind an
  /// agent's search. A cold search answers from Liked Songs alone and reports
  /// whether a crawl is pending. A refill can build the index later if it queues
  /// with `exclude_owned`.
  async fn track_ownership(&self, tracks: &[TrackInfo], turn: Option<DjTurn>) -> Ownership {
    let ids: Vec<String> = tracks.iter().filter_map(|track| track.id.clone()).collect();
    if ids.is_empty() {
      return Ownership {
        owned: HashSet::new(),
        playlists: PlaylistOwnership::Checked,
      };
    }

    let mut owned = library::liked_among(self, &ids).await;
    let app = self.app.lock().await;
    let playlists = match &app.dj.library {
      Some(library) => {
        owned.extend(ids.into_iter().filter(|id| library.ids.contains(id)));
        PlaylistOwnership::Checked
      }
      None if Self::request_library_index(&app, turn) => PlaylistOwnership::IndexPending,
      None => PlaylistOwnership::Unchecked,
    };
    Ownership { owned, playlists }
  }

  /// Ask for the playlist crawl on the Spotify lane when the index is cold.
  /// Returns whether a crawl was requested or is already running.
  ///
  /// A channel send, never a call. This runs *on* the Spotify lane, so the crawl
  /// queues behind the tool call that asked for it rather than extending it; the
  /// channel is unbounded, so the send cannot block the handler it is inside.
  fn request_library_index(app: &crate::core::app::App, turn: Option<DjTurn>) -> bool {
    if app.dj.library_indexing {
      return true;
    }
    // A refill checks the index inline if it queues with exclude_owned. A
    // separate crawl here could post an error before the refill stops.
    if turn.is_some() && app.dj.turn_kind == TurnKind::Refill {
      return false;
    }
    match app.io_tx_clone() {
      Some(sender) => sender.send(IoEvent::DjIndexLibrary).is_ok(),
      // Shutdown only, and unreachable in practice: `App::new` always sets the
      // channel, `close_io_channel` is the sole path that clears it, and
      // `mcp::spawn_listener` refuses to start without it — so an MCP call cannot
      // arrive before it exists.
      None => {
        log::debug!("DJ: no event channel, so the library index was not requested");
        false
      }
    }
  }
}

#[cfg(any(feature = "mcp-server", feature = "ai-dj"))]
fn track_label(track: &TrackInfo) -> String {
  if track.artists.is_empty() {
    track.name.clone()
  } else {
    format!("{} — {}", track.name, track.artists.join(", "))
  }
}

#[cfg(any(feature = "mcp-server", feature = "ai-dj"))]
fn queue_errors(report: &ResolveReport, strings: &crate::core::i18n::Strings) -> Vec<String> {
  let mut reasons = Vec::new();
  let tools = &strings.status.tools;
  if !report.lookup_errors.is_empty() {
    reasons.push(tools.search_failed(&report.lookup_errors.join("; ")));
  }
  if !report.unavailable.is_empty() {
    reasons.push(tools.queue_streaming_unavailable(&report.unavailable.join("; ")));
  }
  if !report.refused.is_empty() {
    reasons.push(tools.queue_refused(&report.refused.join("; ")));
  }
  reasons
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
fn nothing_queued_detail(report: &ResolveReport, strings: &crate::core::i18n::Strings) -> String {
  let s = &strings.status.tools;
  let mut reasons = Vec::new();
  if !report.unresolved.is_empty() {
    reasons.push(s.none_found(&report.unresolved.join("; ")));
  }
  if !report.in_library.is_empty() {
    reasons.push(s.none_owned(&report.in_library.join("; ")));
  }
  if !report.duplicates.is_empty() {
    reasons.push(s.none_duplicate(&report.duplicates.join("; ")));
  }
  reasons.extend(queue_errors(report, strings));
  if reasons.is_empty() {
    return s.none_new.to_string();
  }
  reasons.join("\n")
}

/// What `search_tracks` was able to work out about ownership for one page of
/// results.
#[cfg(any(feature = "mcp-server", feature = "ai-dj"))]
struct Ownership {
  /// Track IDs the listener already has.
  owned: HashSet<String>,
  playlists: PlaylistOwnership,
}

#[cfg(any(feature = "mcp-server", feature = "ai-dj"))]
enum PlaylistOwnership {
  Checked,
  IndexPending,
  Unchecked,
}

/// Helpers shared by both front doors.
impl Network {
  /// Look up exact Spotify URIs so the queue shows real titles rather than raw
  /// URIs. Non-Spotify URIs are queued as-is, since only their own source can
  /// describe them.
  #[cfg_attr(not(any(feature = "mcp-server", feature = "ai-dj")), allow(dead_code))]
  async fn tracks_for_uris(&self, uris: &[String]) -> ResolveReport {
    let mut report = ResolveReport::default();

    let spotify_ids: Vec<&str> = uris
      .iter()
      .filter_map(|uri| uri.strip_prefix("spotify:track:"))
      .collect();

    if !spotify_ids.is_empty() {
      // One `tracks/{id}` per track: the batch `tracks?ids=` endpoint is gone
      // for Development Mode apps.
      let this: &Self = self;
      let lookups = spotify_ids.iter().map(|id| async move {
        this
          .spotify_get_typed::<Option<FullTrack>>(&format!("tracks/{id}"), &[])
          .await
      });
      for (id, result) in spotify_ids
        .iter()
        .zip(futures::future::join_all(lookups).await)
      {
        match result {
          Ok(Some(track)) => report.resolved.push(TrackInfo::from(&track)),
          Ok(None) => report.unresolved.push(format!("spotify:track:{id}")),
          Err(e) if super::requests::is_not_found_error(&e) => {
            report.unresolved.push(format!("spotify:track:{id}"));
          }
          Err(e) => {
            log::debug!("DJ: track lookup failed for {id}: {e}");
            report
              .lookup_errors
              .push(format!("spotify:track:{id}: {e}"));
          }
        }
      }
    }

    for uri in uris.iter().filter(|uri| !uri.starts_with("spotify:track:")) {
      if uri.starts_with("radio:") {
        // A live stream is not a finite track; `add_track_to_native_queue`
        // rejects these, so report rather than silently dropping.
        report
          .unresolved
          .push(format!("{uri} (a radio stream, not a track)"));
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
        report
          .unresolved
          .push(format!("{uri} (not a playable track URI)"));
        continue;
      }
      // Same silent-drop class as `play_now`: the scheme is right, but this build
      // has no router that would consume it, so queueing it would report success
      // for a row that can never play.
      if let Some(feature) = crate::core::queue::missing_source_feature(uri) {
        report.unresolved.push(format!(
          "{uri} (this spotatui was built without the `{feature}` feature)"
        ));
        continue;
      }
      report.resolved.push(TrackInfo {
        generated_name: None,
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
        release_date: None,
      });
    }

    report
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

  /// Spotify lane: crawl the listener's own playlists for the avoid-library
  /// filter, and cache the result on `App`.
  ///
  /// Idempotent by way of `library_indexing`: the in-TUI toggle, the first turn,
  /// and an MCP `search_tracks` can all ask for this, and a second crawl would be
  /// pure cost.
  pub async fn dj_index_library(&mut self) {
    let _ = self.dj_library_index(false).await;
  }

  /// Inline failures are returned to the tool so only the final failed refill
  /// raises an error toast. Background crawls report on their own.
  async fn dj_library_index(&self, inline: bool) -> anyhow::Result<DjLibrary> {
    {
      let mut app = self.app.lock().await;
      if let Some(library) = &app.dj.library {
        return Ok(library.clone());
      }
      if app.dj.library_indexing {
        anyhow::bail!("the Spotify playlist index is still being built");
      }
      app.dj.library_indexing = true;
      app.dj.bump_revision();
    }

    let outcome = self.build_dj_library().await;
    let mut app = self.app.lock().await;
    app.dj.library_indexing = false;
    app.dj.bump_revision();
    match &outcome {
      Ok(library) => {
        let summary = library.summary_with(app.strings());
        app.dj.library = Some(library.clone());
        if !inline {
          let strings = app.strings();
          app.set_status_message(strings.status.infra.dj_status(&summary), 5);
        }
        app.dj.push_line(DjLine::system(summary));
      }
      Err(e) => {
        log::warn!("DJ: could not index the library: {e}");
        if !inline {
          let message = app.strings().status.infra.dj_library_failed(&e.to_string());
          app.dj.push_line(DjLine::system(message.clone()));
          app.set_error_status_message(message, 8);
        }
      }
    }
    outcome
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
        .user()
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

  /// Drop resolved tracks the listener already owns, moving them to the
  /// `in_library` bucket.
  ///
  /// This is the exact gate, and it catches what the name-based one cannot: the
  /// title was written differently enough to normalise apart, but search landed
  /// on the very copy sitting in their playlist. Two sources of truth: the
  /// crawled playlist IDs, and one `me/library/contains` call for Liked Songs,
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
  use crate::infra::network::metadata::tests::{fake_spotify_login, read_http_request};
  use std::sync::Arc;
  use tokio::io::AsyncWriteExt;
  use tokio::sync::Mutex;

  pub(super) const TEST_TRACK_ID: &str = "7o2AeQZzfCERsRmOM86EcB";

  pub(super) async fn spotify_reply(
    status: &'static str,
    body: String,
  ) -> (
    crate::core::auth::SpotifyLogin,
    crate::infra::network::requests::fake::FakeSpotify,
    oneshot::Receiver<String>,
    oneshot::Sender<()>,
  ) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let (login, fake) =
      fake_spotify_login(format!("http://{}/v1/", listener.local_addr().unwrap()));
    let (seen, request) = oneshot::channel();
    let (send, release) = oneshot::channel();
    tokio::spawn(async move {
      let (mut stream, _) = listener.accept().await.unwrap();
      seen.send(read_http_request(&mut stream).await).unwrap();
      release.await.unwrap();
      let response = format!(
        "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
      );
      stream.write_all(response.as_bytes()).await.unwrap();
    });
    (login, fake, request, send)
  }

  #[tokio::test]
  async fn a_queue_summary_uses_the_language_at_publication() {
    use crate::core::i18n::Language;
    let network = unauthenticated_network().await;
    let mut report = ResolveReport {
      resolved: vec![serde_json::from_value(json!({"uri":"file:///literal.mp3", "name":"Literal {title}", "artists":[], "album":"", "duration_ms":0})).unwrap()],
      unresolved: vec!["raw {error}".into()],
      ..ResolveReport::default()
    };
    let expected = Language::ZhHant
      .strings()
      .status
      .infra
      .dj_status(&report.summary_with(Language::ZhHant.strings(), 1));
    let mut guard = network.app.lock().await;
    let publish = network.queue_resolved_tracks(&mut report, None);
    tokio::pin!(publish);
    assert!(futures::poll!(&mut publish).is_pending());
    guard.user_config.behavior.language = Language::ZhHant;
    guard.refresh_language();
    drop(guard);
    assert!(publish.await.is_some());
    assert_eq!(
      network.app.lock().await.status_message(),
      Some(expected.as_str())
    );
  }

  #[tokio::test]
  async fn a_queue_summary_cannot_mix_languages_between_locks() {
    use crate::core::i18n::Language;
    let network = unauthenticated_network().await;
    let mut report = ResolveReport {
      resolved: vec![serde_json::from_value(json!({"uri":"file:///literal.mp3", "name":"Literal {title}", "artists":[], "album":"", "duration_ms":0})).unwrap()],
      ..ResolveReport::default()
    };
    let expected = |language: Language| {
      language
        .strings()
        .status
        .infra
        .dj_status(&report.summary_with(language.strings(), 1))
    };
    let english = expected(Language::En);
    let traditional = expected(Language::ZhHant);
    let held = network.app.lock().await;
    let publish = network.queue_resolved_tracks(&mut report, None);
    let switch = async {
      let mut app = network.app.lock().await;
      let already_published = app.status_message().is_some();
      app.user_config.behavior.language = Language::ZhHant;
      app.refresh_language();
      already_published
    };
    tokio::pin!(publish, switch);
    // Tokio's mutex queues the language change after the first publication lock.
    assert!(futures::poll!(&mut publish).is_pending());
    assert!(futures::poll!(&mut switch).is_pending());
    drop(held);
    let (_, already_published) = tokio::join!(publish, switch);
    let expected = if already_published {
      english
    } else {
      traditional
    };
    assert_eq!(
      network.app.lock().await.status_message(),
      Some(expected.as_str())
    );
  }

  #[cfg(all(feature = "local-files", feature = "soloist"))]
  #[tokio::test]
  async fn dj_queue_reports_only_tracks_the_player_accepted() {
    use crate::infra::soloist::{protocol::Event, SoloistLink};

    let (mut network, events) = network_with_events();
    let (link, _commands) = SoloistLink::test_pair("soloist-device");
    {
      let mut app = network.app.lock().await;
      app.spotify_connected = true;
      app.install_soloist_link(link.clone());
      app.apply_soloist_event(
        &link,
        &Event::Auth {
          logged_in: true,
          is_active: false,
        },
      );
      assert!(app.spotify_external_device_active() && app.soloist_is_target());
    }
    let body = serde_json::to_string(&crate::core::test_helpers::full_track(
      TEST_TRACK_ID,
      "Spotify Pick",
    ))
    .unwrap();
    let (login, _fake, request, release) = spotify_reply("200 OK", body).await;
    release.send(()).unwrap();
    network.spotify = Some(login);
    let spotify = format!("spotify:track:{TEST_TRACK_ID}");
    let file = "file:/music/pick.flac";
    let items = vec![QueueItem::Uri(spotify.clone()), QueueItem::Uri(file.into())];
    let outcome = network.dj_queue(items, false, Vec::new(), None).await;
    assert!(request.await.unwrap().starts_with("GET /v1/tracks/"));
    assert!(!outcome.is_error, "{}", outcome.text);
    assert!(
      outcome
        .text
        .starts_with("Queued 1 track(s):\nSpotify Pick — Test Artist"),
      "{}",
      outcome.text
    );
    let data = outcome.structured.unwrap();
    assert_eq!(data["queued"], json!(["Spotify Pick — Test Artist"]));
    assert_eq!(data["refused"], json!([file]));
    assert!(matches!(events.try_recv(), Ok(IoEvent::AddItemToQueue(uri)) if uri == spotify));
    {
      let app = network.app.lock().await;
      assert!(app.native_queue.is_empty() && app.dj.queued_uris.is_empty());
      assert_eq!(app.status_message(), Some("DJ: queued 1"));
    }

    let outcome = network
      .dj_queue(vec![QueueItem::Uri(file.into())], false, Vec::new(), None)
      .await;
    assert!(
      outcome.is_error && outcome.text.contains(file),
      "{}",
      outcome.text
    );
  }

  #[tokio::test]
  async fn dj_search_only_suggests_retry_when_the_playlist_index_is_pending() {
    let search = json!({"tracks": {
      "items": [crate::core::test_helpers::full_track(TEST_TRACK_ID, "Pick")],
      "href": "", "limit": 5, "offset": 0, "next": null, "previous": null, "total": 1
    }})
    .to_string();
    for (kind, indexing, retry, requests_index) in [
      (Some(TurnKind::Refill), false, false, false),
      (Some(TurnKind::Refill), true, true, false),
      (Some(TurnKind::Ask), false, true, true),
      (None, false, true, true),
    ] {
      let (network, events) = network_with_events();
      let turn = {
        let mut app = network.app.lock().await;
        app.dj.auto_queue = true;
        app.dj.library_indexing = indexing;
        kind.map(|kind| DjTurn {
          generation: app.dj.generation,
          seq: app.dj.begin_turn(kind),
        })
      };
      let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
      let (_login, _fake) =
        fake_spotify_login(format!("http://{}/v1/", listener.local_addr().unwrap()));
      let search = search.clone();
      let server = tokio::spawn(async move {
        for (path, body) in [
          ("GET /v1/search?", search.as_str()),
          ("GET /v1/me/library/contains?", "[true]"),
        ] {
          let (mut stream, _) = listener.accept().await.unwrap();
          let request = read_http_request(&mut stream).await;
          assert!(request.starts_with(path), "{request}");
          let response = format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len());
          stream.write_all(response.as_bytes()).await.unwrap();
        }
      });
      let outcome = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        network.dj_search("Pick", 5, turn),
      )
      .await
      .unwrap();
      server.await.unwrap();
      let data = outcome.structured.unwrap();
      assert_eq!(data["ownership_complete"], false);
      assert_eq!(data["tracks"][0]["owned"], true);
      assert_eq!(
        matches!(events.try_recv(), Ok(IoEvent::DjIndexLibrary)),
        requests_index
      );
      assert!(
        outcome.text.contains("Liked Songs only"),
        "{}",
        outcome.text
      );
      assert_eq!(
        outcome.text.contains("Search again"),
        retry,
        "{}",
        outcome.text
      );
      assert_eq!(
        outcome.text.contains("exclude_owned"),
        !retry,
        "{}",
        outcome.text
      );
    }
  }

  #[test]
  fn a_transfer_picks_a_device_by_id_then_by_name() {
    let devices: Vec<rspotify::model::device::Device> = serde_json::from_value(json!([
      {"id": "d1", "name": "Kitchen", "type": "Speaker", "is_active": false,
       "is_private_session": false, "is_restricted": false, "volume_percent": 50},
      {"id": "kitchen", "name": "Phone", "type": "Smartphone", "is_active": true,
       "is_private_session": false, "is_restricted": false, "volume_percent": 50},
    ]))
    .unwrap();
    assert_eq!(
      pick_device(&devices, "kitchen"),
      Some(("kitchen".to_string(), "Phone".to_string())),
      "an exact id wins over a name"
    );
    assert_eq!(
      pick_device(&devices, "KITCHEN "),
      None,
      "the name match is exact apart from case"
    );
    assert_eq!(
      pick_device(&devices, "Kitchen"),
      Some(("d1".to_string(), "Kitchen".to_string()))
    );
    assert_eq!(pick_device(&devices, "Car"), None);
  }

  #[test]
  fn tool_lines_are_numbered_with_their_uri() {
    let items = vec![
      json!({"title": "One", "uri": "spotify:track:1"}),
      json!({"title": "Two", "uri": null}),
    ];
    assert_eq!(
      numbered(&items),
      "1. One [spotify:track:1]\n2. Two [no uri]"
    );
  }

  async fn unauthenticated_network() -> Network {
    let app = Arc::new(Mutex::new(App::default()));
    Network::new(None, &app)
  }

  fn network_with_events() -> (Network, std::sync::mpsc::Receiver<IoEvent>) {
    let (tx, rx) = std::sync::mpsc::channel();
    let app = Arc::new(Mutex::new(App::new(
      tx,
      crate::core::user_config::UserConfig::new(),
      None,
    )));
    (Network::new(None, &app), rx)
  }

  #[tokio::test]
  async fn play_now_without_a_spotify_session_reports_an_error_without_dispatching() {
    let (mut network, events) = network_with_events();
    let outcome = network
      .dj_tool_outcome(
        DjToolCall::PlayNow {
          uri: "spotify:track:abc".into(),
        },
        None,
      )
      .await;

    assert!(outcome.is_error);
    assert!(outcome.text.contains("no Spotify session"));
    assert!(matches!(
      events.try_recv(),
      Err(std::sync::mpsc::TryRecvError::Empty)
    ));
  }

  #[tokio::test]
  async fn play_now_reports_missing_source_features_without_dispatching() {
    for uri in [
      "file:/music/a.flac",
      "subsonic:track:1",
      "youtube:abc",
      "qobuz:track:1",
    ] {
      let Some(feature) = crate::core::queue::missing_source_feature(uri) else {
        continue;
      };
      let (network, events) = network_with_events();
      let outcome = Network::answer_dj_tool_call_without_spotify(
        &network.app,
        DjToolCall::PlayNow { uri: uri.into() },
        None,
      )
      .await;

      assert!(outcome.is_error, "{uri}: {}", outcome.text);
      assert!(outcome.text.contains(feature), "{}", outcome.text);
      assert!(matches!(
        events.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
      ));
    }
  }

  #[cfg(feature = "local-files")]
  #[tokio::test]
  async fn play_now_requests_an_off_screen_track_without_claiming_it_started() {
    let (network, events) = network_with_events();
    let uri = "file:/music/off-screen.flac";
    let outcome = Network::answer_dj_tool_call_without_spotify(
      &network.app,
      DjToolCall::PlayNow { uri: uri.into() },
      None,
    )
    .await;

    // No pump is running: the reply can only acknowledge the queued request.
    assert!(!outcome.is_error, "{}", outcome.text);
    assert_eq!(outcome.text, format!("Asked the player to start {uri}. Playback has not been confirmed; use get_now_playing to check."));
    assert!(
      matches!(events.try_recv(), Ok(IoEvent::StartPlayback(None, Some(uris), Some(0))) if uris == [uri])
    );
    assert!(matches!(
      events.try_recv(),
      Err(std::sync::mpsc::TryRecvError::Empty)
    ));
  }

  #[cfg(feature = "subsonic")]
  #[tokio::test]
  async fn play_now_reports_an_unconfigured_subsonic_source_without_dispatching() {
    let (network, events) = network_with_events();
    let outcome = Network::answer_dj_tool_call_without_spotify(
      &network.app,
      DjToolCall::PlayNow {
        uri: "subsonic:track:1".into(),
      },
      None,
    )
    .await;

    assert!(outcome.is_error);
    assert_eq!(
      outcome.text,
      crate::core::i18n::EN.status.infra.subsonic_no_server
    );
    assert!(matches!(
      events.try_recv(),
      Err(std::sync::mpsc::TryRecvError::Empty)
    ));
    assert!(play_now_source_error("subsonic:track:1", true, false).is_none());
  }

  #[cfg(feature = "qobuz")]
  #[test]
  fn play_now_requires_a_qobuz_login_before_requesting_playback() {
    // Pass readiness facts rather than changing the process-wide login cache.
    let outcome = play_now_source_error("qobuz:track:51628161", false, false).unwrap();
    assert!(outcome.is_error);
    assert_eq!(
      outcome.text,
      crate::core::i18n::EN.status.infra.qobuz_login_expired
    );
    assert!(play_now_source_error("qobuz:track:51628161", false, true).is_none());
  }

  #[cfg(any(
    feature = "local-files",
    feature = "youtube",
    feature = "subsonic",
    feature = "qobuz"
  ))]
  #[test]
  fn play_now_reports_malformed_uris_for_compiled_sources() {
    let cases: &[(&str, &str)] = &[
      #[cfg(feature = "local-files")]
      ("file://[", "Invalid local file URI"),
      #[cfg(feature = "youtube")]
      ("youtube:id&x=1", "Invalid YouTube URI"),
      #[cfg(feature = "subsonic")]
      ("subsonic:album:1", "Invalid Subsonic track URI"),
      #[cfg(feature = "subsonic")]
      ("subsonic:track:", "Invalid Subsonic track URI"),
      #[cfg(feature = "qobuz")]
      ("qobuz:album:1", "Invalid Qobuz track URI"),
      #[cfg(feature = "qobuz")]
      ("qobuz:track:", "Invalid Qobuz track URI"),
    ];
    for (uri, message) in cases {
      let outcome = play_now_source_error(uri, true, true).unwrap();
      assert!(outcome.is_error, "{uri}");
      assert!(outcome.text.contains(message), "{}", outcome.text);
    }
  }

  #[tokio::test]
  async fn play_now_only_requests_a_catalogue_match_and_reports_a_missing_track() {
    for exists in [true, false] {
      let (network, events) = network_with_events();
      let uri = &format!("spotify:track:{TEST_TRACK_ID}");
      let body = if exists {
        serde_json::to_string(&crate::core::test_helpers::full_track(
          TEST_TRACK_ID,
          "Test Song",
        ))
        .unwrap()
      } else {
        "null".to_string()
      };
      let (_login, _fake, request, release) = spotify_reply("200 OK", body).await;
      release.send(()).unwrap();
      let outcome = network.dj_play_now(uri.clone(), None).await;
      assert!(request
        .await
        .unwrap()
        .starts_with("GET /v1/tracks/7o2AeQZzfCERsRmOM86EcB "));
      assert_eq!(outcome.is_error, !exists, "{}", outcome.text);
      if exists {
        assert_eq!(outcome.text, "Asked the player to start Test Song — Test Artist. Playback has not been confirmed; use get_now_playing to check.");
        assert_eq!(outcome.structured.as_ref().unwrap()["uri"], uri.as_str());
        assert!(
          matches!(events.try_recv(), Ok(IoEvent::StartPlayback(None, Some(uris), Some(0))) if uris == [uri.as_str()])
        );
      } else {
        assert!(outcome.text.contains("not in the catalogue"));
        assert!(matches!(
          events.try_recv(),
          Err(std::sync::mpsc::TryRecvError::Empty)
        ));
      }
    }
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
        None,
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
    // Needs no network, so it skips the auth check and reaches the catch-all
    // arm, which still answers; the point is that the responder is consumed.
    network
      .run_dj_tool_call(DjToolCall::SkipTrack, tx, None)
      .await;
    assert!(rx.await.is_ok());
  }

  #[cfg(feature = "local-files")]
  fn local_batch() -> DjToolCall {
    DjToolCall::QueueTracks {
      items: vec![QueueItem::Uri("file:/music/a.flac".into())],
      exclude_owned: true,
      extra_skip_keys: Vec::new(),
    }
  }

  #[cfg(feature = "local-files")]
  #[tokio::test]
  async fn a_batch_with_no_spotify_item_queues_without_a_session_or_a_crawl() {
    let app = Arc::new(Mutex::new(App::default()));

    let outcome = Network::answer_dj_tool_call_without_spotify(&app, local_batch(), None).await;

    assert!(!outcome.is_error, "{}", outcome.text);
    assert!(outcome.text.contains("Queued 1"), "{}", outcome.text);
    assert!(
      outcome.text.contains("only checks Spotify"),
      "{}",
      outcome.text
    );
    let app = app.lock().await;
    assert!(app.dj.library.is_none() && !app.dj.library_indexing);
    assert_eq!(app.native_queue.len(), 1);
  }

  #[cfg(feature = "local-files")]
  #[tokio::test]
  async fn a_call_whose_caller_gave_up_runs_nothing() {
    let mut network = unauthenticated_network().await;
    let (tx, rx) = oneshot::channel();
    drop(rx);

    network.run_dj_tool_call(local_batch(), tx, None).await;

    assert_eq!(network.app.lock().await.native_queue.len(), 0);
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

    let ResolveReport {
      resolved,
      unresolved: missing,
      ..
    } = network.tracks_for_uris(&uris).await;
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
    let missing = network
      .tracks_for_uris(&["radio:https://example.com/s.aac".to_string()])
      .await
      .unresolved;
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
    let ResolveReport {
      resolved,
      unresolved: missing,
      ..
    } = network.tracks_for_uris(&uris).await;
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
        None,
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
    let detail = nothing_queued_detail(&owned, &crate::core::i18n::EN);
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
    let detail = nothing_queued_detail(&mixed, &crate::core::i18n::EN);
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
    assert!(!nothing_queued_detail(&ResolveReport::default(), &crate::core::i18n::EN).is_empty());
  }

  #[tokio::test]
  async fn a_radio_uri_is_reported_as_a_stream_rather_than_queued_as_a_track() {
    // A live stream is not a finite track, and `add_track_to_native_queue`
    // rejects it — so queueing one would leave a row that can never advance.
    // The reason has to say *stream*: told "not found", the model learns the
    // station does not exist and goes looking for another spelling.
    let network = unauthenticated_network().await;
    let ResolveReport {
      resolved,
      unresolved: missing,
      ..
    } = network.tracks_for_uris(&["radio:somafm".to_string()]).await;
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

  #[tokio::test]
  async fn a_failed_library_index_moves_the_dj_revision_and_reports_it() {
    let mut network = unauthenticated_network().await;
    let before = network.app.lock().await.dj.revision;

    network.dj_index_library().await;

    let app = network.app.lock().await;
    assert!(
      app.dj.revision >= before + 2,
      "the finished crawl needs a revision past the start"
    );
    assert!(app
      .dj
      .transcript
      .last()
      .is_some_and(|line| line.text.contains("could not read your playlists")));
    assert!(!app.dj.library_indexing);
  }
}

// ---------------------------------------------------------------------------
// The in-TUI DJ
// ---------------------------------------------------------------------------

#[cfg(feature = "ai-dj")]
mod in_tui {
  use super::*;
  use crate::core::app::App;
  use crate::infra::dj::agent::{Turn, TurnOutcome};
  use crate::infra::dj::brain::DjBrain;
  use crate::infra::dj::exec::AppExecutor;
  use crate::infra::dj::session;
  use crate::infra::dj::{AskDjRequest, DjState, QUEUE_LOW_WATER};
  use std::sync::Arc;
  use std::time::Instant;

  impl Network {
    /// Service lane: run one DJ turn.
    ///
    /// The turn loop lives in `dj::agent`; this assembles what it needs and
    /// reports how it ended. Tool calls that need the catalogue go down the
    /// Spotify lane from inside the loop, because this lane's `Network` has no
    /// Spotify client.
    ///
    /// Writes no `you` line: the handler already pushed what the listener typed,
    /// synchronously, so it is on screen while this lane is still waiting on the
    /// brain. See `AskDjRequest::extra_instruction`.
    pub async fn ask_dj(&mut self, request: AskDjRequest) {
      let mut id = DjTurn {
        generation: request.generation,
        seq: request.turn_seq,
      };
      let Some(outcome) = self.run_turn(&request, &mut id).await.transpose() else {
        return;
      };
      let mut app = self.app.lock().await;
      let current = id.is_current(&app);
      app.dj.finish_turn(id.seq);
      if !current {
        return;
      }
      let refill = (app.dj.turn_kind == TurnKind::Refill).then_some(app.dj.refill_progress);
      if refill == Some(RefillProgress::Reset) || matches!(&outcome, Ok(turn) if turn.abandoned) {
        return;
      }
      let playback_requested = refill == Some(RefillProgress::PlaybackRequested);
      if playback_requested {
        app.dj.reset_refill_backoff();
      }
      let failed_refill = refill == Some(RefillProgress::Waiting);
      let spotify_wait = self.rate_gate.rate_limit_deadline();
      match outcome {
        Ok(turn) => {
          if failed_refill {
            let reason = turn
              .tool_error
              .map(|error| error.display_text(app.language()).to_string());
            record_empty_refill(&mut app, reason, spotify_wait);
            return;
          }
          // The listener's words only become the standing auto-queue direction
          // when the turn actually did something. Otherwise "what did I play last
          // week?" would silently steer every later refill. A vibe the model set
          // itself is more considered than the raw sentence, so it wins.
          if turn.acted && !turn.vibe_set {
            if let Some(vibe) = request.vibe_on_success {
              app.dj.vibe = Some(vibe);
              app.dj.bump_revision();
            }
          }
          // A refill happens while the listener is on another screen, so the
          // transcript is not a surface they can see. The interactive case is
          // deliberately left transcript-only.
          if request.must_act && !playback_requested {
            if turn.acted || refill == Some(RefillProgress::Queued) {
              let strings = app.strings();
              app.set_status_message(strings.status.infra.dj_topped_up.to_string(), 5);
            } else {
              // A turn that had to queue and did not is a failure, not a quiet
              // success. A vibe shift has just *emptied* the queue, and with
              // auto-queue off nothing retries — so saying nothing leaves the
              // listener with silence and no explanation.
              let message = app.strings().status.infra.dj_nothing_queued;
              app.dj.push_line(DjLine::system(message));
              app.set_error_status_message(message.to_string(), 6);
            }
          }
        }
        Err(e) => {
          log::warn!("DJ: turn failed: {e}");
          // Naming the backend matters when several are configurable: "DJ error"
          // alone does not say whether to check the CLI or the key.
          let backend = session::build_brain(&app.user_config.behavior)
            .map(|brain| brain.label())
            .unwrap_or("DJ");
          let message = app.strings().status.infra.dj_error(backend, &e.to_string());
          if failed_refill {
            record_empty_refill(&mut app, Some(message), spotify_wait);
            return;
          }
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
      id: &mut DjTurn,
    ) -> anyhow::Result<Option<TurnOutcome>> {
      let (brain, io_tx): (DjBrain, _) = {
        let mut app = self.app.lock().await;
        if !id.is_current(&app) {
          log::debug!("DJ: dropping a stale turn before it starts");
          app.dj.finish_turn(id.seq);
          return Ok(None);
        }
        if app.dj.turn_kind == TurnKind::Refill {
          if let Some(until) = self.rate_gate.rate_limit_deadline() {
            app.dj.refill_retry_at = app.dj.refill_retry_at.max(Some(until));
            app.dj.finish_turn(id.seq);
            return Ok(None);
          }
        }
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

      let mut turn = Turn {
        app: &self.app,
        brain: &brain,
        context: &context,
        executor: &executor,
        id: *id,
        must_act: request.must_act,
      };
      let outcome = turn.run().await;
      *id = turn.id;
      outcome.map(Some)
    }

    /// Service lane: refill the queue because it is running low.
    pub async fn dj_top_up(&mut self, generation: u64, turn_seq: u64) {
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

  fn record_empty_refill(app: &mut App, reason: Option<String>, spotify_wait: Option<Instant>) {
    if app.dj.failed_refill(Instant::now()) {
      let strings = &app.strings().status.infra;
      let reason = reason.as_deref().unwrap_or(strings.dj_refill_empty);
      let message = strings.dj_auto_queue_stopped(app.dj.refill_failures as usize, reason);
      app.dj.push_line(DjLine::system(message.clone()));
      app.dj.refill_stop_status = Some(message.clone());
      app.set_error_status_message(message, 8);
    } else {
      app.dj.refill_retry_at = app.dj.refill_retry_at.max(spotify_wait);
      // Intermediate attempts stay in the transcript. Only the final failure
      // takes over the status bar, so errors cannot monopolize it on every tick.
      let reason = reason.unwrap_or_else(|| app.strings().status.infra.dj_refill_empty.into());
      app.dj.push_line(DjLine::system(reason));
    }
  }

  /// Refill when auto-queue is on, the DJ is idle, the native queue is low,
  /// and both the local retry delay and shared Spotify rate limit have elapsed.
  ///
  /// External Spotify Connect and Apple Music use their own queues, so the
  /// native queue's length cannot guide refills while they own playback.
  pub fn wants_top_up(
    queue_len: usize,
    dj: &DjState,
    playback_elsewhere: bool,
    spotify_wait: Option<Instant>,
  ) -> bool {
    !playback_elsewhere
      && dj.auto_queue
      && !dj.thinking
      && queue_len <= QUEUE_LOW_WATER
      && dj
        .refill_retry_at
        .max(spotify_wait)
        .is_none_or(|at| Instant::now() >= at)
  }

  #[cfg(test)]
  mod tests {
    use super::*;
    use crate::core::app::App;

    #[tokio::test]
    async fn a_stale_dj_refill_cannot_publish_resolved_tracks() {
      for replaced in [false, true] {
        let (network, app) = network_with_app().await;
        let mut app = app.lock().await;
        app.dj.auto_queue = true;
        let turn = DjTurn {
          generation: app.dj.generation,
          seq: app.dj.begin_turn(TurnKind::Refill),
        };
        let mut report = ResolveReport {
          resolved: vec![TrackInfo::from(&crate::core::test_helpers::full_track(
            "0000000000000000000001",
            "Pick",
          ))],
          ..ResolveReport::default()
        };
        let publish = network.queue_resolved_tracks(&mut report, Some(turn));
        tokio::pin!(publish);
        assert!(futures::poll!(&mut publish).is_pending());
        if replaced {
          app.dj.begin_turn(TurnKind::Refill);
        } else {
          app.dj.bump_generation();
        }
        drop(app);

        assert_eq!(publish.await, None);
        let app = network.app.lock().await;
        assert!(app.native_queue.is_empty());
        assert_eq!(app.dj.refill_progress, RefillProgress::Waiting);
        assert!(app.status_message().is_none());
      }
    }

    #[test]
    fn reenabling_auto_queue_dismisses_only_the_dj_stop_error() {
      for replacement in [None, Some("Playback error")] {
        let mut app = App::default();
        app.dj.auto_queue = true;
        app.dj.refill_failures = crate::infra::dj::REFILL_FAILURE_LIMIT - 1;
        record_empty_refill(&mut app, None, None);
        assert!(app.status_message_is_error());
        if let Some(message) = replacement {
          app.set_error_status_message(message, 8);
        }
        app.apply(Action::ToggleDjAutoQueue);
        assert_eq!(app.status_message_is_error(), replacement.is_some());
        assert_eq!(
          app.status_message(),
          replacement.or(Some(app.strings().status.app.dj_auto_queue_on))
        );
      }
    }

    #[test]
    fn dj_shutdown_messages_render_each_reason_in_all_languages() {
      use crate::core::i18n::Language;

      let tool_error =
        ToolOutcome::error_localized(|strings| strings.status.tools.catalogue_unavailable);
      for (language, expected) in [
        (
          Language::En,
          [
            "DJ auto-queue stopped after 5 failed refills. DJ error (agent CLI): usage limit reached. Re-enable auto-queue to try again.",
            "DJ auto-queue stopped after 5 failed refills. spotatui has no Spotify session, so the catalogue is unavailable. Log in from the spotatui UI, then try again. Re-enable auto-queue to try again.",
            "DJ auto-queue stopped after 5 failed refills. No tracks were queued. Re-enable auto-queue to try again.",
          ],
        ),
        (
          Language::ZhHans,
          [
            "DJ 自动补充队列连续 5 次失败，已关闭。DJ 错误（agent CLI）：usage limit reached。请重新开启自动补充队列以重试。",
            "DJ 自动补充队列连续 5 次失败，已关闭。spotatui 尚未登录 Spotify，无法访问曲库。请在 spotatui 界面中登录后重试。请重新开启自动补充队列以重试。",
            "DJ 自动补充队列连续 5 次失败，已关闭。这次未加入任何曲目。请重新开启自动补充队列以重试。",
          ],
        ),
        (
          Language::ZhHant,
          [
            "DJ 自動補充佇列連續 5 次失敗，已關閉。DJ 錯誤（agent CLI）：usage limit reached。請重新開啟自動補充佇列以重試。",
            "DJ 自動補充佇列連續 5 次失敗，已關閉。spotatui 尚未登入 Spotify，無法存取曲庫。請在 spotatui 介面中登入後重試。請重新開啟自動補充佇列以重試。",
            "DJ 自動補充佇列連續 5 次失敗，已關閉。這次未加入任何曲目。請重新開啟自動補充佇列以重試。",
          ],
        ),
      ] {
        let strings = &language.strings().status.infra;
        let reasons = [
          Some(strings.dj_error("agent CLI", "usage limit reached")),
          Some(tool_error.display_text(language).to_string()),
          None,
        ];
        for (reason, expected) in reasons.into_iter().zip(expected) {
          let mut app = App::default();
          app.user_config.behavior.language = language;
          app.refresh_language();
          app.dj.auto_queue = true;
          record_empty_refill(&mut app, reason.clone(), None);
          assert_eq!(
            app.dj.transcript.last().unwrap().text,
            reason.as_deref().unwrap_or(strings.dj_refill_empty),
          );
          assert!(app.status_message().is_none());

          app.dj.refill_failures = crate::infra::dj::REFILL_FAILURE_LIMIT - 1;
          record_empty_refill(&mut app, reason, None);
          assert!(!app.dj.auto_queue);
          assert_eq!(app.status_message(), Some(expected));
          assert!(app.status_message_is_error());
          assert_eq!(app.dj.transcript.last().unwrap().text, expected);
        }
      }
    }

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
      let network = Network::new(None, &app);
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
    async fn a_vibe_shift_that_queues_nothing_reports_the_failure_immediately() {
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
        app.status_message_is_error(),
        "and it is reported as a failure"
      );
    }

    #[cfg(unix)]
    fn refill_stub(
      body: &str,
    ) -> (
      Network,
      std::sync::mpsc::Receiver<IoEvent>,
      tempfile::TempDir,
    ) {
      let stub = tempfile::tempdir().unwrap();
      let path = stub.path().join("fake-agent");
      std::fs::write(
        &path,
        format!("cat >/dev/null\necho run >> \"$(dirname \"$0\")/calls\"\n{body}\n"),
      )
      .unwrap();
      let (tx, rx) = std::sync::mpsc::channel();
      let mut config = crate::core::user_config::UserConfig::new();
      config.behavior.dj_backend = "agent_cli".into();
      // The shell reads the script. Direct exec can fail if a concurrent fork
      // inherited a writable descriptor for the file.
      config.behavior.dj_agent_command = vec!["/bin/sh".into(), path.to_string_lossy().into()];
      config.behavior.dj_agent_prompt_via = Some("stdin".into());
      let app = Arc::new(tokio::sync::Mutex::new(App::new(tx, config, None)));
      (Network::new(None, &app), rx, stub)
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_dispatched_dj_refill_waits_for_a_new_spotify_rate_limit_window() {
      use std::time::Duration;

      let (mut network, _events, stub) = refill_stub(r#"echo '{"tool_calls":[]}'"#);
      network.rate_gate = Default::default();
      let seq = {
        let mut app = network.app.lock().await;
        app.dj.auto_queue = true;
        app.dj.refill_failures = 1;
        app.dj.begin_turn(TurnKind::Refill)
      };
      network.rate_gate.rate_limit_for(Duration::from_secs(60));
      let deadline = network.rate_gate.rate_limit_deadline().unwrap();

      network.dj_top_up(0, seq).await;

      let app = network.app.lock().await;
      assert!(
        !stub.path().join("calls").exists(),
        "the refill must wait before calling the brain"
      );
      assert!(app.dj.auto_queue && !app.dj.thinking);
      assert_eq!(app.dj.refill_failures, 1);
      assert_eq!(app.dj.refill_retry_at, Some(deadline));
      assert!(app.dj.transcript.is_empty());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_failed_spotify_refill_reports_why_it_could_not_queue() {
      use super::super::tests::{spotify_reply, TEST_TRACK_ID};

      let named = json!([{"title":"Pick","artist":"Test Artist"}]);
      let uri = json!([{"uri":format!("spotify:track:{TEST_TRACK_ID}")}]);
      let track = serde_json::to_string(&crate::core::test_helpers::full_track(
        TEST_TRACK_ID,
        "Pick",
      ))
      .unwrap();
      let mut cases = vec![
        (
          "429 Too Many Requests",
          "{}",
          "/v1/search?",
          &named,
          false,
          "rate limited by Spotify",
        ),
        (
          "200 OK",
          r#"{"tracks":{"items":[],"href":"","limit":5,"offset":0,"next":null,"previous":null,"total":0}}"#,
          "/v1/search?",
          &named,
          false,
          "Could not find: Pick",
        ),
        (
          "429 Too Many Requests",
          "{}",
          "/v1/tracks/",
          &uri,
          false,
          "rate limited by Spotify",
        ),
        (
          "404 Not Found",
          "{}",
          "/v1/tracks/",
          &uri,
          false,
          "Could not find: spotify:track:",
        ),
        (
          "503 Service Unavailable",
          "catalogue unavailable",
          "/v1/tracks/",
          &uri,
          false,
          "Search failed:",
        ),
        (
          "503 Service Unavailable",
          "library unavailable",
          "/v1/me ",
          &uri,
          true,
          "library unavailable",
        ),
      ];
      if !cfg!(feature = "streaming") {
        cases.push((
          "200 OK",
          &track,
          "/v1/tracks/",
          &uri,
          false,
          "native queue needs the `streaming` feature",
        ));
      }
      for (status, body, path, tracks, exclude_owned, reason) in cases {
        let call = json!({"tool_calls":[{"name":"queue_tracks","arguments":{"tracks":tracks,"exclude_owned":exclude_owned}}]});
        let (mut service, events, _stub) = refill_stub(&format!(
          r#"if [ "$(wc -l < "$(dirname "$0")/calls")" -eq 1 ]; then echo '{call}'; else echo '{{"say":"Done.","tool_calls":[]}}'; fi"#
        ));
        let (login, _fake, request, release) = spotify_reply(status, body.into()).await;
        release.send(()).unwrap();
        service.rate_gate = Default::default();
        let mut lane = service.spotify_lane_network();
        lane.spotify = Some(login);
        let app = Arc::clone(&service.app);
        let seq = {
          let mut app = app.lock().await;
          app.user_config.behavior.ytdlp_path = Some("/nonexistent/spotatui-test/yt-dlp".into());
          app.dj.auto_queue = true;
          app.dj.begin_turn(TurnKind::Refill)
        };
        let tool = async {
          loop {
            if let Ok(IoEvent::DjToolCall(call)) = events.try_recv() {
              let (call, responder, turn) = *call;
              lane.run_dj_tool_call(call, responder, turn).await;
              break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
          }
        };
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
          tokio::join!(service.dj_top_up(0, seq), tool);
        })
        .await
        .unwrap();
        assert!(request.await.unwrap().starts_with(&format!("GET {path}")));

        let window = lane.rate_gate.rate_limit_deadline();
        let app = app.lock().await;
        let message = &app.dj.transcript.last().unwrap().text;
        assert!(message.contains(reason), "{message}");
        assert!(app.native_queue.is_empty() && app.status_message().is_none());
        assert!(!app.dj.library_indexing);
        assert_eq!(app.dj.refill_failures, 1);
        assert_eq!(window.is_some(), status.starts_with("429"));
        if let Some(until) = window {
          assert!(!message.contains("Could not find"), "{message}");
          assert!(app.dj.refill_retry_at.is_some_and(|retry| retry >= until));
        }
      }
    }

    #[cfg(unix)]
    async fn failed_refill_waits_before_retrying(body: &str, expected_calls: usize, reason: &str) {
      let (mut service, rx, stub) = refill_stub(body);
      let app = Arc::clone(&service.app);
      app.lock().await.dj.auto_queue = true;
      let mut spotify_lane = Network::new(None, &app);
      for attempt in 1..=crate::infra::dj::REFILL_FAILURE_LIMIT {
        let (generation, turn_seq) = {
          let mut app = app.lock().await;
          assert!(wants_top_up(0, &app.dj, false, None));
          let seq = app.dj.begin_turn(crate::infra::dj::TurnKind::Refill);
          (app.dj.generation, seq)
        };
        let tools = async {
          loop {
            while let Ok(event) = rx.try_recv() {
              if let IoEvent::DjToolCall(call) = event {
                let (call, responder, turn) = *call;
                spotify_lane.run_dj_tool_call(call, responder, turn).await;
              }
            }
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
          }
        };
        tokio::select! {
          () = service.dj_top_up(generation, turn_seq) => {},
          () = tools => unreachable!(),
          () = tokio::time::sleep(std::time::Duration::from_secs(10)) => panic!("fake refill timed out"),
        }
        let calls = std::fs::read_to_string(stub.path().join("calls")).unwrap();
        assert_eq!(calls.lines().count(), expected_calls * usize::from(attempt));
        let mut app = app.lock().await;
        assert!(app.native_queue.is_empty());
        assert!(!app.dj.thinking);
        assert_eq!(app.dj.refill_failures, attempt);
        assert!(app
          .dj
          .transcript
          .iter()
          .any(|line| line.text.contains(reason)));
        assert!(
          !wants_top_up(0, &app.dj, false, None),
          "a failed refill must not launch another agent on the next tick"
        );
        if attempt < crate::infra::dj::REFILL_FAILURE_LIMIT {
          assert!(app.dj.auto_queue);
          assert!(
            !app.status_message_is_error(),
            "only stopping raises an error toast"
          );
          // Advance the deadline rather than sleeping through the backoff.
          app.dj.refill_retry_at = Some(std::time::Instant::now());
        } else {
          assert!(!app.dj.auto_queue);
          let status = app.status_message().unwrap();
          assert!(status.contains("auto-queue stopped"), "{status}");
          assert!(status.contains(reason), "{status}");
          assert!(app.status_message_is_error());
          assert_eq!(
            app
              .dj
              .transcript
              .iter()
              .filter(|line| line.text.contains("auto-queue stopped"))
              .count(),
            1
          );
        }
      }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn dj_auto_queue_backs_off_when_the_agent_cli_hits_its_usage_limit() {
      failed_refill_waits_before_retrying(
        "echo 'usage limit reached' >&2; exit 1",
        1,
        "usage limit reached",
      )
      .await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn failed_queue_tools_supply_the_reason_when_auto_queue_stops() {
      for (tracks, reason) in [
        (r#"[{"uri":"spotify:track:abc"}]"#, "no Spotify session"),
        ("[]", "Invalid arguments"),
      ] {
        failed_refill_waits_before_retrying(
          &format!(r#"echo '{{"tool_calls":[{{"name":"queue_tracks","arguments":{{"tracks":{tracks}}}}}]}}'"#),
          crate::infra::dj::agent::MAX_STEPS_MUST_ACT,
          reason,
        )
        .await;
      }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn dj_auto_queue_backs_off_when_the_model_only_talks_or_changes_the_vibe() {
      failed_refill_waits_before_retrying(
        r#"echo '{"say":"No matches.","tool_calls":[]}'"#,
        1,
        "No tracks were queued",
      )
      .await;
      failed_refill_waits_before_retrying(
        r#"echo '{"tool_calls":[{"name":"set_dj_vibe","arguments":{"vibe":"quiet"}}]}'"#,
        crate::infra::dj::agent::MAX_STEPS_MUST_ACT,
        "No tracks were queued",
      )
      .await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_stale_dj_refill_failure_leaves_newer_retry_and_progress_state_untouched() {
      let failures = crate::infra::dj::REFILL_FAILURE_LIMIT - 1;
      for (action, expected_failures, expected_progress, replacement) in [
        (
          Action::SetDjVibe(Some("quiet".into())),
          failures,
          RefillProgress::Waiting,
          false,
        ),
        (Action::ToggleDjAutoQueue, 0, RefillProgress::Queued, true),
        (Action::ClearQueue, 0, RefillProgress::Reset, false),
      ] {
        let (mut network, _events, stub) = refill_stub(
          r#"
        touch "$(dirname "$0")/ready"
        while [ ! -e "$(dirname "$0")/release" ]; do sleep 0.01; done
        echo 'usage limit reached' >&2
        exit 1
      "#,
        );
        let app = Arc::clone(&network.app);
        let newer_retry_at = Instant::now() + std::time::Duration::from_secs(60);
        let seq = {
          let mut app = app.lock().await;
          app.dj.auto_queue = true;
          app.dj.refill_failures = failures;
          app.dj.begin_turn(crate::infra::dj::TurnKind::Refill)
        };
        let listener = async {
          while !stub.path().join("ready").exists() {
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
          }
          let mut app = app.lock().await;
          app.apply(action);
          if replacement {
            app.apply(Action::ToggleDjAutoQueue);
            app.dj.step = Some((1, 2));
            app.dj.refill_retry_at = Some(newer_retry_at);
            app.dj.refill_progress = RefillProgress::Queued;
          }
          std::fs::write(stub.path().join("release"), b"").unwrap();
        };
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
          tokio::join!(network.dj_top_up(0, seq), listener);
        })
        .await
        .unwrap();
        let app = app.lock().await;
        assert!(app.dj.auto_queue);
        assert_eq!(
          app.dj.thinking, replacement,
          "only a replacement keeps the progress flag"
        );
        assert_eq!(app.dj.step, replacement.then_some((1, 2)));
        assert_eq!(app.dj.refill_failures, expected_failures);
        assert_eq!(
          app.dj.refill_retry_at,
          replacement.then_some(newer_retry_at)
        );
        assert_eq!(app.dj.refill_progress, expected_progress);
        assert!(!app.status_message_is_error());
      }
    }

    #[cfg(all(unix, feature = "local-files"))]
    #[tokio::test]
    async fn a_dj_refill_resets_backoff_only_for_new_tracks_or_playback_requests() {
      let uri = "file:///dj-test.flac";
      for (play_now, already_queued, fail_followup) in [
        (false, false, false),
        (false, false, true),
        (false, true, false),
        (true, false, false),
        (true, false, true),
      ] {
        let call = if play_now {
          json!({"name":"play_now","arguments":{"uri":uri}})
        } else {
          json!({"name":"queue_tracks","arguments":{"tracks":[{"uri":uri}]}})
        };
        let step = format!("echo '{}'", json!({"tool_calls":[call]}));
        let followup = if fail_followup {
          "echo 'usage limit reached' >&2; exit 1"
        } else if play_now {
          r#"echo '{"say":"Done.","tool_calls":[]}'"#
        } else {
          &step
        };
        let (mut network, events, _stub) = refill_stub(&format!(
          r#"if [ "$(wc -l < "$(dirname "$0")/calls")" -eq 1 ]; then {step}; else {followup}; fi"#
        ));
        let app = Arc::clone(&network.app);
        let seq = {
          let mut app = app.lock().await;
          app.dj.auto_queue = true;
          if already_queued {
            app.apply(Action::QueueTrack(
              serde_json::from_value(json!({
                "uri":uri, "name":"Manual pick", "artists":[], "album":"", "duration_ms":0
              }))
              .unwrap(),
            ));
            app.clear_status_message();
          }
          app.dj.failed_refill(Instant::now());
          app.dj.begin_turn(TurnKind::Refill)
        };
        network.dj_top_up(0, seq).await;
        if play_now {
          assert!(matches!(
            events.try_recv(),
            Ok(IoEvent::StartPlayback(None, Some(uris), Some(0))) if uris == [uri]
          ));
        }
        let app = app.lock().await;
        assert_eq!(app.native_queue.len(), usize::from(!play_now));
        assert_eq!(app.dj.refill_failures, if already_queued { 2 } else { 0 });
        assert_eq!(app.dj.refill_retry_at.is_some(), already_queued);
        assert!(app.dj.auto_queue);
        assert!(!app.dj.thinking);
        if fail_followup {
          assert!(app
            .status_message()
            .unwrap()
            .contains("usage limit reached"));
        } else if play_now || already_queued {
          assert!(app.status_message().is_none());
        } else {
          assert_eq!(
            app.status_message(),
            Some(app.strings().status.infra.dj_topped_up)
          );
        }
        if already_queued {
          assert!(app
            .dj
            .transcript
            .last()
            .unwrap()
            .text
            .contains("Already queued"));
        }
      }
    }

    #[test]
    fn top_up_is_wanted_only_when_enabled_idle_and_low() {
      let mut dj = DjState {
        auto_queue: true,
        ..DjState::default()
      };
      assert!(
        wants_top_up(0, &dj, false, None),
        "an empty queue wants a refill"
      );
      assert!(
        wants_top_up(QUEUE_LOW_WATER, &dj, false, None),
        "at the watermark"
      );
      assert!(
        !wants_top_up(QUEUE_LOW_WATER + 1, &dj, false, None),
        "above it"
      );

      let now = Instant::now();
      assert!(
        wants_top_up(0, &dj, false, Some(now)),
        "the shared window has expired"
      );
      assert!(
        !wants_top_up(
          0,
          &dj,
          false,
          Some(now + std::time::Duration::from_secs(60))
        ),
        "Spotify is rate limiting requests"
      );

      dj.thinking = true;
      assert!(
        !wants_top_up(0, &dj, false, None),
        "must not stack refills while one is in flight"
      );

      dj.thinking = false;
      dj.auto_queue = false;
      assert!(!wants_top_up(0, &dj, false, None), "DJ off means no refill");
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
        !wants_top_up(0, &dj, true, None),
        "an empty native queue proves nothing on a remote device"
      );
      assert!(
        wants_top_up(0, &dj, false, None),
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
      assert!(wants_top_up(0, &app.dj, false, None));
    }
  }
}

#[cfg(feature = "ai-dj")]
pub use in_tui::wants_top_up;
