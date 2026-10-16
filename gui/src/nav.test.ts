import { describe, expect, it } from "vitest";
import {
  fetchFor,
  goBack,
  goForward,
  HOME,
  openAction,
  push,
  type Loaded,
  type View,
} from "./nav";

describe("history", () => {
  it("back and forward walk the opened views", () => {
    const opened = push(push(HOME, { page: "liked" }), { page: "search" });
    const back = goBack(opened);
    expect(back.now).toEqual({ page: "liked" });
    expect(goForward(back).now).toEqual({ page: "search" });
  });

  it("a new view after back drops the forward list", () => {
    const back = goBack(push(HOME, { page: "search" }));
    expect(push(back, { page: "stats" }).forward).toEqual([]);
  });

  it("opening the view already shown keeps the history", () => {
    const opened = push(HOME, { page: "liked" });
    expect(push(opened, { page: "liked" })).toBe(opened);
  });

  it("back at the first view stays there", () => {
    expect(goBack(HOME)).toBe(HOME);
  });
});

describe("openAction", () => {
  it("a Spotify playlist opens by URI and a source playlist by its own action", () => {
    expect(
      openAction({
        page: "playlist",
        uri: "spotify:playlist:1",
        name: "Mix",
        source: "Spotify",
      }),
    ).toEqual({
      Open: { Playlist: { id: "spotify:playlist:1", from_search: false } },
    });
    expect(
      openAction({
        page: "playlist",
        uri: "file:///music",
        name: "Music",
        source: "Local",
      }),
    ).toEqual({ Open: { SourcePlaylist: "file:///music" } });
  });

  it("an artist opens with its name, and a page with no fetch sends nothing", () => {
    expect(openAction({ page: "artist", id: "a1", name: "Kygo" })).toEqual({
      Open: { Artist: { id: "a1", name: "Kygo" } },
    });
    expect(openAction({ page: "party" })).toBeNull();
    expect(openAction({ page: "home" })).toBeNull();
  });

  it("discover asks for Made For You through its library row", () => {
    expect(openAction({ page: "discover" })).toEqual({
      OpenLibrary: "Discover",
    });
  });

  it("a show opens by id, a radio by its seed track, and recent plays by the library route", () => {
    expect(openAction({ page: "show", id: "s1", name: "Pod" })).toEqual({
      Open: { Show: "s1" },
    });
    expect(openAction({ page: "radio", id: "t1", name: "Song" })).toEqual({
      RecommendFromTrackId: { id: "t1", name: "Song" },
    });
    expect(openAction({ page: "recent" })).toEqual({
      Navigate: "RecentlyPlayed",
    });
  });
});

describe("fetchFor", () => {
  const loaded: Loaded = {
    spotify: true,
    liked: true,
    tableUri: "spotify:playlist:1",
    albumId: "a1",
    artistId: "k1",
    showId: "s1",
    radioId: "t1",
  };
  const playlist: View = {
    page: "playlist",
    uri: "spotify:playlist:1",
    name: "Mix",
    source: "Spotify",
  };

  it("a new open fetches even when the channel holds the list", () => {
    expect(fetchFor({ page: "liked" }, true, loaded)).toEqual({
      OpenLibrary: "LikedSongs",
    });
  });

  it("back and forward keep a list the channel still holds", () => {
    expect(fetchFor({ page: "liked" }, false, loaded)).toBeNull();
    expect(fetchFor(playlist, false, loaded)).toBeNull();
    expect(fetchFor({ page: "album", id: "a1" }, false, loaded)).toBeNull();
  });

  it("back and forward keep the show, the radio rows and recent plays the channels hold", () => {
    expect(
      fetchFor({ page: "show", id: "s1", name: "Pod" }, false, loaded),
    ).toBeNull();
    expect(
      fetchFor({ page: "radio", id: "t1", name: "Song" }, false, loaded),
    ).toBeNull();
    expect(fetchFor({ page: "recent" }, false, loaded)).toBeNull();
    expect(
      fetchFor({ page: "radio", id: "t2", name: "Other" }, false, loaded),
    ).toEqual({ RecommendFromTrackId: { id: "t2", name: "Other" } });
  });

  it("back to an album the channel no longer holds fetches it again", () => {
    expect(fetchFor({ page: "album", id: "a2" }, false, loaded)).toEqual({
      Open: { Album: { id: "a2", from_search: false } },
    });
  });

  it("sends no Spotify fetch without a session, but a local list still opens", () => {
    const offline = { ...loaded, spotify: false };
    expect(fetchFor({ page: "home" }, true, offline)).toBeNull();
    expect(
      fetchFor({ page: "artist", id: "k1", name: "Kygo" }, true, offline),
    ).toBeNull();
    expect(
      fetchFor(
        { page: "playlist", uri: "file:///m", name: "M", source: "Local" },
        true,
        offline,
      ),
    ).toEqual({ Open: { SourcePlaylist: "file:///m" } });
  });
});
