import type { ReactElement, ReactNode } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import type { Action } from "./bindings/Action";
import type { PlaylistInfo } from "./bindings/PlaylistInfo";
import type { Source } from "./bindings/Source";
import type { SourcePlaylists } from "./bindings/SourcePlaylists";
import {
  LibraryPanel,
  PlaylistRows,
  ShowRows,
  SortSelect,
} from "./LibraryPanel";
import { ALBUM_SORTS } from "./libraryModel";
import type { View } from "./nav";
import { hostElements } from "./testTree";

const playlist = (
  uri: string,
  name: string,
  collaborative = false,
): PlaylistInfo => ({
  uri,
  name,
  owner: "me",
  track_count: 3,
  id: null,
  owner_id: null,
  collaborative,
  public: null,
  image_url: null,
});

const payload: SourcePlaylists = {
  spotify: [
    playlist("spotify:playlist:mine", "Road trip"),
    playlist("spotify:playlist:theirs", "Their mix"),
    playlist("spotify:playlist:jam", "Jam", true),
    playlist("spotify:playlist:loose", "Loose"),
  ],
  local: [playlist("file:///rips", "Rips")],
  subsonic: [],
  qobuz: [],
  youtube: [playlist("youtube:playlist:yt", "Watch later")],
  radio: [],
  saved_albums: [],
  saved_albums_has_more: false,
  followed_artists: [],
  saved_shows: [
    {
      id: "show1",
      uri: "spotify:show:show1",
      name: "The Daily",
      description: "",
      publisher: "NYT",
      image_url: null,
    },
  ],
  saved_shows_has_more: false,
  saved_show_ids: ["show1"],
  editable: ["spotify:playlist:mine", "spotify:playlist:jam"],
  folders: [
    {
      kind: "folder",
      name: "Moods",
      uri: "spotify:user:me:folder:moods",
      children: [
        { kind: "playlist", uri: "spotify:playlist:mine" },
        { kind: "playlist", uri: "spotify:playlist:theirs" },
      ],
    },
    { kind: "playlist", uri: "spotify:playlist:jam" },
  ],
};

const panel = (active: Source, playlists: SourcePlaylists = payload) =>
  renderToStaticMarkup(
    <LibraryPanel
      playlists={playlists}
      artists={[]}
      source={{ active, compiled: [active] }}
      sync={null}
      view={{ page: "home" }}
      open={() => {}}
      send={() => {}}
    />,
  );

const rows = (
  source: Source,
  options: {
    view?: View;
    expanded?: string[];
    playlists?: SourcePlaylists;
  } = {},
) => {
  const open = vi.fn();
  const send = vi.fn<(action: Action) => void>();
  const onToggle = vi.fn();
  const onMenu = vi.fn();
  const tree = (
    <PlaylistRows
      playlists={options.playlists ?? payload}
      source={source}
      unmatched={0}
      view={options.view ?? { page: "home" }}
      open={open}
      send={send}
      expanded={new Set(options.expanded ?? [])}
      onToggle={onToggle}
      onMenu={onMenu}
    />
  );
  return { tree, open, send, onToggle, onMenu };
};

const rowName = (element: ReactElement<Record<string, unknown>>) =>
  String(element.props.title).split("\n")[0];

const titled = (tree: ReactNode, title: string) => {
  const found = hostElements(tree).find(
    (element) => element.type === "button" && rowName(element) === title,
  );
  if (!found) throw new Error(`no row titled ${title}`);
  return found;
};

const titles = (tree: ReactNode) =>
  hostElements(tree)
    .filter((element) => element.type === "button")
    .map(rowName);

const rightClick = (tree: ReactNode, title: string) => {
  const handler = titled(tree, title).props.onContextMenu as
    ((event: unknown) => void) | undefined;
  if (!handler) return false;
  handler({ preventDefault: () => {}, clientX: 10, clientY: 20 });
  return true;
};

describe("LibraryPanel header and filters", () => {
  it("offers Create playlist for Spotify and YouTube only", () => {
    expect(panel("Spotify")).toContain('aria-label="Create playlist"');
    expect(panel("YouTube")).toContain('aria-label="Create playlist"');
    expect(panel("Local")).not.toContain('aria-label="Create playlist"');
    expect(panel("Radio")).not.toContain('aria-label="Create playlist"');
  });

  it("adds the Podcasts filter for Spotify only", () => {
    expect(panel("Spotify")).toContain('aria-pressed="false">Podcasts<');
    expect(panel("YouTube")).not.toContain(">Podcasts<");
  });

  it("draws Spotify folders closed, keeping their playlists out of the root", () => {
    const html = panel("Spotify");
    expect(html).toMatch(/title="Moods" aria-expanded="false"/);
    expect(html).not.toContain('title="Road trip');
    expect(html).toContain('title="Jam');
    expect(html.indexOf('title="Jam')).toBeLessThan(
      html.indexOf('title="Loose'),
    );
  });

  it("keeps the flat list without a rootlist", () => {
    const html = panel("Spotify", { ...payload, folders: null });
    expect(html).not.toContain("aria-expanded");
    expect(html).toContain('title="Road trip');
  });
});

describe("PlaylistRows", () => {
  it("indents an open folder's playlists and toggles the folder on click", () => {
    const { tree, onToggle } = rows("Spotify", {
      expanded: ["spotify:user:me:folder:moods"],
    });
    expect(titles(tree)).toEqual([
      "Liked Songs",
      "Recently played",
      "Moods",
      "Road trip",
      "Their mix",
      "Jam",
      "Loose",
      "Library health",
    ]);
    expect(titled(tree, "Moods").props["aria-expanded"]).toBe(true);
    expect(titled(tree, "Road trip").props.style).toEqual({ paddingLeft: 24 });
    expect(titled(tree, "Jam").props.style).toBeUndefined();
    (titled(tree, "Moods").props.onClick as () => void)();
    expect(onToggle).toHaveBeenCalledWith("spotify:user:me:folder:moods");
  });

  it("opens Recently played on Spotify and marks it as the open page", () => {
    const { tree, open } = rows("Spotify", { view: { page: "recent" } });
    const recent = titled(tree, "Recently played");
    expect(recent.props["aria-current"]).toBe("page");
    (recent.props.onClick as () => void)();
    expect(open).toHaveBeenCalledWith({ page: "recent" });
    expect(titles(rows("Local").tree)).not.toContain("Recently played");
  });

  it("labels each Spotify playlist's menu by ownership", () => {
    const { tree, onMenu } = rows("Spotify", {
      playlists: { ...payload, folders: null },
    });
    for (const name of ["Road trip", "Their mix", "Jam"])
      expect(rightClick(tree, name)).toBe(true);
    expect(
      onMenu.mock.calls.map(([removal, at]) => [
        removal.label,
        removal.action,
        at,
      ]),
    ).toEqual([
      [
        "Delete",
        { UnfollowPlaylist: "spotify:playlist:mine" },
        { x: 10, y: 20 },
      ],
      [
        "Remove from Your Library",
        { UnfollowPlaylist: "spotify:playlist:theirs" },
        { x: 10, y: 20 },
      ],
      [
        "Remove from Your Library",
        { UnfollowPlaylist: "spotify:playlist:jam" },
        { x: 10, y: 20 },
      ],
    ]);
  });

  it("opens the menu from the ContextMenu key and Shift+F10", () => {
    const { tree, onMenu } = rows("YouTube");
    const onKeyDown = titled(tree, "Watch later").props.onKeyDown as (
      event: unknown,
    ) => void;
    const opener = { getBoundingClientRect: () => ({ left: 100, bottom: 50 }) };
    const press = (key: string, shiftKey = false) =>
      onKeyDown({ key, shiftKey, currentTarget: opener, preventDefault() {} });
    press("ContextMenu");
    press("F10", true);
    press("F10");
    expect(onMenu.mock.calls).toEqual([
      [
        expect.objectContaining({
          label: "Delete",
          action: { DeletePlaylist: "youtube:playlist:yt" },
        }),
        { x: 116, y: 50 },
        opener,
      ],
      [expect.objectContaining({ label: "Delete" }), { x: 116, y: 50 }, opener],
    ]);
  });

  it("gives a local playlist and the fixed rows no menu", () => {
    const { tree } = rows("Local");
    expect(rightClick(tree, "Rips")).toBe(false);
    expect(rightClick(tree, "Liked Songs")).toBe(false);
  });

  it("announces the menu and its keys on a row that has one", () => {
    const youtube = rows("YouTube").tree;
    const row = titled(youtube, "Watch later");
    expect(row.props["aria-haspopup"]).toBe("menu");
    expect(row.props["aria-keyshortcuts"]).toBe("Shift+F10 ContextMenu");
    expect(row.props.title).toBe(
      "Watch later\nRight-click or Shift+F10 for more options",
    );
    expect(renderToStaticMarkup(youtube)).toContain(
      'class="library-more" aria-hidden="true"',
    );
    const local = rows("Local").tree;
    const plain = titled(local, "Rips");
    expect(plain.props["aria-haspopup"]).toBeUndefined();
    expect(plain.props["aria-keyshortcuts"]).toBeUndefined();
    expect(plain.props.title).toBe("Rips");
    expect(renderToStaticMarkup(local)).not.toContain("library-more");
  });

  it("opens the menu from the more glyph below it, without opening the playlist", () => {
    const { tree, open, onMenu } = rows("YouTube");
    const more = hostElements(tree).find(
      (element) => element.props.className === "library-more",
    );
    const row = { tagName: "BUTTON" };
    const stopPropagation = vi.fn();
    (more?.props.onClick as (event: unknown) => void)({
      currentTarget: {
        closest: () => row,
        getBoundingClientRect: () => ({ left: 200, bottom: 60 }),
      },
      stopPropagation,
    });
    expect(stopPropagation).toHaveBeenCalled();
    expect(open).not.toHaveBeenCalled();
    expect(onMenu).toHaveBeenCalledWith(
      expect.objectContaining({ label: "Delete" }),
      { x: 200, y: 60 },
      row,
    );
  });
});

describe("ShowRows", () => {
  it("opens a saved show's page and marks the open one", () => {
    const open = vi.fn();
    const tree = (
      <ShowRows
        shows={payload.saved_shows}
        view={{ page: "show", id: "show1", name: "The Daily" }}
        open={open}
      />
    );
    const row = titled(tree, "The Daily");
    expect(row.props["aria-current"]).toBe("page");
    expect(renderToStaticMarkup(tree)).toContain("Podcast · NYT");
    (row.props.onClick as () => void)();
    expect(open).toHaveBeenCalledWith({
      page: "show",
      id: "show1",
      name: "The Daily",
    });
    expect(
      renderToStaticMarkup(
        <ShowRows shows={[]} view={{ page: "home" }} open={open} />,
      ),
    ).toContain("No saved podcasts.");
  });
});

describe("SortSelect", () => {
  it("lists the options and reports a pick", () => {
    const onChange = vi.fn();
    const tree = (
      <SortSelect
        label="Sort albums"
        options={ALBUM_SORTS}
        value="release_date:desc"
        onChange={onChange}
      />
    );
    const html = renderToStaticMarkup(tree);
    expect(html).toContain('aria-label="Sort albums"');
    expect(html).toMatch(
      /<option value="release_date:desc" selected="">Release date \(newest first\)/,
    );
    const select = hostElements(tree).find(
      (element) => element.type === "select",
    );
    (select?.props.onChange as (event: unknown) => void)({
      target: { value: "name" },
    });
    expect(onChange).toHaveBeenCalledWith("name");
  });
});
