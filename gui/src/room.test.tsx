import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { AlbumInfo } from "./bindings/AlbumInfo";
import type { LyricsPayload } from "./bindings/LyricsPayload";
import type { NowPlaying } from "./bindings/NowPlaying";
import type { TrackInfo } from "./bindings/TrackInfo";
import { Room } from "./Room";

const track = (name: string, index: number): TrackInfo => ({
  uri: `spotify:track:t${index}`,
  name,
  artists: ["Adele"],
  album: "",
  duration_ms: 240_000,
  id: `t${index}`,
  album_id: null,
  artist_refs: [],
  is_playable: true,
  is_local: false,
  track_number: index + 1,
  explicit: false,
  image_url: null,
});

const album: AlbumInfo = {
  id: "21",
  uri: "spotify:album:21",
  name: "21",
  artists: [{ id: "adele", name: "Adele" }],
  album_type: "album",
  release_date: "2011-01-24",
  total_tracks: 4,
  image_url: null,
  tracks: [
    "Rolling in the Deep",
    "Rumour Has It",
    "Turning Tables",
    "Set Fire",
  ].map(track),
};

const item: NowPlaying = {
  title: "Turning Tables",
  artists: ["Adele"],
  album: "21",
  image_url: null,
  duration_ms: 240_000,
  uri: "spotify:track:t2",
  is_playing: false,
  is_live: false,
  shuffle: false,
  repeat: "off",
  context_uri: "spotify:album:21",
};

const render = (
  patch: {
    item?: NowPlaying;
    lyrics?: LyricsPayload;
    showLyrics?: boolean;
  } = {},
) =>
  renderToStaticMarkup(
    <Room
      item={patch.item ?? item}
      position={{ ms: 60_000, at: 0 }}
      connected
      album={album}
      lyrics={patch.lyrics ?? null}
      upNext={[]}
      queueAhead={false}
      showLyrics={patch.showLyrics ?? false}
      active={false}
      send={() => {}}
    />,
  );

describe("Room", () => {
  it("marks the playing row and counts through the album in order", () => {
    const html = render();
    expect(html).toContain("NOW LISTENING · SONG 3 OF 4");
    expect(html).toMatch(/class="now" aria-current="true"/);
    expect(html).toContain("SIDE A");
    expect(html).toContain("9:00 in");
    expect(html).toContain("2011 · 4 songs · 16 minutes · plays from Spotify");
  });

  it("leaves out the album progress in a playlist context", () => {
    const html = render({
      item: { ...item, context_uri: "spotify:playlist:1" },
    });
    expect(html).not.toContain("SONG 3 OF 4");
    expect(html).not.toContain("left");
  });

  it("says a local file has no tracklist", () => {
    const html = render({
      item: { ...item, uri: "file:///21/03.flac", context_uri: null },
    });
    expect(html).toContain("The tracklist shows for Spotify albums.");
  });

  it("shows estimated lyrics without a current line", () => {
    const html = render({
      showLyrics: true,
      lyrics: {
        status: "found",
        synced: false,
        lines: [
          { at_ms: 0, text: "Close enough to start a war" },
          { at_ms: 4_000, text: "All that I have is on the floor" },
        ],
      },
    });
    expect(html).toContain("TIMING ESTIMATED");
    expect(html).not.toContain('aria-current="true"');
    expect(
      render({
        showLyrics: true,
        lyrics: { status: "not_found", synced: false, lines: [] },
      }),
    ).toContain("No lyrics for this song.");
  });
});
