import type { Action } from "./bindings/Action";
import type { EpisodeInfo } from "./bindings/EpisodeInfo";
import type { PodcastPage } from "./bindings/PodcastPage";

/** The row this far from the end of the loaded episodes asks for the next page once it scrolls into view. */
export const NEAR_END = 5;

/** Play the episodes from `index`: every episode with a URI, the offset counting only those. */
export function episodePlay(
  episodes: EpisodeInfo[],
  index: number,
): Action | null {
  if (!episodes[index]?.uri) return null;
  const uris = episodes.flatMap((episode) =>
    episode.uri ? [episode.uri] : [],
  );
  const offset = episodes
    .slice(0, index)
    .filter((episode) => episode.uri).length;
  return { PlayUris: { uris, offset } };
}

/** The newest episode that can play; the list is newest first. -1 when none can. */
export function newestEpisode(episodes: EpisodeInfo[]): number {
  return episodes.findIndex((episode) => episode.uri);
}

/** "45 min", "1 hr 5 min", "2 hr", or "30 sec" under a minute; empty for no length. */
export function episodeLength(ms: number): string {
  if (ms <= 0) return "";
  const minutes = Math.round(ms / 60_000);
  if (minutes === 0) return `${Math.round(ms / 1000)} sec`;
  if (minutes < 60) return `${minutes} min`;
  const hours = Math.floor(minutes / 60);
  return minutes % 60 ? `${hours} hr ${minutes % 60} min` : `${hours} hr`;
}

const releaseDate = new Intl.DateTimeFormat("en-US", {
  year: "numeric",
  month: "short",
  day: "numeric",
  timeZone: "UTC",
});

/** A full release date as "Oct 1, 2026"; a year or a month stays as given. */
export function episodeDate(date: string): string {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(date)) return date;
  return releaseDate.format(new Date(`${date}T00:00:00Z`));
}

/** How far the user got: played through, part way, or not started (null). */
export type Resume =
  { played: true } | { played: false; percent: number; left: string };

export function resumeOf(episode: EpisodeInfo): Resume | null {
  const point = episode.resume_point;
  if (!point) return null;
  if (point.fully_played) return { played: true };
  if (point.resume_position_ms <= 0 || episode.duration_ms <= 0) return null;
  const at = Math.min(point.resume_position_ms, episode.duration_ms);
  const left = episodeLength(episode.duration_ms - at);
  return {
    played: false,
    percent: Math.round((at / episode.duration_ms) * 100),
    left: left ? `${left} left` : "",
  };
}

/** Lets each episode list on screen ask for the next page once. */
export interface EpisodePager {
  /** Note the list on screen; another show or another length may ask again. */
  land(showId: string, length: number): void;
  /** Whether LoadMore should go out for this page now. */
  ask(page: PodcastPage): boolean;
}

export function episodePager(): EpisodePager {
  let shown = "";
  let asked = false;
  const land = (showId: string, length: number) => {
    const key = `${showId}:${length}`;
    if (key === shown) return;
    shown = key;
    asked = false;
  };
  return {
    land,
    ask(page) {
      land(page.show.id ?? "", page.episodes.length);
      if (!page.has_more || asked) return false;
      asked = true;
      return true;
    },
  };
}

/** A description the three-line clamp likely cuts: a long one or one with more than three lines. */
export function longDescription(text: string): boolean {
  return text.length > 240 || text.split("\n").length > 3;
}

export function showLink(id: string): string {
  return `https://open.spotify.com/show/${id}`;
}

export function episodeLink(id: string): string {
  return `https://open.spotify.com/episode/${id}`;
}
