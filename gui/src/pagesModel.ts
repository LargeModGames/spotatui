import { EN, type GuiStrings, fmt } from "./i18n";
import type { Action } from "./bindings/Action";
import type { AlbumInfo } from "./bindings/AlbumInfo";
import type { DiscoverPayload } from "./bindings/DiscoverPayload";
import type { DiscoverPlaylist } from "./bindings/DiscoverPlaylist";
import type { TrackInfo } from "./bindings/TrackInfo";
import type { TrackTablePayload } from "./bindings/TrackTablePayload";
import { playRequest, step } from "./libraryModel";
import type { View } from "./nav";

/** Whether a page still has to ask for the Made For You shelves; a closed socket drops the ask. */
export function asksDiscover(
  discover: DiscoverPayload | null,
  connected: boolean,
): boolean {
  return (
    connected &&
    discover !== null &&
    discover.available &&
    !discover.loading &&
    discover.shelves === null
  );
}

/** The playlist page a Made For You card opens. */
export function discoverView(playlist: DiscoverPlaylist): View {
  return {
    page: "playlist",
    uri: playlist.uri,
    name: playlist.name,
    source: "Spotify",
    image: playlist.image_url,
  };
}

/** "Album", "Single" or "Compilation" from the lowercased album type. */
export function albumKind(type: string | null, s: GuiStrings = EN): string {
  if (type === "single") return s.single;
  if (type === "compilation") return s.compilation;
  return s.album;
}

/** The year of a release date; null when unknown (Spotify says `0000`). */
export function yearOf(date: string | null): string | null {
  const year = date?.slice(0, 4) ?? "";
  return /^\d{4}$/.test(year) && year !== "0000" ? year : null;
}

/** "12 songs, 45 min", or "1 hr 5 min" from one hour. */
export function lengthLine(tracks: TrackInfo[], s: GuiStrings = EN): string {
  const songs = fmt(tracks.length === 1 ? s.one_song_count : s.songs_count, {
    count: tracks.length,
  });
  const minutes = Math.round(
    tracks.reduce((sum, track) => sum + track.duration_ms, 0) / 60_000,
  );
  const time =
    minutes >= 60
      ? fmt(s.hours_minutes, {
          hours: Math.floor(minutes / 60),
          minutes: minutes % 60,
        })
      : fmt(s.minutes_short, { minutes });
  return fmt(s.songs_duration, { songs, time });
}

/** A full date as "March 3, 2017"; a year or a month stays as given. */
export function releaseLine(
  date: string | null,
  s: GuiStrings = EN,
): string | null {
  if (!date || !yearOf(date)) return null;
  if (!/^\d{4}-\d{2}-\d{2}$/.test(date)) return date;
  return new Date(`${date}T00:00:00Z`).toLocaleDateString(s.date_locale, {
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
  done:
    | "copied_album_link"
    | "copied_song_link"
    | "copied_show_link"
    | "copied_episode_link"
    | "copy_party_done",
  send: (action: Action) => void,
  clipboard: Pick<Clipboard, "writeText"> | undefined,
  readStrings: () => GuiStrings = () => EN,
  failure: "copy_link_failed" | "copy_party_failed" = "copy_link_failed",
): Promise<void> {
  const failed = () => send({ NotifyError: [readStrings()[failure], 4] });
  const write = clipboard?.writeText(text);
  if (!write) {
    failed();
    return Promise.resolve();
  }
  return write.then(() => send({ Notify: [readStrings()[done], 3] }), failed);
}

/** The status revision a page opened at, and the id whose load an error after that open failed. */
export interface LoadLatch {
  openedAt: number | null;
  failedId: string | null;
}

/** The next latch: a fresh open or landed data clears it, an error after the open fails `id` until then. */
export function loadLatch(
  latch: LoadLatch,
  load: {
    id: string;
    fresh: boolean;
    landed: boolean;
    statusRev: number | null;
    statusError: boolean;
  },
): LoadLatch {
  if (load.fresh || load.landed)
    return { openedAt: load.statusRev, failedId: null };
  if (load.statusError && load.statusRev !== latch.openedAt)
    return { ...latch, failedId: load.id };
  return latch;
}
