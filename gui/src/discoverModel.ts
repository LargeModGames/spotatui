import type { Action } from "./bindings/Action";
import type { DiscoverPayload } from "./bindings/DiscoverPayload";
import type { DiscoverTimeRange } from "./bindings/DiscoverTimeRange";
import type { TrackInfo } from "./bindings/TrackInfo";

export type DiscoverList = "top" | "mix";

export const RANGES: { range: DiscoverTimeRange; label: string }[] = [
  { range: "Short", label: "4 weeks" },
  { range: "Medium", label: "6 months" },
  { range: "Long", label: "All time" },
];

/** Each queued track can cost a Web API call, so the button queues a few. */
export const QUEUE_CAP = 10;

/** The fetch a list needs, or null when its data is there, loading, or already asked for. */
export function fetchFor(
  discover: DiscoverPayload,
  list: DiscoverList,
  range: DiscoverTimeRange,
  requested: string | null,
): Action | null {
  if (!discover.available || discover.loading) return null;
  if (list === "mix") {
    const wanted =
      discover.artists_mix_available &&
      discover.artists_mix.length === 0 &&
      requested !== "mix";
    return wanted ? { OpenDiscover: "ArtistsMix" } : null;
  }
  const wanted = discover.top_tracks_range !== range && requested !== range;
  return wanted ? { OpenDiscover: { TopTracks: range } } : null;
}

/** The rows of a list; top tracks only when they belong to the chosen range. */
export function rowsFor(
  discover: DiscoverPayload | null,
  list: DiscoverList,
  range: DiscoverTimeRange,
): TrackInfo[] {
  if (!discover) return [];
  if (list === "mix") return discover.artists_mix;
  return discover.top_tracks_range === range ? discover.top_tracks : [];
}

/** One step through the ranges, as `[` and `]` move. */
export function stepRange(
  range: DiscoverTimeRange,
  forward: boolean,
): DiscoverTimeRange {
  const index = RANGES.findIndex((entry) => entry.range === range);
  const next = (index + (forward ? 1 : RANGES.length - 1)) % RANGES.length;
  return RANGES[next].range;
}
