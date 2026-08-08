//! "Do they already have this?" — the two halves of the avoid-library filter.
//!
//! The two halves are deliberately asymmetric, because the API is:
//!
//! * **Liked Songs** — `me/tracks/contains` answers exactly, for up to 50 IDs per
//!   call. A DJ batch is at most [`MAX_BATCH`](super::MAX_BATCH) tracks, so one
//!   call per turn against the canonical truth. Nothing cached, nothing stale.
//! * **Playlists** — no `contains` equivalent exists, so the only way to know is
//!   to crawl every playlist and keep the answer. Hence [`build_index`] and the
//!   [`DjLibrary`] cached on `App`.
//!
//! Both live here rather than in `network/dj.rs` for the same reason
//! [`super::resolve`] does: they take `&Network` and *return* an answer instead of
//! writing into `App`.

use super::{dedupe_key, DjLibrary, MAX_LIBRARY_TRACKS};
use crate::infra::network::Network;
use serde::Deserialize;
use std::collections::HashSet;

/// Playlists per page. The Spotify maximum, to keep the round trips down.
const PLAYLIST_PAGE: usize = 50;
/// Tracks per page. Also the maximum.
const TRACK_PAGE: usize = 100;
/// IDs per `contains` call. A hard API limit, not a tuning choice.
const CONTAINS_CHUNK: usize = 50;

#[derive(Deserialize)]
struct PlaylistPage {
  #[serde(default)]
  items: Vec<PlaylistEntry>,
  #[serde(default)]
  next: Option<String>,
}

#[derive(Deserialize)]
struct PlaylistEntry {
  id: Option<String>,
  #[serde(default)]
  collaborative: bool,
  #[serde(default)]
  owner: Option<Owner>,
}

#[derive(Deserialize)]
struct Owner {
  id: Option<String>,
}

#[derive(Deserialize)]
struct TrackPage {
  #[serde(default)]
  items: Vec<TrackItem>,
  #[serde(default)]
  next: Option<String>,
}

#[derive(Deserialize)]
struct TrackItem {
  /// `playlists/{id}/tracks` returns `track`; the newer item shape returns `item`,
  /// and `normalize_spotify_payload` only rewrites that when `added_at` is present
  /// — which the `fields` filter below strips. Accept both rather than depend on
  /// which one arrives.
  #[serde(default, alias = "item")]
  track: Option<TrackRef>,
}

#[derive(Deserialize)]
struct TrackRef {
  id: Option<String>,
  name: Option<String>,
  #[serde(default)]
  artists: Vec<ArtistRef>,
}

#[derive(Deserialize)]
struct ArtistRef {
  name: Option<String>,
}

/// Whether this playlist counts as one of the listener's own.
///
/// Followed playlists are excluded on purpose. Discover Weekly, Release Radar and
/// the large editorial playlists hold thousands of tracks between them; counting
/// those as "already have it" would reject nearly every good recommendation and
/// read as the filter being broken.
fn is_own_playlist(entry: &PlaylistEntry, owner_id: &str) -> bool {
  entry.collaborative
    || entry
      .owner
      .as_ref()
      .and_then(|owner| owner.id.as_deref())
      .is_some_and(|id| id == owner_id)
}

/// Crawl the listener's own playlists into a [`DjLibrary`].
///
/// Errors only if the playlist listing itself fails. A single unreadable playlist
/// is skipped and logged: a partial index still filters, while failing the whole
/// crawl would turn the feature off for one bad playlist.
pub async fn build_index(net: &Network, owner_id: &str) -> anyhow::Result<DjLibrary> {
  let mut library = DjLibrary::default();
  let playlist_ids = own_playlist_ids(net, owner_id).await?;
  library.playlists = playlist_ids.len();

  for id in playlist_ids {
    if library.tracks >= MAX_LIBRARY_TRACKS {
      library.truncated = true;
      break;
    }
    match collect_playlist(net, &id, &mut library).await {
      Ok(()) => {}
      Err(e) => log::debug!("DJ: skipping playlist {id} while indexing: {e}"),
    }
  }

  Ok(library)
}

/// IDs of every playlist the listener owns or collaborates on.
async fn own_playlist_ids(net: &Network, owner_id: &str) -> anyhow::Result<Vec<String>> {
  let mut ids = Vec::new();
  let mut offset = 0usize;
  loop {
    let page = net
      .spotify_get_typed::<PlaylistPage>(
        "me/playlists",
        &[
          ("limit", PLAYLIST_PAGE.to_string()),
          ("offset", offset.to_string()),
        ],
      )
      .await?;

    let count = page.items.len();
    ids.extend(
      page
        .items
        .iter()
        .filter(|entry| is_own_playlist(entry, owner_id))
        .filter_map(|entry| entry.id.clone()),
    );

    if is_last_page(count, PLAYLIST_PAGE, page.next.is_some()) {
      break;
    }
    offset += PLAYLIST_PAGE;
  }
  Ok(ids)
}

/// Whether to stop paginating.
///
/// Deliberately trusts neither signal alone. A short page is **not** proof of the
/// end: `normalize_spotify_payload` strips the nulls Spotify returns for
/// unreadable playlists and removed tracks, so a middle page can arrive short. And
/// `next` is requested through a `fields` filter on the track endpoint, so it
/// cannot be assumed present either. Stop only when both agree, with `count == 0`
/// as the backstop that makes an unterminated `next` chain finite rather than an
/// infinite loop.
///
/// Cost of being wrong in the safe direction: one extra empty request for a
/// playlist whose length is an exact multiple of the page size.
fn is_last_page(count: usize, page_size: usize, has_next: bool) -> bool {
  count == 0 || (!has_next && count < page_size)
}

/// Fold one playlist's tracks into the index.
async fn collect_playlist(
  net: &Network,
  playlist_id: &str,
  library: &mut DjLibrary,
) -> anyhow::Result<()> {
  let path = format!("playlists/{playlist_id}/tracks");
  let mut offset = 0usize;
  loop {
    let page = net
      .spotify_get_typed::<TrackPage>(
        &path,
        &[
          // Only what the two gates need. A playlist page is otherwise ~40x this
          // size, and the crawl pays that per 100 tracks.
          (
            "fields",
            "items(track(id,name,artists(name))),next".to_string(),
          ),
          ("limit", TRACK_PAGE.to_string()),
          ("offset", offset.to_string()),
        ],
      )
      .await?;

    let count = page.items.len();
    for item in &page.items {
      // Episodes and removed tracks arrive as a null `track`.
      let Some(track) = item.track.as_ref() else {
        continue;
      };
      let Some(name) = track.name.as_deref().filter(|name| !name.is_empty()) else {
        continue;
      };
      let artists = track
        .artists
        .iter()
        .filter_map(|artist| artist.name.clone())
        .collect::<Vec<_>>()
        .join(", ");
      library.keys.insert(dedupe_key(name, &artists));
      if let Some(id) = track.id.clone() {
        library.ids.insert(id);
      }
      library.tracks += 1;
      if library.tracks >= MAX_LIBRARY_TRACKS {
        library.truncated = true;
        return Ok(());
      }
    }

    if is_last_page(count, TRACK_PAGE, page.next.is_some()) {
      break;
    }
    offset += TRACK_PAGE;
  }
  Ok(())
}

/// Which of these track IDs are in the listener's Liked Songs.
///
/// Fails **open**: a check that errors returns no IDs for that chunk, so the
/// tracks are kept. Dropping a good recommendation because a lookup failed is the
/// worse outcome — the user asked for music, not for an empty queue.
pub async fn liked_among(net: &Network, ids: &[String]) -> HashSet<String> {
  let mut liked = HashSet::new();
  for chunk in ids.chunks(CONTAINS_CHUNK) {
    match net
      .spotify_get_typed::<Vec<bool>>("me/tracks/contains", &[("ids", chunk.join(","))])
      .await
    {
      Ok(flags) => {
        for (id, is_liked) in chunk.iter().zip(flags) {
          if is_liked {
            liked.insert(id.clone());
          }
        }
      }
      Err(e) => log::debug!("DJ: Liked Songs check failed, keeping the batch: {e}"),
    }
  }
  liked
}

#[cfg(test)]
mod tests {
  use super::*;

  fn entry(id: &str, owner: Option<&str>, collaborative: bool) -> PlaylistEntry {
    PlaylistEntry {
      id: Some(id.to_string()),
      collaborative,
      owner: owner.map(|id| Owner {
        id: Some(id.to_string()),
      }),
    }
  }

  #[test]
  fn own_and_collaborative_playlists_count_but_followed_ones_do_not() {
    assert!(is_own_playlist(&entry("a", Some("me"), false), "me"));
    // Collaborative: someone else owns it, but the listener files tracks into it.
    assert!(is_own_playlist(&entry("b", Some("friend"), true), "me"));
    // The case that matters: Discover Weekly would otherwise reject nearly
    // everything worth recommending.
    assert!(!is_own_playlist(&entry("c", Some("spotify"), false), "me"));
    assert!(!is_own_playlist(&entry("d", None, false), "me"));
  }

  #[test]
  fn a_playlist_page_parses_the_minimal_shape() {
    let page: PlaylistPage = serde_json::from_str(
      r#"{"items":[{"id":"p1","collaborative":false,"owner":{"id":"me"}}],"next":null}"#,
    )
    .unwrap();
    assert_eq!(page.items.len(), 1);
    assert!(page.next.is_none());
  }

  #[test]
  fn a_track_page_parses_both_the_track_and_item_shapes() {
    let page: TrackPage = serde_json::from_str(
      r#"{"items":[
        {"track":{"id":"t1","name":"Nude","artists":[{"name":"Radiohead"}]}},
        {"item":{"id":"t2","name":"Nightcall","artists":[{"name":"Kavinsky"}]}},
        {"track":null}
      ],"next":null}"#,
    )
    .unwrap();
    assert_eq!(page.items.len(), 3);
    assert!(page.items[0].track.is_some());
    assert!(page.items[1].track.is_some(), "the `item` alias must parse");
    assert!(page.items[2].track.is_none(), "a null track is skipped");
  }

  #[test]
  fn pagination_stops_only_when_both_signals_agree() {
    // A full page always continues, even with `next` absent: the track endpoint
    // asks for `next` through a `fields` filter, so its absence proves nothing.
    assert!(!is_last_page(100, 100, false));
    assert!(!is_last_page(100, 100, true));
    // Short but `next` present: a middle page that lost rows to null-stripping.
    // Stopping here would silently index only part of a playlist.
    assert!(!is_last_page(48, 50, true));
    // Short and no `next`: genuinely the end.
    assert!(is_last_page(48, 50, false));
    // Empty always stops, even with `next` set — the backstop against a `next`
    // chain that never terminates.
    assert!(is_last_page(0, 50, true));
  }

  #[test]
  fn the_contains_chunk_matches_the_api_limit() {
    // Not a tuning knob: 51 IDs is a 400 from Spotify.
    assert_eq!(CONTAINS_CHUNK, 50);
  }

  #[test]
  fn the_index_summary_admits_when_it_stopped_short() {
    let full = DjLibrary {
      tracks: 12,
      playlists: 3,
      ..DjLibrary::default()
    };
    assert!(full.summary().contains("12 track(s) across 3 playlist(s)"));
    assert!(!full.summary().contains("stopped at"));

    let capped = DjLibrary {
      truncated: true,
      ..full
    };
    assert!(
      capped.summary().contains("not filtered"),
      "a partial index has to say so, or the filter looks broken"
    );
  }
}
