import { describe, expect, it } from "vitest";
import type { EpisodeInfo } from "./bindings/EpisodeInfo";
import type { TrackInfo } from "./bindings/TrackInfo";
import { itemLine, queueKey, queueRow } from "./queueModel";

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
  release_date: null,
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

describe("itemLine", () => {
  const episode = (show_name: string): EpisodeInfo => ({
    id: "e",
    uri: "spotify:episode:e",
    name: "Episode 400",
    duration_ms: 3_600_000,
    show_name,
    description: "",
    release_date: "2026-10-01",
    is_playable: true,
    resume_point: null,
    image_url: "cover.jpg",
  });

  it("names an episode by its show instead of an artist", () => {
    expect(
      itemLine({ kind: "episode", track: null, episode: episode("Lex") }),
    ).toEqual({
      uri: "spotify:episode:e",
      title: "Episode 400",
      subtitle: "Lex",
      image: "cover.jpg",
      duration_ms: 3_600_000,
    });
    expect(
      itemLine({ kind: "episode", track: null, episode: episode("") })
        ?.subtitle,
    ).toBe("Podcast episode");
  });

  it("keeps a track's artists and drops an item with no payload", () => {
    expect(
      itemLine({ kind: "track", track: track("a"), episode: null })?.title,
    ).toBe("a");
    expect(itemLine({ kind: "episode", track: null, episode: null })).toBe(
      null,
    );
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
