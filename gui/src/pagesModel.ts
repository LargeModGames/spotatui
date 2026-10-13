import type { Action } from "./bindings/Action";
import type { AlbumInfo } from "./bindings/AlbumInfo";
import type { DiscoverPayload } from "./bindings/DiscoverPayload";
import type { TrackInfo } from "./bindings/TrackInfo";
import type { TrackTablePayload } from "./bindings/TrackTablePayload";
import { playRequest, step } from "./libraryModel";

/** One row of cards, as Spotify's Home shows it. */
export const SHELF_MAX = 8;
/** Liked Songs plus seven playlists fill the quick-access grid. */
export const QUICK_PLAYLISTS = 7;

/** The Home greeting for a local hour. */
export function greeting(hour: number): string {
  if (hour >= 5 && hour < 12) return "Good morning";
  if (hour >= 12 && hour < 18) return "Good afternoon";
  return "Good evening";
}

/** The first track of each album in play order, for the Recently played shelf. */
export function recentAlbums(recent: TrackInfo[]): TrackInfo[] {
  const seen = new Set<string>();
  return recent.filter((track) => {
    if (!track.album_id || seen.has(track.album_id)) return false;
    seen.add(track.album_id);
    return true;
  });
}

/** Which Discover list Home still has to ask for; each is asked for once. */
export function homeFetch(
  discover: DiscoverPayload | null,
  asked: { top: boolean; mix: boolean },
): "top" | "mix" | null {
  if (discover && (!discover.available || discover.loading)) return null;
  if (!asked.top && (discover?.top_tracks.length ?? 0) === 0) return "top";
  if (!asked.mix && discover && discover.artists_mix.length === 0) return "mix";
  return null;
}

/** "Album", "Single" or "Compilation" from the lowercased album type. */
export function albumKind(type: string | null): string {
  if (type === "single") return "Single";
  if (type === "compilation") return "Compilation";
  return "Album";
}

/** The year of a release date; null when unknown (Spotify says `0000`). */
export function yearOf(date: string | null): string | null {
  const year = date?.slice(0, 4) ?? "";
  return /^\d{4}$/.test(year) && year !== "0000" ? year : null;
}

/** "12 songs, 45 min", or "1 hr 5 min" from one hour. */
export function lengthLine(tracks: TrackInfo[]): string {
  const songs = `${tracks.length} ${tracks.length === 1 ? "song" : "songs"}`;
  const minutes = Math.round(
    tracks.reduce((sum, track) => sum + track.duration_ms, 0) / 60_000,
  );
  const time =
    minutes >= 60
      ? `${Math.floor(minutes / 60)} hr ${minutes % 60} min`
      : `${minutes} min`;
  return `${songs}, ${time}`;
}

/** A full date as "March 3, 2017"; a year or a month stays as given. */
export function releaseLine(date: string | null): string | null {
  if (!date || !yearOf(date)) return null;
  if (!/^\d{4}-\d{2}-\d{2}$/.test(date)) return date;
  return new Date(`${date}T00:00:00Z`).toLocaleDateString("en-US", {
    year: "numeric",
    month: "long",
    day: "numeric",
    timeZone: "UTC",
  });
}

/** Play an album from a track: its context with the offset, or its track URIs. */
export function albumPlay(album: AlbumInfo, index: number): Action | null {
  if (album.uri) return { PlayContext: { uri: album.uri, offset: index } };
  return playRequest(album.tracks, index);
}

/** What a key does in a track table: play or queue the cursor row, or move the cursor. */
export function trackKey(
  key: string,
  cursor: number,
  length: number,
):
  { kind: "play" } | { kind: "queue" } | { kind: "move"; next: number } | null {
  if (key === "Enter") return { kind: "play" };
  if (key === "q") return { kind: "queue" };
  const move = step(key, cursor, length, false);
  return move ? { kind: "move", next: move.next } : null;
}

/** The radio's rows; null while the shared track table holds other rows. */
export function radioRows(
  table: TrackTablePayload | null,
  seed: string,
): TrackInfo[] | null {
  return table && table.radio === seed ? table.tracks : null;
}

/** Copy a link and report it in a toast, the way the track menu does. */
export function copyLink(
  text: string,
  done: string,
  send: (action: Action) => void,
  clipboard: Pick<Clipboard, "writeText"> | undefined,
): Promise<void> {
  const failed = () => send({ NotifyError: ["Could not copy the link.", 4] });
  const write = clipboard?.writeText(text);
  if (!write) {
    failed();
    return Promise.resolve();
  }
  return write.then(() => send({ Notify: [done, 3] }), failed);
}
