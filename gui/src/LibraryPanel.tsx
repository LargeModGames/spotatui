import {
  memo,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type MouseEvent,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import type { Action } from "./bindings/Action";
import type { ArtistInfo } from "./bindings/ArtistInfo";
import type { ListTarget } from "./bindings/ListTarget";
import type { PlaylistSyncPayload } from "./bindings/PlaylistSyncPayload";
import type { ShowInfo } from "./bindings/ShowInfo";
import type { Source } from "./bindings/Source";
import type { SourcePayload } from "./bindings/SourcePayload";
import type { SourcePlaylists } from "./bindings/SourcePlaylists";
import { pageStorage } from "./commandModel";
import { ConfirmDialog } from "./ConfirmDialog";
import { unmatchedTotal } from "./healthModel";
import { Glyph } from "./Icon";
import {
  ALBUM_SORTS,
  ARTIST_SORTS,
  createPlaylistAction,
  createsPlaylists,
  focusAfterAsk,
  libraryTree,
  loadExpanded,
  nearEnd,
  openRow,
  playlistRemoval,
  playlistsFor,
  removePlaylist,
  saveExpanded,
  songCount,
  sortAlbums,
  sortArtists,
  sortChoice,
  wantsNextPage,
  type PlaylistRemoval,
  type SidebarRow,
} from "./libraryModel";
import "./LibraryPanel.css";
import type { OpenView, View } from "./nav";
import { PlaylistNameDialog } from "./PlaylistNameDialog";
import { MenuList } from "./TrackMenu";
import { Cover } from "./ui";

type Filter = "Playlists" | "Albums" | "Artists" | "Podcasts";

type Point = { x: number; y: number };

/** Opens a row's menu at a window point; `opener` takes the focus back afterwards. */
type OpenMenu = (
  removal: PlaylistRemoval,
  at: Point,
  opener: HTMLElement,
) => void;

/** The left "Your Library" panel; memoized, so the shell's playback ticks skip it. */
export const LibraryPanel = memo(function LibraryPanel({
  playlists,
  artists,
  source,
  sync,
  view,
  open,
  send,
  albumSort,
  artistSort,
  loggedOut = false,
}: {
  playlists: SourcePlaylists | null;
  /** The followed artists; null before the library channel arrived. */
  artists: ArtistInfo[] | null;
  source: SourcePayload | null;
  sync: PlaylistSyncPayload | null;
  /** The view the main panel shows, to mark the open row. */
  view: View;
  open: OpenView;
  send: (action: Action) => void;
  /** The saved albums' sort until the user picks one here. */
  albumSort?: string | null;
  /** The followed artists' sort until the user picks one here. */
  artistSort?: string | null;
  /** True when the server says there is no Spotify session. */
  loggedOut?: boolean;
}) {
  const active = source?.active ?? null;
  const spotify = active === "Spotify";
  const [chosen, setChosen] = useState<Filter>("Playlists");
  // Albums, Artists and Podcasts exist for Spotify only; another source shows its playlists.
  const filter = spotify ? chosen : "Playlists";
  const [albumPick, setAlbumPick] = useState<string | null>(null);
  const [artistPick, setArtistPick] = useState<string | null>(null);
  const albumOrder = sortChoice(ALBUM_SORTS, albumPick ?? albumSort);
  const artistOrder = sortChoice(ARTIST_SORTS, artistPick ?? artistSort);
  const [expanded, setExpanded] = useState(() => loadExpanded(pageStorage()));
  const [naming, setNaming] = useState<string | null>(null);
  const [menu, setMenu] = useState<{
    removal: PlaylistRemoval;
    at: Point;
    opener: HTMLElement;
  } | null>(null);
  const [asking, setAsking] = useState<{
    removal: PlaylistRemoval;
    opener: HTMLElement;
  } | null>(null);
  const createButton = useRef<HTMLButtonElement>(null);
  const rowsBox = useRef<HTMLDivElement>(null);
  const askedFor = useRef<unknown>(null);

  // Each pick refetches the first page, so new saves and follows show.
  const choose = (next: Filter) => {
    setChosen(next);
    askedFor.current = null;
    if (next !== "Playlists") send({ OpenLibrary: next });
  };
  const filters: Filter[] = spotify
    ? ["Playlists", "Albums", "Artists", "Podcasts"]
    : ["Playlists"];
  const compiled = source?.compiled ?? [];

  const pageTarget: ListTarget | null =
    filter === "Albums"
      ? "SavedAlbums"
      : filter === "Podcasts"
        ? "SavedShows"
        : null;
  const pageList =
    pageTarget === "SavedAlbums"
      ? playlists?.saved_albums
      : playlists?.saved_shows;
  const pageMore =
    (pageTarget === "SavedAlbums"
      ? playlists?.saved_albums_has_more
      : playlists?.saved_shows_has_more) ?? false;
  const nextPage = useCallback(() => {
    const box = rowsBox.current;
    if (!pageTarget || !box) return;
    if (!wantsNextPage(nearEnd(box), pageMore, pageList, askedFor.current))
      return;
    askedFor.current = pageList;
    send({ LoadMore: pageTarget });
  }, [pageTarget, pageMore, pageList, send]);
  // A landed page that still leaves the list short of the end asks for the next one.
  useEffect(() => nextPage(), [nextPage]);

  const savedAlbums = playlists?.saved_albums;
  const albums = useMemo(
    () =>
      filter === "Albums" && savedAlbums
        ? sortAlbums(savedAlbums, albumOrder)
        : [],
    [filter, savedAlbums, albumOrder],
  );
  const sortedArtists = useMemo(
    () =>
      filter === "Artists" && artists ? sortArtists(artists, artistOrder) : [],
    [filter, artists, artistOrder],
  );
  const links = sync?.links;
  const unmatched = useMemo(() => unmatchedTotal(links ?? []), [links]);

  const toggle = useCallback(
    (uri: string) => {
      const next = new Set(expanded);
      if (!next.delete(uri)) next.add(uri);
      setExpanded(next);
      saveExpanded(pageStorage(), next);
    },
    [expanded],
  );
  const openMenu: OpenMenu = useCallback(
    (removal, at, opener) => setMenu({ removal, at, opener }),
    [],
  );
  const closeAsk = (confirmed: boolean) => {
    if (asking)
      focusAfterAsk(confirmed, asking.opener, [
        createButton.current,
        rowsBox.current,
      ])?.focus();
    setAsking(null);
  };
  const closeNaming = () => {
    setNaming(null);
    createButton.current?.focus();
  };

  return (
    <nav className="library-panel" aria-label="Your Library">
      <header className="library-head">
        <h2>
          <Glyph name="library" size={22} />
          Your Library
        </h2>
        <div className="library-tools">
          {createsPlaylists(active) && (
            <button
              ref={createButton}
              type="button"
              className="library-create"
              aria-label="Create playlist"
              title="Create playlist"
              onClick={() => setNaming("")}
            >
              <Glyph name="plus" size={18} />
            </button>
          )}
          {active && compiled.length > 1 && (
            <select
              className="library-source"
              aria-label="Source"
              value={active}
              onChange={(event) =>
                send({ SelectSource: event.target.value as Source })
              }
            >
              {compiled.map((choice) => (
                <option key={choice} value={choice}>
                  {choice}
                </option>
              ))}
            </select>
          )}
        </div>
      </header>
      {spotify && loggedOut && <SpotifyLogin send={send} />}
      <div className="library-filters" role="group" aria-label="Show">
        {filters.map((name) => (
          <button
            key={name}
            type="button"
            aria-pressed={name === filter}
            onClick={() => choose(name)}
          >
            {name}
          </button>
        ))}
      </div>
      {filter === "Albums" && (
        <SortSelect
          label="Sort albums"
          options={ALBUM_SORTS}
          value={albumOrder}
          onChange={setAlbumPick}
        />
      )}
      {filter === "Artists" && (
        <SortSelect
          label="Sort artists"
          options={ARTIST_SORTS}
          value={artistOrder}
          onChange={setArtistPick}
        />
      )}
      <div
        className="library-rows"
        ref={rowsBox}
        tabIndex={-1}
        onScroll={nextPage}
      >
        {filter === "Playlists" && (
          <PlaylistRows
            playlists={playlists}
            source={active}
            unmatched={unmatched}
            view={view}
            open={open}
            send={send}
            expanded={expanded}
            onToggle={toggle}
            onMenu={openMenu}
          />
        )}
        {filter === "Albums" &&
          (albums.length ? (
            albums.map((album, index) => (
              <Row
                key={`${index}-${album.id ?? album.name}`}
                cover={<Cover src={album.image_url} size={48} />}
                title={album.name}
                subline={
                  album.artists.length
                    ? `Album · ${album.artists.map((a) => a.name).join(", ")}`
                    : "Album"
                }
                current={view.page === "album" && view.id === album.id}
                onClick={
                  album.id
                    ? () => open({ page: "album", id: album.id ?? "" })
                    : undefined
                }
              />
            ))
          ) : (
            <p className="library-empty">No saved albums.</p>
          ))}
        {filter === "Artists" &&
          (artists === null ? (
            <p className="library-empty">Loading artists…</p>
          ) : artists.length === 0 ? (
            <p className="library-empty">No followed artists.</p>
          ) : (
            sortedArtists.map((artist, index) => (
              <Row
                key={`${index}-${artist.id ?? artist.name}`}
                cover={
                  <Cover
                    src={artist.image_url}
                    size={48}
                    round
                    fallback="user"
                  />
                }
                title={artist.name}
                subline="Artist"
                current={view.page === "artist" && view.id === artist.id}
                onClick={
                  artist.id
                    ? () =>
                        open({
                          page: "artist",
                          id: artist.id ?? "",
                          name: artist.name,
                        })
                    : undefined
                }
              />
            ))
          ))}
        {filter === "Podcasts" && (
          <ShowRows
            shows={playlists?.saved_shows ?? []}
            view={view}
            open={open}
          />
        )}
      </div>
      {menu &&
        createPortal(
          <MenuList
            label={menu.removal.name}
            groups={[
              [
                {
                  label: menu.removal.label,
                  kind: "action",
                  action: menu.removal.action,
                },
              ],
            ]}
            at={menu.at}
            run={() => {
              setAsking({ removal: menu.removal, opener: menu.opener });
              setMenu(null);
            }}
            onClose={() => {
              if (menu.opener.isConnected) menu.opener.focus();
              setMenu(null);
            }}
            root
          />,
          document.body,
        )}
      {asking &&
        createPortal(
          <ConfirmDialog
            title={asking.removal.title}
            body={asking.removal.body}
            confirm={asking.removal.confirm}
            onConfirm={() => {
              removePlaylist(asking.removal, view, send, open);
              closeAsk(true);
            }}
            onCancel={() => closeAsk(false)}
          />,
          document.body,
        )}
      {naming !== null &&
        createPortal(
          <PlaylistNameDialog
            name={naming}
            onName={setNaming}
            onCreate={(name) => {
              const action = createPlaylistAction(active, name);
              if (action) send(action);
              closeNaming();
            }}
            onCancel={closeNaming}
          />,
          document.body,
        )}
    </nav>
  );
});

/** Starts the device login; the server answers with the pairing code in the status line. */
export function SpotifyLogin({ send }: { send: (action: Action) => void }) {
  return (
    <div className="library-login">
      <p>Spotify is not connected.</p>
      <button type="button" onClick={() => send({ SelectSource: "Spotify" })}>
        Log in to Spotify
      </button>
    </div>
  );
}

/** The sort picker above the Albums or Artists rows. */
export function SortSelect({
  label,
  options,
  value,
  onChange,
}: {
  label: string;
  options: { value: string; label: string }[];
  value: string;
  onChange: (value: string) => void;
}) {
  return (
    <div className="library-sort">
      <select
        aria-label={label}
        value={value}
        onChange={(event) => onChange(event.target.value)}
      >
        {options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
    </div>
  );
}

/** The saved podcast shows; a row opens its show page. */
export function ShowRows({
  shows,
  view,
  open,
}: {
  shows: ShowInfo[];
  view: View;
  open: OpenView;
}) {
  if (shows.length === 0)
    return <p className="library-empty">No saved podcasts.</p>;
  return shows.map((show, index) => (
    <Row
      key={`${index}-${show.id ?? show.name}`}
      cover={<Cover src={show.image_url} size={48} />}
      title={show.name}
      subline={show.publisher ? `Podcast · ${show.publisher}` : "Podcast"}
      current={view.page === "show" && view.id === show.id}
      onClick={
        show.id
          ? () => open({ page: "show", id: show.id ?? "", name: show.name })
          : undefined
      }
    />
  ));
}

/** Liked Songs, Recently played, the active source's playlists or stations, and Library health. */
export const PlaylistRows = memo(function PlaylistRows({
  playlists,
  source,
  unmatched,
  view,
  open,
  send,
  expanded,
  onToggle,
  onMenu,
}: {
  playlists: SourcePlaylists | null;
  source: Source | null;
  unmatched: number;
  view: View;
  open: OpenView;
  send: (action: Action) => void;
  /** The URIs of the open folders. */
  expanded: ReadonlySet<string>;
  onToggle: (uri: string) => void;
  onMenu: OpenMenu;
}) {
  const tree =
    playlists && source === "Spotify" && playlists.folders
      ? libraryTree(playlists.folders, playlists.spotify, expanded)
      : null;
  const rows = playlists && source ? playlistsFor(playlists, source) : [];
  const menus = source === "Spotify" || source === "YouTube";
  const pick = (row: SidebarRow) => {
    if (!source) return;
    if (source === "Radio") {
      const action = openRow(source, row);
      if (action) send(action);
      return;
    }
    open({ page: "playlist", uri: row.uri, name: row.name, source });
  };
  const playlistRow = (row: SidebarRow, key: string, depth = 0) => (
    <Row
      key={key}
      cover={<Cover src={row.image} size={48} />}
      title={row.name}
      subline={
        source === "Radio"
          ? "Radio station"
          : row.count === null
            ? "Playlist"
            : `Playlist · ${songCount(row.count)}`
      }
      current={view.page === "playlist" && view.uri === row.uri}
      depth={depth}
      onClick={() => pick(row)}
      onMenu={
        menus && playlists
          ? (at, opener) => {
              const removal = playlistRemoval(source, row, playlists);
              if (removal) onMenu(removal, at, opener);
            }
          : undefined
      }
    />
  );
  return (
    <>
      <Row
        cover={
          <span className="liked-tile" aria-hidden="true">
            <Glyph name="heart" size={20} filled />
          </span>
        }
        title="Liked Songs"
        subline="Playlist"
        current={view.page === "liked"}
        onClick={() => open({ page: "liked" })}
      />
      {source === "Spotify" && (
        <Row
          cover={
            <span className="recent-tile" aria-hidden="true">
              <Glyph name="clock" size={20} />
            </span>
          }
          title="Recently played"
          subline="Your listening history"
          current={view.page === "recent"}
          onClick={() => open({ page: "recent" })}
        />
      )}
      {tree
        ? tree.map((item, index) =>
            item.kind === "folder" ? (
              <FolderRow
                key={`${index}-${item.uri}`}
                name={item.name}
                depth={item.depth}
                expanded={item.expanded}
                onToggle={() => onToggle(item.uri)}
              />
            ) : (
              playlistRow(item.row, `${index}-${item.row.uri}`, item.depth)
            ),
          )
        : rows.map((row) => playlistRow(row, row.uri))}
      <Row
        cover={<Cover src={null} size={48} fallback="link" />}
        title="Library health"
        subline={
          unmatched > 0 ? (
            <span className="warn">{unmatched} unmatched</span>
          ) : (
            "Playlist sync"
          )
        }
        current={view.page === "health"}
        onClick={() => open({ page: "health" })}
      />
    </>
  );
});

const MENU_HINT = "Right-click or Shift+F10 for more options";

const indent = (depth: number) =>
  depth > 0 ? { paddingLeft: 8 + depth * 16 } : undefined;

/** A playlist folder: a disclosure button; the arrow keys open and close it too. */
function FolderRow({
  name,
  depth,
  expanded,
  onToggle,
}: {
  name: string;
  depth: number;
  expanded: boolean;
  onToggle: () => void;
}) {
  const onKeyDown = (event: KeyboardEvent<HTMLButtonElement>) => {
    if (event.key !== (expanded ? "ArrowLeft" : "ArrowRight")) return;
    event.preventDefault();
    onToggle();
  };
  return (
    <button
      type="button"
      className="library-row library-folder"
      title={name}
      aria-expanded={expanded}
      style={indent(depth)}
      onClick={onToggle}
      onKeyDown={onKeyDown}
    >
      <span className="folder-tile" aria-hidden="true">
        <Glyph name="library" size={20} />
      </span>
      <span className="library-row-text">
        <span className="library-row-title">{name}</span>
        <span className="library-row-sub">Folder</span>
      </span>
      <span className="folder-chevron" aria-hidden="true">
        <Glyph name="forward" size={16} />
      </span>
    </button>
  );
}

/** One compact row: the cover, the name and a gray subline. */
function Row({
  cover,
  title,
  subline,
  current,
  depth = 0,
  onClick,
  onMenu,
}: {
  cover: ReactNode;
  title: string;
  subline: ReactNode;
  current: boolean;
  depth?: number;
  /** Absent, the row cannot open anything. */
  onClick?: () => void;
  /** Absent, the row has no context menu. */
  onMenu?: (at: Point, opener: HTMLElement) => void;
}) {
  const onContextMenu = (event: MouseEvent<HTMLButtonElement>) => {
    if (!onMenu) return;
    event.preventDefault();
    onMenu({ x: event.clientX, y: event.clientY }, event.currentTarget);
  };
  const onKeyDown = (event: KeyboardEvent<HTMLButtonElement>) => {
    if (
      !onMenu ||
      !(event.key === "ContextMenu" || (event.shiftKey && event.key === "F10"))
    )
      return;
    event.preventDefault();
    const box = event.currentTarget.getBoundingClientRect();
    onMenu({ x: box.left + 16, y: box.bottom }, event.currentTarget);
  };
  const onMore = (event: MouseEvent<HTMLSpanElement>) => {
    const row = event.currentTarget.closest("button");
    if (!onMenu || !row) return;
    event.stopPropagation();
    const box = event.currentTarget.getBoundingClientRect();
    onMenu({ x: box.left, y: box.bottom }, row);
  };
  return (
    <button
      type="button"
      className="library-row"
      title={onMenu ? `${title}\n${MENU_HINT}` : title}
      aria-current={current ? "page" : undefined}
      aria-haspopup={onMenu ? "menu" : undefined}
      aria-keyshortcuts={onMenu ? "Shift+F10 ContextMenu" : undefined}
      style={indent(depth)}
      disabled={!onClick}
      onClick={onClick}
      onContextMenu={onMenu ? onContextMenu : undefined}
      onKeyDown={onMenu ? onKeyDown : undefined}
    >
      {cover}
      <span className="library-row-text">
        <span className="library-row-title">{title}</span>
        <span className="library-row-sub">{subline}</span>
      </span>
      {onMenu && (
        <span className="library-more" aria-hidden="true" onClick={onMore}>
          <Glyph name="more" size={16} />
        </span>
      )}
    </button>
  );
}
