import type { Action } from "./bindings/Action";
import type { AlbumInfo } from "./bindings/AlbumInfo";
import type { ArtistInfo } from "./bindings/ArtistInfo";
import type { LibraryNode } from "./bindings/LibraryNode";
import type { PlaylistInfo } from "./bindings/PlaylistInfo";
import type { QueuePayload } from "./bindings/QueuePayload";
import type { Source } from "./bindings/Source";
import type { SourcePlaylists } from "./bindings/SourcePlaylists";
import type { TrackInfo } from "./bindings/TrackInfo";
import type { OpenView, View } from "./nav";

/** A sidebar row of the active source: a playlist, a folder or a radio station. */
export interface SidebarRow {
  uri: string;
  name: string;
  count: number | null;
  /** The cover; optional so a hand-made row needs none. */
  image?: string | null;
}

const PLAYLIST_KEYS: Record<
  Exclude<Source, "Radio" | "AppleMusic">,
  "spotify" | "local" | "subsonic" | "qobuz" | "youtube"
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
  // Apple Music has no browse screens yet, so no sidebar rows.
  if (source === "AppleMusic") return [];
  if (source === "Radio")
    return playlists.radio.map((station) => ({
      uri: station.uri ?? station.name,
      name: station.name,
      count: null,
      image: station.image_url,
    }));
  return playlists[PLAYLIST_KEYS[source]].map(sidebarRow);
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

/** "1 song", "42 songs". */
export function songCount(count: number): string {
  return count === 1 ? "1 song" : `${count} songs`;
}

/** The cover of a library playlist of any source, by URI. */
export function playlistImage(
  playlists: SourcePlaylists | null,
  uri: string,
): string | null {
  if (!playlists) return null;
  const lists = Object.values(PLAYLIST_KEYS).map((key) => playlists[key]);
  return (
    lists.flat().find((playlist) => playlist.uri === uri)?.image_url ?? null
  );
}

/** The sidebar row of a playlist. */
export function sidebarRow(playlist: PlaylistInfo): SidebarRow {
  return {
    uri: playlist.uri,
    name: playlist.name,
    count: playlist.track_count,
    image: playlist.image_url,
  };
}

/** Whether the sidebar offers "Create playlist" for a source. */
export function createsPlaylists(source: Source | null): boolean {
  return source === "Spotify" || source === "YouTube";
}

/** The request that creates a playlist named `name`; null for a blank name or another source. */
export function createPlaylistAction(
  source: Source | null,
  name: string,
): Action | null {
  const trimmed = name.trim();
  if (!trimmed) return null;
  if (source === "Spotify")
    return { CreatePlaylist: { name: trimmed, track_uris: [] } };
  if (source === "YouTube") return { CreateYouTubePlaylist: trimmed };
  return null;
}

/** A playlist's menu entry and the question it asks before it acts. */
export interface PlaylistRemoval {
  uri: string;
  name: string;
  /** The menu entry. */
  label: string;
  action: Action;
  title: string;
  body: string;
  /** The dialog's confirm button. */
  confirm: string;
}

/** What removing a sidebar playlist means; null where the source cannot remove one. */
export function playlistRemoval(
  source: Source | null,
  row: { uri: string; name: string },
  playlists: SourcePlaylists,
): PlaylistRemoval | null {
  const { uri, name } = row;
  if (source === "YouTube")
    return {
      uri,
      name,
      label: "Delete",
      action: { DeletePlaylist: uri },
      title: "Delete playlist?",
      body: `This will delete ${name} from spotatui. This cannot be undone.`,
      confirm: "Delete",
    };
  if (source !== "Spotify") return null;
  const info = playlists.spotify.find((playlist) => playlist.uri === uri);
  // Editable is the ownership signal; a collaborative playlist is editable but not owned.
  const owned =
    playlists.editable.includes(uri) && !(info?.collaborative ?? false);
  const verb = owned ? "delete" : "remove";
  return {
    uri,
    name,
    label: owned ? "Delete" : "Remove from Your Library",
    action: { UnfollowPlaylist: uri },
    title: owned ? "Delete from Your Library?" : "Remove from Your Library?",
    body: `This will ${verb} ${name} from Your Library.`,
    confirm: owned ? "Delete" : "Remove",
  };
}

/** Sends a confirmed removal; the open view of that playlist goes home. */
export function removePlaylist(
  removal: PlaylistRemoval,
  view: View,
  send: (action: Action) => void,
  open: OpenView,
) {
  send(removal.action);
  if (view.page === "playlist" && view.uri === removal.uri)
    open({ page: "home" });
}

/** Where the focus goes when the removal question closes: a confirmed removal skips the opener, whose row the refresh drops. */
export function focusAfterAsk<T extends { isConnected: boolean }>(
  confirmed: boolean,
  opener: T,
  stable: (T | null)[],
): T | null {
  const order = confirmed ? stable : [opener, ...stable];
  return order.find((element) => element?.isConnected) ?? null;
}

/** One row of the folder tree: a folder or a playlist, indented by depth. */
export type TreeRow =
  | {
      kind: "folder";
      uri: string;
      name: string;
      depth: number;
      expanded: boolean;
    }
  | { kind: "playlist"; depth: number; row: SidebarRow };

/** The rootlist as rows: a collapsed folder hides its children, and playlists outside the tree end the list. */
export function libraryTree(
  nodes: LibraryNode[],
  playlists: PlaylistInfo[],
  expanded: ReadonlySet<string>,
): TreeRow[] {
  const byUri = new Map(playlists.map((playlist) => [playlist.uri, playlist]));
  const placed = new Set<string>();
  const rows: TreeRow[] = [];
  const walk = (list: LibraryNode[], depth: number, shown: boolean) => {
    for (const node of list) {
      if (node.kind === "folder") {
        const open = expanded.has(node.uri);
        if (shown)
          rows.push({
            kind: "folder",
            uri: node.uri,
            name: node.name,
            depth,
            expanded: open,
          });
        walk(node.children, depth + 1, shown && open);
        continue;
      }
      placed.add(node.uri);
      const playlist = byUri.get(node.uri);
      if (shown && playlist)
        rows.push({ kind: "playlist", depth, row: sidebarRow(playlist) });
    }
  };
  walk(nodes, 0, true);
  for (const playlist of playlists)
    if (!placed.has(playlist.uri))
      rows.push({ kind: "playlist", depth: 0, row: sidebarRow(playlist) });
  return rows;
}

/** The album sort select's options, the values `behavior.default_sort_saved_albums` takes. */
export const ALBUM_SORTS: { value: string; label: string }[] = [
  { value: "default", label: "Recently added" },
  { value: "name", label: "Name (A-Z)" },
  { value: "name:desc", label: "Name (Z-A)" },
  { value: "date_added", label: "Date added (oldest first)" },
  { value: "date_added:desc", label: "Date added (newest first)" },
  { value: "artist", label: "Artist (A-Z)" },
  { value: "artist:desc", label: "Artist (Z-A)" },
  { value: "release_date", label: "Release date (oldest first)" },
  { value: "release_date:desc", label: "Release date (newest first)" },
];

/** The artist sort select's options, the values `behavior.default_sort_saved_artists` takes. */
export const ARTIST_SORTS: { value: string; label: string }[] = [
  { value: "default", label: "Recently added" },
  { value: "name", label: "Name (A-Z)" },
  { value: "name:desc", label: "Name (Z-A)" },
];

/** A known option's value, else "default". */
export function sortChoice(
  options: { value: string }[],
  value: string | null | undefined,
): string {
  return options.find((option) => option.value === value)?.value ?? "default";
}

const byText = (a: string, b: string) =>
  a.toLowerCase().localeCompare(b.toLowerCase());

function sortBy<T>(
  items: T[],
  desc: boolean,
  compare: (a: T, b: T) => number,
): T[] {
  return [...items].sort(desc ? (a, b) => compare(b, a) : compare);
}

/** Saved albums in `sort` order. The server sends them newest-added first, and ties keep that order. */
export function sortAlbums(albums: AlbumInfo[], sort: string): AlbumInfo[] {
  const [field, order] = sortChoice(ALBUM_SORTS, sort).split(":");
  const desc = order === "desc";
  switch (field) {
    case "date_added":
      return desc ? albums : [...albums].reverse();
    case "name":
      return sortBy(albums, desc, (a, b) => byText(a.name, b.name));
    case "artist":
      return sortBy(albums, desc, (a, b) =>
        byText(a.artists[0]?.name ?? "", b.artists[0]?.name ?? ""),
      );
    case "release_date":
      // Spotify dates are YYYY, YYYY-MM or YYYY-MM-DD, so the strings order by time.
      return sortBy(albums, desc, (a, b) => {
        const x = a.release_date ?? "";
        const y = b.release_date ?? "";
        return x < y ? -1 : x > y ? 1 : 0;
      });
    default:
      return albums;
  }
}

/** Followed artists in `sort` order; "default" keeps the server's order. */
export function sortArtists(artists: ArtistInfo[], sort: string): ArtistInfo[] {
  const [field, order] = sortChoice(ARTIST_SORTS, sort).split(":");
  if (field !== "name") return artists;
  return sortBy(artists, order === "desc", (a, b) => byText(a.name, b.name));
}

/** Whether a laid-out scroll box shows its last `margin` pixels; a box too short to scroll does, a hidden one does not. */
export function nearEnd(
  box: { scrollTop: number; scrollHeight: number; clientHeight: number },
  margin = 240,
): boolean {
  return (
    box.clientHeight > 0 &&
    box.scrollHeight - box.scrollTop - box.clientHeight <= margin
  );
}

/** Whether to ask for the next page; `askedFor` is the list pushed at the last ask, so each push asks once. */
export function wantsNextPage(
  atEnd: boolean,
  hasMore: boolean,
  list: unknown,
  askedFor: unknown,
): boolean {
  return atEnd && hasMore && list !== askedFor;
}

const FOLDERS_KEY = "spotatui.library.expandedFolders";

/** The folder URIs this browser left open; empty when storage is blocked or holds junk. */
export function loadExpanded(storage: Pick<Storage, "getItem">): Set<string> {
  try {
    const value: unknown = JSON.parse(storage.getItem(FOLDERS_KEY) ?? "[]");
    return new Set(
      Array.isArray(value)
        ? value.filter((item): item is string => typeof item === "string")
        : [],
    );
  } catch {
    return new Set();
  }
}

export function saveExpanded(
  storage: Pick<Storage, "setItem">,
  expanded: ReadonlySet<string>,
) {
  try {
    storage.setItem(FOLDERS_KEY, JSON.stringify([...expanded]));
  } catch {
    // A full or blocked storage only forgets the open folders.
  }
}
