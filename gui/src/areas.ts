import type { Page } from "./nav";

/** The pages that digit keys open, in key order. */
export const AREAS: { area: Page; label: string; key: string }[] = [
  { area: "home", label: "Home", key: "1" },
  { area: "search", label: "Search", key: "2" },
  { area: "discover", label: "Discover", key: "3" },
  { area: "session", label: "Session", key: "4" },
  { area: "stats", label: "Stats", key: "5" },
  { area: "party", label: "Party", key: "6" },
];

export type ShellCommand =
  | { area: Page }
  | "search"
  | "command"
  | "queue"
  | "room"
  | "lyrics"
  | "toggle"
  | "escape"
  | "settings"
  | "mini";

/** Whether the page runs on macOS, where Cmd takes the place of Ctrl. */
export const MAC =
  typeof navigator !== "undefined" &&
  /Mac|iPhone|iPad/.test(navigator.platform);

/** The settings shortcut, as `aria-keyshortcuts` names it and as a title shows it. */
export const SETTINGS_KEY = MAC
  ? { aria: "Meta+,", label: "⌘," }
  : { aria: "Control+,", label: "Ctrl+," };

/** A key press as the window handler sees it, reduced to plain data. */
export interface ShellKey {
  key: string;
  modified: boolean;
  /** Only the platform's command modifier is held: Cmd on macOS, Ctrl elsewhere. */
  command: boolean;
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
  if (press.handled || press.composing) return null;
  if (press.command && press.key === ",")
    return press.overlay ? null : "settings";
  if (press.modified) return null;
  if (press.key === "Escape") return "escape";
  if (press.typing || press.overlay) return null;
  if (press.key === " ") return press.interactive ? null : "toggle";
  // The terminal's queue key; a lower-case q adds the row under the cursor.
  if (press.key === "Q") return "queue";
  if (press.key === "/") return "search";
  if (press.key === ":") return "command";
  if (press.key === "r") return "room";
  if (press.key === "l") return "lyrics";
  // The terminal's mini player key.
  if (press.key === "T") return "mini";
  const target = AREAS.find((entry) => entry.key === press.key);
  return target ? { area: target.area } : null;
}
