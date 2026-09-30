import {
  memo,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";
import type { Action } from "./bindings/Action";
import type { DiscoverPayload } from "./bindings/DiscoverPayload";
import type { DiscoverTimeRange } from "./bindings/DiscoverTimeRange";
import "./Discover.css";
import {
  fetchFor,
  QUEUE_CAP,
  RANGES,
  rowsFor,
  stepRange,
  type DiscoverList,
} from "./discoverModel";
import { clock } from "./format";
import { KeyHints } from "./KeyHints";
import { clampCursor, playRequest, step } from "./libraryModel";

/** Discover as lists: your top tracks per range and the top artists mix. The map needs data that does not exist yet. */
export const Discover = memo(function Discover({
  discover,
  active,
  playingUri,
  send,
}: {
  discover: DiscoverPayload | null;
  active: boolean;
  playingUri: string | null;
  send: (action: Action) => void;
}) {
  const [list, setList] = useState<DiscoverList>("top");
  const [range, setRange] = useState<DiscoverTimeRange>("Medium");
  const [want, setWant] = useState(0);
  const rows = rowsFor(discover, list, range);
  const cursor = clampCursor(want, rows.length);

  // The last target asked for. It stays set after a failure or an empty answer, so
  // nothing loops; choosing the list or the range again lifts it for one retry.
  const requested = useRef<string | null>(null);
  const loading = discover?.loading ?? false;
  // A retry of the list already shown changes no other state, so it counts here to run the fetch.
  const [retries, setRetries] = useState(0);
  const retry = (target: string) => {
    if (requested.current !== target) return;
    requested.current = null;
    setRetries((count) => count + 1);
  };
  useEffect(() => {
    if (!active || !discover) return;
    const action = fetchFor(discover, list, range, requested.current);
    if (!action) return;
    requested.current = list === "mix" ? "mix" : range;
    send(action);
  }, [active, discover, list, range, retries, send]);

  const listRef = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    listRef.current
      ?.querySelector('[aria-selected="true"]')
      ?.scrollIntoView({ block: "nearest" });
  }, [cursor]);

  const play = (index: number) => {
    const action = playRequest(rows, index);
    if (action) send(action);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.ctrlKey || event.metaKey || event.altKey) return;
    if (event.key === "Enter") play(cursor);
    else if (event.key === "[" || event.key === "]") {
      setList("top");
      setRange(stepRange(range, event.key === "]"));
    } else {
      const move = step(event.key, cursor, rows.length, false);
      if (!move) return;
      setWant(clampCursor(move.next, rows.length));
    }
    event.preventDefault();
  };

  const liked = new Set(discover?.liked_ids ?? []);
  const available = discover?.available ?? false;
  const blocked = list === "mix" && discover?.artists_mix_available === false;
  const notice = blocked
    ? "The top artists mix is not available with this Spotify app key."
    : rows.length === 0
      ? loading
        ? "Loading…"
        : "Nothing here yet."
      : null;
  const heading =
    list === "mix"
      ? "Top artists mix"
      : `Your top tracks, ${RANGES.find((entry) => entry.range === range)?.label.toLowerCase()}`;
  return (
    <div className="discover">
      <div className="discover-head">
        <h1>Discover</h1>
        <span>What you play most, and the songs around it.</span>
      </div>
      {!available ? (
        <p className="empty">Discover needs a Spotify session.</p>
      ) : (
        <div className="discover-body">
          <section aria-label={heading}>
            <div className="discover-controls">
              <div role="group" aria-label="List" className="segmented">
                <button
                  type="button"
                  aria-pressed={list === "top"}
                  onClick={() => {
                    retry(range);
                    setList("top");
                  }}
                >
                  Your top tracks
                </button>
                <button
                  type="button"
                  aria-pressed={list === "mix"}
                  onClick={() => {
                    retry("mix");
                    setList("mix");
                  }}
                >
                  Top artists mix
                </button>
              </div>
              {list === "top" && (
                <div role="group" aria-label="Range" className="ranges">
                  {RANGES.map((entry) => (
                    <button
                      key={entry.range}
                      type="button"
                      aria-pressed={entry.range === range}
                      onClick={() => {
                        retry(entry.range);
                        setRange(entry.range);
                      }}
                    >
                      {entry.label}
                    </button>
                  ))}
                </div>
              )}
            </div>
            {/* The status stays mounted outside the list, which may hold only options. */}
            <p className={notice ? "empty" : "sr-only"} role="status">
              {notice}
            </p>
            {/* The list stays mounted in every state, so it keeps the keyboard across a range change. */}
            <div
              ref={listRef}
              className="discover-rows"
              role="listbox"
              aria-label={heading}
              aria-activedescendant={
                rows.length > 0 ? `discover-${cursor}` : undefined
              }
              tabIndex={0}
              data-focus
              onKeyDown={onKeyDown}
            >
              {!blocked &&
                rows.map((track, index) => {
                  const now = playingUri !== null && track.uri === playingUri;
                  return (
                    <div
                      key={`${index}-${track.uri ?? track.name}`}
                      id={`discover-${index}`}
                      role="option"
                      aria-selected={index === cursor}
                      className={now ? "pick now" : "pick"}
                      onClick={() => setWant(index)}
                      onDoubleClick={() => play(index)}
                    >
                      <span className="n">{now ? "▶" : index + 1}</span>
                      <span className="what">
                        <b>{track.name}</b>
                        <span>
                          {[track.artists.join(", "), track.album]
                            .filter(Boolean)
                            .join(" · ")}
                        </span>
                      </span>
                      <span className="mark">
                        {track.id && liked.has(track.id) ? "♥ liked" : ""}
                      </span>
                      <span className="time">{clock(track.duration_ms)}</span>
                    </div>
                  );
                })}
            </div>
          </section>
          <aside aria-label="This list">
            <span className="eyebrow">IN THIS LIST</span>
            <h2>{heading}</h2>
            <p>
              {rows.length > 0
                ? `${rows.length} songs from Spotify.`
                : "No songs yet."}
            </p>
            <div className="discover-actions">
              <button
                type="button"
                className="primary"
                disabled={rows.length === 0}
                onClick={() => {
                  const action = playRequest(rows, 0);
                  if (action) send(action);
                }}
              >
                Play this list
              </button>
              <button
                type="button"
                disabled={rows.length === 0}
                onClick={() =>
                  rows
                    .slice(0, QUEUE_CAP)
                    .forEach((track) => send({ QueueTrack: track }))
                }
              >
                Add {Math.min(rows.length, QUEUE_CAP)} to queue
              </button>
            </div>
          </aside>
        </div>
      )}
      <KeyHints hints={["j k move", "enter play", "[ ] range"]} />
    </div>
  );
});
