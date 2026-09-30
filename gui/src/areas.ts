/** A screen of the page. The area is page state: the app route cannot name most of them. */
export type Area =
  "library" | "search" | "discover" | "session" | "stats" | "party" | "room";

/** The top bar's areas, in the order of their digit keys. */
export const AREAS: { area: Area; label: string; key: string }[] = [
  { area: "library", label: "Library", key: "1" },
  { area: "search", label: "Search", key: "2" },
  { area: "discover", label: "Discover", key: "3" },
  { area: "session", label: "Session", key: "4" },
  { area: "stats", label: "Stats", key: "5" },
  { area: "party", label: "Party", key: "6" },
];

export type ShellCommand = { area: Area } | "toggle" | "escape";

/** A key press as the window handler sees it, reduced to plain data. */
export interface ShellKey {
  key: string;
  modified: boolean;
  composing: boolean;
  handled: boolean;
  /** The target takes text: an input, a textarea, a select or an editable element. */
  typing: boolean;
  /** The target acts on space by itself: a button, a link or a slider. */
  interactive: boolean;
  overlay: boolean;
}

/** What a key does on the whole page; null leaves it to the browser. Screen keys run first. */
export function shellKey(press: ShellKey): ShellCommand | null {
  if (press.handled || press.modified || press.composing) return null;
  if (press.key === "Escape") return "escape";
  if (press.typing || press.overlay) return null;
  if (press.key === " ") return press.interactive ? null : "toggle";
  const target = AREAS.find((entry) => entry.key === press.key);
  return target ? { area: target.area } : null;
}
