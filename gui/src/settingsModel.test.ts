import { describe, expect, it } from "vitest";
import type { SettingRow } from "./bindings/SettingRow";
import type { UpdateStatus } from "./bindings/UpdateStatus";
import {
  commitValue,
  readyVersion,
  sections,
  setSetting,
  updateLine,
} from "./settingsModel";

const row = (
  id: string,
  group: string,
  value: SettingRow["value"],
): SettingRow => ({ id, group, name: id, description: "", value });

const seek = row("behavior.seek_milliseconds", "playback", {
  kind: "number",
  value: 5000,
});
const name = row("behavior.streaming_device_name", "streaming", {
  kind: "text",
  value: "spotatui",
});

describe("sections", () => {
  it("orders the groups and drops an empty one", () => {
    const rpc = row("behavior.enable_discord_rpc", "integrations", {
      kind: "bool",
      value: true,
    });
    expect(
      sections([rpc, seek], false).map((section) => section.heading),
    ).toEqual(["Playback", "Integrations"]);
  });

  it("shows Updates whenever there is an updater, and only then", () => {
    expect(sections([seek], true).map((section) => section.group)).toEqual([
      "playback",
      "updates",
    ]);
    const delay = row("behavior.auto_update_delay", "updates", {
      kind: "text",
      value: "0",
    });
    expect(sections([delay], false)).toEqual([]);
  });
});

describe("commitValue", () => {
  it("a number field commits a changed whole number", () => {
    expect(commitValue(seek, " 7500 ")).toEqual({ Number: 7500 });
    expect(commitValue(seek, "5000")).toBeNull();
    expect(commitValue(seek, "2.5")).toBeNull();
    expect(commitValue(seek, "")).toBeNull();
  });

  it("a text field commits any changed text", () => {
    expect(commitValue(name, "Desk")).toEqual({ Text: "Desk" });
    expect(commitValue(name, "spotatui")).toBeNull();
  });

  it("wraps the value in a SetSetting action", () => {
    expect(setSetting(seek.id, { Number: 1 })).toEqual({
      SetSetting: { id: "behavior.seek_milliseconds", value: { Number: 1 } },
    });
  });
});

describe("updateLine", () => {
  const line = (status: UpdateStatus) => updateLine(status, "0.43.0");

  it("offers a check while nothing is pending", () => {
    for (const status of [
      { state: "idle" },
      { state: "up_to_date" },
    ] as UpdateStatus[])
      expect(line(status)).toEqual({
        text: "You have the latest version (0.43.0)",
        button: { label: "Check for updates", ask: "check", disabled: false },
        failed: false,
      });
    expect(line({ state: "checking" })).toMatchObject({
      text: "Looking for updates…",
      button: { disabled: true },
    });
  });

  it("walks a release from available to ready", () => {
    expect(line({ state: "available", version: "0.44.0" })).toMatchObject({
      text: "Version 0.44.0 is available",
      button: { label: "Download", ask: "download" },
    });
    expect(
      line({ state: "downloading", version: "0.44.0", percent: 41.6 }),
    ).toEqual({
      text: "Downloading version 0.44.0 (42%)",
      button: null,
      failed: false,
    });
    expect(
      line({ state: "downloading", version: "0.44.0", percent: null }).text,
    ).toBe("Downloading version 0.44.0");
    expect(line({ state: "ready", version: "0.44.0" })).toMatchObject({
      text: "Version 0.44.0 is ready",
      button: { label: "Restart to update", ask: "install" },
    });
  });

  it("sends a package install to the release page", () => {
    expect(line({ state: "manual", version: "0.44.0" })).toEqual({
      text: "Version 0.44.0 is out. Download it from the release page.",
      button: null,
      failed: false,
    });
  });

  it("shows a failure with a retry", () => {
    expect(line({ state: "failed", message: "Network down" })).toEqual({
      text: "Network down",
      button: { label: "Try again", ask: "check", disabled: false },
      failed: true,
    });
  });
});

describe("readyVersion", () => {
  it("is set only for a downloaded update", () => {
    expect(readyVersion({ state: "ready", version: "0.44.0" })).toBe("0.44.0");
    expect(readyVersion({ state: "available", version: "0.44.0" })).toBeNull();
    expect(readyVersion(null)).toBeNull();
  });
});
