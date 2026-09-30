import { Fragment, useEffect, useState, type KeyboardEvent } from "react";
import type { Action } from "./bindings/Action";
import type { NowPlaying } from "./bindings/NowPlaying";
import type { QueuePayload } from "./bindings/QueuePayload";
import type { SessionPlay } from "./bindings/SessionPlay";
import type { TrackInfo } from "./bindings/TrackInfo";
import type { Position } from "./connection";
import { clock, sourceOf } from "./format";
import { KeyHints } from "./KeyHints";
import { clampCursor } from "./libraryModel";
import { type QueueCursor, queueKey, queueRow } from "./queueModel";
import "./Session.css";
import {
  estimates,
  headline,
  markers,
  positionAt,
  replayRun,
  timeOfDay,
  type Marker,
} from "./sessionModel";
import { SourceBadge } from "./SourceBadge";
import { usePosition } from "./usePosition";

const PAST_COVERS = 5;
const NEXT_COVERS = 5;

/** This listening session as a line: what played, what plays, what comes next. */
export function Session({
  plays,
  item,
  position,
  queue,
  upNext,
  send,
}: {
  plays: SessionPlay[];
  item: NowPlaying | null;
  position: Position | null;
  queue: QueuePayload | null;
  upNext: TrackInfo[];
  send: (action: Action) => void;
}) {
  const running = item?.is_playing ?? false;
  const ms = usePosition(position, running);
  const [inspect, setInspect] = useState<Marker | null>(null);
  // The Earlier cursor names its play, so a new finished play does not move it to another.
  const [earlierAt, setEarlierAt] = useState<number | null>(null);
  const [nextWant, setNextWant] = useState<QueueCursor>({
    index: 0,
    uri: null,
  });
  // The estimates move by the minute; a clock in state keeps the render pure. The wall clock
  // and the page clock are sampled together, so the position below is taken at that same moment.
  const [now, setNow] = useState(() => ({
    wall: Date.now(),
    page: performance.now(),
  }));
  useEffect(() => {
    const timer = window.setInterval(
      () => setNow({ wall: Date.now(), page: performance.now() }),
      30_000,
    );
    return () => window.clearInterval(timer);
  }, []);
  const sampledMs = positionAt(position, running, now.page);

  const native = queue?.native ?? [];
  const found = markers(plays);
  const recent = plays.slice(-PAST_COVERS);
  const firstShown = plays.length - recent.length;
  const starts = estimates(
    {
      durationMs: item?.is_live ? 0 : (item?.duration_ms ?? 0),
      positionMs: sampledMs,
      running,
    },
    now.wall,
    upNext,
  );
  const newestFirst = [...plays].reverse();
  const earlier = Math.max(
    newestFirst.findIndex((play) => play.started_at_ms === earlierAt),
    newestFirst.length > 0 ? 0 : -1,
  );
  const pickEarlier = (row: number) => {
    const play = newestFirst[clampCursor(row, newestFirst.length)];
    if (play) setEarlierAt(play.started_at_ms);
  };
  const next = queueRow(native, nextWant);

  const onEarlierKey = (event: KeyboardEvent<HTMLOListElement>) => {
    if (event.ctrlKey || event.metaKey || event.altKey) return;
    if (event.key === "Enter") {
      const action = replayRun(plays, plays.length - 1 - earlier);
      if (action) send(action);
    } else if (event.key === "j" || event.key === "ArrowDown")
      pickEarlier(earlier + 1);
    else if (event.key === "k" || event.key === "ArrowUp")
      pickEarlier(earlier - 1);
    else return;
    event.preventDefault();
  };

  return (
    <div className="session" data-focus tabIndex={-1}>
      <div className="session-head">
        <h1>This session</h1>
        <span>{headline(plays, item?.uri ?? null)}</span>
        <kbd>enter replay from here · J K reorder · x remove</kbd>
      </div>

      <div className="timeline" aria-label="Timeline">
        <div className="lane past">
          <span className="eyebrow">← PAST</span>
          <div className="covers">
            {firstShown > 0 && <span className="more">+{firstShown} more</span>}
            {recent.map((entry, index) => {
              const at = firstShown + index;
              const marks = found.filter((mark) => mark.before === at);
              return (
                <Fragment key={`${at}-${entry.started_at_ms}`}>
                  {marks.map((marker, stack) => (
                    <button
                      key={`${marker.kind}-${stack}`}
                      type="button"
                      className={`marker ${marker.kind}`}
                      style={{ marginTop: stack * 14 }}
                      aria-label={markerTitle(marker)}
                      onClick={() => setInspect(marker)}
                    >
                      <span>{markerLabel(marker)}</span>
                    </button>
                  ))}
                  <Cover
                    image={entry.image_url}
                    title={entry.title}
                    meta={entry.artists.join(", ")}
                    fade={0.55 + index * 0.1}
                  />
                </Fragment>
              );
            })}
          </div>
        </div>
        <div className="lane now">
          <span className="eyebrow">NOW</span>
          {item ? (
            <div className="now-block">
              {item.image_url ? (
                <img src={item.image_url} alt="" />
              ) : (
                <span className="tile" />
              )}
              <b>{item.title}</b>
              <span>
                {item.artists.join(", ")} ·{" "}
                {clock(Math.min(ms, item.duration_ms || ms))} /{" "}
                {item.is_live ? "live" : clock(item.duration_ms)}
              </span>
            </div>
          ) : (
            <p className="empty">Nothing is playing.</p>
          )}
        </div>
        <div className="lane upcoming">
          <span className="eyebrow">UP NEXT →</span>
          <div className="covers">
            {upNext.slice(0, NEXT_COVERS).map((track, index) => (
              <Cover
                key={`${index}-${track.uri ?? track.name}`}
                image={track.image_url}
                title={track.name}
                meta={
                  starts[index] !== null
                    ? `≈ ${timeOfDay(starts[index] as number)}`
                    : track.artists.join(", ")
                }
                fade={1}
              />
            ))}
          </div>
        </div>
      </div>

      {inspect && (
        <div
          className="marker-dialog"
          role="dialog"
          aria-label="Marker details"
        >
          <span className="eyebrow">
            {inspect.kind === "source" ? "SOURCE CHANGE" : "PAUSED"} ·{" "}
            {timeOfDay(inspect.at)}
          </span>
          <span>{markerDetail(inspect, plays)}</span>
          <button type="button" onClick={() => setInspect(null)}>
            Close
          </button>
        </div>
      )}

      <div className="session-lists">
        <section aria-label="Earlier">
          <div className="section-head">
            <h2>Earlier</h2>
            <kbd>enter replay from here</kbd>
          </div>
          {newestFirst.length === 0 ? (
            <p className="empty">Nothing has finished playing yet.</p>
          ) : (
            <ol
              role="listbox"
              aria-label="Earlier"
              aria-activedescendant={`earlier-${earlier}`}
              tabIndex={0}
              onKeyDown={onEarlierKey}
            >
              {newestFirst.map((entry, index) => {
                const source = sourceOf(entry.uri);
                return (
                  <li
                    key={`${index}-${entry.started_at_ms}`}
                    id={`earlier-${index}`}
                    role="option"
                    aria-selected={index === earlier}
                    onClick={() => pickEarlier(index)}
                    onDoubleClick={() => {
                      const action = replayRun(plays, plays.length - 1 - index);
                      if (action) send(action);
                    }}
                  >
                    <span className="at">{timeOfDay(entry.started_at_ms)}</span>
                    <span className="name">
                      <b>{entry.title}</b>{" "}
                      <span>{entry.artists.join(", ")}</span>
                    </span>
                    {source && <SourceBadge source={source} />}
                  </li>
                );
              })}
            </ol>
          )}
        </section>
        <section aria-label="Up next">
          <div className="section-head">
            <h2>Up next</h2>
            <kbd>J K move · x remove</kbd>
          </div>
          {upNext.length === 0 ? (
            <p className="empty">Nothing is queued.</p>
          ) : (
            <>
              {native.length > 0 && (
                <ol
                  role="listbox"
                  aria-label="Your queue"
                  aria-activedescendant={`session-q-${next}`}
                  tabIndex={0}
                  onKeyDown={(event) => {
                    if (event.ctrlKey || event.metaKey || event.altKey) return;
                    const press = queueKey(event.key, next, native);
                    if (!press) return;
                    event.preventDefault();
                    setNextWant(press.cursor);
                    if (press.action) send(press.action);
                  }}
                >
                  {native.map((track, index) => (
                    <NextRow
                      key={`${index}-${track.uri ?? track.name}`}
                      id={`session-q-${index}`}
                      track={track}
                      start={starts[index] ?? null}
                      selected={index === next}
                      movable
                      onPick={() => setNextWant({ index, uri: track.uri })}
                    />
                  ))}
                </ol>
              )}
              <ol className="mirror">
                {upNext.slice(native.length).map((track, index) => (
                  <NextRow
                    key={`${index}-${track.uri ?? track.name}`}
                    track={track}
                    start={starts[native.length + index] ?? null}
                    selected={false}
                    movable={false}
                  />
                ))}
              </ol>
            </>
          )}
        </section>
      </div>
      <KeyHints
        hints={["j k move", "enter replay or play", "J K reorder", "x remove"]}
      />
    </div>
  );
}

function Cover({
  image,
  title,
  meta,
  fade,
}: {
  image: string | null;
  title: string;
  meta: string;
  fade: number;
}) {
  return (
    <div className="cover-card" style={{ opacity: Math.min(fade, 1) }}>
      {image ? <img src={image} alt="" /> : <span className="tile" />}
      <b>{title}</b>
      <span>{meta}</span>
    </div>
  );
}

function NextRow({
  id,
  track,
  start,
  selected,
  movable,
  onPick,
}: {
  id?: string;
  track: TrackInfo;
  start: number | null;
  selected: boolean;
  movable: boolean;
  onPick?: () => void;
}) {
  const source = sourceOf(track.uri);
  return (
    <li
      id={id}
      role={movable ? "option" : undefined}
      aria-selected={movable ? selected : undefined}
      onClick={onPick}
    >
      <span className="grip" aria-hidden="true">
        {movable ? "⋮⋮" : ""}
      </span>
      <span className="at">
        {start !== null ? `≈ ${timeOfDay(start)}` : ""}
      </span>
      <span className="name">
        <b>{track.name}</b> <span>{track.artists.join(", ")}</span>
      </span>
      {source && <SourceBadge source={source} />}
    </li>
  );
}

function markerLabel(marker: Marker): string {
  return marker.kind === "source"
    ? `${(marker.from ?? "?").toUpperCase()} → ${(marker.to ?? "?").toUpperCase()}`
    : `paused ${marker.minutes} min`;
}

function markerTitle(marker: Marker): string {
  return `${marker.kind === "source" ? "Source change" : "Pause"} at ${timeOfDay(marker.at)}`;
}

function markerDetail(marker: Marker, plays: SessionPlay[]): string {
  const after = plays[marker.before];
  if (marker.kind === "pause")
    return `Playback stopped for about ${marker.minutes} min${after ? `, then ${after.title} played` : ""}.`;
  return `${marker.from ?? "Unknown"} → ${marker.to ?? "unknown"}. ${after ? `${after.title} played from ${marker.to ?? "another source"}.` : ""}`;
}
