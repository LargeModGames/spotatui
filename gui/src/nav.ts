import type { Action } from "./bindings/Action";
import type { Source } from "./bindings/Source";

/** What the main view shows. A list page names the list it shows. */
export type View =
  | { page: "home" }
  | { page: "liked" }
  | { page: "playlist"; uri: string; name: string; source: Source }
  | { page: "album"; id: string }
  | { page: "artist"; id: string; name: string }
  | { page: "search" }
  | { page: "lyrics" }
  | { page: "discover" }
  | { page: "session" }
  | { page: "stats" }
  | { page: "party" }
  | { page: "health" }
  | { page: "settings" };

export type Page = View["page"];

/** Opens a view: pages call it, the shell fetches and navigates. */
export type OpenView = (view: View) => void;

export interface History {
  back: View[];
  now: View;
  forward: View[];
}

export const HOME: History = { back: [], now: { page: "home" }, forward: [] };

export function sameView(a: View, b: View): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

/** A new view drops the forward list; the view already shown changes nothing. */
export function push(history: History, view: View): History {
  if (sameView(history.now, view)) return history;
  return { back: [...history.back, history.now], now: view, forward: [] };
}

export function goBack(history: History): History {
  const now = history.back.at(-1);
  if (!now) return history;
  return {
    back: history.back.slice(0, -1),
    now,
    forward: [history.now, ...history.forward],
  };
}

export function goForward(history: History): History {
  const [now, ...forward] = history.forward;
  if (!now) return history;
  return { back: [...history.back, history.now], now, forward };
}

/** The fetch a view asks for when it opens; null for a page that loads by itself. */
export function openAction(view: View): Action | null {
  switch (view.page) {
    case "home":
      return { Navigate: "RecentlyPlayed" };
    case "liked":
      return { OpenLibrary: "LikedSongs" };
    case "playlist":
      return view.source === "Spotify"
        ? { Open: { Playlist: { id: view.uri, from_search: false } } }
        : { Open: { SourcePlaylist: view.uri } };
    case "album":
      return { Open: { Album: { id: view.id, from_search: false } } };
    case "artist":
      return { Open: { Artist: { id: view.id, name: view.name } } };
    case "stats":
      return { OpenLibrary: "Stats" };
    default:
      return null;
  }
}

/** What the channels already hold, so a revisited page can skip its fetch. */
export interface Loaded {
  spotify: boolean;
  liked: boolean;
  tableUri: string | null;
  albumId: string | null;
  artistId: string | null;
}

/** The fetch to send when a view shows: a new open always fetches, back and
 * forward only when the channel holds another item; Spotify data needs a session. */
export function fetchFor(
  view: View,
  fresh: boolean,
  loaded: Loaded,
): Action | null {
  const spotifyOnly =
    view.page === "home" ||
    view.page === "liked" ||
    view.page === "album" ||
    view.page === "artist" ||
    (view.page === "playlist" && view.source === "Spotify");
  if (spotifyOnly && !loaded.spotify) return null;
  if (!fresh) {
    const held =
      view.page === "home" ||
      (view.page === "liked" && loaded.liked) ||
      (view.page === "playlist" && loaded.tableUri === view.uri) ||
      (view.page === "album" && loaded.albumId === view.id) ||
      (view.page === "artist" && loaded.artistId === view.id);
    if (held) return null;
  }
  return openAction(view);
}
