import type { Action } from "./bindings/Action";
import type { QueueItemSnapshot } from "./bindings/QueueItemSnapshot";
import type { TrackInfo } from "./bindings/TrackInfo";
import { clampCursor } from "./libraryModel";

/** What a drawer row shows, for a track or a podcast episode. */
export interface QueueLine {
  uri: string | null;
  title: string;
  /** The artists of a track, the show of an episode. */
  subtitle: string;
  image: string | null;
  duration_ms: number;
}

export function trackLine(track: TrackInfo): QueueLine {
  return {
    uri: track.uri,
    title: track.name,
    subtitle: track.artists.join(", "),
    image: track.image_url,
    duration_ms: track.duration_ms,
  };
}

/** A Spotify queue item's row; null for an item with no payload. */
export function itemLine(item: QueueItemSnapshot): QueueLine | null {
  if (item.track) return trackLine(item.track);
  const episode = item.episode;
  if (!episode) return null;
  return {
    uri: episode.uri,
    title: episode.name,
    subtitle: episode.show_name || "Podcast episode",
    image: episode.image_url,
    duration_ms: episode.duration_ms,
  };
}

/** A cursor that follows a track, so a key pressed before the queue update lands still acts on that track. */
export interface QueueCursor {
  index: number;
  uri: string | null;
}

/** The cursor after a key on the native queue, and the Action the key sends. */
export interface QueueKey {
  cursor: QueueCursor;
  action: Action | null;
}

/** The row of the followed track nearest its index, or the index kept in range. */
export function queueRow(native: TrackInfo[], want: QueueCursor): number {
  let row = -1;
  native.forEach((track, index) => {
    if (want.uri === null || track.uri !== want.uri) return;
    if (row < 0 || Math.abs(index - want.index) < Math.abs(row - want.index))
      row = index;
  });
  return row >= 0 ? row : clampCursor(want.index, native.length);
}

/** The terminal queue menu's keys; null for a key the list does not use. */
export function queueKey(
  key: string,
  cursor: number,
  native: TrackInfo[],
): QueueKey | null {
  const last = native.length - 1;
  const uri = native[cursor]?.uri ?? null;
  const on = (index: number): QueueCursor => ({
    index,
    uri: native[index]?.uri ?? null,
  });
  switch (key) {
    case "j":
    case "ArrowDown":
      return {
        cursor: on(clampCursor(cursor + 1, native.length)),
        action: null,
      };
    case "k":
    case "ArrowUp":
      return {
        cursor: on(clampCursor(cursor - 1, native.length)),
        action: null,
      };
    case "Enter":
      return {
        cursor: on(cursor),
        action: uri ? { PlayQueueItem: { uri, position: cursor } } : null,
      };
    case "x":
      return {
        // The row that takes the removed one's place.
        cursor: {
          index: clampCursor(cursor, last),
          uri: (native[cursor + 1] ?? native[cursor - 1])?.uri ?? null,
        },
        action: uri ? { RemoveFromQueue: { uri, position: cursor } } : null,
      };
    case "J":
    case "K": {
      const to = key === "J" ? cursor + 1 : cursor - 1;
      if (!uri || to < 0 || to > last)
        return { cursor: on(cursor), action: null };
      return {
        cursor: { index: to, uri },
        action: { MoveQueueItem: { uri, from: cursor, to } },
      };
    }
    default:
      return null;
  }
}
