import type { Source } from "./bindings/Source";

const PREFIXES: [string, Source][] = [
  ["spotify:", "Spotify"],
  ["file://", "Local"],
  ["qobuz:", "Qobuz"],
  ["subsonic:", "Subsonic"],
  ["youtube:", "YouTube"],
  ["radio:", "Radio"],
];

/** The source that plays a URI, from its scheme; null for a scheme no source owns. */
export function sourceOf(uri: string | null): Source | null {
  if (!uri) return null;
  return PREFIXES.find(([prefix]) => uri.startsWith(prefix))?.[1] ?? null;
}

/** A duration as `m:ss`, or `h:mm:ss` from one hour. */
export function clock(ms: number): string {
  const total = Math.floor(Math.max(0, ms) / 1000);
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor(total / 60) % 60;
  const seconds = String(total % 60).padStart(2, "0");
  return hours > 0
    ? `${hours}:${String(minutes).padStart(2, "0")}:${seconds}`
    : `${minutes}:${seconds}`;
}
