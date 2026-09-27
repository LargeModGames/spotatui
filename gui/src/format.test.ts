import { describe, expect, it } from "vitest";
import { clock, sourceOf } from "./format";

describe("sourceOf", () => {
  it("names the source of each scheme", () => {
    expect(sourceOf("spotify:track:4uLU6hMCjMI75M1A2tKUQC")).toBe("Spotify");
    expect(sourceOf("file:///music/21/05.flac")).toBe("Local");
    expect(sourceOf("qobuz:track:123")).toBe("Qobuz");
    expect(sourceOf("subsonic:track:abc")).toBe("Subsonic");
    expect(sourceOf("youtube:dQw4w9WgXcQ")).toBe("YouTube");
    expect(sourceOf("radio:https://example.com/stream")).toBe("Radio");
  });

  it("has no source for a missing or unknown uri", () => {
    expect(sourceOf(null)).toBeNull();
    expect(sourceOf("")).toBeNull();
    expect(sourceOf("https://example.com/song.mp3")).toBeNull();
  });
});

describe("clock", () => {
  it("shows minutes and padded seconds", () => {
    expect(clock(0)).toBe("0:00");
    expect(clock(100_000)).toBe("1:40");
    expect(clock(242_973)).toBe("4:02");
  });

  it("adds hours from one hour", () => {
    expect(clock(3_600_000)).toBe("1:00:00");
    expect(clock(3_725_000)).toBe("1:02:05");
  });

  it("clamps a negative duration to zero", () => {
    expect(clock(-5)).toBe("0:00");
  });
});
