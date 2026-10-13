import { describe, expect, it } from "vitest";
import type { Action } from "./bindings/Action";
import type { AlbumInfo } from "./bindings/AlbumInfo";
import type { DiscoverPayload } from "./bindings/DiscoverPayload";
import type { TrackInfo } from "./bindings/TrackInfo";
import type { TrackTablePayload } from "./bindings/TrackTablePayload";
import {
  albumKind,
  albumPlay,
  copyLink,
  greeting,
  homeFetch,
  lengthLine,
  radioRows,
  recentAlbums,
  releaseLine,
  trackKey,
  yearOf,
} from "./pagesModel";

const track = (
  id: string,
  album_id: string | null,
  ms = 180_000,
): TrackInfo => ({
  uri: `spotify:track:${id}`,
  name: id,
  artists: ["Kygo"],
  album: `Album ${album_id}`,
  duration_ms: ms,
  id,
  album_id,
  artist_refs: [],
  is_playable: true,
  is_local: false,
  track_number: 0,
  explicit: false,
  image_url: null,
  release_date: null,
});

const discover = (over: Partial<DiscoverPayload>): DiscoverPayload => ({
  available: true,
  loading: false,
  top_tracks_range: null,
  top_tracks: [],
  artists_mix: [],
  liked_ids: [],
  ...over,
});

describe("greeting", () => {
  it("follows the local hour", () => {
    expect(greeting(6)).toBe("Good morning");
    expect(greeting(13)).toBe("Good afternoon");
    expect(greeting(20)).toBe("Good evening");
    expect(greeting(2)).toBe("Good evening");
  });
});

describe("recentAlbums", () => {
  it("keeps the first track of each album and drops tracks without one", () => {
    const rows = recentAlbums([
      track("1", "a"),
      track("2", "b"),
      track("3", "a"),
      track("4", null),
    ]);
    expect(rows.map((row) => row.id)).toEqual(["1", "2"]);
  });
});

describe("homeFetch", () => {
  const none = { top: false, mix: false };

  it("asks for the top tracks first, even before the channel landed", () => {
    expect(homeFetch(null, none)).toBe("top");
    expect(homeFetch(discover({}), none)).toBe("top");
  });

  it("asks for the artists mix once the top tracks were asked for", () => {
    expect(homeFetch(discover({}), { top: true, mix: false })).toBe("mix");
    expect(homeFetch(discover({}), { top: true, mix: true })).toBeNull();
  });

  it("waits while a fetch runs and without a Spotify session", () => {
    expect(homeFetch(discover({ loading: true }), none)).toBeNull();
    expect(homeFetch(discover({ available: false }), none)).toBeNull();
  });

  it("skips lists that already have tracks", () => {
    const full = discover({
      top_tracks: [track("1", "a")],
      artists_mix: [track("2", "b")],
    });
    expect(homeFetch(full, none)).toBeNull();
  });
});

describe("album lines", () => {
  it("names the album kind", () => {
    expect(albumKind("single")).toBe("Single");
    expect(albumKind("compilation")).toBe("Compilation");
    expect(albumKind(null)).toBe("Album");
  });

  it("reads the year and hides an unknown one", () => {
    expect(yearOf("2017-03-03")).toBe("2017");
    expect(yearOf("0000")).toBeNull();
    expect(yearOf(null)).toBeNull();
  });

  it("counts songs and minutes, with hours from one hour", () => {
    expect(lengthLine([track("1", "a")])).toBe("1 song, 3 min");
    expect(lengthLine(Array(25).fill(track("1", "a")))).toBe(
      "25 songs, 1 hr 15 min",
    );
  });

  it("spells out a full release date and keeps a bare year", () => {
    expect(releaseLine("2017-03-03")).toBe("March 3, 2017");
    expect(releaseLine("2017")).toBe("2017");
    expect(releaseLine("0000")).toBeNull();
  });
});

describe("albumPlay", () => {
  const album: AlbumInfo = {
    id: "al",
    uri: "spotify:album:al",
    name: "Cloud Nine",
    artists: [],
    album_type: "album",
    release_date: null,
    total_tracks: 2,
    image_url: null,
    tracks: [track("1", "al"), track("2", "al")],
  };

  it("plays the album context from the track", () => {
    expect(albumPlay(album, 1)).toEqual({
      PlayContext: { uri: "spotify:album:al", offset: 1 },
    });
  });

  it("plays the track URIs when the album has no URI", () => {
    expect(albumPlay({ ...album, uri: null }, 1)).toEqual({
      PlayUris: { uris: ["spotify:track:1", "spotify:track:2"], offset: 1 },
    });
  });
});

describe("trackKey", () => {
  it("plays on Enter, queues on q and moves on j and k", () => {
    expect(trackKey("Enter", 0, 3)).toEqual({ kind: "play" });
    expect(trackKey("q", 0, 3)).toEqual({ kind: "queue" });
    expect(trackKey("j", 0, 3)).toEqual({ kind: "move", next: 1 });
    expect(trackKey("ArrowUp", 2, 3)).toEqual({ kind: "move", next: 1 });
    expect(trackKey("x", 0, 3)).toBeNull();
  });
});

describe("radioRows", () => {
  const table = (radio: string | null): TrackTablePayload => ({
    uri: null,
    tracks: [track("1", "a")],
    has_more: false,
    positions: null,
    radio,
  });

  it("hands out the rows only when they are this seed's radio", () => {
    expect(radioRows(table("seed"), "seed")?.map((row) => row.id)).toEqual([
      "1",
    ]);
    expect(radioRows(table("other"), "seed")).toBeNull();
    expect(radioRows(table(null), "seed")).toBeNull();
    expect(radioRows(null, "seed")).toBeNull();
  });
});

describe("copyLink", () => {
  const link = "https://open.spotify.com/album/al";

  it("writes the link and confirms it", async () => {
    const sent: Action[] = [];
    const written: string[] = [];
    await copyLink(link, "Copied the album link.", (a) => sent.push(a), {
      writeText: async (text) => {
        written.push(text);
      },
    });
    expect(written).toEqual([link]);
    expect(sent).toEqual([{ Notify: ["Copied the album link.", 3] }]);
  });

  it("reports an error when the write fails or there is no clipboard", async () => {
    const sent: Action[] = [];
    await copyLink(link, "done", (a) => sent.push(a), {
      writeText: () => Promise.reject(new Error("denied")),
    });
    await copyLink(link, "done", (a) => sent.push(a), undefined);
    expect(sent).toEqual([
      { NotifyError: ["Could not copy the link.", 4] },
      { NotifyError: ["Could not copy the link.", 4] },
    ]);
  });
});
