import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";
import type { Action } from "./bindings/Action";
import type { DeviceInfo } from "./bindings/DeviceInfo";
import { shellKey, type Area, type ShellKey } from "./areas";
import type { State } from "./connection";
import { CommandMode } from "./CommandMode";
import { pageStorage } from "./commandModel";
import { Library } from "./Library";
import { upNext } from "./libraryModel";
import { sourceOf } from "./format";
import { PlayerBar } from "./PlayerBar";
import { QueueDrawer } from "./QueueDrawer";
import { Search } from "./Search";
import { Stats } from "./Stats";
import { Toast } from "./Toast";
import { TopBar } from "./TopBar";

const NO_DEVICES: DeviceInfo[] = [];

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

  // Stats reload on every visit, as the terminal's Stats row does.
  useEffect(() => {
    if (area === "stats") send({ OpenLibrary: "Stats" });
  }, [area, send]);

  // The Web API queue is fetched on request only: on open and on each Spotify track change.
  useEffect(() => {
    if (overlay === "queue" && spotifyPlays) send("RefreshQueue");
  }, [overlay, spotifyPlays, playingUri, send]);

  const screens: Partial<Record<Area, ReactNode>> = {
    library: (
      <Library
        playlists={channels.library?.payload ?? null}
        liked={channels.liked?.payload ?? null}
        source={channels.source?.payload ?? null}
        playingUri={playingUri}
        upNext={queued}
        send={send}
      />
    ),
    search: (
      <Search
        search={channels.search?.payload ?? null}
        searchRev={channels.search?.rev ?? null}
        statusRev={channels.status?.rev ?? null}
        source={channels.source?.payload ?? null}
        playingUri={playingUri}
        query={query}
        onQuery={setQuery}
        send={send}
      />
    ),
    stats: <Stats stats={channels.stats?.payload ?? null} send={send} />,
  };
  const ready = (target: Area) => target in screens;
  const readyKeys = Object.keys(screens).join(" ");
  // The search field takes the keyboard even when Search is already the area.
  const openSearch = useCallback(() => {
    if (area === "search") focusScreen("search");
    else go("search");
  }, [area, go]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const command = shellKey(describe(event, overlay !== null));
      if (!command) return;
      if (command === "toggle") send("TogglePlayback");
      else if (command === "queue") setOverlay("queue");
      else if (command === "command") setOverlay("command");
      else if (command === "search") openSearch();
      else if (command === "escape") {
        if (overlay) setOverlay(null);
        else blurText();
      } else if (readyKeys.split(" ").includes(command.area)) go(command.area);
      else return;
      event.preventDefault();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [go, send, readyKeys, overlay, openSearch]);

  // A switch or a closed overlay hands the keyboard to the screen; the first render leaves focus alone.
  const shown = useRef({ area, overlay });
  useEffect(() => {
    const before = shown.current;
    shown.current = { area, overlay };
    if (overlay || (before.area === area && !before.overlay)) return;
    focusScreen(area);
  }, [area, overlay]);

  return (
    <main className="shell">
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
      <PlayerBar
        playback={playback}
        position={state.position}
        connected={state.connected}
        send={send}
      />
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
  return (
    element.isContentEditable ||
    ["INPUT", "TEXTAREA", "SELECT"].includes(element.tagName)
  );
}

function focusScreen(area: Area) {
  const screen = document.querySelector(`[data-area="${area}"]`);
  const target = screen?.querySelector<HTMLElement>("[data-focus]");
  target?.focus();
  target
    ?.querySelector('[aria-selected="true"]')
    ?.scrollIntoView({ block: "nearest" });
}

function blurText() {
  const active = document.activeElement;
  if (active instanceof HTMLElement && isText(active)) active.blur();
}
