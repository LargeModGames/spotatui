import { describe, expect, it } from "vitest";
import type { AlbumInfo } from "./bindings/AlbumInfo";
import type { NowPlaying } from "./bindings/NowPlaying";
import type { TrackInfo } from "./bindings/TrackInfo";
import {
  activeLyric,
  albumClock,
  albumContext,
  platter,
  playingIndex,
  sides,
  spotifyTrackId,
} from "./roomModel";

const track = (name: string, seconds = 200, album = "21"): TrackInfo => ({
  uri: `spotify:track:${name.replace(/\W/g, "")}`,
  name,
  artists: ["Adele"],
  album,
  duration_ms: seconds * 1000,
  id: null,
  album_id: null,
  artist_refs: [],
  is_playable: true,
  is_local: false,
  track_number: 0,
  explicit: false,
  image_url: null,
});

const tracks = ["One", "Two", "Three"].map((name) => track(name));

const album: AlbumInfo = {
  id: "21",
  uri: "spotify:album:21",
  name: "21",
  artists: [],
  album_type: null,
  release_date: "2011-01-24",
  total_tracks: 3,
  image_url: null,
  tracks,
};

const item = (patch: Partial<NowPlaying> = {}): NowPlaying => ({
  title: "Two",
  artists: ["Adele"],
  album: "21",
  image_url: null,
  duration_ms: 200_000,
  uri: "spotify:track:Two",
  is_playing: true,
  is_live: false,
  shuffle: false,
  repeat: "off",
  context_uri: "spotify:album:21",
  ...patch,
});

describe("roomModel", () => {
  it("finds the playing row by uri, else by title", () => {
    expect(playingIndex(tracks, item(), "21")).toBe(1);
    expect(
      playingIndex(tracks, item({ uri: "spotify:track:relinked" }), "21"),
    ).toBe(1);
    expect(playingIndex(tracks, null, "21")).toBe(-1);
    expect(
      playingIndex(
        tracks,
        item({ uri: "spotify:track:other", album: "25" }),
        "21",
      ),
    ).toBe(-1);
  });

  it("treats the album as what plays next only in order and as the context", () => {
    expect(albumContext(album, item(), false)).toBe(true);
    expect(albumContext(album, item({ shuffle: true }), false)).toBe(false);
    expect(
      albumContext(album, item({ context_uri: "spotify:playlist:1" }), false),
    ).toBe(false);
    expect(albumContext(album, item(), true)).toBe(false);
  });

  it("adds the finished tracks to the position", () => {
    expect(albumClock(tracks, 1, 50_000)).toEqual({
      elapsed: 250_000,
      left: 350_000,
    });
  });

  it("splits eleven songs five and six", () => {
    const eleven = Array.from({ length: 11 }, (_, index) => track(`T${index}`));
    const [a, b] = sides(eleven);
    expect([a.length, b.length]).toEqual([5, 6]);
  });

  it("lists the next albums in the queue without the playing one", () => {
    const upNext = [
      track("Four"),
      track("Freeze", 487, "Cloud Nine"),
      track("Stay", 239, "Cloud Nine"),
      track("Suite", 602, "Film Suites"),
    ];
    expect(platter(upNext, "21").map((next) => next.title)).toEqual([
      "Cloud Nine",
      "Film Suites",
    ]);
  });

  it("picks the last lyric line at or before the position", () => {
    const lines = [
      { at_ms: 0, text: "a" },
      { at_ms: 5_000, text: "b" },
    ];
    expect(activeLyric(lines, 4_999)).toBe(0);
    expect(activeLyric(lines, 5_000)).toBe(1);
    expect(activeLyric([{ at_ms: 1_000, text: "a" }], 0)).toBe(-1);
  });

  it("fetches an album only for a Spotify track", () => {
    expect(spotifyTrackId("spotify:track:4uLU6hMCjMI75M1A2tKUQC")).toBe(
      "4uLU6hMCjMI75M1A2tKUQC",
    );
    expect(spotifyTrackId("spotify:episode:1")).toBeNull();
    expect(spotifyTrackId("file:///a.flac")).toBeNull();
  });
});
