import type { Action } from "./bindings/Action";
import type { SessionPlay } from "./bindings/SessionPlay";
import type { Source } from "./bindings/Source";
import type { TrackInfo } from "./bindings/TrackInfo";
import type { Position } from "./connection";
import { sourceOf } from "./format";

/** The playback position at page-clock time `page`, carried on from the last server position. */
export function positionAt(
  position: Position | null,
  running: boolean,
  page: number,
): number {
  if (position?.ms == null) return 0;
  return position.ms + (running ? page - position.at : 0);
}

/** A gap of this length or more between two plays, or inside one, is a pause. */
const PAUSE_MS = 60_000;

/**
 * When each queued item starts, from the time left on the playing one. Nothing
 * after an item with no length (a stream), and nothing while paused.
 */
export function estimates(
  playing: { durationMs: number; positionMs: number; running: boolean },
  nowMs: number,
  upNext: TrackInfo[],
): (number | null)[] {
  let at: number | null =
    playing.running && playing.durationMs > 0
      ? nowMs + Math.max(playing.durationMs - playing.positionMs, 0)
      : null;
  return upNext.map((track) => {
    const start = at;
    at = at !== null && track.duration_ms > 0 ? at + track.duration_ms : null;
    return start;
  });
}

export interface Marker {
  /// `gap` is time between two plays: playback stopped, or it played something the history does not keep.
  kind: "source" | "pause" | "gap";
  /** Unix milliseconds. */
  at: number;
  /** The play the marker stands before. */
  before: number;
  from?: Source | null;
  to?: Source | null;
  minutes?: number;
}

/** Source changes, gaps between plays and pauses inside one, oldest first. */
export function markers(plays: SessionPlay[]): Marker[] {
  const found: Marker[] = [];
  plays.forEach((play, index) => {
    const previous = plays[index - 1];
    if (previous) {
      const gap = play.started_at_ms - previous.ended_at_ms;
      if (gap >= PAUSE_MS)
        found.push({
          kind: "gap",
          at: previous.ended_at_ms,
          before: index,
          minutes: Math.round(gap / 60_000),
        });
      const from = sourceOf(previous.uri);
      const to = sourceOf(play.uri);
      // A Spotify local file has no URI and so no source; it is no change.
      if (from && to && from !== to)
        found.push({
          kind: "source",
          at: play.started_at_ms,
          before: index,
          from,
          to,
        });
    }
    const held = play.ended_at_ms - play.started_at_ms - play.listened_ms;
    if (held >= PAUSE_MS)
      found.push({
        kind: "pause",
        at: play.started_at_ms,
        before: index + 1,
        minutes: Math.round(held / 60_000),
      });
  });
  return found;
}

/**
 * Replay from a play to the newest one of the same source. A start plays every
 * URI through the source of the first, and only Spotify and Local files can
 * start from a URI alone.
 */
export function replayRun(plays: SessionPlay[], index: number): Action | null {
  const source = sourceOf(plays[index]?.uri ?? null);
  if (source !== "Spotify" && source !== "Local") return null;
  const uris: string[] = [];
  for (const play of plays.slice(index)) {
    if (!play.uri || sourceOf(play.uri) !== source) break;
    uris.push(play.uri);
  }
  return { PlayUris: { uris, offset: 0 } };
}

/** "since 19:40 · 31 songs · 2 sources"; the playing song counts as a source. */
export function headline(plays: SessionPlay[], nowUri: string | null): string {
  const sources = new Set(
    [...plays.map((play) => play.uri), nowUri]
      .map(sourceOf)
      .filter((source) => source !== null),
  );
  return [
    plays[0] && `since ${timeOfDay(plays[0].started_at_ms)}`,
    `${plays.length} ${plays.length === 1 ? "song" : "songs"}`,
    sources.size > 0 &&
      `${sources.size} ${sources.size === 1 ? "source" : "sources"}`,
  ]
    .filter(Boolean)
    .join(" · ");
}

/** A local `hh:mm`. */
export function timeOfDay(ms: number): string {
  const date = new Date(ms);
  return `${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`;
}
