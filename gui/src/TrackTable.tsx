import { memo, useLayoutEffect, useRef, type KeyboardEvent } from "react";
import type { TrackInfo } from "./bindings/TrackInfo";
import { clock, sourceOf } from "./format";
import { SourceBadge } from "./SourceBadge";

/** The `# / TITLE / ALBUM / PLAYS FROM / TIME` table; the cursor row is `aria-selected`. */
export const TrackTable = memo(function TrackTable({
  tracks,
  cursor,
  playingUri,
  onKeyDown,
  onPick,
  onPlay,
}: {
  tracks: TrackInfo[];
  cursor: number;
  playingUri: string | null;
  onKeyDown: (event: KeyboardEvent<HTMLDivElement>) => void;
  onPick: (index: number) => void;
  onPlay: (index: number) => void;
}) {
  const list = useRef<HTMLDivElement>(null);
  // The cursor row follows a key press and a page that lands under the cursor.
  useLayoutEffect(() => {
    list.current
      ?.querySelector('[aria-selected="true"]')
      ?.scrollIntoView({ block: "nearest" });
  }, [cursor, tracks.length]);

  return (
    <>
      <div className="row head eyebrow" aria-hidden="true">
        <span>#</span>
        <span>TITLE</span>
        <span className="album">ALBUM</span>
        <span>PLAYS FROM</span>
        <span className="time">TIME</span>
      </div>
      <div
        ref={list}
        className="tracks"
        role="listbox"
        aria-label="Liked Songs"
        aria-activedescendant={cursor >= 0 ? `liked-${cursor}` : undefined}
        tabIndex={0}
        onKeyDown={onKeyDown}
      >
        {tracks.map((track, index) => {
          const now = playingUri !== null && track.uri === playingUri;
          const source = sourceOf(track.uri);
          return (
            <div
              key={`${index}-${track.uri ?? track.name}`}
              id={`liked-${index}`}
              role="option"
              className={now ? "row now" : "row"}
              aria-selected={index === cursor}
              onClick={() => onPick(index)}
              onDoubleClick={() => onPlay(index)}
            >
              <span className="n">{now ? "▶" : index + 1}</span>
              <span className="name">
                <b>{track.name}</b> <span>{track.artists.join(", ")}</span>
              </span>
              <span className="album">{track.album}</span>
              <span>{source && <SourceBadge source={source} />}</span>
              <span className="time">
                {track.duration_ms > 0 && clock(track.duration_ms)}
              </span>
            </div>
          );
        })}
      </div>
    </>
  );
});
