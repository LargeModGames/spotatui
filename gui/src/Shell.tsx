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
import { shellKey, type Area, type ShellKey } from "./areas";
import type { State } from "./connection";
import { CommandMode } from "./CommandMode";
import { pageStorage } from "./commandModel";
import { Discover } from "./Discover";
import { Library } from "./Library";
import { upNext } from "./libraryModel";
import { sourceOf } from "./format";
import { Party } from "./Party";
import { PlayerBar } from "./PlayerBar";
import { QueueDrawer } from "./QueueDrawer";
import { Room } from "./Room";
import { Search } from "./Search";
import { Session } from "./Session";
import { Stats } from "./Stats";
import { Toast } from "./Toast";
import { TopBar } from "./TopBar";

const NO_DEVICES: DeviceInfo[] = [];
/** A search that got no answer stops showing "Searching…" after this long. */
const PENDING_MS = 10_000;
const NO_PLAYS: SessionPlay[] = [];

/** The frame: the top bar, the screen of the active area, the player bar and the toast. */
export function Shell({
  state,
  send,
}: {
  state: State;
  send: (action: Action) => void;
}) {
  const [area, setArea] = useState<Area>("library");
  // A screen mounts on its first visit and then stays mounted, so its cursor survives a switch.
  const [visited, setVisited] = useState<ReadonlySet<Area>>(
    () => new Set<Area>(["library"]),
  );
  const go = useCallback((next: Area) => {
    setArea(next);
    setVisited((seen) => (seen.has(next) ? seen : new Set(seen).add(next)));
  }, []);

  const { channels } = state;
  const playback = channels.playback?.payload ?? null;
  const playingUri = playback?.item?.uri ?? null;
  const queue = channels.queue?.payload;
  const spotifyPlays = sourceOf(playingUri) === "Spotify";
  const queued = useMemo(
    () => upNext(queue, spotifyPlays),
    [queue, spotifyPlays],
  );
  const [overlay, setOverlay] = useState<"queue" | "command" | null>(null);
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
    },
    [send, searchRev, statusRev],
  );
  // Where the Room returns to, and whether it shows the lyrics instead of the sides.
  const [back, setBack] = useState<Area>("library");
  const [showLyrics, setShowLyrics] = useState(false);

  // Stats reload on every visit, as the terminal's Stats row does.
  useEffect(() => {
    if (area === "stats") send({ OpenLibrary: "Stats" });
  }, [area, send]);

  // The Web API queue is fetched on request only: on open and on each Spotify track change.
  const showsQueue =
    overlay === "queue" ||
    area === "room" ||
    area === "session" ||
    area === "party";
  useEffect(() => {
    if (showsQueue && spotifyPlays) send("RefreshQueue");
  }, [showsQueue, spotifyPlays, playingUri, send]);

  const screens: Partial<Record<Area, ReactNode>> = {
    library: (
      <Library
        playlists={channels.library?.payload ?? null}
        liked={channels.liked?.payload ?? null}
        source={channels.source?.payload ?? null}
        playingUri={playingUri}
        upNext={queued}
        sync={channels.playlist_sync?.payload ?? null}
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
        onQuery={setQuery}
        onRun={runSearch}
        send={send}
      />
    ),
    session: (
      <Session
        plays={channels.session?.payload ?? NO_PLAYS}
        item={playback?.item ?? null}
        position={state.position}
        queue={queue ?? null}
        upNext={queued}
        active={area === "session"}
        send={send}
      />
    ),
    discover: (
      <Discover
        discover={channels.discover?.payload ?? null}
        active={area === "discover"}
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
        active={area === "party"}
        send={send}
      />
    ),
    room: (
      <Room
        item={playback?.item ?? null}
        position={state.position}
        connected={state.connected}
        album={channels.album?.payload.album ?? null}
        lyrics={channels.lyrics?.payload ?? null}
        upNext={queued}
        queueAhead={queue?.now != null || (queue?.native.length ?? 0) > 0}
        showLyrics={showLyrics}
        active={area === "room"}
        send={send}
      />
    ),
  };
  const ready = (target: Area) => target in screens;
  const readyKeys = Object.keys(screens).join(" ");
  // The search field takes the keyboard even when Search is already the area.
  const openSearch = useCallback(() => {
    if (area === "search") focusScreen("search");
    else go("search");
  }, [area, go]);
  const openRoom = useCallback(
    (lyrics: boolean) => {
      if (area !== "room") setBack(area);
      setShowLyrics(lyrics);
      go("room");
    },
    [area, go],
  );

  // A layout effect: the keys work from the first painted frame.
  useLayoutEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const command = shellKey(describe(event, overlay !== null));
      if (!command) return;
      if (command === "toggle") send("TogglePlayback");
      else if (command === "queue") setOverlay("queue");
      else if (command === "command") setOverlay("command");
      else if (command === "search") openSearch();
      else if (command === "room") {
        if (area === "room") go(back);
        else openRoom(false);
      } else if (command === "lyrics") {
        if (area === "room") setShowLyrics((shown) => !shown);
        else openRoom(true);
      } else if (command === "escape") {
        if (overlay) setOverlay(null);
        else if (!blurText() && area === "room") go(back);
      } else if (readyKeys.split(" ").includes(command.area)) go(command.area);
      else return;
      event.preventDefault();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [go, send, readyKeys, overlay, openSearch, openRoom, area, back]);

  // A switch or a closed overlay hands the keyboard to the screen; the first render leaves focus alone.
  const shown = useRef({ area, overlay });
  useEffect(() => {
    const before = shown.current;
    shown.current = { area, overlay };
    if (overlay || (before.area === area && !before.overlay)) return;
    focusScreen(area);
  }, [area, overlay]);

  return (
    <main className={area === "room" ? "shell room-shell" : "shell"}>
      <TopBar
        area={area}
        ready={ready}
        onArea={go}
        device={playback?.device ?? null}
        connected={state.connected}
        query={query}
        onSearch={openSearch}
        queued={queued.length}
        queueOpen={overlay === "queue"}
        onQueue={() => setOverlay(overlay === "queue" ? null : "queue")}
      />
      <div className="screens">
        {[...visited].map((seen) => (
          <div
            key={seen}
            className="screen"
            data-area={seen}
            hidden={seen !== area}
          >
            {screens[seen]}
          </div>
        ))}
      </div>
      {area !== "room" && (
        <PlayerBar
          playback={playback}
          position={state.position}
          connected={state.connected}
          send={send}
          onRoom={openRoom}
        />
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
            go("search");
          }}
          onClose={() => setOverlay(null)}
        />
      )}
      {overlay === "queue" && (
        <QueueDrawer
          queue={queue ?? null}
          item={playback?.item ?? null}
          send={send}
          onClose={() => setOverlay(null)}
        />
      )}
      <Toast
        status={channels.status?.payload ?? null}
        route={channels.route?.payload ?? null}
        send={send}
      />
    </main>
  );
}

function describe(event: KeyboardEvent, overlay: boolean): ShellKey {
  const target = event.target instanceof HTMLElement ? event.target : null;
  return {
    key: event.key,
    modified: event.ctrlKey || event.metaKey || event.altKey,
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

function focusScreen(area: Area) {
  const screen = document.querySelector(`[data-area="${area}"]`);
  const target = screen?.querySelector<HTMLElement>("[data-focus]");
  target?.focus();
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
