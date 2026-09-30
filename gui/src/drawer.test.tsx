import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { NowPlaying } from "./bindings/NowPlaying";
import type { QueuePayload } from "./bindings/QueuePayload";
import type { TrackInfo } from "./bindings/TrackInfo";
import { QueueDrawer } from "./QueueDrawer";

const track = (name: string, uri: string): TrackInfo => ({
  uri,
  name,
  artists: ["Adele"],
  album: "21",
  duration_ms: 243_000,
  id: null,
  album_id: null,
  artist_refs: [],
  is_playable: true,
  is_local: false,
  track_number: 0,
  explicit: false,
  image_url: null,
});

const queue: QueuePayload = {
  now: null,
  native: [track("Lovesong", "file:///21/10.flac")],
  spotify: {
    currently_playing: null,
    items: [
      {
        kind: "track",
        track: track("Freeze", "spotify:track:1"),
        episode: null,
      },
    ],
  },
};

const item = (uri: string): NowPlaying => ({
  title: "Set Fire to the Rain",
  artists: ["Adele"],
  album: "21",
  image_url: null,
  duration_ms: 243_000,
  uri,
  is_playing: true,
  is_live: false,
  shuffle: false,
  repeat: "off",
});

const drawer = (uri: string) =>
  renderToStaticMarkup(
    <QueueDrawer
      queue={queue}
      item={item(uri)}
      send={() => {}}
      onClose={() => {}}
    />,
  );

describe("QueueDrawer", () => {
  it("lists now playing, the native queue and the read-only Spotify queue", () => {
    const html = drawer("spotify:track:0");
    expect(html).toContain("Set Fire to the Rain");
    expect(html).toMatch(/id="queue-0"[^>]*aria-selected="true"/);
    expect(html).toContain("Lovesong");
    expect(html).toContain("SPOTIFY QUEUE");
    expect(html).toContain("Freeze");
  });

  it("hides the Spotify queue while another source plays", () => {
    const html = drawer("file:///21/05.flac");
    expect(html).not.toContain("SPOTIFY QUEUE");
    expect(html).not.toContain("Freeze");
  });
});
