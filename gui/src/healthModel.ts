import type { Source } from "./bindings/Source";
import type { SyncLinkView } from "./bindings/SyncLinkView";
import type { UnmatchReason } from "./bindings/UnmatchReason";

/** The sources a playlist can sync between; Local files cannot. */
export const SYNC_SOURCES: Source[] = [
  "Qobuz",
  "Subsonic",
  "Spotify",
  "YouTube",
];

export type CellRole = "master" | "synced" | "unmatched" | "never" | "none";

/** What a link holds on one source: its master, a mirror in some state, or nothing. */
export function cellFor(
  link: SyncLinkView,
  source: Source,
): { role: CellRole; count: number } {
  if (link.source === source) return { role: "master", count: 0 };
  const mirror = link.mirrors.find((entry) => entry.source === source);
  if (!mirror) return { role: "none", count: 0 };
  // A skipped or failed mirror keeps its old lists and never gets a run stamp.
  if (mirror.last_run === null) return { role: "never", count: 0 };
  const count = mirror.unmatched.length;
  return { role: count > 0 ? "unmatched" : "synced", count };
}

export function unmatchedTotal(links: SyncLinkView[]): number {
  return links.reduce(
    (sum, link) =>
      sum +
      link.mirrors.reduce(
        (inner, mirror) => inner + mirror.unmatched.length,
        0,
      ),
    0,
  );
}

/** The terminal's wording for why a track has no mirror. */
export function reasonText(reason: UnmatchReason): string {
  if (reason === "NoCandidate") return "no match";
  if (reason === "NotSyncable") return "cannot sync";
  return `search failed: ${reason.SearchFailed}`;
}

/** The newest mirror run of a link, as a local `YYYY-MM-DD hh:mm`. */
export function lastSync(link: SyncLinkView): string {
  const stamps = link.mirrors
    .map((mirror) => mirror.last_run)
    .filter((stamp): stamp is string => stamp !== null)
    .map((stamp) => Date.parse(stamp))
    .filter((ms) => !Number.isNaN(ms));
  if (stamps.length === 0) return "never";
  const date = new Date(Math.max(...stamps));
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

/** The selected link by id, else the first; a run can add or drop links. */
export function selectedIndex(
  links: SyncLinkView[],
  id: string | null,
): number {
  const index = links.findIndex((link) => link.id === id);
  return index >= 0 ? index : links.length > 0 ? 0 : -1;
}
