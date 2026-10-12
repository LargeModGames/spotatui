import { describe, expect, it } from "vitest";
import type { PlaylistInfo } from "./bindings/PlaylistInfo";
import type { TrackInfo } from "./bindings/TrackInfo";
import { trackMenu, type MenuEntry } from "./trackMenuModel";

const track = (patch: Partial<TrackInfo> = {}): TrackInfo => ({
  uri: "spotify:track:t1",
  name: "Clarity",
  artists: ["Zedd", "Foxes"],
  album: "Clarity",
  duration_ms: 271000,
  id: "t1",
  album_id: "a1",
  artist_refs: [
    { id: "zedd", name: "Zedd" },
    { id: "foxes", name: "Foxes" },
  ],
  is_playable: true,
  is_local: false,
  track_number: 1,
  explicit: false,
  image_url: null,
  release_date: null,
  ...patch,
});

const playlist: PlaylistInfo = {
  uri: "spotify:playlist:p1",
  name: "Mine",
  owner: "me",
  track_count: 3,
  id: "p1",
  owner_id: "me",
  collaborative: false,
  public: null,
  image_url: null,
};

const labels = (groups: MenuEntry[][]) =>
  groups.map((group) => group.map((entry) => entry.label));

describe("trackMenu", () => {
  it("offers Spotify's track entries in three groups", () => {
    expect(labels(trackMenu(track(), false, [playlist]))).toEqual([
      ["Add to playlist", "Save to your Liked Songs", "Add to queue"],
      ["Go to artist", "Go to album"],
      ["Share"],
    ]);
  });

  it("adds the track to the chosen playlist by URI", () => {
    const [add] = trackMenu(track(), false, [playlist])[0];
    expect(add.kind === "submenu" && add.entries[0]).toEqual({
      label: "Mine",
      kind: "action",
      action: {
        AddTrackToPlaylist: {
          playlist: "spotify:playlist:p1",
          track: "spotify:track:t1",
        },
      },
    });
  });

  it("names the removal for a liked track and leaves out Add to playlist with no editable playlist", () => {
    expect(labels(trackMenu(track(), true, []))[0]).toEqual([
      "Remove from your Liked Songs",
      "Add to queue",
    ]);
  });

  it("goes straight to a lone artist and lists several in a submenu", () => {
    const lone = track({ artist_refs: [{ id: "zedd", name: "Zedd" }] });
    expect(trackMenu(lone, false, [])[1][0]).toEqual({
      label: "Go to artist",
      kind: "open",
      view: { page: "artist", id: "zedd", name: "Zedd" },
    });
    const many = trackMenu(track(), false, [])[1][0];
    expect(many.kind === "submenu" && many.entries.map((e) => e.label)).toEqual(
      ["Zedd", "Foxes"],
    );
  });

  it("copies the open.spotify.com link of the track", () => {
    const share = trackMenu(track(), false, [])[2][0];
    expect(share.kind === "submenu" && share.entries[0]).toMatchObject({
      kind: "copy",
      text: "https://open.spotify.com/track/t1",
    });
  });

  it("offers only Add to queue for a local file and nothing for a radio station", () => {
    const local = track({
      uri: "file:///music/a.flac",
      id: null,
      album_id: null,
      artist_refs: [],
    });
    expect(labels(trackMenu(local, false, [playlist]))).toEqual([
      ["Add to queue"],
    ]);
    expect(
      trackMenu(track({ uri: "radio:https://x", id: null }), false, []),
    ).toEqual([]);
  });
});
