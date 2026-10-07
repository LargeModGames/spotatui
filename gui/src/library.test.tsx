import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { LikedSongs } from "./bindings/LikedSongs";
import type { SourcePayload } from "./bindings/SourcePayload";
import type { SourcePlaylists } from "./bindings/SourcePlaylists";
import type { TrackInfo } from "./bindings/TrackInfo";
import { Library } from "./Library";
import { UpNext } from "./UpNext";

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

const source: SourcePayload = {
  active: "Local",
  compiled: ["Spotify", "Local"],
};

const liked: LikedSongs = {
  tracks: [
    track("Rolling in the Deep", "spotify:track:1"),
    track("Set Fire to the Rain", "file:///21/05.flac"),
  ],
  total: 40,
  has_more: true,
  available: true,
  loaded: true,
};

const playlists: SourcePlaylists = {
  spotify: [],
  local: [
    {
      uri: "file:///rips",
      name: "Soundtrack CD rips",
      owner: "",
      track_count: 318,
      id: null,
      owner_id: null,
      collaborative: false,
      public: null,
      image_url: null,
    },
  ],
  subsonic: [],
  qobuz: [],
  youtube: [],
  radio: [],
};

const render = (songs: LikedSongs | null, playingUri: string | null = null) =>
  renderToStaticMarkup(
    <Library
      playlists={playlists}
      liked={songs}
      table={null}
      source={source}
      playingUri={playingUri}
      upNext={[]}
      sync={null}
      statusRev={null}
      statusError={false}
      send={() => {}}
    />,
  );

describe("Library", () => {
  it("shows the active source's playlists with their counts under its heading", () => {
    const html = render(liked);
    expect(html).toContain("PLAYLISTS · LOCAL");
    expect(html).toContain("Soundtrack CD rips");
    expect(html).toContain('class="count">318<');
    expect(html).toMatch(
      /<button type="button" aria-current="page">Liked Songs</,
    );
    expect(html).toMatch(/<button type="button" title="Soundtrack CD rips">/);
  });

  it("presses the chip of the active source only", () => {
    const html = render(liked);
    expect(html).toContain('aria-pressed="true">Local<');
    expect(html).toContain('aria-pressed="false">Spotify<');
  });

  it("marks the first row as the cursor and the playing row with an arrow", () => {
    const html = render(liked, "file:///21/05.flac");
    expect(html).toContain('aria-activedescendant="liked-0"');
    expect(html).toMatch(/id="liked-0"[^>]*aria-selected="true"/);
    expect(html).toMatch(/id="liked-1"[^>]*class="row now"/);
    expect(html).toContain(">▶<");
    expect(html).toContain('class="swatch local"');
    expect(html).toContain(">4:03<");
    expect(html).toContain("2 of 40");
  });

  it("explains an empty list: no session, still loading, or none liked", () => {
    const none = { ...liked, tracks: [], total: 0, has_more: false };
    expect(render({ ...none, available: false })).toContain(
      "Liked Songs needs a Spotify session.",
    );
    expect(render({ ...none, loaded: false })).toContain(
      "Loading Liked Songs…",
    );
    expect(render(none)).toContain("No liked songs yet.");
  });
});

describe("UpNext", () => {
  it("numbers the queued tracks and says when nothing is queued", () => {
    const html = renderToStaticMarkup(
      <UpNext tracks={[track("Lovesong", "file:///21/10.flac")]} />,
    );
    expect(html).toContain('class="n">1<');
    expect(html).toContain("Lovesong");
    expect(renderToStaticMarkup(<UpNext tracks={[]} />)).toContain(
      "Nothing is queued.",
    );
  });
});
