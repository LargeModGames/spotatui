import { describe, expect, it } from "vitest";
import type { SearchPayload } from "./bindings/SearchPayload";
import type { TrackInfo } from "./bindings/TrackInfo";
import {
  actionFor,
  complete,
  directAction,
  loadRecent,
  pageStorage,
  parse,
  pushRecent,
  rowsFor,
  spotifyLink,
} from "./commandModel";

const track = (name: string, uri: string): TrackInfo => ({
  uri,
  name,
  artists: ["Adele"],
  album: "21",
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

const search: SearchPayload = {
  ran: true,
  query: "adele",
  tracks: [
    track("Rolling in the Deep", "spotify:track:1"),
    track("Station", "radio:https://radio.example"),
    track("Someone Like You", "spotify:track:2"),
  ],
  artists: [
    { id: "a", uri: "spotify:artist:a", name: "Adele", image_url: null },
  ],
  albums: [
    {
      id: "b",
      uri: "spotify:album:b",
      name: "21",
      artists: [{ id: "a", name: "Adele" }],
      album_type: "album",
      release_date: "2011-01-24",
      total_tracks: null,
      image_url: null,
      tracks: [],
    },
  ],
  playlists: [],
  liked: [],
};

describe("parse", () => {
  it("splits a known verb, an optional kind and the argument", () => {
    expect(parse(":play album 21")).toEqual({
      verb: "play",
      kind: "album",
      arg: "21",
    });
    expect(parse("queue someone like")).toEqual({
      verb: "queue",
      kind: null,
      arg: "someone like",
    });
  });

  it("treats an unknown first word as a search over the whole input", () => {
    expect(parse("adele 21")).toEqual({
      verb: null,
      kind: null,
      arg: "adele 21",
    });
  });
});

describe("complete", () => {
  it("completes a unique verb prefix and leaves an ambiguous one", () => {
    expect(complete("pl", [], [])).toBe("play ");
    expect(complete("p", [], [])).toBe("p");
  });

  it("completes a compiled source after source", () => {
    expect(complete("source you", ["Spotify", "YouTube"], [])).toBe(
      "source YouTube",
    );
  });
});

describe("rows and actions", () => {
  it("lists albums before songs for a bare play and plays a song on through the results", () => {
    const parsed = parse("play adele");
    const rows = rowsFor(parsed, search, []);
    expect(rows.map((row) => row.kind)).toEqual([
      "album",
      "song",
      "song",
      "song",
    ]);
    expect(rows[0].meta).toBe("Adele · 2011");
    expect(actionFor(parsed, rows[0], search, false)).toEqual({
      PlayContext: { uri: "spotify:album:b", offset: null },
    });
    expect(actionFor(parsed, rows[3], search, false)).toEqual({
      PlayUris: {
        uris: [
          "spotify:track:1",
          "radio:https://radio.example",
          "spotify:track:2",
        ],
        offset: 2,
      },
    });
  });

  it("queues the whole track and plays it now on shift enter", () => {
    const parsed = parse("queue adele");
    const rows = rowsFor(parsed, search, []);
    expect(rows.map((row) => row.title)).toEqual([
      "Rolling in the Deep",
      "Station",
      "Someone Like You",
    ]);
    expect(actionFor(parsed, rows[0], search, false)).toEqual({
      QueueTrack: search.tracks[0],
    });
    expect(actionFor(parsed, rows[0], search, true)).toMatchObject({
      PlayUris: { offset: 0 },
    });
  });

  it("transfers playback to a device and skips a device without an id", () => {
    const parsed = parse("device kit");
    const rows = rowsFor(parsed, null, [
      {
        id: "d1",
        name: "Kitchen",
        kind: "Speaker",
        is_active: false,
        volume_percent: null,
      },
      {
        id: null,
        name: "Kitchen 2",
        kind: "Speaker",
        is_active: false,
        volume_percent: null,
      },
    ]);
    expect(rows).toHaveLength(1);
    expect(actionFor(parsed, rows[0], null, false)).toEqual({
      TransferPlayback: { device_id: "d1", persist: true },
    });
  });

  it("runs party and source commands without a row", () => {
    expect(directAction(parse("party start"), [])).toBe("StartParty");
    expect(directAction(parse("party join abc12 Sam"), [])).toEqual({
      JoinParty: { code: "ABC12", name: "Sam" },
    });
    expect(directAction(parse("party join abc12"), [])).toBeNull();
    expect(
      directAction(parse("source youtube"), ["Spotify", "YouTube"]),
    ).toEqual({ SelectSource: "YouTube" });
  });
});

describe("spotifyLink", () => {
  it("plays a pasted track or album link without a search", () => {
    expect(spotifyLink("spotify:track:4uLU6hMCjMI75M1A2tKUQC")).toEqual({
      PlayUris: {
        uris: ["spotify:track:4uLU6hMCjMI75M1A2tKUQC"],
        offset: null,
      },
    });
    expect(
      spotifyLink("https://open.spotify.com/album/1A2GTWGtFfWp7KSQTwWOyo?si=x"),
    ).toEqual({
      PlayContext: {
        uri: "spotify:album:1A2GTWGtFfWp7KSQTwWOyo",
        offset: null,
      },
    });
    expect(spotifyLink("adele")).toBeNull();
  });
});

describe("recent", () => {
  it("keeps the newest entry first without duplicates", () => {
    expect(pushRecent(["search a", "play b"], "play b")).toEqual([
      "play b",
      "search a",
    ]);
  });

  it("reads an empty history from a storage that throws", () => {
    expect(
      loadRecent({
        getItem: () => {
          throw new Error("blocked");
        },
      }),
    ).toEqual([]);
  });
});

describe("pageStorage", () => {
  it("keeps nothing when the browser storage cannot be reached", () => {
    const storage = pageStorage();
    storage.setItem("spotatui.commands", "[]");
    expect(storage.getItem("spotatui.commands")).toBeNull();
  });
});
