import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { TrackInfo } from "./bindings/TrackInfo";
import { TrackActionsContext, type TrackActions } from "./TrackMenu";
import { TrackTable } from "./TrackTable";

const track = (id: string | null, uri: string): TrackInfo => ({
  uri,
  name: `Song ${id ?? "local"}`,
  artists: ["Zedd"],
  album: "Clarity",
  duration_ms: 271000,
  id,
  album_id: null,
  artist_refs: [],
  is_playable: true,
  is_local: id === null,
  track_number: 1,
  explicit: false,
  image_url: null,
  release_date: null,
});

const rows = [
  track("t1", "spotify:track:t1"),
  track("t2", "spotify:track:t2"),
  track(null, "file:///music/a.flac"),
];

const render = (actions: TrackActions | null) =>
  renderToStaticMarkup(
    <TrackActionsContext.Provider value={actions}>
      <TrackTable
        label="Mix"
        idPrefix="mix"
        tracks={rows}
        cursor={0}
        playingUri={null}
        onKeyDown={() => {}}
        onPick={() => {}}
        onPlay={() => {}}
      />
    </TrackActionsContext.Provider>,
  );

const actions: TrackActions = {
  send: () => {},
  open: () => {},
  liked: new Set(["t1"]),
  playlists: [],
};

describe("TrackTable row actions", () => {
  it("fills the heart of a liked Spotify row and offers to save the others", () => {
    const html = render(actions);
    expect(html).toMatch(
      /aria-label="Remove from Liked Songs" aria-pressed="true"/,
    );
    expect(html).toMatch(
      /aria-label="Save to Liked Songs" aria-pressed="false"/,
    );
  });

  it("gives every row a more button but no heart to a local file", () => {
    const html = render(actions);
    expect(html.match(/aria-label="More options for /g)).toHaveLength(3);
    expect(html.match(/Liked Songs" aria-pressed/g)).toHaveLength(2);
  });

  it("draws no heart and no menu button without the row actions", () => {
    const html = render(null);
    expect(html).not.toContain("More options");
    expect(html).not.toContain("Liked Songs");
  });
});
