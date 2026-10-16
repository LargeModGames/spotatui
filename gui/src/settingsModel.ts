import type { Action } from "./bindings/Action";
import type { SettingInput } from "./bindings/SettingInput";
import type { SettingRow } from "./bindings/SettingRow";
import type { UpdateAsk } from "./bindings/UpdateAsk";
import type { UpdateStatus } from "./bindings/UpdateStatus";
import { ALBUM_SORTS, ARTIST_SORTS } from "./libraryModel";
import { SORT_OPTIONS } from "./sortModel";

/** The page's sections, in display order. */
export const GROUPS = [
  { group: "playback", heading: "Playback" },
  { group: "library", heading: "Library" },
  { group: "sources", heading: "Sources" },
  { group: "streaming", heading: "Streaming" },
  { group: "integrations", heading: "Integrations" },
  { group: "updates", heading: "Updates" },
] as const;

export interface Section {
  group: string;
  heading: string;
  rows: SettingRow[];
}

/** The sections with rows that hold every word of `query`; Updates shows with an updater and no query. */
export function sections(
  rows: SettingRow[],
  updater: boolean,
  query = "",
): Section[] {
  const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
  return GROUPS.map(({ group, heading }) => ({
    group,
    heading,
    rows: rows.filter(
      (row) => row.group === group && matches(row, heading, terms),
    ),
  })).filter((section) =>
    section.group === "updates"
      ? updater && (terms.length === 0 || section.rows.length > 0)
      : section.rows.length > 0,
  );
}

function matches(row: SettingRow, heading: string, terms: string[]): boolean {
  const haystack = `${heading} ${row.name} ${row.description}`.toLowerCase();
  return terms.every((term) => haystack.includes(term));
}

const OPTION_LABELS: Record<string, { value: string; label: string }[]> = {
  "behavior.default_sort_playlist_tracks": SORT_OPTIONS,
  "behavior.default_sort_saved_albums": ALBUM_SORTS,
  "behavior.default_sort_saved_artists": ARTIST_SORTS,
};

/** The label a choice row shows for `option`; the option itself when it has none. */
export function optionLabel(id: string, option: string): string {
  return (
    OPTION_LABELS[id]?.find((known) => known.value === option)?.label ?? option
  );
}

export function setSetting(id: string, value: SettingInput): Action {
  return { SetSetting: { id, value } };
}

/** The value a field commits; null when it is unchanged, not a whole number, or an empty password. */
export function commitValue(
  row: SettingRow,
  draft: string,
): SettingInput | null {
  if (row.value.kind === "number") {
    const text = draft.trim();
    const number = Number(text);
    if (text === "" || !Number.isSafeInteger(number)) return null;
    return number === row.value.value ? null : { Number: number };
  }
  if (row.value.kind === "text")
    return draft === row.value.value ? null : { Text: draft };
  // An empty Text clears a saved password, so a blank field sends nothing.
  if (row.value.kind === "secret") return draft === "" ? null : { Text: draft };
  return null;
}

export interface UpdateLine {
  text: string;
  button: { label: string; ask: UpdateAsk; disabled: boolean } | null;
  failed: boolean;
}

/** The Updates section's status line and its one action. */
export function updateLine(
  status: UpdateStatus,
  version: string | null,
): UpdateLine {
  const line = (
    text: string,
    button: UpdateLine["button"] = null,
    failed = false,
  ): UpdateLine => ({ text, button, failed });
  switch (status.state) {
    case "idle":
    case "up_to_date":
      return line(
        version
          ? `You have the latest version (${version})`
          : "You have the latest version",
        { label: "Check for updates", ask: "check", disabled: false },
      );
    case "checking":
      return line("Looking for updates…", {
        label: "Check for updates",
        ask: "check",
        disabled: true,
      });
    case "available":
      return line(`Version ${status.version} is available`, {
        label: "Download",
        ask: "download",
        disabled: false,
      });
    case "downloading":
      return line(
        status.percent === null
          ? `Downloading version ${status.version}`
          : `Downloading version ${status.version} (${Math.round(status.percent)}%)`,
      );
    case "ready":
      return line(`Version ${status.version} is ready`, {
        label: "Restart to update",
        ask: "install",
        disabled: false,
      });
    case "manual":
      return line(
        `Version ${status.version} is out. Download it from the release page.`,
      );
    case "failed":
      return line(
        status.message,
        { label: "Try again", ask: "check", disabled: false },
        true,
      );
  }
}

/** The version the top bar offers to install: only a downloaded one. */
export function readyVersion(status: UpdateStatus | null): string | null {
  return status?.state === "ready" ? status.version : null;
}
