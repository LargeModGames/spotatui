import type { Action } from "./bindings/Action";
import type { TrackInfo } from "./bindings/TrackInfo";
import { clampCursor } from "./libraryModel";

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
