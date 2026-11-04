import { EN, type GuiStrings } from "./i18n";
import type { PaneChord } from "./bindings/PaneChord";
import type { PaneKeys } from "./bindings/PaneKeys";
import type { WindowEdge } from "./bindings/WindowEdge";
import type { Page } from "./nav";

/** The pages that digit keys open, in key order. */
export const areas = (
  s: GuiStrings = EN,
): { area: Page; label: string; key: string }[] => [
  { area: "home", label: s.home, key: "1" },
  { area: "search", label: s.search, key: "2" },
  { area: "discover", label: s.discover, key: "3" },
  { area: "session", label: s.session, key: "4" },
  { area: "stats", label: s.stats, key: "5" },
  { area: "party", label: s.party, key: "6" },
  { area: "visualizer", label: s.visualizer, key: "7" },
];
export const AREAS = areas();

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
  /** `KeyboardEvent.code`: the key's place, the same under Option on macOS. */
  code: string;
  alt: boolean;
  ctrl: boolean;
  shift: boolean;
}

export type PaneCommand = "detach" | "close" | "reset" | { focus: WindowEdge };

/** A key press with any modifier held. */
export const modified = (event: {
  altKey: boolean;
  ctrlKey: boolean;
  metaKey: boolean;
  shiftKey: boolean;
}) => event.altKey || event.ctrlKey || event.metaKey || event.shiftKey;

/** The pane key from `config.yml` that a press matches; never while typing or under an overlay. */
export function paneKey(
  press: ShellKey,
  keys: PaneKeys | null,
): PaneCommand | null {
  if (!keys || press.handled || press.composing) return null;
  if (press.typing || press.overlay) return null;
  const is = (chord: PaneChord | null) =>
    chord !== null &&
    chord.code === press.code &&
    chord.alt === press.alt &&
    chord.ctrl === press.ctrl &&
    chord.shift === press.shift;
  if (is(keys.detach)) return "detach";
  if (is(keys.close)) return "close";
  if (is(keys.reset)) return "reset";
  if (is(keys.focus_left)) return { focus: "left" };
  if (is(keys.focus_right)) return { focus: "right" };
  if (is(keys.focus_up)) return { focus: "top" };
  if (is(keys.focus_down)) return { focus: "bottom" };
  return null;
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
