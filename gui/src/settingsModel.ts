import type { Action } from "./bindings/Action";
import type { SettingInput } from "./bindings/SettingInput";
import type { SettingRow } from "./bindings/SettingRow";
import type { UpdateAsk } from "./bindings/UpdateAsk";
import type { UpdateStatus } from "./bindings/UpdateStatus";

/** The page's sections, in display order. */
export const GROUPS = [
  { group: "playback", heading: "Playback" },
  { group: "streaming", heading: "Streaming" },
  { group: "integrations", heading: "Integrations" },
  { group: "updates", heading: "Updates" },
] as const;

export interface Section {
  group: string;
  heading: string;
  rows: SettingRow[];
}

/** The sections that have rows; Updates shows whenever there is an updater. */
export function sections(rows: SettingRow[], updater: boolean): Section[] {
  return GROUPS.map(({ group, heading }) => ({
    group,
    heading,
    rows: rows.filter((row) => row.group === group),
  })).filter((section) =>
    section.group === "updates" ? updater : section.rows.length > 0,
  );
}

export function setSetting(id: string, value: SettingInput): Action {
  return { SetSetting: { id, value } };
}

/** The value a text or number field commits; null when it is unchanged or not a whole number. */
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
