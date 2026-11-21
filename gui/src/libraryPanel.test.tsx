import {
  isValidElement,
  type ComponentProps,
  type ReactElement,
  type ReactNode,
} from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Action } from "./bindings/Action";
import type { PlaylistInfo } from "./bindings/PlaylistInfo";
import type { Source } from "./bindings/Source";
import type { SourcePlaylists } from "./bindings/SourcePlaylists";
import {
  LibraryPanel,
  SourceSelect,
  PlaylistRemovalDialog,
  PlaylistRows,
  ShowRows,
  SortSelect,
  PlaylistSortSelect,
} from "./LibraryPanel";
import { EN, deviceKind } from "./i18n";
import {
  removalTarget,
  removalText,
  playlistRemoval,
  playlistMenuItems,
} from "./libraryModel";
import { ALBUM_SORTS } from "./libraryModel";
import type { View } from "./nav";
import { hostElements } from "./testTree";
import { MenuList } from "./TrackMenu";
import type { MenuEntry } from "./trackMenuModel";

// Keep the normal server-rendered tests intact; the push tests retain component
// state and run layout effects after committing their simulated DOM changes.
const hooks = vi.hoisted(() => ({
  active: false,
  cursor: 0,
  values: [] as unknown[],
  layout: [] as (() => void)[],
}));
vi.mock("react", async (original) => {
  const react = await original<typeof import("react")>();
  return {
    ...react,
    useState: (initial: unknown) => {
      if (!hooks.active) return react.useState(initial);
      const index = hooks.cursor++;
      if (!(index in hooks.values))
        hooks.values[index] =
          typeof initial === "function" ? initial() : initial;
      return [
        hooks.values[index],
        (next: unknown) => {
          hooks.values[index] =
            typeof next === "function" ? next(hooks.values[index]) : next;
        },
      ];
    },
    useRef: (initial: unknown) => {
      if (!hooks.active) return react.useRef(initial);
      const index = hooks.cursor++;
      if (!(index in hooks.values)) hooks.values[index] = { current: initial };
      return hooks.values[index];
    },
    useMemo: (factory: () => unknown, deps: readonly unknown[]) =>
      hooks.active ? factory() : react.useMemo(factory, deps),
    useCallback: <T extends (...args: never[]) => unknown>(
      callback: T,
      deps: readonly unknown[],
    ) => (hooks.active ? callback : react.useCallback(callback, deps)),
    useEffect: (effect: () => void, deps: readonly unknown[]) => {
      if (!hooks.active) react.useEffect(effect, deps);
    },
    useLayoutEffect: (effect: () => void, deps: readonly unknown[]) => {
      if (hooks.active) hooks.layout.push(effect);
      else react.useLayoutEffect(effect, deps);
    },
  };
});
vi.mock("react-dom", async (original) => {
  const dom = await original<typeof import("react-dom")>();
  return {
    ...dom,
    createPortal: (children: ReactNode, container: Element) =>
      hooks.active ? children : dom.createPortal(children, container),
  };
});

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
  playlist_sidebar_order: "spotify",
  pinned: [],
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
  apple_music: [],
  apple_music_uncounted: [],
  radio: [],
  saved_albums: [],
  saved_albums_has_more: false,
  followed_artists: [],
  followed_artists_has_more: false,
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
    needle?: string;
    s?: typeof EN;
    loggedOut?: boolean;
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
      loggedOut={options.loggedOut}
      unmatched={0}
      view={options.view ?? { page: "home" }}
      open={open}
      send={send}
      expanded={new Set(options.expanded ?? [])}
      onToggle={onToggle}
      onMenu={onMenu}
      needle={options.needle}
      s={options.s}
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
  it("offers the persisted playlist order only for the Spotify sidebar", () => {
    const html = panel("Spotify", {
      ...payload,
      playlist_sidebar_order: "recently_played",
    });
    expect(html).toContain('aria-label="Sort playlists"');
    expect(html).toContain(
      '<option value="recently_played" selected="">Recently played</option>',
    );
    for (const source of ["Local", "Radio", "YouTube"] as const)
      expect(panel(source)).not.toContain('aria-label="Sort playlists"');
  });

  it("sends the three playlist orders through the shared sort action", () => {
    const send = vi.fn<(action: Action) => void>();
    const tree = <PlaylistSortSelect order="name" send={send} />;
    const select = hostElements(tree).find(
      (element) => element.type === "select",
    );
    expect(select?.props.value).toBe("name");
    const change = select?.props.onChange as (event: unknown) => void;
    for (const [value, field] of [
      ["spotify", "Default"],
      ["recently_played", "RecentlyPlayed"],
      ["name", "Name"],
    ]) {
      change({ target: { value } });
      expect(send).toHaveBeenLastCalledWith({
        Sort: { context: "SidebarPlaylists", field },
      });
    }
  });

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

  it("labels the library search separately from the list page filter", () => {
    expect(panel("Local")).toContain('aria-label="Filter Your Library"');
  });
});

describe("LibraryPanel focus after a library push", () => {
  afterEach(() => {
    hooks.active = false;
    vi.unstubAllGlobals();
  });

  type FocusRow = {
    key: string;
    props: Record<string, unknown>;
    dataset: { playlistUri?: string };
    isConnected: boolean;
    focus: ReturnType<typeof vi.fn<() => void>>;
    getBoundingClientRect: () => { left: number; bottom: number };
  };
  const elements = (
    node: ReactNode,
  ): ReactElement<Record<string, unknown>>[] => {
    if (Array.isArray(node)) return node.flatMap(elements);
    if (!isValidElement<Record<string, unknown>>(node)) return [];
    return [node, ...elements(node.props.children as ReactNode)];
  };

  const mountPinned = (uri: string, expanded = false) => {
    hooks.active = true;
    hooks.values = [];
    const document = { body: {}, activeElement: null as object | null };
    vi.stubGlobal("document", document);
    const send = vi.fn<(action: Action) => void>();
    const component = LibraryPanel as unknown as {
      type: (props: ComponentProps<typeof LibraryPanel>) => ReactNode;
    };
    let current: SourcePlaylists = {
      ...payload,
      pinned: [uri],
      folders: [
        { kind: "playlist", uri },
        ...payload.folders!.filter(
          (node) => node.kind !== "playlist" || node.uri !== uri,
        ),
      ],
    };
    let visible: FocusRow[] = [];
    const box = { querySelectorAll: () => visible, focus: vi.fn() };
    const commit = (playlists: SourcePlaylists) => {
      current = playlists;
      hooks.cursor = 0;
      hooks.layout = [];
      const tree = component.type({
        playlists,
        artists: [],
        source: { active: "Spotify", compiled: ["Spotify"] },
        sync: null,
        view: { page: "home" },
        open: vi.fn(),
        send,
      });
      const list = elements(tree).find((node) => node.type === PlaylistRows)!;
      const previous = visible;
      visible = hostElements(list)
        .filter((node) => node.type === "button")
        .map((node) => {
          const uri = node.props["data-playlist-uri"] as string | undefined;
          const depth = (
            node.props.style as { paddingLeft?: number } | undefined
          )?.paddingLeft;
          const key = JSON.stringify([uri ?? rowName(node), depth]);
          const row: FocusRow = previous.find((row) => row.key === key) ?? {
            key,
            props: {},
            dataset: { playlistUri: uri },
            isConnected: true,
            focus: vi.fn(() => {
              document.activeElement = row;
            }),
            getBoundingClientRect: () => ({ left: 0, bottom: 10 }),
          };
          row.props = node.props;
          return row;
        });
      for (const row of previous) row.isConnected = visible.includes(row);
      // Browsers can blur a moved DOM node, as well as an unmounted shortcut.
      const focused = previous.find((row) => row === document.activeElement);
      if (focused && previous.indexOf(focused) !== visible.indexOf(focused))
        document.activeElement = document.body;
      const container = elements(tree).find(
        (node) => node.props.className === "library-rows",
      )!;
      (container.props.ref as { current: unknown }).current = box;
      for (const effect of hooks.layout) effect();
      return tree;
    };
    const tree = commit(current);
    if (expanded) {
      const list = elements(tree).find((node) => node.type === PlaylistRows)!;
      (list.props.onToggle as (uri: string) => void)(
        "spotify:user:me:folder:moods",
      );
      commit(current);
    }
    const copies = () =>
      visible.filter((row) => row.dataset.playlistUri === uri);
    const unpin = () => {
      const opener = copies()[0];
      opener.focus();
      (opener.props.onKeyDown as (event: unknown) => void)({
        key: "F10",
        shiftKey: true,
        preventDefault: vi.fn(),
        currentTarget: opener,
      });
      const menu = elements(commit(current)).find(
        (node) => node.type === MenuList,
      )!;
      const entry = (menu.props.groups as MenuEntry[][])[0][0];
      expect(entry).toMatchObject({
        id: "pin-playlist",
        action: { SetPlaylistPinned: { uri, pinned: false } },
      });
      (menu.props.run as (entry: MenuEntry) => void)(entry);
      commit(current);
      expect(send).toHaveBeenCalledWith({
        SetPlaylistPinned: { uri, pinned: false },
      });
      expect(document.activeElement).toBe(opener);
      return opener;
    };
    return {
      document,
      copies,
      unpin,
      push: () => commit({ ...payload, pinned: [] }),
      unrelatedPush: () => commit({ ...current }),
      named: (name: string) =>
        visible.find((row) => String(row.props.title).split("\n")[0] === name),
    };
  };

  it("refocuses the same root playlist after the pushed library moves its keyed row", () => {
    const page = mountPinned("spotify:playlist:jam");
    const opener = page.unpin();
    opener.focus.mockClear();
    page.push();
    expect(page.copies()).toEqual([opener]);
    expect(opener.focus).toHaveBeenCalledOnce();
    expect(page.document.activeElement).toBe(opener);
  });

  it("focuses the visible folder copy after the pushed library removes the root shortcut", () => {
    const page = mountPinned("spotify:playlist:mine", true);
    const copy = page.copies()[1];
    const opener = page.unpin();
    page.push();
    expect(opener.isConnected).toBe(false);
    expect(page.copies()).toEqual([copy]);
    expect(copy.focus).toHaveBeenCalledOnce();
    expect(page.document.activeElement).toBe(copy);
  });

  it("focuses the adjacent visible row when the pushed library removes a shortcut into a closed folder", () => {
    const page = mountPinned("spotify:playlist:mine");
    const folder = page.named("Moods");
    const opener = page.unpin();
    page.push();
    expect(opener.isConnected).toBe(false);
    expect(page.copies()).toEqual([]);
    expect(folder?.focus).toHaveBeenCalledOnce();
    expect(page.document.activeElement).toBe(folder);
  });

  it("waits for the pin change instead of consuming an unrelated library push", () => {
    const page = mountPinned("spotify:playlist:mine");
    page.unpin();
    page.unrelatedPush();
    page.push();
    expect(page.document.activeElement).toBe(page.named("Moods"));
  });

  it("does not take focus from another control when the pin push arrives", () => {
    const page = mountPinned("spotify:playlist:mine");
    page.unpin();
    const search = {};
    page.document.activeElement = search;
    page.push();
    expect(page.document.activeElement).toBe(search);
    expect(page.named("Moods")?.focus).not.toHaveBeenCalled();
  });
});

describe("PlaylistRows", () => {
  it("marks a pin at the root and inside a folder with the translated accessible label", () => {
    const { tree } = rows("Spotify", {
      playlists: {
        ...payload,
        pinned: ["spotify:playlist:mine"],
        folders: [
          { kind: "playlist", uri: "spotify:playlist:mine" },
          ...payload.folders!,
        ],
      },
      expanded: ["spotify:user:me:folder:moods"],
      s: { ...EN, pinned: "已置顶" },
    });
    const markers = hostElements(tree).filter(
      (element) => element.props["aria-label"] === "已置顶",
    );
    expect(markers).toHaveLength(2);
    expect(markers.every((marker) => marker.props.role === "img")).toBe(true);
    expect(renderToStaticMarkup(tree)).toContain('width="12" height="12"');
  });

  it("gives a root pin and its folder copy distinct stable row keys", () => {
    const { tree } = rows("Spotify", {
      playlists: {
        ...payload,
        pinned: ["spotify:playlist:mine"],
        folders: [
          { kind: "playlist", uri: "spotify:playlist:mine" },
          ...payload.folders!,
        ],
      },
      expanded: ["spotify:user:me:folder:moods"],
    });
    // Inspect the rows before hostElements consumes their component keys.
    const component = tree.type as {
      type: (props: typeof tree.props) => ReactNode;
    };
    const keys: string[] = [];
    const collect = (node: ReactNode) => {
      if (Array.isArray(node)) return node.forEach(collect);
      if (!isValidElement<{ children?: ReactNode }>(node)) return;
      if (node.key !== null) keys.push(String(node.key));
      collect(node.props.children);
    };
    collect(component.type(tree.props));
    expect(new Set(keys).size).toBe(keys.length);
    expect(keys).toContain('["spotify:playlist:mine"]:0');
    expect(keys).toContain(
      '["spotify:user:me:folder:moods","spotify:playlist:mine"]:0',
    );
  });

  it("offers only a local pin for a cached Spotify row without a session or removal", () => {
    const { tree, onMenu } = rows("Spotify", { loggedOut: true });
    expect(rightClick(tree, "Jam")).toBe(true);
    const [target] = onMenu.mock.calls[0];
    expect(target).toMatchObject({
      uri: "spotify:playlist:jam",
      pinned: false,
      pinsEditable: true,
      removal: null,
    });
    expect(playlistMenuItems(target, EN)).toEqual([
      {
        id: "pin-playlist",
        label: "Pin to top",
        kind: "action",
        action: {
          SetPlaylistPinned: { uri: "spotify:playlist:jam", pinned: true },
        },
      },
    ]);
  });

  it("offers no pin menu under another source, including YouTube's deletion menu", () => {
    const { tree, onMenu } = rows("YouTube");
    rightClick(tree, "Watch later");
    expect(onMenu.mock.calls[0][0].pinsEditable).toBe(false);
    expect(
      playlistMenuItems(onMenu.mock.calls[0][0], EN).map((entry) => entry.id),
    ).toEqual(["remove-library"]);
    for (const source of [
      "Local",
      "Subsonic",
      "Qobuz",
      "Radio",
      "AppleMusic",
    ] as const) {
      const other = rows(source, {
        playlists: { ...payload, pinned: ["file:///rips"] },
      });
      expect(renderToStaticMarkup(other.tree)).not.toContain(
        'aria-label="Pinned"',
      );
      expect(other.onMenu).not.toHaveBeenCalled();
    }
  });

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
      onMenu.mock.calls.map(([target, at]) => [
        removalText(target.removal, EN).label,
        target.removal.action,
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
          removal: expect.objectContaining({
            kind: "delete",
            action: { DeletePlaylist: "youtube:playlist:yt" },
          }),
        }),
        { x: 116, y: 50 },
        opener,
      ],
      [
        expect.objectContaining({
          removal: expect.objectContaining({ kind: "delete" }),
        }),
        { x: 116, y: 50 },
        opener,
      ],
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
      expect.objectContaining({
        removal: expect.objectContaining({ kind: "delete" }),
      }),
      { x: 200, y: 60 },
      row,
    );
  });

  it("finds playlists inside closed folders without indentation", () => {
    const { tree, onToggle } = rows("Spotify", {
      needle: "trip ROAD",
    });

    expect(titles(tree)).toEqual(["Road trip"]);
    expect(titled(tree, "Road trip").props.style).toBeUndefined();
    expect(onToggle).not.toHaveBeenCalled();
  });

  it("filters fixed rows and playlists from other sources", () => {
    expect(titles(rows("Spotify", { needle: "liked" }).tree)).toEqual([
      "Liked Songs",
    ]);
    expect(titles(rows("Spotify", { needle: "recently" }).tree)).toEqual([
      "Recently played",
    ]);
    expect(titles(rows("Local", { needle: "health" }).tree)).toEqual([
      "Library health",
    ]);
    expect(titles(rows("Local", { needle: "rip" }).tree)).toEqual(["Rips"]);
    expect(titles(rows("YouTube", { needle: "watch" }).tree)).toEqual([
      "Watch later",
    ]);
    expect(titles(rows("Local", { needle: "recently" }).tree)).toEqual([]);
  });

  it("announces no matches when only a folder name matches", () => {
    const html = renderToStaticMarkup(
      rows("Spotify", { needle: "moods" }).tree,
    );

    expect(html).toContain('role="status"');
    expect(html).toContain("No matches for “moods”.");
    expect(html).not.toContain('title="Moods"');
  });

  it("keeps the existing folder view for blank queries", () => {
    for (const expanded of [[], ["spotify:user:me:folder:moods"]]) {
      const original = renderToStaticMarkup(rows("Spotify", { expanded }).tree);
      const blank = renderToStaticMarkup(
        rows("Spotify", { expanded, needle: "   " }).tree,
      );

      expect(blank).toBe(original);
    }
  });

  it("matches the displayed translated titles of fixed rows", () => {
    const s = { ...EN, liked_songs: "Lieblingssongs" };

    expect(titles(rows("Local", { needle: "lieblings", s }).tree)).toEqual([
      "Lieblingssongs",
    ]);
    expect(titles(rows("Local", { needle: "liked", s }).tree)).toEqual([]);
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

describe("localized library identities", () => {
  it("sends raw sources when the visible choices use Chinese labels", () => {
    const send = vi.fn();
    const s = { ...EN, source_local: "本地文件", source_radio: "网络电台" };
    const tree = (
      <SourceSelect
        s={s}
        active="Local"
        compiled={["Spotify", "Local", "Radio"]}
        send={send}
      />
    );
    const elements = hostElements(tree);
    const options = elements.filter((element) => element.type === "option");
    expect(options.map((element) => element.props.value)).toEqual([
      "Spotify",
      "Local",
      "Radio",
    ]);
    expect(options[1].props.children).toBe("本地文件");
    const select = elements.find((element) => element.type === "select")!;
    expect(select.props.value).toBe("Local");
    (select.props.onChange as (event: unknown) => void)({
      target: { value: options[2].props.value },
    });
    expect(send).toHaveBeenCalledWith({ SelectSource: "Radio" });
  });

  it("renders an open removal question with the current table and the original identity", () => {
    const removal = playlistRemoval(
      "Spotify",
      { uri: "spotify:playlist:mine", name: "Literal {name}" },
      payload,
    )!;
    const target = removalTarget(removal);
    expect(Object.keys(target).sort()).toEqual([
      "action",
      "kind",
      "name",
      "uri",
    ]);
    const s = {
      ...EN,
      delete_library_question: "删除播放列表？",
      delete_library_body: "删除 {name}？",
      delete: "删除",
    };
    const question = (strings: typeof EN) =>
      renderToStaticMarkup(
        <PlaylistRemovalDialog
          s={strings}
          target={target}
          onConfirm={() => {}}
          onCancel={() => {}}
        />,
      );
    expect(question(EN)).toContain(EN.delete_library_question);
    expect(question(s)).toContain("删除播放列表？");
    expect(question(s)).toContain("删除 Literal {name}？");
    expect(question(s)).not.toContain(EN.delete_library_question);
    expect(target.action).toEqual({
      UnfollowPlaylist: "spotify:playlist:mine",
    });
  });

  it("translates the lowercase device kinds sent by the protocol", () => {
    const s = {
      ...EN,
      device_cast_audio: "Cast 音频",
      device_cast_video: "Cast 视频",
    };
    expect(deviceKind("castaudio", s)).toBe("Cast 音频");
    expect(deviceKind("castvideo", s)).toBe("Cast 视频");
    expect(deviceKind("speaker", s)).toBe("speaker");
    expect(deviceKind("castaudio", EN)).toBe("castaudio");
    expect(deviceKind("castvideo", EN)).toBe("castvideo");
  });
});

it("displays every known lowercase protocol device kind and leaves unknown kinds literal", () => {
  const table = {
    ...EN,
    device_computer: "电脑",
    device_tablet: "平板",
    device_smartphone: "手机",
    device_smartwatch: "手表",
    device_speaker: "扬声器",
    device_tv: "电视",
    device_avr: "接收器",
    device_stb: "机顶盒",
    device_audio_dongle: "音频转接器",
    device_game_console: "游戏主机",
    device_automobile: "车载设备",
    device_unknown: "未知",
  };
  for (const [kind, label] of [
    ["computer", table.device_computer],
    ["tablet", table.device_tablet],
    ["smartphone", table.device_smartphone],
    ["smartwatch", table.device_smartwatch],
    ["speaker", table.device_speaker],
    ["tv", table.device_tv],
    ["avr", table.device_avr],
    ["stb", table.device_stb],
    ["audiodongle", table.device_audio_dongle],
    ["gameconsole", table.device_game_console],
    ["automobile", table.device_automobile],
    ["unknown", table.device_unknown],
  ]) {
    expect(deviceKind(kind, table)).toBe(label);
    expect(deviceKind(kind, EN)).toBe(kind);
  }
  expect(deviceKind("provider {device}", table)).toBe("provider {device}");
});
