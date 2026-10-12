import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { NowPlaying } from "./bindings/NowPlaying";
import type { UpdateAsk } from "./bindings/UpdateAsk";
import type { UpdateStatus } from "./bindings/UpdateStatus";
import type { Page } from "./nav";
import { PlayerBar, type Panel } from "./PlayerBar";
import { byName } from "./testTree";
import { TopBar } from "./TopBar";

const song: NowPlaying = {
  title: "Set Fire to the Rain",
  artists: ["Adele"],
  album: "21",
  image_url: null,
  duration_ms: 243_000,
  uri: "file:///music/21/05.flac",
  is_playing: false,
  is_live: false,
  shuffle: false,
  repeat: "off",
  context_uri: null,
};

/** The opening tag of the element with this aria-label. */
const tag = (html: string, label: string) =>
  html.match(new RegExp(`<[^>]*aria-label="${label}"[^>]*>`))?.[0] ?? "";

const player = (
  item: NowPlaying | null,
  {
    liked = false,
    volume = 72,
    connected = true,
    panel = "now" as Panel,
    lyricsOpen = false,
  } = {},
) =>
  renderToStaticMarkup(
    <PlayerBar
      playback={{ item, volume, device: "This PC", liked }}
      position={{ ms: 100_000, at: 0 }}
      connected={connected}
      devices={[]}
      panel={panel}
      onPanel={() => {}}
      lyricsOpen={lyricsOpen}
      onLyrics={() => {}}
      onRoom={() => {}}
      send={() => {}}
    />,
  );

describe("PlayerBar", () => {
  it("shows the source, the position and the length of a paused song", () => {
    const html = player(song);
    expect(html).toContain("Set Fire to the Rain");
    expect(html).toContain(">Adele<");
    expect(html).toContain("LOCAL");
    expect(html).toContain(">1:40<");
    expect(html).toContain(">4:03<");
    expect(tag(html, "Play")).toContain("<button");
  });

  it("exposes seek and volume as focusable sliders", () => {
    const html = player(song);
    expect(tag(html, "Seek")).toContain('role="slider"');
    expect(tag(html, "Seek")).toContain('tabindex="0"');
    expect(tag(html, "Seek")).toContain('aria-valuetext="1:40 of 4:03"');
    expect(tag(html, "Volume")).toContain('tabindex="0"');
    expect(tag(html, "Volume")).toContain('aria-valuetext="72%"');
  });

  it("marks shuffle and repeat one as pressed", () => {
    const html = player({ ...song, shuffle: true, repeat: "track" });
    expect(tag(html, "Shuffle")).toContain('aria-pressed="true"');
    expect(tag(html, "Repeat one")).toContain('aria-pressed="true"');
    expect(html).toContain("<sup>1</sup>");
  });

  it("offers the heart for a Spotify song only, pressed when it is liked", () => {
    const spotify = { ...song, uri: "spotify:track:4uLU6hMCjMI75M1A2tKUQC" };
    expect(tag(player(song), "Like")).toBe("");
    expect(tag(player(spotify), "Like")).toContain('aria-pressed="false"');
    expect(tag(player(spotify, { liked: true }), "Like")).toContain(
      'aria-pressed="true"',
    );
    expect(player(spotify)).not.toContain("SPOTIFY");
  });

  it("counts the time of a stream up and shows it as live", () => {
    const html = player({ ...song, is_live: true, duration_ms: 0 });
    expect(html).toContain(">1:40<");
    expect(html).toContain(">live<");
  });

  it("shows nothing playing without an item", () => {
    expect(player(null)).toContain("Nothing playing");
  });

  it("presses the panel buttons that match the open panel and the lyrics page", () => {
    const now = player(song, { panel: "now" });
    expect(tag(now, "Now playing view")).toContain('aria-pressed="true"');
    expect(tag(now, "Queue")).toContain('aria-pressed="false"');
    expect(tag(now, "Lyrics")).toContain('aria-pressed="false"');
    const queue = player(song, { panel: "queue", lyricsOpen: true });
    expect(tag(queue, "Now playing view")).toContain('aria-pressed="false"');
    expect(tag(queue, "Queue")).toContain('aria-pressed="true"');
    expect(tag(queue, "Lyrics")).toContain('aria-pressed="true"');
  });

  it("offers unmute at zero volume and mute otherwise", () => {
    expect(tag(player(song), "Mute")).toContain("<button");
    expect(tag(player(song, { volume: 0 }), "Unmute")).toContain("<button");
  });

  it("disables the transport and the device picker without a connection", () => {
    const html = player(song, { connected: false });
    for (const label of ["Play", "Next", "Connect to a device", "Mute"])
      expect(tag(html, label)).toContain('disabled=""');
    expect(tag(html, "Seek")).toContain('aria-disabled="true"');
    expect(tag(html, "Full screen")).not.toContain("disabled");
    expect(tag(html, "Connect to a device")).toContain('aria-expanded="false"');
  });
});

describe("TopBar", () => {
  const topBar = ({
    page = "home" as Page,
    device = "This PC" as string | null,
    connected = true,
    canBack = false,
    query = "",
    update = null as UpdateStatus | null,
    onPage = (() => {}) as (page: Page) => void,
    onUpdate = (() => {}) as (ask: UpdateAsk) => void,
  } = {}) => (
    <TopBar
      page={page}
      canBack={canBack}
      canForward={false}
      onBack={() => {}}
      onForward={() => {}}
      onPage={onPage}
      query={query}
      onQuery={() => {}}
      onRun={() => {}}
      device={device}
      connected={connected}
      update={update}
      onUpdate={onUpdate}
    />
  );
  const bar = (options: Parameters<typeof topBar>[0] = {}) =>
    renderToStaticMarkup(topBar(options));

  it("the gear opens Settings and is marked on the Settings page", () => {
    const pages: Page[] = [];
    (
      byName(topBar({ onPage: (page) => pages.push(page) }), "Settings").props
        .onClick as () => void
    )();
    expect(pages).toEqual(["settings"]);
    expect(tag(bar(), "Settings")).toMatch(/title="Settings \((Ctrl\+|⌘),\)"/);
    expect(tag(bar(), "Settings")).not.toContain("aria-current");
    expect(tag(bar({ page: "settings" }), "Settings")).toContain(
      'aria-current="page"',
    );
  });

  it("offers a downloaded update as a pill that installs it", () => {
    const asks: UpdateAsk[] = [];
    const ready = topBar({
      update: { state: "ready", version: "0.44.0" },
      onUpdate: (ask) => asks.push(ask),
    });
    expect(renderToStaticMarkup(ready)).toContain(
      'title="Restart to install version 0.44.0"',
    );
    (byName(ready, "Update ready").props.onClick as () => void)();
    expect(asks).toEqual(["install"]);
    for (const update of [
      null,
      { state: "available", version: "0.44.0" },
      { state: "downloading", version: "0.44.0", percent: 40 },
    ] as UpdateStatus[])
      expect(bar({ update })).not.toContain("Update ready");
  });

  it("marks the current page and names each page's digit key", () => {
    const home = bar();
    expect(tag(home, "Home")).toContain('aria-current="page"');
    expect(tag(home, "Stats")).not.toContain("aria-current");
    expect(tag(home, "Stats")).toContain('title="Stats (5)"');
    expect(tag(bar({ page: "party" }), "Party")).toContain(
      'aria-current="page"',
    );
    for (const label of ["Discover", "Session", "Stats", "Party"])
      expect(tag(home, label)).toContain("<button");
  });

  it("disables back and forward when the history cannot move", () => {
    expect(tag(bar(), "Go back")).toContain('disabled=""');
    expect(tag(bar({ canBack: true }), "Go back")).not.toContain("disabled");
    expect(tag(bar(), "Go forward")).toContain('disabled=""');
  });

  it("shows the query in the search field", () => {
    const field = tag(bar({ query: "adele" }), "Search");
    expect(field).toContain('id="top-search"');
    expect(field).toContain('type="search"');
    expect(field).toContain('placeholder="What do you want to play?"');
    expect(field).toContain('value="adele"');
  });

  it("lights the device dot only for a connected device", () => {
    expect(bar()).toContain('class="dot on"');
    expect(bar({ device: null })).toContain("No device");
    expect(bar({ device: null })).not.toContain('class="dot on"');
    expect(bar({ connected: false })).not.toContain('class="dot on"');
  });
});
