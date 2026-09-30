import type { AlbumInfo } from "./bindings/AlbumInfo";
import type { LyricLine } from "./bindings/LyricLine";
import type { NowPlaying } from "./bindings/NowPlaying";
import type { TrackInfo } from "./bindings/TrackInfo";

/**
 * The playing row: by URI, else by title for a Spotify track on this very
 * album, which Spotify can relink to another URI. A same-titled song on
 * another album does not match.
 */
export function playingIndex(
  tracks: TrackInfo[],
  item: NowPlaying | null,
  albumName: string | null,
): number {
  if (!item) return -1;
  const byUri = tracks.findIndex((track) => track.uri === item.uri);
  if (byUri >= 0 || !item.uri?.startsWith("spotify:")) return byUri;
  if (albumName === null || albumName !== item.album) return -1;
  return tracks.findIndex((track) => track.name === item.title);
}

/**
 * The album is what plays next only when it is the Spotify context, in order,
 * with no queue slot in front of it.
 */
export function albumContext(
  album: AlbumInfo | null,
  item: NowPlaying | null,
  queueNow: boolean,
): boolean {
  return (
    album !== null &&
    item !== null &&
    album.uri !== null &&
    item.context_uri === album.uri &&
    !item.shuffle &&
    !queueNow &&
    playingIndex(album.tracks, item, album.name) >= 0
  );
}

/** Time into the album and time left, from the finished tracks and the position. */
export function albumClock(
  tracks: TrackInfo[],
  index: number,
  positionMs: number,
): { elapsed: number; left: number } {
  const total = tracks.reduce((sum, track) => sum + track.duration_ms, 0);
  const before = tracks
    .slice(0, Math.max(index, 0))
    .reduce((sum, track) => sum + track.duration_ms, 0);
  const current = tracks[index]?.duration_ms ?? 0;
  const elapsed = before + Math.min(positionMs, current);
  return { elapsed, left: Math.max(total - elapsed, 0) };
}

/** Side A and side B, split as the design draws an album of eleven: five and six. */
export function sides(tracks: TrackInfo[]): [TrackInfo[], TrackInfo[]] {
  const half = Math.floor(tracks.length / 2);
  return [tracks.slice(0, half), tracks.slice(half)];
}

export interface PlatterAlbum {
  title: string;
  artist: string;
  uri: string | null;
  image: string | null;
}

/** The next albums in the queue: runs of songs from one album, the playing album left out. */
export function platter(upNext: TrackInfo[], current: string): PlatterAlbum[] {
  const albums: PlatterAlbum[] = [];
  let last = current;
  for (const track of upNext) {
    if (!track.album || track.album === last) continue;
    last = track.album;
    albums.push({
      title: track.album,
      artist: track.artists[0] ?? "",
      uri: track.uri,
      image: track.image_url,
    });
    if (albums.length === 3) break;
  }
  return albums;
}

/** The line playing at the position: the last one that started at or before it. */
export function activeLyric(lines: LyricLine[], positionMs: number): number {
  let active = -1;
  for (const [index, line] of lines.entries()) {
    if (line.at_ms > positionMs) break;
    active = index;
  }
  return active;
}

/** The Spotify track id of a playable track URI; the Room fetches its album by it. */
export function spotifyTrackId(uri: string | null): string | null {
  return /^spotify:track:([A-Za-z0-9]+)$/.exec(uri ?? "")?.[1] ?? null;
}

/** Whole minutes, for the caption. */
export function minutes(ms: number): number {
  return Math.round(ms / 60_000);
}
