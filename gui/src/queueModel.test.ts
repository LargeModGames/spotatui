import { describe, expect, it } from "vitest";
import type { TrackInfo } from "./bindings/TrackInfo";
import { queueKey } from "./queueModel";

const track = (uri: string | null): TrackInfo => ({
  uri,
  name: uri ?? "?",
  artists: [],
  album: "",
  duration_ms: 1000,
  id: null,
  album_id: null,
  artist_refs: [],
  is_playable: true,
  is_local: false,
  track_number: 0,
  explicit: false,
  image_url: null,
});

const native = [track("a"), track("b"), track("c")];

describe("queueKey", () => {
  it("moves the cursor inside the list", () => {
    expect(queueKey("j", 2, native)).toEqual({ cursor: 2, action: null });
    expect(queueKey("ArrowUp", 0, native)).toEqual({ cursor: 0, action: null });
  });

  it("plays and removes the row by uri and position", () => {
    expect(queueKey("Enter", 1, native)?.action).toEqual({
      PlayQueueItem: { uri: "b", position: 1 },
    });
    expect(queueKey("x", 2, native)).toEqual({
      cursor: 1,
      action: { RemoveFromQueue: { uri: "c", position: 2 } },
    });
  });

  it("moves the row with the cursor and stops at the ends", () => {
    expect(queueKey("J", 0, native)).toEqual({
      cursor: 1,
      action: { MoveQueueItem: { uri: "a", from: 0, to: 1 } },
    });
    expect(queueKey("K", 0, native)).toEqual({ cursor: 0, action: null });
  });

  it("sends nothing for a row without a uri or a key it does not use", () => {
    expect(queueKey("Enter", 0, [track(null)])?.action).toBeNull();
    expect(queueKey("z", 0, native)).toBeNull();
  });
});
