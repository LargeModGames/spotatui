import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { Action } from "./bindings/Action";
import type { NowPlaying } from "./bindings/NowPlaying";
import { MiniCard, miniCommand, MiniPlayer } from "./MiniPlayer";
import { byName } from "./testTree";

const song: NowPlaying = {
  title: "Set Fire to the Rain",
  artists: ["Adele", "Guest"],
  album: "21",
  image_url: "https://img/21.jpg",
  duration_ms: 243_000,
  uri: "spotify:track:1",
  is_playing: false,
  is_live: false,
  shuffle: false,
  repeat: "off",
  context_uri: null,
  context_name: null,
  quality: null,
};

/** The opening tag of the element with this aria-label. */
const tag = (html: string, label: string) =>
  html.match(new RegExp(`<[^>]*aria-label="${label}"[^>]*>`))?.[0] ?? "";

const mini = (
  item: NowPlaying | null,
  { connected = true, liked = false } = {},
) =>
  renderToStaticMarkup(
    <MiniPlayer
      playback={{ item, volume: 50, device: "This PC", liked }}
      position={{ ms: 100_000, at: 0 }}
      connected={connected}
      send={() => {}}
      onExit={() => {}}
    />,
  );

const card = (
  item: NowPlaying | null,
  send: (action: Action) => void,
  onExit: () => void = () => {},
) => (
  <MiniCard
    playback={{ item, volume: 50, device: "This PC", liked: false }}
    elapsed={60_750}
    connected
    send={send}
    onExit={onExit}
  />
);

const click = (node: ReturnType<typeof card>, name: string) =>
  (byName(node, name).props.onClick as () => void)();

describe("MiniPlayer", () => {
  it("shows the cover, the title, the artists and the position", () => {
    const html = mini(song);
    expect(html).toContain('src="https://img/21.jpg"');
    expect(html).toContain(">Set Fire to the Rain<");
    expect(html).toContain(">Adele, Guest<");
    expect(html).toContain(">1:40<");
    expect(html).toContain(">4:03<");
    expect(tag(html, "Seek")).toContain('aria-valuetext="1:40 of 4:03"');
    expect(tag(html, "Play")).toContain("<button");
  });

  it("offers the heart for a Spotify song and the badge for another source", () => {
    expect(tag(mini(song, { liked: true }), "Like")).toContain(
      'aria-pressed="true"',
    );
    const local = mini({ ...song, uri: "file:///music/21/05.flac" });
    expect(tag(local, "Like")).toBe("");
    expect(local).toContain("LOCAL");
  });

  it("sends the transport, like and seek actions", () => {
    const sent: Action[] = [];
    const node = card(song, (action) => sent.push(action));
    for (const name of [
      "Shuffle",
      "Previous",
      "Play",
      "Next",
      "Repeat",
      "Like",
    ])
      click(node, name);
    const seek = byName(node, "Seek").props.onKeyDown as (event: {
      key: string;
      preventDefault: () => void;
    }) => void;
    seek({ key: "ArrowRight", preventDefault: () => {} });
    expect(sent).toEqual([
      "ToggleShuffle",
      "PreviousTrack",
      "TogglePlayback",
      "NextTrack",
      "CycleRepeat",
      "ToggleSaveCurrentItem",
      "SeekForward",
    ]);
  });

  it("the exit button goes back to the full window", () => {
    let exits = 0;
    click(
      card(
        song,
        () => {},
        () => exits++,
      ),
      "Exit mini player",
    );
    expect(exits).toBe(1);
    expect(tag(mini(song), "Exit mini player")).toContain(
      'aria-keyshortcuts="Shift+T Escape"',
    );
  });

  it("shows a quiet empty state with the exit button when nothing plays", () => {
    const html = mini(null);
    expect(html).toContain("Nothing playing");
    expect(tag(html, "Exit mini player")).toContain("<button");
    expect(tag(html, "Play")).toBe("");
    expect(tag(html, "Seek")).toBe("");
  });

  it("keeps only play/pause, the mini key and escape over the hidden page", () => {
    for (const kept of ["toggle", "mini", "escape"] as const)
      expect(miniCommand(kept)).toBe(kept);
    for (const dropped of [
      "command",
      "search",
      "queue",
      "room",
      "lyrics",
      "settings",
      { area: "stats" },
    ] as const)
      expect(miniCommand(dropped)).toBeNull();
    expect(miniCommand(null)).toBeNull();
  });

  it("disables the controls without a connection but keeps the exit", () => {
    const html = mini(song, { connected: false });
    for (const label of [
      "Shuffle",
      "Previous",
      "Play",
      "Next",
      "Repeat",
      "Like",
    ])
      expect(tag(html, label)).toContain('disabled=""');
    expect(tag(html, "Seek")).toContain('aria-disabled="true"');
    expect(tag(html, "Exit mini player")).not.toContain("disabled");
  });
});
