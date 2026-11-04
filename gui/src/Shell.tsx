import { EN, type GuiStrings } from "./i18n";
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";
import type { PointerEvent as ReactPointerEvent, ReactNode } from "react";
import type { Action } from "./bindings/Action";
import type { AnnouncementEntry } from "./bindings/AnnouncementEntry";
import type { DeviceInfo } from "./bindings/DeviceInfo";
import type { FrameBar } from "./bindings/FrameBar";
import type { GuiLayout } from "./bindings/GuiLayout";
import type { Pane } from "./bindings/Pane";
import type { SessionPlay } from "./bindings/SessionPlay";
import type { UpdateAsk } from "./bindings/UpdateAsk";
import type { WindowEdge } from "./bindings/WindowEdge";
import type { WindowMode } from "./bindings/WindowMode";
import {
  MAC,
  modified,
  paneKey,
  shellKey,
  type PaneCommand,
  type ShellKey,
} from "./areas";
import {
  barPreview,
  DEFAULT_BARS,
  edgeAt,
  edgeOf,
  frameGrid,
  moveBar,
} from "./barsModel";
import { AlbumPage } from "./AlbumPage";
import { AnnouncementDialog } from "./AnnouncementDialog";
import { ArtistPage } from "./ArtistPage";
import "./Chrome.css";
import type { State } from "./connection";
import { CommandMode } from "./CommandMode";
import { pageStorage } from "./commandModel";
import { Discover } from "./Discover";
import { sourceOf } from "./format";
import { Home } from "./Home";
import { Glyph } from "./Icon";
import { LibraryHealth } from "./LibraryHealth";
import { LibraryPanel } from "./LibraryPanel";
import { upNext } from "./libraryModel";
import { ListPage } from "./ListPage";
import { LyricsPage } from "./LyricsPage";
import { miniCommand, MiniPlayer } from "./MiniPlayer";
import {
  fetchFor,
  goBack,
  goForward,
  HOME,
  openAction,
  push,
  type History,
  type Page,
  type View,
} from "./nav";
import { NowPlayingPanel } from "./NowPlayingPanel";
import { PaneArea, type PaneView } from "./PaneArea";
import {
  closeQueue,
  contains,
  DEFAULT_LAYOUT,
  firstPane,
  isLeaf,
  layoutRects,
  neighbor,
  open as openPane,
  placed,
  showMain,
  SIDE_PANES,
  type SidePane,
  toggle,
  without,
} from "./paneModel";
import { Party } from "./Party";
import { placeBox, trackDrag, type Point } from "./pointerDrag";
import { PodcastPage } from "./PodcastPage";
import { PlayerBar, type CoverSrc } from "./PlayerBar";
import type { WatchVisualizer } from "./PlayerBarVisualizer";
import { QueueDrawer } from "./QueueDrawer";
import { RadioPage } from "./RadioPage";
import { RecentlyPlayed } from "./RecentlyPlayed";
import { Room } from "./Room";
import { Search } from "./Search";
import { Session } from "./Session";
import { Settings } from "./Settings";
import { SoloistOfferDialog } from "./SoloistOfferDialog";
import { SoloistPairingDialog } from "./SoloistPairingDialog";
import { heldShowId } from "./showModel";
import { Stats } from "./Stats";
import { Toast } from "./Toast";
import { TopBar } from "./TopBar";
import { TrackActionsContext, type TrackActions } from "./TrackMenu";
import type { ScreenFeed } from "./screenModel";
import { Cover } from "./ui";
import { VISUALIZER_SETTING } from "./visualizerModel";
import { VisualizerScreen } from "./VisualizerScreen";

const NO_DEVICES: DeviceInfo[] = [];
/** A search that got no answer stops showing "Searching…" after this long. */
const PENDING_MS = 10_000;
const NO_PLAYS: SessionPlay[] = [];
const NO_ANNOUNCEMENTS: AnnouncementEntry[] = [];
/** The width from which panes sit beside the main view; Panes.css has it too. */
const WIDE = "(min-width: 1101px)";
let wideQuery: MediaQueryList | null = null;
const wideList = () => (wideQuery ??= window.matchMedia(WIDE));
const isWide = () => wideList().matches;
const watchWide = (notify: () => void) => {
  const query = wideList();
  query.addEventListener("change", notify);
  return () => query.removeEventListener("change", notify);
};
const ARROW_EDGES: Partial<Record<string, WindowEdge>> = {
  ArrowUp: "top",
  ArrowDown: "bottom",
  ArrowLeft: "left",
  ArrowRight: "right",
};

/** The frame: the top bar, the library panel, the main view, the panes, the player bar. */
export function Shell({
  s = EN,
  state,
  send,
  onUpdate,
  onWindow,
  onQuit,
  watchVisualizer,
  visualizerScreen,
  coverSrc,
  askChangelog,
  onLayout = () => {},
}: {
  s?: GuiStrings;
  state: State;
  send: (action: Action) => void;
  onUpdate: (ask: UpdateAsk) => void;
  /** Resizes the desktop shell's window; null without one. */
  onWindow: ((mode: WindowMode) => void) | null;
  onQuit: () => void;
  watchVisualizer?: WatchVisualizer;
  visualizerScreen?: ScreenFeed;
  coverSrc?: CoverSrc;
  /** Asks the server for the changelog the Home page shows. */
  askChangelog: () => void;
  /** Sends the pane layout to the server; null resets it. */
  onLayout?: (layout: GuiLayout | null) => void;
}) {
  const { channels } = state;
  const [history, setHistory] = useState<History>(HOME);
  const navigationFocus = useRef<Element | null>(null);
  const view = history.now;
  const page = view.page;
  // A page mounts on its first visit and then stays mounted, so its cursor survives a switch.
  // Each page kind keeps the last view it showed.
  const [views, setViews] = useState<Partial<Record<Page, View>>>({
    home: HOME.now,
  });
  const spotify = channels.liked?.payload.available ?? false;
  const likedLoaded = channels.liked?.payload.loaded ?? false;
  const tableUri = channels.track_table?.payload.uri ?? null;
  const albumId = channels.album?.payload.album?.id ?? null;
  const artistId = channels.artist?.payload.page?.id ?? null;
  const showId = heldShowId(channels.show?.payload.page);
  const radioId = channels.track_table?.payload.radio ?? null;
  const wide = useSyncExternalStore(watchWide, isWide);
  // The layout this page sent last wins until the server's next revision answers it.
  const layoutChannel = channels.pane_layout;
  const serverRev = layoutChannel?.rev ?? -1;
  const [sentLayout, setSentLayout] = useState<{
    layout: GuiLayout;
    rev: number;
  } | null>(null);
  const served = layoutChannel?.payload.layout ?? DEFAULT_LAYOUT;
  // An answer equal to the sent layout keeps the sent object, so nothing keyed on it reruns.
  const layout = useMemo(
    () =>
      sentLayout &&
      (serverRev <= sentLayout.rev ||
        JSON.stringify(served) === JSON.stringify(sentLayout.layout))
        ? sentLayout.layout
        : served,
    [sentLayout, serverRev, served],
  );
  const commitLayout = useCallback(
    (next: GuiLayout | null) => {
      setSentLayout({ layout: next ?? DEFAULT_LAYOUT, rev: serverRev });
      onLayout(next);
    },
    [serverRev, onLayout],
  );
  const show = useCallback(
    // `base` is a layout the caller changes in the same step; a hidden Main shows on each new page.
    (next: History, fresh: boolean, base?: GuiLayout) => {
      // A late target can take focus from the control that opened it.
      navigationFocus.current = document.activeElement;
      const action = fetchFor(next.now, fresh, {
        spotify,
        liked: likedLoaded,
        tableUri,
        albumId,
        artistId,
        showId,
        radioId,
      });
      if (action) send(action);
      setHistory(next);
      const after = base ?? layout;
      if (!contains(after.tree, "main"))
        commitLayout({ ...after, tree: showMain(after.tree) });
      else if (after !== layout) commitLayout(after);
      setViews((seen) =>
        seen[next.now.page] === next.now
          ? seen
          : { ...seen, [next.now.page]: next.now },
      );
    },
    [
      send,
      spotify,
      likedLoaded,
      tableUri,
      albumId,
      artistId,
      showId,
      radioId,
      layout,
      commitLayout,
    ],
  );
  const open = useCallback(
    (next: View) => show(push(history, next), true),
    [history, show],
  );
  const playback = channels.playback?.payload ?? null;
  const item = playback?.item ?? null;
  const playingUri = item?.uri ?? null;
  const queue = channels.queue?.payload;
  const spotifyPlays = sourceOf(playingUri) === "Spotify";
  const queued = useMemo(
    () => upNext(queue, spotifyPlays),
    [queue, spotifyPlays],
  );
  const playlists = channels.library?.payload ?? null;
  const likedMarks = channels.liked_marks?.payload;
  const [overlay, setOverlay] = useState<"command" | null>(null);
  const paneKeys = layoutChannel?.payload.keys ?? null;
  const [panesFit, setPanesFit] = useState(true);
  // A narrow window has no panes; the side panel opens over the main view.
  const [overlayPane, setOverlayPane] = useState<SidePane | null>(null);
  // A queue open at launch leaves the keyboard where it is.
  const [queueGrab, setQueueGrab] = useState(false);
  const [room, setRoom] = useState(false);
  const [mini, setMini] = useState(false);
  const showMini = useCallback((on: boolean) => {
    setMini(on);
    if (!on) return;
    setOverlay(null);
    setRoom(false);
  }, []);
  // The desktop shell follows the mode once the hello brings onWindow: a reload can find its window mini.
  const windowWas = useRef<boolean | null>(null);
  useEffect(() => {
    if (!onWindow || windowWas.current === mini) return;
    windowWas.current = mini;
    onWindow(mini ? "mini" : "full");
  }, [mini, onWindow]);
  // Leaving the mini player hands the keyboard back to the page; the mini player takes it itself.
  const miniWas = useRef(mini);
  useEffect(() => {
    const left = miniWas.current && !mini;
    miniWas.current = mini;
    if (!left) return;
    focusScreen(page);
    if (document.activeElement === document.body)
      document
        .querySelector<HTMLElement>(".playerbar [data-mini-toggle]")
        ?.focus();
  }, [mini, page]);
  // The Room fetches the playing track's album over the one the album page showed.
  const roomWas = useRef(room);
  useEffect(() => {
    const closed = roomWas.current && !room;
    if (!roomWas.current && room)
      document.querySelector<HTMLElement>(".room-layer [data-focus]")?.focus();
    roomWas.current = room;
    if (closed && view.page === "album" && albumId !== view.id) {
      const action = openAction(view);
      if (action) send(action);
    }
  }, [room, view, albumId, send]);
  const [roomLyrics, setRoomLyrics] = useState(false);
  const [query, setQuery] = useState("");
  // A search is answered by results for its own query, or by an error status; after
  // PENDING_MS the wait ends anyway, because an error can hide behind an earlier one.
  const [searchPending, setSearchPending] = useState<{
    query: string;
    search: number | null;
    status: number | null;
  } | null>(null);
  const searchRev = channels.search?.rev ?? null;
  const statusRev = channels.status?.rev ?? null;
  const statusError =
    (channels.status?.payload.is_error ?? false) ||
    channels.status?.payload.api_error != null;
  const searchEnded =
    searchPending !== null &&
    ((channels.search?.payload.query === searchPending.query &&
      searchRev !== searchPending.search) ||
      (statusError && statusRev !== searchPending.status));
  // An ended wait is cleared, so a later answer to another query cannot restart it.
  if (searchEnded) setSearchPending(null);
  const searchWaiting = searchPending !== null && !searchEnded;
  useEffect(() => {
    if (!searchPending) return;
    const timer = window.setTimeout(() => setSearchPending(null), PENDING_MS);
    return () => window.clearTimeout(timer);
  }, [searchPending]);
  const runSearch = useCallback(
    (text: string) => {
      const query = text.trim();
      if (!query) return;
      send({ SearchActiveSource: query });
      setSearchPending({ query, search: searchRev, status: statusRev });
      open({ page: "search" });
    },
    [send, searchRev, statusRev, open],
  );

  const panesShown = wide && panesFit && !room && !mini;
  const paneShown = (pane: Pane) => panesShown && placed(layout, pane);
  // A hidden Main hides the page too, so a Lyrics or Visualizer pane draws in its place.
  const mainHidden = panesShown && !contains(layout.tree, "main");
  const pageShown = (kind: Page) => page === kind && !mainHidden;
  const lyricsPaned = paneShown("lyrics");
  const lyricsPaneActive = lyricsPaned && !pageShown("lyrics");
  const visualizerPaneActive =
    paneShown("visualizer") && !pageShown("visualizer");
  // The Web API queue is fetched on request only: on open and on each Spotify track change.
  const showsQueue =
    paneShown("queue") ||
    paneShown("now_playing") ||
    (!wide && overlayPane !== null) ||
    room ||
    page === "session" ||
    page === "party";
  useEffect(() => {
    if (showsQueue && spotifyPlays) send("RefreshQueue");
  }, [showsQueue, spotifyPlays, playingUri, send]);

  const viewOf = <P extends Page>(kind: P) =>
    views[kind] as Extract<View, { page: P }> | undefined;
  const playlistView = viewOf("playlist");
  const albumView = viewOf("album");
  const artistView = viewOf("artist");
  const showView = viewOf("show");
  const radioView = viewOf("radio");
  // The Library defaults from Settings: the sort each list starts with.
  const settingRows = channels.settings?.payload.rows;
  const choice = (id: string) => {
    const value = settingRows?.find((row) => row.id === id)?.value;
    return value?.kind === "choice" ? value.value : null;
  };
  const trackSort = choice("behavior.default_sort_playlist_tracks");
  const pages: Record<Page, ReactNode> = {
    home: (
      <Home
        s={s}
        welcome={state.welcome}
        announcements={
          channels.announcement?.payload.history ?? NO_ANNOUNCEMENTS
        }
        changelog={state.changelog}
        connected={state.connected}
        active={page === "home"}
        askChangelog={askChangelog}
        send={send}
      />
    ),
    liked: (
      <ListPage
        s={s}
        view={{ page: "liked" }}
        liked={channels.liked?.payload ?? null}
        table={channels.track_table?.payload ?? null}
        playlists={playlists}
        source={channels.source?.payload ?? null}
        playingUri={playingUri}
        statusRev={statusRev}
        defaultSort={trackSort}
        statusError={statusError}
        active={page === "liked"}
        open={open}
        send={send}
      />
    ),
    playlist: playlistView && (
      <ListPage
        s={s}
        view={playlistView}
        liked={channels.liked?.payload ?? null}
        table={channels.track_table?.payload ?? null}
        playlists={playlists}
        source={channels.source?.payload ?? null}
        playingUri={playingUri}
        statusRev={statusRev}
        defaultSort={trackSort}
        statusError={statusError}
        active={page === "playlist"}
        open={open}
        send={send}
      />
    ),
    album: albumView && (
      <AlbumPage
        s={s}
        id={albumView.id}
        album={channels.album?.payload.album ?? null}
        saved={channels.album?.payload.saved ?? false}
        playingUri={playingUri}
        open={open}
        send={send}
      />
    ),
    artist: artistView && (
      <ArtistPage
        s={s}
        id={artistView.id}
        name={artistView.name}
        artist={channels.artist?.payload ?? null}
        playingUri={playingUri}
        open={open}
        send={send}
      />
    ),
    show: showView && (
      <PodcastPage
        s={s}
        view={showView}
        id={showView.id}
        name={showView.name}
        show={channels.show?.payload ?? null}
        statusRev={statusRev}
        statusError={statusError}
        playingUri={playingUri}
        active={page === "show"}
        open={open}
        send={send}
      />
    ),
    radio: radioView && (
      <RadioPage
        s={s}
        view={radioView}
        id={radioView.id}
        name={radioView.name}
        table={channels.track_table?.payload ?? null}
        available={spotify}
        statusRev={statusRev}
        statusError={statusError}
        playingUri={playingUri}
        active={page === "radio"}
        open={open}
        send={send}
      />
    ),
    recent: (
      <RecentlyPlayed
        s={s}
        recent={channels.recently_played?.payload ?? null}
        available={spotify}
        playingUri={playingUri}
        active={page === "recent"}
        open={open}
        send={send}
      />
    ),
    search: (
      <Search
        s={s}
        search={channels.search?.payload ?? null}
        waiting={searchWaiting}
        source={channels.source?.payload ?? null}
        playingUri={playingUri}
        query={query}
        run={runSearch}
        open={open}
        send={send}
      />
    ),
    lyrics: (
      <LyricsPage
        s={s}
        item={playback?.item ?? null}
        lyrics={channels.lyrics?.payload ?? null}
        position={state.position}
        active={pageShown("lyrics")}
        connected={state.connected}
        send={send}
      />
    ),
    session: (
      <Session
        s={s}
        plays={channels.session?.payload ?? NO_PLAYS}
        item={playback?.item ?? null}
        position={state.position}
        queue={queue ?? null}
        upNext={queued}
        active={page === "session"}
        send={send}
      />
    ),
    discover: (
      <Discover
        s={s}
        discover={channels.discover?.payload ?? null}
        connected={state.connected}
        active={page === "discover"}
        open={open}
        send={send}
      />
    ),
    stats: <Stats s={s} stats={channels.stats?.payload ?? null} send={send} />,
    party: (
      <Party
        s={s}
        party={channels.party?.payload ?? null}
        connected={state.connected}
        item={playback?.item ?? null}
        position={state.position}
        queue={queue ?? null}
        active={page === "party"}
        send={send}
      />
    ),
    visualizer: (
      <VisualizerScreen
        s={s}
        screens={state.localization?.strings.screens}
        item={playback?.item ?? null}
        position={state.position}
        active={pageShown("visualizer") && !room}
        // The player bar's setting row is offered only where the build can capture.
        available={choice(VISUALIZER_SETTING) !== null}
        feed={visualizerScreen}
      />
    ),
    health: (
      <LibraryHealth
        s={s}
        sync={channels.playlist_sync?.payload ?? null}
        send={send}
        onBack={() => show(goBack(history), false)}
      />
    ),
    settings: (
      <Settings
        s={s}
        rows={channels.settings?.payload.rows ?? null}
        update={channels.update?.payload ?? null}
        version={state.version}
        send={send}
        onUpdate={onUpdate}
      />
    ),
  };

  // The search field takes the keyboard even when Search is already the page.
  const openSearch = useCallback(() => {
    if (page === "search") focusScreen("search");
    else open({ page: "search" });
  }, [page, open]);

  // Leaving a pane hands the keyboard back to the page when the pane held it, or to the
  // first pane when Main hides.
  const changeLayout = useCallback(
    (next: GuiLayout, leaving?: Pane) => {
      const held =
        leaving !== undefined &&
        (document
          .querySelector(`[data-pane="${leaving}"]`)
          ?.contains(document.activeElement) ??
          false);
      commitLayout(next);
      if (held) focusPane(firstPane(next.tree), page);
    },
    [commitLayout, page],
  );
  // The player-bar buttons open and close a side pane, or the overlay on a narrow window.
  const togglePanel = useCallback(
    (pane: SidePane) => {
      if (!wide) {
        setOverlayPane((shown) => (shown === pane ? null : pane));
        return;
      }
      if (pane === "queue" && !placed(layout, pane)) setQueueGrab(true);
      changeLayout(toggle(layout, pane), pane);
    },
    [wide, layout, changeLayout],
  );
  // The queue's own close and Q bring Now Playing back to its slot.
  const leaveQueue = useCallback(() => {
    if (!wide) setOverlayPane("now_playing");
    else changeLayout(closeQueue(layout), "queue");
  }, [wide, layout, changeLayout]);
  const queueKey = useCallback(() => {
    if (!wide) {
      setOverlayPane((shown) => (shown === "queue" ? "now_playing" : "queue"));
      return;
    }
    if (placed(layout, "queue")) {
      leaveQueue();
      return;
    }
    setQueueGrab(true);
    changeLayout(openPane(layout, "queue"));
  }, [wide, layout, leaveQueue, changeLayout]);
  const closePane = useCallback(
    (pane: Pane) => {
      if (pane === "queue") leaveQueue();
      else changeLayout(without(layout, pane), pane);
    },
    [layout, leaveQueue, changeLayout],
  );
  const toggleMain = useCallback(() => {
    if (mainHidden) commitLayout({ ...layout, tree: showMain(layout.tree) });
    else closePane("main");
  }, [mainHidden, layout, commitLayout, closePane]);
  // A page with a pane of its own moves beside the page it came from.
  const detachPage = useCallback(
    (pane: "lyrics" | "visualizer") => {
      const next = openPane(layout, pane);
      if (page === pane) show(goBack(history), false, next);
      else commitLayout(next);
    },
    [commitLayout, layout, page, history, show],
  );
  const toggleLyrics = useCallback(() => {
    if (lyricsPaned) closePane("lyrics");
    else if (page === "lyrics") show(goBack(history), false);
    else open({ page: "lyrics" });
  }, [lyricsPaned, page, history, show, open, closePane]);

  const paneCommand = useCallback(
    (command: PaneCommand) => {
      const focused = (
        document.activeElement?.closest("[data-pane]") as HTMLElement | null
      )?.dataset.pane as Pane | undefined;
      const current = focused ?? firstPane(layout.tree);
      if (command === "reset") commitLayout(null);
      else if (command === "close") closePane(current);
      else if (command === "detach") {
        if (current === "lyrics" || current === "visualizer") {
          show(
            push(history, { page: current }),
            true,
            without(layout, current),
          );
        } else if (page === "lyrics" || page === "visualizer") detachPage(page);
      } else {
        const next = neighbor(
          layoutRects(layout.tree).rects,
          current,
          command.focus,
        );
        if (next) focusPane(next, page);
      }
    },
    [commitLayout, closePane, detachPage, history, layout, page, show],
  );

  // A layout effect: the keys work from the first painted frame.
  useLayoutEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const press = describe(event, overlay !== null);
      const pane = mini || room ? null : paneKey(press, paneKeys);
      if (pane) {
        if (panesShown) paneCommand(pane);
        event.preventDefault();
        return;
      }
      const pressed = shellKey(press);
      const command = mini ? miniCommand(pressed) : pressed;
      if (!command) return;
      if (command === "toggle") send("TogglePlayback");
      else if (command === "queue") queueKey();
      else if (command === "command") setOverlay("command");
      else if (command === "search") {
        // An open Room owns the keyboard; / stays with it.
        if (room) {
          event.preventDefault();
          return;
        }
        openSearch();
      } else if (command === "room") {
        setRoomLyrics(false);
        setRoom((open) => !open);
      } else if (command === "lyrics") {
        if (room) setRoomLyrics((shown) => !shown);
        else toggleLyrics();
      } else if (command === "mini") {
        showMini(!mini);
      } else if (command === "escape") {
        if (mini) setMini(false);
        else if (overlay) setOverlay(null);
        else if (!blurText() && room) setRoom(false);
      } else if (command === "settings") {
        setRoom(false);
        open({ page: "settings" });
      } else if (room) {
        // Area digits do not change the page under an open Room.
        event.preventDefault();
        return;
      } else if (command.area === "search") openSearch();
      else open({ page: command.area } as View);
      event.preventDefault();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [
    send,
    overlay,
    openSearch,
    room,
    mini,
    showMini,
    toggleLyrics,
    open,
    queueKey,
    paneKeys,
    panesShown,
    paneCommand,
  ]);

  // A switch or a closed overlay hands the keyboard to the page; the first render leaves focus alone.
  // The Soloist offer opens Settings under itself, so it holds the focus until it closes.
  const covered =
    overlay !== null || channels.status?.payload.soloist_offer != null;
  const shown = useRef({ page, covered });
  useEffect(() => {
    const before = shown.current;
    shown.current = { page, covered };
    if (covered || (before.page === page && !before.covered)) return;
    focusScreen(page);
  }, [page, covered]);

  // A closed Room also hands the keyboard back to the page.
  const roomShown = useRef(room);
  useEffect(() => {
    const was = roomShown.current;
    roomShown.current = room;
    if (was && !room && !overlay) focusScreen(page);
  }, [room, overlay, page]);

  // A page whose list lands after navigation still gets the keyboard once it mounts.
  useEffect(() => {
    if (overlay || room || page === "search") return;
    const screen = document.querySelector(`[data-area="${page}"]`);
    if (!screen || screen.querySelector("[data-focus]")) return;
    const before = navigationFocus.current;
    const seen = new MutationObserver(() => {
      if (!screen.querySelector("[data-focus]")) return;
      seen.disconnect();
      if (
        document.activeElement === document.body ||
        document.activeElement === before
      )
        focusIn(screen);
    });
    seen.observe(screen, { childList: true, subtree: true });
    return () => seen.disconnect();
    // Keyed to the full view, so another playlist/album re-arms the watch.
  }, [view, page, overlay, room]);

  const trackActions = useMemo<TrackActions>(() => {
    const editable = new Set(playlists?.editable ?? []);
    return {
      send,
      open,
      liked: new Set(likedMarks ?? []),
      playlists: (playlists?.spotify ?? []).filter((playlist) =>
        editable.has(playlist.uri),
      ),
    };
  }, [send, open, playlists, likedMarks]);

  const mounted = (Object.keys(views) as Page[]).filter(
    (kind) => pages[kind] != null,
  );

  // Below the wide width the bars go back to their default edges; the saved ones stay.
  const bars = wide ? layout.bars : DEFAULT_BARS;
  const grid = frameGrid(bars);
  useEffect(() => {
    document.documentElement.style.setProperty(
      "--inset-bottom",
      `${grid.insetBottom}px`,
    );
  }, [grid.insetBottom]);
  const barPreviewBox = useRef<HTMLDivElement>(null);
  const moveBarTo = (bar: FrameBar, edge: WindowEdge) => {
    const next = moveBar(layout.bars, bar, edge);
    if (next !== layout.bars) commitLayout({ ...layout, bars: next });
  };
  const dragBar =
    (bar: FrameBar) => (event: ReactPointerEvent<HTMLButtonElement>) => {
      if (event.button !== 0) return;
      const edgeUnder = (at: Point) =>
        edgeAt(at.x, at.y, window.innerWidth, window.innerHeight);
      let shown: WindowEdge | null = null;
      trackDrag(event, {
        paint: (at) => {
          const edge = edgeUnder(at);
          if (edge === shown) return;
          shown = edge;
          placeBox(
            barPreviewBox.current,
            barPreview(bar, edge, window.innerWidth, window.innerHeight),
          );
        },
        end: (drop) => {
          placeBox(barPreviewBox.current, null);
          if (drop) moveBarTo(bar, edgeUnder(drop));
        },
      });
    };
  const grip = (bar: FrameBar) =>
    wide ? (
      <button
        type="button"
        className="bar-grip"
        aria-label={s.move_bar}
        title={s.move_bar}
        aria-keyshortcuts="ArrowUp ArrowDown ArrowLeft ArrowRight"
        onPointerDown={dragBar(bar)}
        onKeyDown={(event) => {
          const edge = ARROW_EDGES[event.key];
          // A modified arrow can be a pane key, so it goes on to the page.
          if (!edge || modified(event)) return;
          event.preventDefault();
          event.stopPropagation();
          moveBarTo(bar, edge);
        }}
      />
    ) : null;

  const coverUri =
    item?.image_url ??
    (playingUri?.startsWith("file:") ? (coverSrc?.(playingUri) ?? null) : null);
  const nowPanel = (onClose: () => void) => (
    <NowPlayingPanel
      s={s}
      playback={playback}
      upNext={queued}
      open={open}
      send={send}
      onClose={onClose}
      onCover={
        wide && !placed(layout, "cover_art")
          ? () => commitLayout(openPane(layout, "cover_art"))
          : undefined
      }
    />
  );
  const queuePanel = (grab: boolean) => (
    <QueueDrawer
      s={s}
      queue={queue ?? null}
      item={item}
      send={send}
      grab={grab}
      onClose={leaveQueue}
    />
  );
  const paneViews: Partial<Record<Pane, PaneView>> = {
    now_playing: {
      title: s.now_playing_view,
      ownHead: true,
      node: nowPanel(() => closePane("now_playing")),
    },
    queue: { title: s.queue, ownHead: true, node: queuePanel(queueGrab) },
    lyrics: {
      title: s.lyrics,
      node: (
        <LyricsPage
          s={s}
          item={item}
          lyrics={channels.lyrics?.payload ?? null}
          position={state.position}
          active={lyricsPaneActive}
          connected={state.connected}
          send={send}
        />
      ),
    },
    visualizer: {
      title: s.visualizer,
      node: (
        <VisualizerScreen
          s={s}
          screens={state.localization?.strings.screens}
          item={item}
          position={state.position}
          active={visualizerPaneActive}
          available={choice(VISUALIZER_SETTING) !== null}
          feed={visualizerScreen}
        />
      ),
    },
    cover_art: {
      title: s.cover_art,
      node: (
        <div className="cover-pane">
          <Cover src={coverUri} />
        </div>
      ),
    },
  };
  const panels: SidePane[] = wide
    ? SIDE_PANES.filter((pane) => placed(layout, pane))
    : overlayPane
      ? [overlayPane]
      : [];
  const mainPane = (
    <>
      <LibraryPanel
        s={s}
        playlists={channels.library?.payload ?? null}
        artists={playlists?.followed_artists ?? null}
        albumSort={choice("behavior.default_sort_saved_albums")}
        artistSort={choice("behavior.default_sort_saved_artists")}
        source={channels.source?.payload ?? null}
        sync={channels.playlist_sync?.payload ?? null}
        loggedOut={channels.liked?.payload.available === false}
        errorRev={statusError ? statusRev : null}
        view={view}
        open={open}
        send={send}
      />
      <div className="main-view">
        <div className="screens">
          {mounted.map((kind) => (
            <div
              key={kind}
              className="screen"
              data-area={kind}
              hidden={kind !== page}
            >
              {pages[kind]}
              {wide && (kind === "lyrics" || kind === "visualizer") && (
                <button
                  type="button"
                  className="panel-close pane-detach"
                  aria-label={s.detach_pane}
                  title={s.detach_pane}
                  onClick={() => detachPage(kind)}
                >
                  <Glyph name="panel" />
                </button>
              )}
            </div>
          ))}
        </div>
      </div>
    </>
  );
  return (
    <TrackActionsContext.Provider value={trackActions}>
      {mini && (
        <MiniPlayer
          s={s}
          playback={playback}
          position={state.position}
          connected={state.connected}
          send={send}
          onExit={() => setMini(false)}
        />
      )}
      {/* Hidden, not unmounted: the pages keep their cursors for the way back. */}
      <main
        className="shell"
        hidden={mini}
        style={{
          gridTemplateAreas: grid.areas,
          gridTemplateRows: grid.rows,
          gridTemplateColumns: grid.columns,
          padding: grid.padding,
        }}
      >
        <TopBar
          s={s}
          page={page}
          canBack={history.back.length > 0}
          canForward={history.forward.length > 0}
          onBack={() => show(goBack(history), false)}
          onForward={() => show(goForward(history), false)}
          onPage={(next) => open({ page: next } as View)}
          query={query}
          onQuery={setQuery}
          onRun={runSearch}
          device={playback?.device ?? null}
          connected={state.connected}
          update={channels.update?.payload ?? null}
          onUpdate={onUpdate}
          edge={edgeOf(bars, "top")}
          grip={grip("top")}
          mainHidden={
            panesShown && (mainHidden || !isLeaf(layout.tree))
              ? mainHidden
              : undefined
          }
          onMain={toggleMain}
        />
        <PaneArea
          s={s}
          layout={layout}
          wide={wide}
          main={mainPane}
          views={paneViews}
          onLayout={commitLayout}
          onShown={setPanesFit}
          onClose={closePane}
        />
        {!wide && overlayPane && (
          <aside className="right-panel" aria-label={s.side_panel}>
            {overlayPane === "queue"
              ? queuePanel(true)
              : nowPanel(() => setOverlayPane(null))}
          </aside>
        )}
        <PlayerBar
          s={s}
          playback={playback}
          position={state.position}
          connected={state.connected}
          devices={channels.devices?.payload ?? NO_DEVICES}
          panels={panels}
          onPanel={togglePanel}
          lyricsOpen={page === "lyrics" || lyricsPaned}
          onLyrics={toggleLyrics}
          onRoom={() => setRoom(true)}
          onMini={() => showMini(true)}
          send={send}
          visualizer={choice(VISUALIZER_SETTING)}
          watchVisualizer={watchVisualizer}
          covered={room}
          look={channels.settings?.payload.visualizer ?? null}
          coverSrc={coverSrc}
          edge={edgeOf(bars, "player")}
          grip={grip("player")}
        />
        {room && (
          <div className="room-layer">
            <Room
              s={s}
              item={playback?.item ?? null}
              position={state.position}
              connected={state.connected}
              album={channels.album?.payload.album ?? null}
              lyrics={channels.lyrics?.payload ?? null}
              upNext={queued}
              queueAhead={queue?.now != null || (queue?.native.length ?? 0) > 0}
              showLyrics={roomLyrics}
              active={room}
              send={send}
              onClose={() => setRoom(false)}
            />
          </div>
        )}
        {overlay === "command" && (
          <CommandMode
            s={s}
            search={channels.search?.payload ?? null}
            searchRev={channels.search?.rev ?? null}
            source={channels.source?.payload ?? null}
            devices={channels.devices?.payload ?? NO_DEVICES}
            storage={pageStorage()}
            send={send}
            onSearch={(text) => {
              setQuery(text);
              runSearch(text);
            }}
            onQuit={onQuit}
            onClose={() => setOverlay(null)}
          />
        )}
      </main>
      <div ref={barPreviewBox} className="bar-preview" data-show="false" />
      {/* Outside the main view, so the mini player shows it too. */}
      <Toast
        s={s}
        status={channels.status?.payload ?? null}
        route={channels.route?.payload ?? null}
        send={send}
      />
      <AnnouncementDialog
        s={s}
        active={channels.announcement?.payload.active ?? null}
        send={send}
      />
      <SoloistPairingDialog
        guide={channels.status?.payload.soloist_pairing ?? null}
        send={send}
      />
      <SoloistOfferDialog
        offer={channels.status?.payload.soloist_offer ?? null}
        send={send}
        onSetUp={() => open({ page: "settings" })}
      />
    </TrackActionsContext.Provider>
  );
}

function describe(event: KeyboardEvent, overlay: boolean): ShellKey {
  const target = event.target instanceof HTMLElement ? event.target : null;
  return {
    key: event.key,
    modified: event.ctrlKey || event.metaKey || event.altKey,
    command:
      !event.altKey &&
      (MAC ? event.metaKey : event.ctrlKey) &&
      !(MAC ? event.ctrlKey : event.metaKey),
    composing: event.isComposing,
    handled: event.defaultPrevented,
    typing: target !== null && isText(target),
    interactive: target?.closest("button, a[href], [role=slider]") != null,
    overlay,
    code: event.code,
    alt: event.altKey,
    ctrl: event.ctrlKey || event.metaKey,
    shift: event.shiftKey,
  };
}

function isText(element: HTMLElement): boolean {
  if (element instanceof HTMLInputElement)
    return element.type !== "checkbox" && element.type !== "radio";
  return (
    element.isContentEditable ||
    ["TEXTAREA", "SELECT"].includes(element.tagName)
  );
}

/** Gives the keyboard to a pane's own focus target, or to the page for Main. */
function focusPane(pane: Pane, page: Page) {
  if (pane === "main") {
    focusScreen(page);
    return;
  }
  const slot = document.querySelector<HTMLElement>(`[data-pane="${pane}"]`);
  focusIn(slot, slot);
}

function focusScreen(page: Page) {
  if (page === "search") {
    document.querySelector<HTMLInputElement>("#top-search")?.focus();
    return;
  }
  focusIn(document.querySelector(`[data-area="${page}"]`));
}

/** Focuses the `[data-focus]` target in `container`, or `fallback`, and shows a list's selected row. */
function focusIn(
  container: Element | null,
  fallback: HTMLElement | null = null,
) {
  const target =
    container?.querySelector<HTMLElement>("[data-focus]") ?? fallback;
  // A tall list would scroll the page header away.
  target?.focus({ preventScroll: true });
  if (target?.getAttribute("role") === "listbox")
    target
      .querySelector('[aria-selected="true"]')
      ?.scrollIntoView({ block: "nearest" });
}

/** Leaves a text field; false when none had the keyboard. */
function blurText(): boolean {
  const active = document.activeElement;
  if (!(active instanceof HTMLElement) || !isText(active)) return false;
  active.blur();
  return true;
}
