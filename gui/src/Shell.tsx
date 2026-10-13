import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import type { ReactNode } from "react";
import type { Action } from "./bindings/Action";
import type { DeviceInfo } from "./bindings/DeviceInfo";
import type { SessionPlay } from "./bindings/SessionPlay";
import type { UpdateAsk } from "./bindings/UpdateAsk";
import type { WindowMode } from "./bindings/WindowMode";
import { MAC, shellKey, type ShellKey } from "./areas";
import { AlbumPage } from "./AlbumPage";
import { ArtistPage } from "./ArtistPage";
import "./Chrome.css";
import type { State } from "./connection";
import { CommandMode } from "./CommandMode";
import { pageStorage } from "./commandModel";
import { Discover } from "./Discover";
import { sourceOf } from "./format";
import { Home } from "./Home";
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
import { Party } from "./Party";
import { PodcastPage } from "./PodcastPage";
import { PlayerBar, type Panel } from "./PlayerBar";
import { QueueDrawer } from "./QueueDrawer";
import { RadioPage } from "./RadioPage";
import { RecentlyPlayed } from "./RecentlyPlayed";
import { Room } from "./Room";
import { Search } from "./Search";
import { Session } from "./Session";
import { Settings } from "./Settings";
import { Stats } from "./Stats";
import { Toast } from "./Toast";
import { TopBar } from "./TopBar";
import { TrackActionsContext, type TrackActions } from "./TrackMenu";

const NO_DEVICES: DeviceInfo[] = [];
/** A search that got no answer stops showing "Searching…" after this long. */
const PENDING_MS = 10_000;
const NO_PLAYS: SessionPlay[] = [];
/** The width from which the right panel sits beside the main view; style.css has it too. */
const WIDE = "(min-width: 1101px)";

/** The frame: the top bar, the library panel, the main view, the right panel, the player bar. */
export function Shell({
  state,
  send,
  onUpdate,
  onWindow,
  onQuit,
}: {
  state: State;
  send: (action: Action) => void;
  onUpdate: (ask: UpdateAsk) => void;
  /** Resizes the desktop shell's window; null without one. */
  onWindow: ((mode: WindowMode) => void) | null;
  onQuit: () => void;
}) {
  const { channels } = state;
  const [history, setHistory] = useState<History>(HOME);
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
  const showId = channels.show?.payload.page?.show.id ?? null;
  const radioId = channels.track_table?.payload.radio ?? null;
  const show = useCallback(
    (next: History, fresh: boolean) => {
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
      setViews((seen) =>
        seen[next.now.page] === next.now
          ? seen
          : { ...seen, [next.now.page]: next.now },
      );
    },
    [send, spotify, likedLoaded, tableUri, albumId, artistId, showId, radioId],
  );
  const open = useCallback(
    (next: View) => show(push(history, next), true),
    [history, show],
  );
  // Home asks for its data once a Spotify session is known.
  const homeFetched = useRef(false);
  useEffect(() => {
    if (!spotify || homeFetched.current) return;
    homeFetched.current = true;
    const action = openAction(HOME.now);
    if (action) send(action);
  }, [spotify, send]);

  const playback = channels.playback?.payload ?? null;
  const playingUri = playback?.item?.uri ?? null;
  const queue = channels.queue?.payload;
  const spotifyPlays = sourceOf(playingUri) === "Spotify";
  const queued = useMemo(
    () => upNext(queue, spotifyPlays),
    [queue, spotifyPlays],
  );
  const playlists = channels.library?.payload ?? null;
  const likedMarks = channels.liked_marks?.payload;
  const [overlay, setOverlay] = useState<"command" | null>(null);
  // A narrow window starts with the main view alone; the panel opens over it.
  const [panel, setPanel] = useState<Panel>(() =>
    window.matchMedia(WIDE).matches ? "now" : null,
  );
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
        .querySelector<HTMLElement>(
          '.playerbar [aria-label="Open mini player"]',
        )
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

  // The Web API queue is fetched on request only: on open and on each Spotify track change.
  const showsQueue =
    panel !== null || room || page === "session" || page === "party";
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
  const settingRows = channels.settings?.payload;
  const choice = (id: string) => {
    const value = settingRows?.find((row) => row.id === id)?.value;
    return value?.kind === "choice" ? value.value : null;
  };
  const trackSort = choice("behavior.default_sort_playlist_tracks");
  const pages: Record<Page, ReactNode> = {
    home: (
      <Home
        recent={channels.recently_played?.payload ?? null}
        playlists={channels.library?.payload ?? null}
        discover={channels.discover?.payload ?? null}
        active={page === "home"}
        playingUri={playingUri}
        open={open}
        send={send}
      />
    ),
    liked: (
      <ListPage
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
        id={showView.id}
        name={showView.name}
        show={channels.show?.payload ?? null}
        playingUri={playingUri}
        active={page === "show"}
        open={open}
        send={send}
      />
    ),
    radio: radioView && (
      <RadioPage
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
        item={playback?.item ?? null}
        lyrics={channels.lyrics?.payload ?? null}
        position={state.position}
        active={page === "lyrics"}
      />
    ),
    session: (
      <Session
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
        discover={channels.discover?.payload ?? null}
        active={page === "discover"}
        playingUri={playingUri}
        send={send}
      />
    ),
    stats: <Stats stats={channels.stats?.payload ?? null} send={send} />,
    party: (
      <Party
        party={channels.party?.payload ?? null}
        connected={state.connected}
        item={playback?.item ?? null}
        position={state.position}
        queue={queue ?? null}
        active={page === "party"}
        send={send}
      />
    ),
    health: (
      <LibraryHealth
        sync={channels.playlist_sync?.payload ?? null}
        send={send}
        onBack={() => show(goBack(history), false)}
      />
    ),
    settings: (
      <Settings
        rows={channels.settings?.payload ?? null}
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
  const toggleLyrics = useCallback(() => {
    if (page === "lyrics") show(goBack(history), false);
    else open({ page: "lyrics" });
  }, [page, history, show, open]);

  // A layout effect: the keys work from the first painted frame.
  useLayoutEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const pressed = shellKey(describe(event, overlay !== null));
      const command = mini ? miniCommand(pressed) : pressed;
      if (!command) return;
      if (command === "toggle") send("TogglePlayback");
      else if (command === "queue")
        setPanel((shown) => (shown === "queue" ? "now" : "queue"));
      else if (command === "command") setOverlay("command");
      else if (command === "search") openSearch();
      else if (command === "room") {
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
      } else if (command.area === "search") openSearch();
      else open({ page: command.area } as View);
      event.preventDefault();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [send, overlay, openSearch, room, mini, showMini, toggleLyrics, open]);

  // A switch or a closed overlay hands the keyboard to the page; the first render leaves focus alone.
  const shown = useRef({ page, overlay });
  useEffect(() => {
    const before = shown.current;
    shown.current = { page, overlay };
    if (overlay || (before.page === page && !before.overlay)) return;
    focusScreen(page);
  }, [page, overlay]);

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
  return (
    <TrackActionsContext.Provider value={trackActions}>
      {mini && (
        <MiniPlayer
          playback={playback}
          position={state.position}
          connected={state.connected}
          send={send}
          onExit={() => setMini(false)}
        />
      )}
      {/* Hidden, not unmounted: the pages keep their cursors for the way back. */}
      <main className={panel ? "shell" : "shell no-right"} hidden={mini}>
        <TopBar
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
        />
        <LibraryPanel
          playlists={channels.library?.payload ?? null}
          artists={playlists?.followed_artists ?? null}
          albumSort={choice("behavior.default_sort_saved_albums")}
          artistSort={choice("behavior.default_sort_saved_artists")}
          source={channels.source?.payload ?? null}
          sync={channels.playlist_sync?.payload ?? null}
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
              </div>
            ))}
          </div>
        </div>
        {panel && (
          <aside className="right-panel" aria-label="Side panel">
            {panel === "queue" ? (
              <QueueDrawer
                queue={queue ?? null}
                item={playback?.item ?? null}
                send={send}
                onClose={() => setPanel("now")}
              />
            ) : (
              <NowPlayingPanel
                playback={playback}
                upNext={queued}
                open={open}
                send={send}
                onClose={() => setPanel(null)}
              />
            )}
          </aside>
        )}
        <PlayerBar
          playback={playback}
          position={state.position}
          connected={state.connected}
          devices={channels.devices?.payload ?? NO_DEVICES}
          panel={panel}
          onPanel={setPanel}
          lyricsOpen={page === "lyrics"}
          onLyrics={toggleLyrics}
          onRoom={() => setRoom(true)}
          onMini={() => showMini(true)}
          send={send}
        />
        {room && (
          <div className="room-layer">
            <Room
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
      {/* Outside the main view, so the mini player shows it too. */}
      <Toast
        status={channels.status?.payload ?? null}
        route={channels.route?.payload ?? null}
        send={send}
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

function focusScreen(page: Page) {
  if (page === "search") {
    document.querySelector<HTMLInputElement>("#top-search")?.focus();
    return;
  }
  const screen = document.querySelector(`[data-area="${page}"]`);
  const target = screen?.querySelector<HTMLElement>("[data-focus]");
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
