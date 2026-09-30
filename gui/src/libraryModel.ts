import type { Action } from "./bindings/Action";
import type { QueuePayload } from "./bindings/QueuePayload";
import type { Source } from "./bindings/Source";
import type { SourcePlaylists } from "./bindings/SourcePlaylists";
import type { TrackInfo } from "./bindings/TrackInfo";

/** A sidebar row of the active source: a playlist, a folder or a radio station. */
export interface SidebarRow {
  uri: string;
  name: string;
  count: number | null;
}

const PLAYLIST_KEYS: Record<
  Exclude<Source, "Radio">,
  Exclude<keyof SourcePlaylists, "radio">
> = {
  Spotify: "spotify",
  Local: "local",
  Subsonic: "subsonic",
  Qobuz: "qobuz",
  YouTube: "youtube",
};

/** The queue after the playing track: the native queue first, then Spotify's while Spotify plays. */
export function upNext(
  queue: QueuePayload | undefined,
  spotifyPlays: boolean,
): TrackInfo[] {
  return [
    ...(queue?.native ?? []),
    ...((spotifyPlays && queue?.spotify.items) || []).flatMap((item) =>
      item.track ? [item.track] : [],
    ),
  ];
}

/** The sidebar list for a source; stations carry no count. */
export function playlistsFor(
  playlists: SourcePlaylists,
  source: Source,
): SidebarRow[] {
  if (source === "Radio")
    return playlists.radio.map((station) => ({
      uri: station.uri ?? station.name,
      name: station.name,
      count: null,
    }));
  return playlists[PLAYLIST_KEYS[source]].map((playlist) => ({
    uri: playlist.uri,
    name: playlist.name,
    count: playlist.track_count,
  }));
}

/** The cursor kept inside the list; -1 for an empty list. */
export function clampCursor(want: number, length: number): number {
  return Math.min(Math.max(want, 0), length - 1);
}

/** Where a key moves the cursor, and whether the list end asks for the next page. */
export function step(
  key: string,
  cursor: number,
  length: number,
  hasMore: boolean,
): { next: number; loadMore: boolean } | null {
  switch (key) {
    case "j":
    case "ArrowDown":
      return { next: cursor + 1, loadMore: cursor >= length - 1 && hasMore };
    case "k":
    case "ArrowUp":
      return { next: cursor - 1, loadMore: false };
    default:
      return null;
  }
}

/** A sidebar row's click: a station plays, any other row opens its list. */
export function openRow(source: Source, row: SidebarRow): Action | null {
  if (source === "Radio")
    return row.uri.startsWith("radio:")
      ? { PlayUris: { uris: [row.uri], offset: null } }
      : null;
  if (source === "Spotify")
    return { Open: { Playlist: { id: row.uri, from_search: false } } };
  return { Open: { SourcePlaylist: row.uri } };
}

/** Play an opened list from `cursor`: a Spotify playlist as its context, a source list as URIs. */
export function listPlayRequest(
  source: Source,
  listUri: string,
  tracks: TrackInfo[],
  cursor: number,
): Action | null {
  if (source !== "Spotify") return playRequest(tracks, cursor);
  const track = tracks[cursor]?.uri;
  return track ? { PlayTrackInContext: { context: listUri, track } } : null;
}

/** Play the list from `cursor`, the same request the terminal sends from Liked Songs. */
export function playRequest(
  tracks: TrackInfo[],
  cursor: number,
): Action | null {
  if (!tracks[cursor]?.uri) return null;
  const uris = tracks.flatMap((track) => (track.uri ? [track.uri] : []));
  const offset = tracks.slice(0, cursor).filter((track) => track.uri).length;
  return { PlayUris: { uris, offset } };
}
