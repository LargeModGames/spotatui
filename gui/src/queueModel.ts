import type { Action } from "./bindings/Action";
import type { TrackInfo } from "./bindings/TrackInfo";
import { clampCursor } from "./libraryModel";

/** The cursor after a key on the native queue, and the Action the key sends. */
export interface QueueKey {
  cursor: number;
  action: Action | null;
}

/** The terminal queue menu's keys; null for a key the list does not use. */
export function queueKey(
  key: string,
  cursor: number,
  native: TrackInfo[],
): QueueKey | null {
  const last = native.length - 1;
  const uri = native[cursor]?.uri ?? null;
  switch (key) {
    case "j":
    case "ArrowDown":
      return { cursor: clampCursor(cursor + 1, native.length), action: null };
    case "k":
    case "ArrowUp":
      return { cursor: clampCursor(cursor - 1, native.length), action: null };
    case "Enter":
      return {
        cursor,
        action: uri ? { PlayQueueItem: { uri, position: cursor } } : null,
      };
    case "x":
      return {
        cursor: clampCursor(cursor, last),
        action: uri ? { RemoveFromQueue: { uri, position: cursor } } : null,
      };
    case "J":
    case "K": {
      const to = key === "J" ? cursor + 1 : cursor - 1;
      if (!uri || to < 0 || to > last) return { cursor, action: null };
      return {
        cursor: to,
        action: { MoveQueueItem: { uri, from: cursor, to } },
      };
    }
    default:
      return null;
  }
}
