import { EN, type GuiStrings, fmt } from "./i18n";
import type { Action } from "./bindings/Action";
import type { AlbumInfo } from "./bindings/AlbumInfo";
import type { ArtistInfo } from "./bindings/ArtistInfo";
import type { LibraryNode } from "./bindings/LibraryNode";
import type { ListTarget } from "./bindings/ListTarget";
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
  Exclude<Source, "Radio">,
  "spotify" | "local" | "subsonic" | "qobuz" | "youtube" | "apple_music"
> = {
  Spotify: "spotify",
  Local: "local",
  Subsonic: "subsonic",
  Qobuz: "qobuz",
  YouTube: "youtube",
  AppleMusic: "apple_music",
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
      image: station.image_url,
    }));
  // A Music row whose count Music has not given carries a placeholder 0.
  const uncounted = new Set(
    source === "AppleMusic" ? playlists.apple_music_uncounted : [],
  );
  return playlists[PLAYLIST_KEYS[source]].map((playlist) => {
    const row = sidebarRow(playlist);
    return uncounted.has(row.uri) ? { ...row, count: null } : row;
  });
}

/** Every search term must occur in the fields; a blank query matches everything. */
export function matchesFilter(
  needle: string,
  ...fields: (string | null | undefined)[]
): boolean {
  const terms = needle.toLowerCase().split(/\s+/).filter(Boolean);
  const haystack = fields.filter(Boolean).join(" ").toLowerCase();
  return terms.every((term) => haystack.includes(term));
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
export function songCount(count: number, s: GuiStrings = EN): string {
  return count === 1 ? s.one_song : fmt(s.songs_count, { count });
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
export interface PlaylistRemovalTarget {
  uri: string;
  name: string;
  action: Action;
  kind: "delete" | "owned" | "unfollow";
}

export interface PlaylistRemoval extends PlaylistRemovalTarget {
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
  s: GuiStrings = EN,
): PlaylistRemoval | null {
  const { uri, name } = row;
  if (source !== "YouTube" && source !== "Spotify") return null;
  const info = playlists.spotify.find((playlist) => playlist.uri === uri);
  const owned =
    playlists.editable.includes(uri) && !(info?.collaborative ?? false);
  const target: PlaylistRemovalTarget = {
    uri,
    name,
    kind: source === "YouTube" ? "delete" : owned ? "owned" : "unfollow",
    action:
      source === "YouTube"
        ? { DeletePlaylist: uri }
        : { UnfollowPlaylist: uri },
  };
  return { ...target, ...removalText(target, s) };
}

/** Strip presentation text before retaining a menu or confirmation target. */
export function removalTarget({
  uri,
  name,
  action,
  kind,
}: PlaylistRemovalTarget): PlaylistRemovalTarget {
  return { uri, name, action, kind };
}

export function removalText(
  { kind, name }: PlaylistRemovalTarget,
  s: GuiStrings = EN,
) {
  if (kind === "delete")
    return {
      label: s.delete,
      title: s.delete_playlist_question,
      body: fmt(s.delete_playlist_body, { name }),
      confirm: s.delete,
    };
  const owned = kind === "owned";
  return {
    label: owned ? s.delete : s.remove_from_library,
    title: owned ? s.delete_library_question : s.remove_library_question,
    body: fmt(owned ? s.delete_library_body : s.remove_library_body, { name }),
    confirm: owned ? s.delete : s.remove,
  };
}

/** Sends a confirmed removal; the open view of that playlist goes home. */
export function removePlaylist(
  removal: PlaylistRemovalTarget,
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
export function albumSorts(
  s: GuiStrings = EN,
): { value: string; label: string }[] {
  return [
    { value: "default", label: s.sort_recent },
    { value: "name", label: s.sort_name },
    { value: "name:desc", label: s.sort_name_desc },
    { value: "date_added", label: s.sort_date },
    { value: "date_added:desc", label: s.sort_date_desc },
    { value: "artist", label: s.sort_artist },
    { value: "artist:desc", label: s.sort_artist_desc },
    { value: "release_date", label: s.sort_release },
    { value: "release_date:desc", label: s.sort_release_desc },
  ];
}
export const ALBUM_SORTS = albumSorts();

/** The artist sort select's options, the values `behavior.default_sort_saved_artists` takes. */
export function artistSorts(
  s: GuiStrings = EN,
): { value: string; label: string }[] {
  return [
    { value: "default", label: s.sort_recent },
    { value: "name", label: s.sort_name },
    { value: "name:desc", label: s.sort_name_desc },
  ];
}
export const ARTIST_SORTS = artistSorts();

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

/** The list a Library filter pages through: its LoadMore target, the list shown and whether more remain. */
export function libraryPager(
  filter: "Playlists" | "Albums" | "Artists" | "Podcasts",
  playlists: SourcePlaylists | null,
): { target: ListTarget; list: unknown; more: boolean } | null {
  switch (filter) {
    case "Albums":
      return {
        target: "SavedAlbums",
        list: playlists?.saved_albums,
        more: playlists?.saved_albums_has_more ?? false,
      };
    case "Artists":
      return {
        target: "SavedArtists",
        list: playlists?.followed_artists,
        more: playlists?.followed_artists_has_more ?? false,
      };
    case "Podcasts":
      return {
        target: "SavedShows",
        list: playlists?.saved_shows,
        more: playlists?.saved_shows_has_more ?? false,
      };
    case "Playlists":
      return null;
  }
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
