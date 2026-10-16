import { describe, expect, it } from "vitest";
import type { TrackInfo } from "./bindings/TrackInfo";
import {
  filterTracks,
  isDefaultSort,
  parseSort,
  removeAction,
  rowKeys,
  shufflePlayRequest,
  shownPlayRequest,
  SORT_OPTIONS,
  sortTracks,
  sortValue,
} from "./sortModel";
import { trackMenu } from "./trackMenuModel";

const track = (
  name: string,
  artists: string[],
  album: string,
  duration_ms: number,
  uri: string | null = `spotify:track:${name}`,
): TrackInfo => ({
  uri,
  name,
  artists,
  album,
  duration_ms,
  id: uri?.split(":")[2] ?? null,
  album_id: null,
  artist_refs: [],
  is_playable: true,
  is_local: false,
  track_number: 1,
  explicit: false,
  image_url: null,
  release_date: null,
});

const tracks = [
  track("banana", ["Zedd", "Foxes"], "Clarity", 200_000),
  track("Apple", ["adele"], "21", 300_000),
  track("cherry", ["Björk"], "Post", 100_000),
  track("apple", ["Muse"], "Drones", 300_000),
];

const names = (sort: string) =>
  sortTracks(tracks, parseSort(sort)).map((row) => row.track.name);
const indexes = (sort: string) =>
  sortTracks(tracks, parseSort(sort)).map((row) => row.index);

describe("parseSort", () => {
  it("reads a field and an optional descending order", () => {
    expect(parseSort("name")).toEqual({ field: "name", desc: false });
    expect(parseSort("duration:desc")).toEqual({
      field: "duration",
      desc: true,
    });
  });

  it("falls back to the list's own order for anything unknown", () => {
    for (const value of [null, undefined, "", "rating", "name:up", "a:desc:x"])
      expect(parseSort(value)).toEqual({ field: "default", desc: false });
    expect(parseSort("default:desc")).toEqual({
      field: "default",
      desc: false,
    });
  });

  it("round-trips every select option", () => {
    for (const { value } of SORT_OPTIONS)
      expect(sortValue(parseSort(value))).toBe(value);
  });

  it("counts date added as the list's own order", () => {
    expect(isDefaultSort(parseSort("default"))).toBe(true);
    expect(isDefaultSort(parseSort("date_added"))).toBe(true);
    expect(isDefaultSort(parseSort("date_added:desc"))).toBe(false);
    expect(isDefaultSort(parseSort("name"))).toBe(false);
  });
});

describe("sortTracks", () => {
  it("keeps the loaded order by default and by date added", () => {
    expect(indexes("default")).toEqual([0, 1, 2, 3]);
    expect(indexes("date_added")).toEqual([0, 1, 2, 3]);
    expect(indexes("date_added:desc")).toEqual([3, 2, 1, 0]);
  });

  it("sorts names ignoring case, ties in loaded order", () => {
    expect(indexes("name")).toEqual([1, 3, 0, 2]);
    expect(names("name:desc")).toEqual(["cherry", "banana", "Apple", "apple"]);
  });

  it("sorts by the first artist and by album", () => {
    expect(indexes("artist")).toEqual([1, 2, 3, 0]);
    expect(indexes("album")).toEqual([1, 0, 3, 2]);
    expect(indexes("album:desc")).toEqual([2, 3, 0, 1]);
  });

  it("sorts durations as numbers, ties in loaded order both ways", () => {
    expect(indexes("duration")).toEqual([2, 0, 1, 3]);
    expect(indexes("duration:desc")).toEqual([1, 3, 0, 2]);
  });

  it("leaves the tracks it was given untouched", () => {
    const before = [...tracks];
    names("name");
    names("date_added:desc");
    expect(tracks).toEqual(before);
  });
});

describe("filterTracks", () => {
  const filtered = (query: string) =>
    filterTracks(sortTracks(tracks, parseSort("default")), query).map(
      (row) => row.index,
    );

  it("matches every term across name, album and artists, ignoring case", () => {
    expect(filtered("APPLE")).toEqual([1, 3]);
    expect(filtered("apple muse")).toEqual([3]);
    expect(filtered("foxes clarity")).toEqual([0]);
    expect(filtered("björk")).toEqual([2]);
    expect(filtered("apple zedd")).toEqual([]);
  });

  it("keeps every row for an empty or blank query", () => {
    expect(filtered("")).toEqual([0, 1, 2, 3]);
    expect(filtered("  \t ")).toEqual([0, 1, 2, 3]);
  });

  it("keeps the sorted order and the loaded indexes", () => {
    const rows = filterTracks(sortTracks(tracks, parseSort("name")), "apple");
    expect(rows.map((row) => row.index)).toEqual([1, 3]);
  });
});

describe("removeAction", () => {
  const mix = { source: "Spotify" as const, uri: "spotify:playlist:mix" };
  const positions = [10, 11, 12, 13];

  it("removes an editable Spotify playlist row at its loaded position", () => {
    expect(removeAction(mix, [mix.uri], positions, tracks[2], 2)).toEqual({
      RemoveTrackFromPlaylist: {
        playlist: mix.uri,
        track: "spotify:track:cherry",
        position: 12,
      },
    });
  });

  it("maps a sorted and filtered row back to its position", () => {
    const rows = filterTracks(sortTracks(tracks, parseSort("name")), "apple");
    const shown = rows[1];
    expect(
      removeAction(mix, [mix.uri], positions, shown.track, shown.index),
    ).toEqual({
      RemoveTrackFromPlaylist: {
        playlist: mix.uri,
        track: "spotify:track:apple",
        position: 13,
      },
    });
  });

  it("offers no remove without edit rights, positions or a list", () => {
    expect(removeAction(mix, [], positions, tracks[0], 0)).toBeNull();
    expect(removeAction(mix, [mix.uri], null, tracks[0], 0)).toBeNull();
    expect(removeAction(mix, [mix.uri], [10], tracks[1], 1)).toBeNull();
    expect(removeAction(null, [mix.uri], positions, tracks[0], 0)).toBeNull();
    const rips = { source: "Local" as const, uri: "file:///rips" };
    expect(removeAction(rips, [rips.uri], positions, tracks[0], 0)).toBeNull();
  });

  it("removes a YouTube row by its video id", () => {
    const list = { source: "YouTube" as const, uri: "youtube:playlist:y1" };
    const video = {
      ...track("Clip", ["Chan"], "", 1, "youtube:x"),
      id: "vid123",
    };
    expect(removeAction(list, [], null, video, 5)).toEqual({
      RemoveTrackFromPlaylist: {
        playlist: list.uri,
        track: "vid123",
        position: 0,
      },
    });
  });

  it("puts the remove entry in the row menu only when it applies", () => {
    const labels = (remove: ReturnType<typeof removeAction>) =>
      trackMenu(tracks[0], false, [], remove ? { remove } : {})
        .flat()
        .map((entry) => entry.label);
    expect(
      labels(removeAction(mix, [mix.uri], positions, tracks[0], 0)),
    ).toContain("Remove from this playlist");
    expect(
      labels(removeAction(mix, [], positions, tracks[0], 0)),
    ).not.toContain("Remove from this playlist");
  });
});

describe("shownPlayRequest", () => {
  const mix = { source: "Spotify" as const, uri: "spotify:playlist:mix" };
  const shown = [tracks[2], track("gone", [], "", 1, null), tracks[0]];

  it("keeps the playlist context in the loaded order", () => {
    expect(shownPlayRequest(mix, false, tracks, 1)).toEqual({
      PlayTrackInContext: { context: mix.uri, track: "spotify:track:Apple" },
    });
  });

  it("plays the shown URIs once sorted or filtered, skipping rows without one", () => {
    expect(shownPlayRequest(mix, true, shown, 2)).toEqual({
      PlayUris: {
        uris: ["spotify:track:cherry", "spotify:track:banana"],
        offset: 1,
      },
    });
    expect(shownPlayRequest(mix, true, shown, 1)).toBeNull();
    expect(shownPlayRequest(null, true, shown, 0)).toEqual({
      PlayUris: {
        uris: ["spotify:track:cherry", "spotify:track:banana"],
        offset: 0,
      },
    });
  });
});

describe("shufflePlayRequest", () => {
  const shown = [
    tracks[0],
    track("gone", [], "", 1, null),
    tracks[1],
    tracks[2],
  ];

  it("plays every shown song with a URI once, in the order the random picks give", () => {
    expect(shufflePlayRequest(shown, () => 0)).toEqual({
      PlayUris: {
        uris: [
          "spotify:track:Apple",
          "spotify:track:cherry",
          "spotify:track:banana",
        ],
        offset: 0,
      },
    });
    expect(shufflePlayRequest(shown, () => 0.999)).toEqual({
      PlayUris: {
        uris: [
          "spotify:track:banana",
          "spotify:track:Apple",
          "spotify:track:cherry",
        ],
        offset: 0,
      },
    });
  });

  it("sends nothing when no shown row has a URI", () => {
    expect(
      shufflePlayRequest([track("gone", [], "", 1, null)], () => 0),
    ).toBeNull();
    expect(shufflePlayRequest([], () => 0)).toBeNull();
  });

  it("keeps a single song a single-song request and keeps duplicate URIs", () => {
    expect(shufflePlayRequest([tracks[0]], () => 0)).toEqual({
      PlayUris: { uris: ["spotify:track:banana"], offset: 0 },
    });
    expect(shufflePlayRequest([tracks[0], tracks[0]], () => 0)).toEqual({
      PlayUris: {
        uris: ["spotify:track:banana", "spotify:track:banana"],
        offset: 0,
      },
    });
  });

  it("does not reorder the caller's rows", () => {
    const before = [...shown];
    shufflePlayRequest(shown, () => 0);
    expect(shown).toEqual(before);
  });

  it("keeps a URI whatever its source, like the play path does", () => {
    const radio = track("radio", [], "", 1, "radio:track:1");
    expect(shufflePlayRequest([radio], () => 0)).toEqual({
      PlayUris: { uris: ["radio:track:1"], offset: 0 },
    });
  });

  it("passes the whole shuffled list on, so the server-side window can pick any of it", () => {
    const many = Array.from({ length: 101 }, (_, i) => `spotify:track:t${i}`);
    const rows = many.map((uri) => track(uri, [], "", 1, uri));
    const rng = () => 0;
    const request = shufflePlayRequest(rows, rng) as {
      PlayUris: { uris: string[] };
    };
    expect(request.PlayUris.uris).toHaveLength(101);
    expect(new Set(request.PlayUris.uris).size).toBe(101);
    expect(request.PlayUris.uris[0]).toBe(many[1]);
  });
});

describe("rowKeys", () => {
  it("numbers each uri's occurrences and keys a row without a uri by index", () => {
    const x = track("x", [], "", 0);
    const local = track("local", [], "", 0, null);
    expect(rowKeys([x, local, track("y", [], "", 0), x])).toEqual([
      "spotify:track:x#0",
      "@1",
      "spotify:track:y#0",
      "spotify:track:x#1",
    ]);
  });

  it("keeps a unique row's key across a sort and a filter", () => {
    const loaded = new Map(
      tracks.map((row, index) => [row, rowKeys(tracks)[index]]),
    );
    const shown = filterTracks(
      sortTracks(tracks, parseSort("name:desc")),
      "a",
    ).map((row) => row.track);
    expect(shown.length).toBeLessThan(tracks.length);
    expect(rowKeys(shown)).toEqual(shown.map((row) => loaded.get(row)));
  });
});
