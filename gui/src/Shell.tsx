import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";
import type { Action } from "./bindings/Action";
import { shellKey, type Area, type ShellKey } from "./areas";
import type { State } from "./connection";
import { Library } from "./Library";
import { upNext } from "./libraryModel";
import { PlayerBar } from "./PlayerBar";
import { Toast } from "./Toast";
import { TopBar } from "./TopBar";

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
  const queued = useMemo(() => upNext(queue), [queue]);

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
  };
  const ready = (target: Area) => target in screens;
  const readyKeys = Object.keys(screens).join(" ");

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const command = shellKey(describe(event, false));
      if (!command) return;
      if (command === "toggle") send("TogglePlayback");
      else if (command === "escape") blurText();
      else if (readyKeys.split(" ").includes(command.area)) go(command.area);
      else return;
      event.preventDefault();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [go, send, readyKeys]);

  // A switch hands the keyboard to the new screen's list; the first render leaves focus alone.
  const shown = useRef(area);
  useEffect(() => {
    if (shown.current === area) return;
    shown.current = area;
    const screen = document.querySelector(`[data-area="${area}"]`);
    const target = screen?.querySelector<HTMLElement>("[data-focus]");
    target?.focus();
    target
      ?.querySelector('[aria-selected="true"]')
      ?.scrollIntoView({ block: "nearest" });
  }, [area]);

  return (
    <main className="shell">
      <TopBar
        area={area}
        ready={ready}
        onArea={go}
        device={playback?.device ?? null}
        connected={state.connected}
        queued={queued.length}
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

function blurText() {
  const active = document.activeElement;
  if (active instanceof HTMLElement && isText(active)) active.blur();
}
