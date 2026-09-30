import { describe, expect, it } from "vitest";
import type { SessionPlay } from "./bindings/SessionPlay";
import type { TrackInfo } from "./bindings/TrackInfo";
import {
  estimates,
  headline,
  markers,
  positionAt,
  replayRun,
} from "./sessionModel";

const MIN = 60_000;

const play = (
  title: string,
  uri: string | null,
  start: number,
  length = 4 * MIN,
  listened = length,
): SessionPlay => ({
  started_at_ms: start,
  ended_at_ms: start + length,
  listened_ms: listened,
  duration_ms: length,
  title,
  artists: ["Adele"],
  album: "21",
  uri,
  image_url: null,
});

const queued = (seconds: number): TrackInfo => ({
  uri: "file:///a.flac",
  name: "a",
  artists: [],
  album: "",
  duration_ms: seconds * 1000,
  id: null,
  album_id: null,
  artist_refs: [],
  is_playable: true,
  is_local: true,
  track_number: 0,
  explicit: false,
  image_url: null,
});

describe("sessionModel", () => {
  it("estimates start times from the time left on the playing track", () => {
    expect(
      estimates({ durationMs: 4 * MIN, positionMs: MIN, running: true }, 0, [
        queued(120),
        queued(0),
        queued(60),
      ]),
    ).toEqual([3 * MIN, 5 * MIN, null]);
  });

  it("estimates nothing while paused", () => {
    expect(
      estimates({ durationMs: 4 * MIN, positionMs: 0, running: false }, 0, [
        queued(60),
      ]),
    ).toEqual([null]);
  });

  it("marks a source change and a pause of a minute or more", () => {
    const plays = [
      play("Firework", "spotify:track:1", 0),
      play("Rolling in the Deep", "file:///21/01.flac", 10 * MIN),
    ];
    expect(markers(plays)).toEqual([
      { kind: "pause", at: 4 * MIN, before: 1, minutes: 6 },
      { kind: "source", at: 10 * MIN, before: 1, from: "Spotify", to: "Local" },
    ]);
  });

  it("marks a pause inside a play", () => {
    expect(
      markers([play("Long", "spotify:track:1", 0, 8 * MIN, 4 * MIN)]),
    ).toEqual([{ kind: "pause", at: 0, before: 1, minutes: 4 }]);
  });

  it("replays the same-source run from a row to the newest", () => {
    const plays = [
      play("a", "file:///1.flac", 0),
      play("b", "file:///2.flac", MIN),
      play("c", "spotify:track:3", 2 * MIN),
    ];
    expect(replayRun(plays, 0)).toEqual({
      PlayUris: { uris: ["file:///1.flac", "file:///2.flac"], offset: 0 },
    });
    expect(replayRun([play("r", "radio:https://a", 0)], 0)).toBeNull();
    expect(replayRun([play("x", null, 0)], 0)).toBeNull();
  });

  it("counts the songs and the sources, the playing one included", () => {
    expect(headline([], "spotify:track:1")).toBe("0 songs · 1 source");
  });
});

describe("positionAt", () => {
  it("carries the position on only while playing", () => {
    expect(positionAt({ ms: 1000, at: 500 }, true, 2500)).toBe(3000);
    expect(positionAt({ ms: 1000, at: 500 }, false, 2500)).toBe(1000);
    expect(positionAt(null, true, 2500)).toBe(0);
  });

  it("gives the same start estimate for samples taken at different times", () => {
    const position = { ms: MIN, at: 0 };
    const sample = (wall: number, page: number) =>
      estimates(
        {
          durationMs: 4 * MIN,
          positionMs: positionAt(position, true, page),
          running: true,
        },
        wall,
        [queued(180)],
      );
    expect(sample(1_000_000, 0)).toEqual(sample(1_010_000, 10_000));
  });
});
