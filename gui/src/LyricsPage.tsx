import { useLayoutEffect, useRef } from "react";
import type { Action } from "./bindings/Action";
import type { LyricsPayload } from "./bindings/LyricsPayload";
import type { NowPlaying } from "./bindings/NowPlaying";
import type { Position } from "./connection";
import "./LyricsPage.css";
import { LyricText } from "./LyricText";
import { activeLyric } from "./roomModel";
import { usePosition } from "./usePosition";

/** The lyrics of the playing track in the main view. */
export function LyricsPage({
  item,
  lyrics,
  position,
  active,
  connected,
  send,
}: {
  item: NowPlaying | null;
  lyrics: LyricsPayload | null;
  position: Position | null;
  active: boolean;
  connected: boolean;
  send: (action: Action) => void;
}) {
  // A hidden page keeps no animation frame running.
  const ms = usePosition(position, (item?.is_playing ?? false) && active);
  const synced = lyrics?.synced ?? false;
  const lines = lyrics?.lines ?? [];
  const current = synced ? activeLyric(lines, ms) : -1;
  const page = useRef<HTMLDivElement>(null);
  // The page scrolls itself, so the current line can sit in its middle.
  useLayoutEffect(() => {
    const root = page.current;
    const line = root?.querySelector<HTMLElement>('[aria-current="true"]');
    if (!active || !root || !line) return;
    root.scrollTo({
      top: line.offsetTop + line.offsetHeight / 2 - root.clientHeight / 2,
      behavior: "smooth",
    });
  }, [current, active]);

  const note = !item
    ? "Nothing is playing. Play a song to see its lyrics here."
    : !lyrics || lyrics.status === "loading" || lyrics.status === "not_started"
      ? "Loading lyrics…"
      : lyrics.status === "not_found" || lines.length === 0
        ? "No lyrics for this song"
        : null;
  return (
    <div ref={page} className="lyrics-page" data-focus tabIndex={-1}>
      {note ? (
        <p className="lyrics-page-note" role="status">
          {note}
        </p>
      ) : (
        <>
          {!synced && (
            <p className="lyrics-page-unsynced">These lyrics are not synced</p>
          )}
          <ol className={synced ? "lyrics-lines synced" : "lyrics-lines"}>
            {lines.map((line, row) => (
              <li
                key={`${row}-${line.at_ms}`}
                aria-current={row === current ? "true" : undefined}
                className={
                  !synced || row === current
                    ? undefined
                    : row < current
                      ? "past"
                      : "future"
                }
              >
                <LyricText
                  line={line}
                  synced={synced}
                  tabStop={row === Math.max(current, 0)}
                  connected={connected}
                  send={send}
                />
              </li>
            ))}
          </ol>
        </>
      )}
    </div>
  );
}
