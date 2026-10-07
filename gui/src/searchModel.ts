import type { SearchPayload } from "./bindings/SearchPayload";
import type { Source } from "./bindings/Source";

export type Tab = "Everything" | "Tracks" | "Albums" | "Artists" | "Playlists";

export const TABS: Tab[] = [
  "Everything",
  "Tracks",
  "Albums",
  "Artists",
  "Playlists",
];

/** Whether a tab has anything to show; Everything always does. */
export function tabHasResults(search: SearchPayload, tab: Tab): boolean {
  switch (tab) {
    case "Everything":
      return true;
    case "Tracks":
      return search.tracks.length > 0;
    case "Albums":
      return search.albums.length > 0;
    case "Artists":
      return search.artists.length > 0;
    case "Playlists":
      return search.playlists.length > 0;
  }
}

/** The year of a release date such as `2016-05-13`; empty when unknown. */
export function year(releaseDate: string | null): string {
  return releaseDate?.slice(0, 4) ?? "";
}

/** The scope picker's label. Local has no catalogue search, so it searches Spotify. */
export function scopeLabel(source: Source): string {
  return source === "Local" ? "Local has no search · Spotify" : `In ${source}`;
}

/** The other compiled sources the query can run in; Local is left out because it searches Spotify. */
export function otherSources(compiled: Source[], active: Source): Source[] {
  return compiled.filter((source) => source !== active && source !== "Local");
}
