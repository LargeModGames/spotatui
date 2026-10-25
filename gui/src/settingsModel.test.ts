import { describe, expect, it } from "vitest";
import type { SettingRow } from "./bindings/SettingRow";
import type { UpdateStatus } from "./bindings/UpdateStatus";
import { ALBUM_SORTS, ARTIST_SORTS } from "./libraryModel";
import {
  commitValue,
  optionLabel,
  readyVersion,
  sections,
  setSetting,
  updateLine,
} from "./settingsModel";

const row = (
  id: string,
  group: string,
  value: SettingRow["value"],
): SettingRow => ({
  id,
  group,
  name: id,
  description: "",
  value,
  button: null,
});

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

  it("puts the default sort rows under Library, after Playback", () => {
    const sorts = [
      "default_sort_playlist_tracks",
      "default_sort_saved_albums",
      "default_sort_saved_artists",
    ].map((id) =>
      row(`behavior.${id}`, "library", {
        kind: "choice",
        value: "default",
        options: ["default", "name"],
        labels: ["default", "name"],
      }),
    );
    const shown = sections([...sorts, seek], false);
    expect(shown.map((section) => section.heading)).toEqual([
      "Playback",
      "Library",
    ]);
    expect(shown[1].rows).toEqual(sorts);
  });

  it("puts the source rows under Sources, after Library and before Updates", () => {
    const library = row("behavior.default_sort_saved_albums", "library", {
      kind: "choice",
      value: "default",
      options: ["default"],
      labels: ["Custom order"],
    });
    const music = row("behavior.local_music_path", "sources", {
      kind: "text",
      value: "",
    });
    expect(
      sections([music, library, seek], true).map((section) => section.heading),
    ).toEqual(["Playback", "Library", "Sources", "Updates"]);
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

describe("sections with a query", () => {
  const named = (
    name: string,
    group: string,
    description = "",
  ): SettingRow => ({
    id: `behavior.${name}`,
    group,
    name,
    description,
    button: null,
    value: { kind: "bool", value: true },
  });
  const all = [
    named(
      "Discord Rich Presence",
      "integrations",
      "Show your current track in Discord",
    ),
    named("Streaming Bitrate", "streaming", "Native streaming quality in kbps"),
    named(
      "Media Key Controls",
      "playback",
      "Let OS media keys control playback",
    ),
    named("Auto-Update Delay", "updates", "How long a release must be out"),
  ];
  const shown = (query: string) =>
    sections(all, true, query).map((section) => [
      section.heading,
      section.rows.map((row) => row.name),
    ]);

  it("keeps the rows whose name or description holds every word, ignoring case", () => {
    expect(shown("DISCORD")).toEqual([
      ["Integrations", ["Discord Rich Presence"]],
    ]);
    expect(shown("kbps")).toEqual([["Streaming", ["Streaming Bitrate"]]]);
    expect(shown("media playback")).toEqual([
      ["Playback", ["Media Key Controls"]],
    ]);
  });

  it("a query that names a section keeps that section's rows", () => {
    expect(shown("integrations")).toEqual([
      ["Integrations", ["Discord Rich Presence"]],
    ]);
  });

  it("hides every section without a match, Updates included", () => {
    expect(shown("bitrate")).toEqual([["Streaming", ["Streaming Bitrate"]]]);
    expect(shown("nothing like this")).toEqual([]);
  });

  it("requires every word to match the same row", () => {
    expect(shown("discord kbps")).toEqual([]);
  });

  it("shows matching Updates rows only when there is an updater", () => {
    expect(shown("auto update")).toEqual([["Updates", ["Auto-Update Delay"]]]);
    expect(sections(all, false, "updates")).toEqual([]);
    expect(sections([], true, "updates")).toEqual([]);
  });

  it("a blank query changes nothing", () => {
    expect(sections(all, true, "  ")).toEqual(sections(all, true));
    expect(sections(all, false, "")).toEqual(sections(all, false));
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

  it("a password field commits typed text and never an empty one", () => {
    const password = row("behavior.subsonic_password", "sources", {
      kind: "secret",
      set: true,
    });
    expect(commitValue(password, "hunter2")).toEqual({ Text: "hunter2" });
    expect(commitValue(password, "")).toBeNull();
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

describe("optionLabel", () => {
  it("labels each default sort row's tokens", () => {
    expect(
      optionLabel("behavior.default_sort_playlist_tracks", "default"),
    ).toBe("Custom order");
    expect(
      optionLabel("behavior.default_sort_playlist_tracks", "date_added:desc"),
    ).toBe("Date added (newest first)");
    expect(optionLabel("behavior.default_sort_saved_albums", "name:desc")).toBe(
      ALBUM_SORTS.find((sort) => sort.value === "name:desc")?.label,
    );
    expect(optionLabel("behavior.default_sort_saved_artists", "name")).toBe(
      ARTIST_SORTS.find((sort) => sort.value === "name")?.label,
    );
  });

  it("shows the option itself for any other row or an unknown token", () => {
    expect(optionLabel("behavior.streaming_bitrate", "320")).toBe("320");
    expect(optionLabel("behavior.default_sort_saved_albums", "rating")).toBe(
      "rating",
    );
  });
});
