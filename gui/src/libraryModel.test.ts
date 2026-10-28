import { describe, expect, it, vi } from "vitest";
import type { AlbumInfo } from "./bindings/AlbumInfo";
import type { LibraryNode } from "./bindings/LibraryNode";
import type { QueuePayload } from "./bindings/QueuePayload";
import type { Source } from "./bindings/Source";
import type { SourcePlaylists } from "./bindings/SourcePlaylists";
import type { TrackInfo } from "./bindings/TrackInfo";
import {
  ALBUM_SORTS,
  ARTIST_SORTS,
  clampCursor,
  createPlaylistAction,
  createsPlaylists,
  focusAfterAsk,
  libraryPager,
  libraryTree,
  matchesFilter,
  listPlayRequest,
  loadExpanded,
  nearEnd,
  openRow,
  playlistImage,
  playlistRemoval,
  playlistsFor,
  playRequest,
  removePlaylist,
  saveExpanded,
  sortAlbums,
  sortArtists,
  sortChoice,
  step,
  upNext,
  wantsNextPage,
  type TreeRow,
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
  release_date: null,
});

const empty: SourcePlaylists = {
  spotify: [],
  local: [],
  subsonic: [],
  qobuz: [],
  youtube: [],
  apple_music: [],
  apple_music_uncounted: [],
  radio: [],
  saved_albums: [],
  saved_albums_has_more: false,
  followed_artists: [],
  followed_artists_has_more: false,
  saved_shows: [],
  saved_shows_has_more: false,
  saved_show_ids: [],
  folders: null,
  editable: [],
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
      apple_music: [
        {
          uri: "applemusic:library",
          name: "All songs",
          owner: "Music",
          track_count: 0,
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
      { uri: "file:///rips", name: "Rips", count: 3, image: null },
    ]);
    expect(playlistsFor(playlists, "Radio")).toEqual([
      {
        uri: "radio:https://radio.example",
        name: "Score radio",
        count: null,
        image: null,
      },
    ]);
    expect(playlistsFor(playlists, "Qobuz")).toEqual([]);
    expect(playlistsFor(playlists, "AppleMusic")).toEqual([
      { uri: "applemusic:library", name: "All songs", count: 0, image: null },
    ]);
    // A Music row whose count Music has not given shows none, not 0.
    expect(
      playlistsFor(
        { ...playlists, apple_music_uncounted: ["applemusic:library"] },
        "AppleMusic",
      ),
    ).toEqual([
      {
        uri: "applemusic:library",
        name: "All songs",
        count: null,
        image: null,
      },
    ]);
  });
});

describe("matchesFilter", () => {
  it("matches every word in any field, ignoring case", () => {
    expect(matchesFilter("ROAD", "Road trip")).toBe(true);
    expect(matchesFilter("trip road", "Road trip")).toBe(true);
    expect(matchesFilter("  trip   ROAD  ", "Road trip")).toBe(true);
    expect(matchesFilter("adele 21", "21", "Adele")).toBe(true);
    expect(matchesFilter("adele 25", "21", "Adele")).toBe(false);
    expect(matchesFilter("nyt", "The Daily", null)).toBe(false);
  });

  it("matches everything for a blank query", () => {
    expect(matchesFilter("", "Road trip")).toBe(true);
    expect(matchesFilter("   ", "Road trip")).toBe(true);
    expect(matchesFilter("", null, undefined)).toBe(true);
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

describe("playlistImage", () => {
  const playlist = (uri: string, image_url: string | null) => ({
    uri,
    name: uri,
    owner: "me",
    track_count: 1,
    id: null,
    owner_id: null,
    collaborative: false,
    public: null,
    image_url,
  });

  it("finds the cover of a playlist of any source by URI", () => {
    const lists = {
      ...empty,
      spotify: [playlist("spotify:playlist:1", "https://i.scdn.co/1")],
      youtube: [playlist("youtube:playlist:2", "https://i.ytimg.com/2")],
    };
    expect(playlistImage(lists, "youtube:playlist:2")).toBe(
      "https://i.ytimg.com/2",
    );
    expect(playlistImage(lists, "spotify:playlist:9")).toBeNull();
    expect(playlistImage(null, "spotify:playlist:1")).toBeNull();
  });
});

const list = (uri: string, name: string, collaborative = false) => ({
  uri,
  name,
  owner: "me",
  track_count: 2,
  id: null,
  owner_id: null,
  collaborative,
  public: null,
  image_url: null,
});

describe("createPlaylistAction", () => {
  it("creates a Spotify or a YouTube playlist by trimmed name", () => {
    expect(createPlaylistAction("Spotify", "  Road trip ")).toEqual({
      CreatePlaylist: { name: "Road trip", track_uris: [] },
    });
    expect(createPlaylistAction("YouTube", "Mix")).toEqual({
      CreateYouTubePlaylist: "Mix",
    });
  });

  it("refuses a blank name and a source without playlist creation", () => {
    expect(createPlaylistAction("Spotify", "   ")).toBeNull();
    expect(createPlaylistAction("Local", "Mix")).toBeNull();
    expect(createsPlaylists("Spotify")).toBe(true);
    expect(createsPlaylists("YouTube")).toBe(true);
    expect(createsPlaylists("Qobuz")).toBe(false);
    expect(createsPlaylists(null)).toBe(false);
  });
});

describe("playlistRemoval", () => {
  const lists: SourcePlaylists = {
    ...empty,
    spotify: [
      list("spotify:playlist:mine", "Mine"),
      list("spotify:playlist:jam", "Jam", true),
      list("spotify:playlist:theirs", "Theirs"),
    ],
    editable: ["spotify:playlist:mine", "spotify:playlist:jam"],
  };
  const removal = (source: Source, uri: string, name: string) =>
    playlistRemoval(source, { uri, name }, lists);

  it("deletes an owned Spotify playlist and removes any other one", () => {
    expect(removal("Spotify", "spotify:playlist:mine", "Mine")).toEqual({
      kind: "owned",
      uri: "spotify:playlist:mine",
      name: "Mine",
      label: "Delete",
      action: { UnfollowPlaylist: "spotify:playlist:mine" },
      title: "Delete from Your Library?",
      body: "This will delete Mine from Your Library.",
      confirm: "Delete",
    });
    for (const [uri, name] of [
      ["spotify:playlist:jam", "Jam"],
      ["spotify:playlist:theirs", "Theirs"],
    ])
      expect(removal("Spotify", uri, name)).toMatchObject({
        label: "Remove from Your Library",
        action: { UnfollowPlaylist: uri },
        body: `This will remove ${name} from Your Library.`,
        confirm: "Remove",
      });
  });

  it("deletes a YouTube playlist and leaves the other sources alone", () => {
    expect(removal("YouTube", "youtube:playlist:1", "Later")).toMatchObject({
      label: "Delete",
      action: { DeletePlaylist: "youtube:playlist:1" },
      body: "This will delete Later from spotatui. This cannot be undone.",
    });
    for (const source of ["Local", "Subsonic", "Qobuz", "Radio"] as const)
      expect(removal(source, "x", "X")).toBeNull();
  });
});

describe("removePlaylist", () => {
  it("sends the removal and leaves the open playlist for home", () => {
    const removal = playlistRemoval(
      "Spotify",
      { uri: "spotify:playlist:p", name: "P" },
      empty,
    );
    if (!removal) throw new Error("no removal");
    const send = vi.fn();
    const open = vi.fn();
    removePlaylist(
      removal,
      {
        page: "playlist",
        uri: "spotify:playlist:p",
        name: "P",
        source: "Spotify",
      },
      send,
      open,
    );
    expect(send).toHaveBeenCalledWith({
      UnfollowPlaylist: "spotify:playlist:p",
    });
    expect(open).toHaveBeenCalledWith({ page: "home" });
    open.mockClear();
    removePlaylist(removal, { page: "liked" }, send, open);
    expect(open).not.toHaveBeenCalled();
  });
});

describe("focusAfterAsk", () => {
  const row = { name: "row", isConnected: true };
  const create = { name: "create", isConnected: true };
  const rows = { name: "rows", isConnected: true };

  it("returns to the opener on cancel and skips it after a confirmed removal", () => {
    expect(focusAfterAsk(false, row, [create, rows])).toBe(row);
    expect(focusAfterAsk(true, row, [create, rows])).toBe(create);
  });

  it("falls back to a stable element that is still in the page", () => {
    const gone = { ...row, isConnected: false };
    expect(focusAfterAsk(false, gone, [null, rows])).toBe(rows);
    expect(focusAfterAsk(true, row, [null, null])).toBeNull();
  });
});

describe("libraryTree", () => {
  const playlists = [
    list("p:a", "A"),
    list("p:b", "B"),
    list("p:c", "C"),
    list("p:loose", "Loose"),
  ];
  const nodes: LibraryNode[] = [
    {
      kind: "folder",
      name: "Outer",
      uri: "f:outer",
      children: [
        { kind: "playlist", uri: "p:a" },
        {
          kind: "folder",
          name: "Inner",
          uri: "f:inner",
          children: [{ kind: "playlist", uri: "p:b" }],
        },
        { kind: "playlist", uri: "p:gone" },
      ],
    },
    { kind: "playlist", uri: "p:c" },
  ];
  const shape = (rows: TreeRow[]) =>
    rows.map((row) =>
      row.kind === "folder"
        ? `${row.depth} [${row.expanded ? "-" : "+"}] ${row.name}`
        : `${row.depth} ${row.row.name}`,
    );

  it("hides a closed folder's playlists and ends with the playlists outside the tree", () => {
    expect(shape(libraryTree(nodes, playlists, new Set()))).toEqual([
      "0 [+] Outer",
      "0 C",
      "0 Loose",
    ]);
    expect(shape(libraryTree(nodes, playlists, new Set(["f:inner"])))).toEqual([
      "0 [+] Outer",
      "0 C",
      "0 Loose",
    ]);
  });

  it("indents the open folders' rows and skips playlists it has no info for", () => {
    expect(
      shape(libraryTree(nodes, playlists, new Set(["f:outer", "f:inner"]))),
    ).toEqual(["0 [-] Outer", "1 A", "1 [-] Inner", "2 B", "0 C", "0 Loose"]);
  });
});

describe("sortAlbums", () => {
  const album = (
    name: string,
    artist: string | null,
    release_date: string | null,
  ): AlbumInfo => ({
    id: name,
    uri: null,
    name,
    artists: artist ? [{ id: null, name: artist }] : [],
    album_type: null,
    release_date,
    total_tracks: null,
    image_url: null,
    tracks: [],
  });
  // In the server's order: newest added first.
  const albums = [
    album("beta", "Zed", "2001"),
    album("Alpha", "abba", "1999-05"),
    album("gamma", null, null),
  ];
  const names = (sort: string) => sortAlbums(albums, sort).map((a) => a.name);

  it("keeps the server order for default and newest added, and reverses it for oldest added", () => {
    expect(names("default")).toEqual(["beta", "Alpha", "gamma"]);
    expect(names("date_added:desc")).toEqual(["beta", "Alpha", "gamma"]);
    expect(names("date_added")).toEqual(["gamma", "Alpha", "beta"]);
    expect(names("bogus")).toEqual(["beta", "Alpha", "gamma"]);
  });

  it("sorts by name and first artist ignoring case, and by release date", () => {
    expect(names("name")).toEqual(["Alpha", "beta", "gamma"]);
    expect(names("name:desc")).toEqual(["gamma", "beta", "Alpha"]);
    expect(names("artist")).toEqual(["gamma", "Alpha", "beta"]);
    expect(names("artist:desc")).toEqual(["beta", "Alpha", "gamma"]);
    expect(names("release_date")).toEqual(["gamma", "Alpha", "beta"]);
    expect(names("release_date:desc")).toEqual(["beta", "Alpha", "gamma"]);
    expect(albums.map((a) => a.name)).toEqual(["beta", "Alpha", "gamma"]);
  });
});

describe("sortArtists", () => {
  const artist = (name: string) => ({
    id: name,
    uri: null,
    name,
    image_url: null,
  });
  const artists = [artist("björk"), artist("ABBA"), artist("Coldplay")];
  const names = (sort: string) => sortArtists(artists, sort).map((a) => a.name);

  it("keeps the server order by default and sorts names ignoring case", () => {
    expect(names("default")).toEqual(["björk", "ABBA", "Coldplay"]);
    expect(names("date_added")).toEqual(["björk", "ABBA", "Coldplay"]);
    expect(names("name")).toEqual(["ABBA", "björk", "Coldplay"]);
    expect(names("name:desc")).toEqual(["Coldplay", "björk", "ABBA"]);
  });
});

describe("sortChoice", () => {
  it("takes a known option and falls back to default", () => {
    expect(sortChoice(ALBUM_SORTS, "artist:desc")).toBe("artist:desc");
    expect(sortChoice(ARTIST_SORTS, "artist:desc")).toBe("default");
    expect(sortChoice(ARTIST_SORTS, null)).toBe("default");
  });

  it("labels the orders as the list page and Settings do", () => {
    expect(ALBUM_SORTS.map((option) => option.label)).toEqual([
      "Recently added",
      "Name (A-Z)",
      "Name (Z-A)",
      "Date added (oldest first)",
      "Date added (newest first)",
      "Artist (A-Z)",
      "Artist (Z-A)",
      "Release date (oldest first)",
      "Release date (newest first)",
    ]);
    expect(ARTIST_SORTS).toEqual(ALBUM_SORTS.slice(0, 3));
  });
});

describe("next page", () => {
  it("counts a box within the margin of its end, or too short to scroll, as near the end", () => {
    const box = (scrollTop: number, scrollHeight: number) => ({
      scrollTop,
      scrollHeight,
      clientHeight: 500,
    });
    expect(nearEnd(box(0, 2000))).toBe(false);
    expect(nearEnd(box(1300, 2000))).toBe(true);
    expect(nearEnd(box(0, 300))).toBe(true);
  });

  it("does not count a hidden box as near the end", () => {
    expect(nearEnd({ scrollTop: 0, scrollHeight: 0, clientHeight: 0 })).toBe(
      false,
    );
  });

  it("asks once per pushed list, and only while more pages exist", () => {
    const first = [1, 2];
    // An index bump on cached pages pushes an equal list of the same length.
    const bumped = [1, 2];
    expect(wantsNextPage(true, true, first, null)).toBe(true);
    expect(wantsNextPage(true, true, first, first)).toBe(false);
    expect(wantsNextPage(true, true, bumped, first)).toBe(true);
    expect(wantsNextPage(false, true, bumped, first)).toBe(false);
    expect(wantsNextPage(true, false, bumped, first)).toBe(false);
  });
});

describe("expanded folders storage", () => {
  it("round-trips the open folders and survives junk or a blocked storage", () => {
    const saved = new Map<string, string>();
    const storage = {
      getItem: (key: string) => saved.get(key) ?? null,
      setItem: (key: string, value: string) => void saved.set(key, value),
    };
    saveExpanded(storage, new Set(["f:a", "f:b"]));
    expect([...loadExpanded(storage)]).toEqual(["f:a", "f:b"]);
    for (const key of saved.keys()) saved.set(key, "{not json");
    expect(loadExpanded(storage).size).toBe(0);
    const blocked = {
      getItem: (): string | null => {
        throw new Error("blocked");
      },
      setItem: () => {
        throw new Error("blocked");
      },
    };
    expect(loadExpanded(blocked).size).toBe(0);
    expect(() => saveExpanded(blocked, new Set(["f:a"]))).not.toThrow();
  });
});

describe("libraryPager", () => {
  it("pages the followed artists with their own has-more flag", () => {
    const lists = { ...empty, followed_artists_has_more: true };
    expect(libraryPager("Artists", lists)).toEqual({
      target: "SavedArtists",
      list: lists.followed_artists,
      more: true,
    });
  });

  it("pages albums and podcasts, and leaves playlists alone", () => {
    expect(libraryPager("Albums", empty)?.target).toBe("SavedAlbums");
    expect(libraryPager("Podcasts", empty)?.target).toBe("SavedShows");
    expect(libraryPager("Playlists", empty)).toBe(null);
  });
});
