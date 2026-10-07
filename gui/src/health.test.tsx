import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { PlaylistSyncPayload } from "./bindings/PlaylistSyncPayload";
import { LibraryHealth } from "./LibraryHealth";

const sync: PlaylistSyncPayload = {
  running: false,
  last_summary: "Playlist sync: 1 link, 0 added",
  last_failed: false,
  links: [
    {
      id: "a",
      source: "Spotify",
      name: "Road Trip",
      last_line: "Road Trip: skipped (Qobuz is not logged in)",
      last_failed: false,
      mirrors: [
        {
          source: "Qobuz",
          matched: 40,
          unmatched: [
            {
              master_key: "k",
              title: "Levels",
              artist: "Avicii",
              reason: "NoCandidate",
            },
          ],
          last_run: "2026-09-29T12:00:00Z",
        },
      ],
    },
  ],
};

const render = (payload: PlaylistSyncPayload | null) =>
  renderToStaticMarkup(
    <LibraryHealth sync={payload} send={() => {}} onBack={() => {}} />,
  );

describe("LibraryHealth", () => {
  it("shows each link's master and mirror state, the unmatched tracks and the last run", () => {
    const html = render(sync);
    expect(html).toContain('class="cell master">MASTER<');
    expect(html).toContain('class="cell unmatched">1 unmatched<');
    expect(html).toContain("Levels – Avicii: no match");
    expect(html).toContain("skipped (Qobuz is not logged in)");
    expect(html).toContain("Sync all now");
  });

  it("says which build can link when there are no links", () => {
    expect(render({ ...sync, links: [] })).toContain(
      "A build with Qobuz, Subsonic or YouTube can mirror a playlist",
    );
    expect(render({ ...sync, links: [], running: true })).toContain(
      "Loading the linked playlists…",
    );
  });
});
