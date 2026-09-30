import { describe, expect, it } from "vitest";
import type { TrackInfo } from "./bindings/TrackInfo";
import { queueKey, queueRow } from "./queueModel";

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
    expect(queueKey("j", 2, native)).toEqual({
      cursor: { index: 2, uri: "c" },
      action: null,
    });
    expect(queueKey("ArrowUp", 0, native)).toEqual({
      cursor: { index: 0, uri: "a" },
      action: null,
    });
  });

  it("plays and removes the row by uri and position", () => {
    expect(queueKey("Enter", 1, native)?.action).toEqual({
      PlayQueueItem: { uri: "b", position: 1 },
    });
    expect(queueKey("x", 2, native)).toEqual({
      cursor: { index: 1, uri: "b" },
      action: { RemoveFromQueue: { uri: "c", position: 2 } },
    });
  });

  it("moves the row with the cursor and stops at the ends", () => {
    expect(queueKey("J", 0, native)).toEqual({
      cursor: { index: 1, uri: "a" },
      action: { MoveQueueItem: { uri: "a", from: 0, to: 1 } },
    });
    expect(queueKey("K", 0, native)).toEqual({
      cursor: { index: 0, uri: "a" },
      action: null,
    });
  });

  it("sends nothing for a row without a uri or a key it does not use", () => {
    expect(queueKey("Enter", 0, [track(null)])?.action).toBeNull();
    expect(queueKey("z", 0, native)).toBeNull();
  });
});

describe("queueRow", () => {
  it("keeps a moved track under the cursor until the queue update lands", () => {
    const moved = queueKey("J", 0, native)!.cursor;
    expect(queueRow(native, moved)).toBe(0);
    expect(queueKey("J", queueRow(native, moved), native)?.action).toEqual({
      MoveQueueItem: { uri: "a", from: 0, to: 1 },
    });
    expect(queueRow([track("b"), track("a"), track("c")], moved)).toBe(1);
  });

  it("falls back to the index when the track left the queue", () => {
    expect(queueRow(native, { index: 5, uri: "z" })).toBe(2);
    expect(
      queueRow([track("a"), track("b"), track("a")], { index: 2, uri: "a" }),
    ).toBe(2);
  });
});
