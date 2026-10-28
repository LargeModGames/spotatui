import { EN } from "./i18n";
import { describe, expect, it } from "vitest";
import type { Action } from "./bindings/Action";
import type { AlbumInfo } from "./bindings/AlbumInfo";
import type { DiscoverPayload } from "./bindings/DiscoverPayload";
import type { TrackInfo } from "./bindings/TrackInfo";
import type { TrackTablePayload } from "./bindings/TrackTablePayload";
import {
  albumKind,
  albumPlay,
  asksDiscover,
  copyLink,
  discoverView,
  loadLatch,
  lengthLine,
  radioRows,
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
  shelves: null,
  ...over,
});

describe("asksDiscover", () => {
  it("asks once the channel says a session exists and nothing landed", () => {
    expect(asksDiscover(discover({}), true)).toBe(true);
  });

  it("waits for the channel, a session and a running fetch", () => {
    expect(asksDiscover(null, true)).toBe(false);
    expect(asksDiscover(discover({ available: false }), true)).toBe(false);
    expect(asksDiscover(discover({ loading: true }), true)).toBe(false);
  });

  it("waits for the socket during a reconnect, when the store still holds the old channel", () => {
    expect(asksDiscover(discover({}), false)).toBe(false);
  });

  it("skips shelves that already landed, even empty ones", () => {
    expect(asksDiscover(discover({ shelves: [] }), true)).toBe(false);
  });
});

describe("discoverView", () => {
  const mix = (uri: string) => ({
    uri,
    name: uri,
    description: null,
    image_url: `https://i.scdn.co/${uri}`,
  });

  it("opens a card as a Spotify playlist that carries its cover", () => {
    expect(discoverView(mix("a"))).toEqual({
      page: "playlist",
      uri: "a",
      name: "a",
      source: "Spotify",
      image: "https://i.scdn.co/a",
    });
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
    await copyLink(link, "copied_album_link", (a) => sent.push(a), {
      writeText: async (text) => {
        written.push(text);
      },
    });
    expect(written).toEqual([link]);
    expect(sent).toEqual([{ Notify: ["Copied the album link.", 3] }]);
  });

  it("reports an error when the write fails or there is no clipboard", async () => {
    const sent: Action[] = [];
    await copyLink(link, "copied_album_link", (a) => sent.push(a), {
      writeText: () => Promise.reject(new Error("denied")),
    });
    await copyLink(link, "copied_album_link", (a) => sent.push(a), undefined);
    expect(sent).toEqual([
      { NotifyError: ["Could not copy the link.", 4] },
      { NotifyError: ["Could not copy the link.", 4] },
    ]);
  });
});

for (const key of [
  "copied_album_link",
  "copied_song_link",
  "copied_show_link",
  "copied_episode_link",
  "copy_party_done",
] as const) {
  for (const failure of [false, true]) {
    it(`${key} reads the current table when a pending clipboard write ${failure ? "fails" : "finishes"}`, async () => {
      let current = EN;
      let finish!: () => void;
      let fail!: (error: Error) => void;
      const deferred = new Promise<void>((resolve, reject) => {
        finish = resolve;
        fail = reject;
      });
      const sent: Action[] = [];
      const party = key === "copy_party_done";
      const pending = copyLink(
        "literal {url}",
        key,
        (action) => sent.push(action),
        { writeText: () => deferred },
        () => current,
        party ? "copy_party_failed" : "copy_link_failed",
      );
      current = {
        ...EN,
        [key]: "new completion",
        copy_link_failed: "new failure",
        copy_party_failed: "new party failure",
      };
      if (failure) fail(new Error("raw error"));
      else finish();
      await pending;
      expect(sent).toEqual([
        failure
          ? { NotifyError: [party ? "new party failure" : "new failure", 4] }
          : { Notify: ["new completion", 3] },
      ]);
    });
  }
}

describe("loadLatch", () => {
  const load = {
    id: "seed",
    fresh: false,
    landed: false,
    statusRev: 5,
    statusError: true,
  };

  it("keeps the page failed after the error status expires", () => {
    const failed = loadLatch({ openedAt: 4, failedId: null }, load);
    expect(failed.failedId).toBe("seed");
    const expired = loadLatch(failed, {
      ...load,
      statusRev: 6,
      statusError: false,
    });
    expect(expired.failedId).toBe("seed");
  });

  it("clears the failure once the data lands", () => {
    const landed = loadLatch(
      { openedAt: 4, failedId: "seed" },
      { ...load, landed: true },
    );
    expect(landed).toEqual({ openedAt: 5, failedId: null });
  });

  it("ignores an error from before the open", () => {
    expect(loadLatch({ openedAt: 5, failedId: null }, load).failedId).toBe(
      null,
    );
  });

  it("clears the failure when the page opens again, even under a live error", () => {
    const reopened = loadLatch(
      { openedAt: 4, failedId: "seed" },
      { ...load, fresh: true },
    );
    expect(reopened).toEqual({ openedAt: 5, failedId: null });
    expect(loadLatch(reopened, load).failedId).toBe(null);
  });
});
