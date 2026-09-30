import { describe, expect, it } from "vitest";
import type { QueuePayload } from "./bindings/QueuePayload";
import type { SourcePlaylists } from "./bindings/SourcePlaylists";
import type { TrackInfo } from "./bindings/TrackInfo";
import {
  clampCursor,
  listPlayRequest,
  openRow,
  playlistsFor,
  playRequest,
  step,
  upNext,
} from "./libraryModel";

const track = (name: string, uri: string | null): TrackInfo => ({
  uri,
  name,
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

const empty: SourcePlaylists = {
  spotify: [],
  local: [],
  subsonic: [],
  qobuz: [],
  youtube: [],
  radio: [],
};

describe("upNext", () => {
  it("lists the native queue first, then Spotify tracks only while Spotify plays", () => {
    const queue: QueuePayload = {
      now: null,
      native: [track("Native", "file:///a.flac")],
      spotify: {
        currently_playing: null,
        items: [
          {
            kind: "track",
            track: track("Spotify", "spotify:track:1"),
            episode: null,
          },
          { kind: "episode", track: null, episode: null },
        ],
      },
    };
    expect(upNext(queue, true).map((t) => t.name)).toEqual([
      "Native",
      "Spotify",
    ]);
    expect(upNext(queue, false).map((t) => t.name)).toEqual(["Native"]);
    expect(upNext(undefined, true)).toEqual([]);
  });
});

describe("playlistsFor", () => {
  it("takes the active source's list and gives stations no count", () => {
    const playlists: SourcePlaylists = {
      ...empty,
      local: [
        {
          uri: "file:///rips",
          name: "Rips",
          owner: "",
          track_count: 3,
          id: null,
          owner_id: null,
          collaborative: false,
          public: null,
          image_url: null,
        },
      ],
      radio: [track("Score radio", "radio:https://radio.example")],
    };
    expect(playlistsFor(playlists, "Local")).toEqual([
      { uri: "file:///rips", name: "Rips", count: 3 },
    ]);
    expect(playlistsFor(playlists, "Radio")).toEqual([
      { uri: "radio:https://radio.example", name: "Score radio", count: null },
    ]);
    expect(playlistsFor(playlists, "Qobuz")).toEqual([]);
  });
});

describe("clampCursor", () => {
  it("keeps the cursor inside the list", () => {
    expect(clampCursor(5, 3)).toBe(2);
    expect(clampCursor(-1, 3)).toBe(0);
    expect(clampCursor(0, 0)).toBe(-1);
  });
});

describe("step", () => {
  it("moves on j, k and the arrows, and asks for a page at the end", () => {
    expect(step("j", 0, 3, true)).toEqual({ next: 1, loadMore: false });
    expect(step("ArrowDown", 2, 3, true)).toEqual({ next: 3, loadMore: true });
    expect(step("j", 2, 3, false)).toEqual({ next: 3, loadMore: false });
    expect(step("k", 0, 3, true)).toEqual({ next: -1, loadMore: false });
    expect(step("ArrowUp", 2, 3, true)).toEqual({ next: 1, loadMore: false });
    expect(step("x", 1, 3, true)).toBeNull();
  });
});

describe("playRequest", () => {
  it("plays every loaded URI from the cursor's row, counted by position", () => {
    const tracks = [
      track("A", "spotify:track:a"),
      track("Unplayable", null),
      track("B", "spotify:track:b"),
      track("A again", "spotify:track:a"),
    ];
    expect(playRequest(tracks, 3)).toEqual({
      PlayUris: {
        uris: ["spotify:track:a", "spotify:track:b", "spotify:track:a"],
        offset: 2,
      },
    });
    expect(playRequest(tracks, 1)).toBeNull();
    expect(playRequest([], 0)).toBeNull();
  });
});

describe("openRow", () => {
  const row = (uri: string) => ({ uri, name: "Row", count: null });

  it("opens a Spotify playlist by URI and any other list as a source playlist", () => {
    expect(openRow("Spotify", row("spotify:playlist:p1"))).toEqual({
      Open: { Playlist: { id: "spotify:playlist:p1", from_search: false } },
    });
    expect(openRow("Subsonic", row("subsonic:playlist:7"))).toEqual({
      Open: { SourcePlaylist: "subsonic:playlist:7" },
    });
  });

  it("plays a station and skips a station with no stream", () => {
    expect(openRow("Radio", row("radio:https://example.org/live"))).toEqual({
      PlayUris: { uris: ["radio:https://example.org/live"], offset: null },
    });
    expect(openRow("Radio", row("Nameless station"))).toBeNull();
  });
});

describe("listPlayRequest", () => {
  const tracks = [track("A", "spotify:track:a"), track("B", "spotify:track:b")];

  it("plays a Spotify playlist row inside its playlist context", () => {
    expect(
      listPlayRequest("Spotify", "spotify:playlist:p1", tracks, 1),
    ).toEqual({
      PlayTrackInContext: {
        context: "spotify:playlist:p1",
        track: "spotify:track:b",
      },
    });
    expect(
      listPlayRequest("Spotify", "spotify:playlist:p1", [track("X", null)], 0),
    ).toBeNull();
  });

  it("plays a source list as its URIs from the row", () => {
    expect(listPlayRequest("Qobuz", "qobuz:playlist:1", tracks, 1)).toEqual({
      PlayUris: { uris: ["spotify:track:a", "spotify:track:b"], offset: 1 },
    });
  });
});
