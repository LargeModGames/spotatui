import type { Action } from "./bindings/Action";
import type { Source } from "./bindings/Source";
import type { TrackInfo } from "./bindings/TrackInfo";
import { listPlayRequest, playRequest } from "./libraryModel";

export type SortField =
  "default" | "name" | "date_added" | "artist" | "album" | "duration";

export interface Sort {
  field: SortField;
  desc: boolean;
}

/** A shown row and its index in the list as loaded. */
export interface ShownRow {
  track: TrackInfo;
  index: number;
}

/** The sort select's options, the values `behavior.default_sort_playlist_tracks` takes. */
export const SORT_OPTIONS: { value: string; label: string }[] = [
  { value: "default", label: "Custom order" },
  { value: "name", label: "Title (A-Z)" },
  { value: "name:desc", label: "Title (Z-A)" },
  { value: "date_added", label: "Date added (oldest first)" },
  { value: "date_added:desc", label: "Date added (newest first)" },
  { value: "artist", label: "Artist (A-Z)" },
  { value: "artist:desc", label: "Artist (Z-A)" },
  { value: "album", label: "Album (A-Z)" },
  { value: "album:desc", label: "Album (Z-A)" },
  { value: "duration", label: "Duration (shortest first)" },
  { value: "duration:desc", label: "Duration (longest first)" },
];

const FIELDS: readonly SortField[] = [
  "default",
  "name",
  "date_added",
  "artist",
  "album",
  "duration",
];

const collator = new Intl.Collator(undefined, { sensitivity: "accent" });

/** `field[:desc]`; anything unknown is the list's own order. */
export function parseSort(value: string | null | undefined): Sort {
  const [field, order, extra] = (value ?? "").trim().split(":");
  if (
    !FIELDS.includes(field as SortField) ||
    extra !== undefined ||
    (order !== undefined && order !== "desc")
  )
    return { field: "default", desc: false };
  const desc = order === "desc" && field !== "default";
  return { field: field as SortField, desc };
}

/** The select value of a sort. */
export function sortValue(sort: Sort): string {
  return sort.desc ? `${sort.field}:desc` : sort.field;
}

/** True when the sort keeps the list's own order. */
export function isDefaultSort(sort: Sort): boolean {
  return (
    (sort.field === "default" || sort.field === "date_added") && !sort.desc
  );
}

function compare(field: SortField): (a: ShownRow, b: ShownRow) => number {
  switch (field) {
    case "name":
      return (a, b) => collator.compare(a.track.name, b.track.name);
    case "album":
      return (a, b) => collator.compare(a.track.album, b.track.album);
    case "artist":
      return (a, b) =>
        collator.compare(a.track.artists[0] ?? "", b.track.artists[0] ?? "");
    case "duration":
      return (a, b) => a.track.duration_ms - b.track.duration_ms;
    default:
      return () => 0;
  }
}

/** The rows in `sort` order; ties keep the loaded order. */
export function sortTracks(tracks: TrackInfo[], sort: Sort): ShownRow[] {
  const rows = tracks.map((track, index) => ({ track, index }));
  if (sort.field === "default") return rows;
  if (sort.field === "date_added") return sort.desc ? rows.reverse() : rows;
  const by = compare(sort.field);
  return rows.sort(sort.desc ? (a, b) => by(b, a) : by);
}

/** The rows whose name, album and artists hold every whitespace-separated term. */
export function filterTracks(rows: ShownRow[], query: string): ShownRow[] {
  const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (terms.length === 0) return rows;
  return rows.filter(({ track }) => {
    const haystack =
      `${track.name} ${track.album} ${track.artists.join(" ")}`.toLowerCase();
    return terms.every((term) => haystack.includes(term));
  });
}

/** Row keys that survive a sort or filter: the uri and its occurrence count, else the index. */
export function rowKeys(tracks: TrackInfo[]): string[] {
  const seen = new Map<string, number>();
  return tracks.map((track, index) => {
    if (!track.uri) return `@${index}`;
    const count = seen.get(track.uri) ?? 0;
    seen.set(track.uri, count + 1);
    return `${track.uri}#${count}`;
  });
}

/** The remove action for a row of a list: an editable Spotify playlist or a YouTube one. */
export function removeAction(
  list: { source: Source; uri: string } | null,
  editable: string[],
  positions: number[] | null,
  track: TrackInfo,
  original: number,
): Action | null {
  if (!list) return null;
  if (list.source === "YouTube") {
    const video = track.id ?? track.uri;
    return video
      ? {
          RemoveTrackFromPlaylist: {
            playlist: list.uri,
            track: video,
            position: 0,
          },
        }
      : null;
  }
  if (list.source !== "Spotify" || !editable.includes(list.uri)) return null;
  const position = positions?.[original];
  if (position === undefined || !track.uri) return null;
  return {
    RemoveTrackFromPlaylist: { playlist: list.uri, track: track.uri, position },
  };
}

/** Play from a shown row: the list's own request in loaded order, the shown URIs once reordered. */
export function shownPlayRequest(
  list: { source: Source; uri: string } | null,
  reordered: boolean,
  shown: TrackInfo[],
  index: number,
): Action | null {
  return list && !reordered
    ? listPlayRequest(list.source, list.uri, shown, index)
    : playRequest(shown, index);
}
