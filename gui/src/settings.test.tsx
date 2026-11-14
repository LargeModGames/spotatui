import type { ReactElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import type { Action } from "./bindings/Action";
import type { SettingRow } from "./bindings/SettingRow";
import type { UpdateAsk } from "./bindings/UpdateAsk";
import type { UpdateStatus } from "./bindings/UpdateStatus";
import {
  RowButton,
  SettingControl,
  Settings,
  SettingsSearch,
  UpdateRow,
} from "./Settings";
import { byName } from "./testTree";

const rows: SettingRow[] = [
  {
    id: "behavior.seek_milliseconds",
    group: "playback",
    name: "Seek Duration (ms)",
    description: "Milliseconds to skip when seeking",
    button: null,
    value: { kind: "number", value: 5000 },
  },
  {
    id: "behavior.streaming_bitrate",
    group: "streaming",
    name: "Streaming Bitrate",
    description: "Audio quality for native streaming",
    button: null,
    value: {
      kind: "choice",
      value: "320",
      options: ["96", "160", "320"],
      labels: ["96", "160", "320"],
    },
  },
  {
    id: "behavior.enable_discord_rpc",
    group: "integrations",
    name: "Discord Rich Presence",
    description: "Show the playing track on Discord",
    button: null,
    value: { kind: "bool", value: true },
  },
  {
    id: "behavior.auto_update_delay",
    group: "updates",
    name: "Auto-update delay",
    description: "How long a release must be out before it downloads",
    button: null,
    value: { kind: "text", value: "0" },
  },
];

const page = (update: UpdateStatus | null, shown = rows) =>
  renderToStaticMarkup(
    <Settings
      rows={shown}
      update={update}
      version="0.43.0"
      send={() => {}}
      onUpdate={() => {}}
    />,
  );

/** The handlers a control element carries. */
interface Handlers {
  onClick?: () => void;
  onCommit?: (draft: string) => void;
  onChange?: (event: { currentTarget: { value: string } }) => void;
}

/** The actions a row's control sends when `act` drives its handlers. */
const sent = (row: SettingRow, act: (handlers: Handlers) => void) => {
  const actions: Action[] = [];
  const control = SettingControl({
    row,
    send: (action) => actions.push(action),
  }) as ReactElement<Handlers>;
  act(control.props);
  return actions;
};

describe("Settings", () => {
  it("shows each row under its group's heading, in order, with its control", () => {
    const html = page({ state: "idle" });
    const order = [
      ">Settings</h1>",
      ">Playback</h2>",
      "Seek Duration (ms)",
      ">Streaming</h2>",
      "Streaming Bitrate",
      ">Integrations</h2>",
      "Discord Rich Presence",
      ">Updates</h2>",
      "Auto-update delay",
    ].map((text) => html.indexOf(text));
    expect(order.every((at) => at >= 0)).toBe(true);
    expect([...order].sort((a, b) => a - b)).toEqual(order);
    expect(html).toContain("Show the playing track on Discord");
    expect(html).toMatch(
      /role="switch"[^>]*aria-checked="true"[^>]*aria-labelledby="behavior.enable_discord_rpc-name"/,
    );
    expect(html).toMatch(/<input[^>]*type="number"[^>]*value="5000"/);
    expect(html).toMatch(/<option value="320" selected="">320<\/option>/);
    expect(html).toMatch(/<input[^>]*type="text"[^>]*value="0"/);
    expect(html).toContain("data-focus");
  });

  it("shows the default sort rows under a Library heading", () => {
    const library: SettingRow = {
      id: "behavior.default_sort_playlist_tracks",
      group: "library",
      name: "Default Playlist Sort",
      description: "How a playlist's tracks are sorted when it opens",
      button: null,
      value: {
        kind: "choice",
        value: "name:desc",
        options: ["default", "name", "name:desc"],
        labels: ["Custom order", "Title (A-Z)", "Title (Z-A)"],
      },
    };
    const html = page(null, [rows[0], library]);
    const order = [">Playback</h2>", ">Library</h2>", "Default Playlist Sort"];
    const at = order.map((text) => html.indexOf(text));
    expect(at.every((index) => index >= 0)).toBe(true);
    expect([...at].sort((a, b) => a - b)).toEqual(at);
    expect(html).toContain('aria-labelledby="settings-library"');
    expect(html).toMatch(
      /<option value="name:desc" selected="">Title \(Z-A\)<\/option>/,
    );
    expect(html).toContain('<option value="default">Custom order</option>');
  });

  it("shows the source rows under a Sources heading, with a password field that only says whether it is set", () => {
    const source = (
      id: string,
      name: string,
      value: SettingRow["value"],
    ): SettingRow => ({
      id,
      group: "sources",
      name,
      description: "",
      value,
      button: null,
    });
    const shown = [
      rows[0],
      source("behavior.subsonic_url", "Subsonic Server URL", {
        kind: "text",
        value: "https://music.example",
      }),
      source("behavior.subsonic_password", "Subsonic Password", {
        kind: "secret",
        set: true,
      }),
      rows[3],
    ];
    const html = page({ state: "idle" }, shown);
    const order = [
      ">Playback</h2>",
      ">Sources</h2>",
      "Subsonic Server URL",
      "Subsonic Password",
      ">Updates</h2>",
    ].map((text) => html.indexOf(text));
    expect(order.every((at) => at >= 0)).toBe(true);
    expect([...order].sort((a, b) => a - b)).toEqual(order);
    expect(html).toContain('aria-labelledby="settings-sources"');
    expect(html).toMatch(
      /<input[^>]*type="password"[^>]*autoComplete="new-password"[^>]*placeholder="Saved"[^>]*aria-labelledby="behavior.subsonic_password-name"/,
    );
    expect(html).not.toMatch(/type="password"[^>]*value=/);
    const unset = renderToStaticMarkup(
      <SettingControl
        row={source("behavior.subsonic_password", "", {
          kind: "secret",
          set: false,
        })}
        send={() => {}}
      />,
    );
    expect(unset).toContain('placeholder="Not set"');
  });

  it("a password commit sends the typed text, and an empty one sends nothing", () => {
    const password: SettingRow = {
      id: "behavior.subsonic_password",
      group: "sources",
      name: "Subsonic Password",
      description: "",
      button: null,
      value: { kind: "secret", set: false },
    };
    const actions = sent(password, (control) => {
      control.onCommit?.("hunter2");
      control.onCommit?.("");
    });
    expect(actions).toEqual([
      {
        SetSetting: {
          id: "behavior.subsonic_password",
          value: { Text: "hunter2" },
        },
      },
    ]);
  });

  it("a row button sends its press, and a busy one is off", () => {
    const soloist = (busy: boolean): SettingRow => ({
      id: "behavior.soloist_path",
      group: "streaming",
      name: "Soloist Path",
      description: "",
      button: { label: "Download from Spotify", busy },
      value: { kind: "text", value: "" },
    });
    const actions: Action[] = [];
    const button = byName(
      <RowButton
        row={soloist(false)}
        send={(action) => actions.push(action)}
      />,
      "Download from Spotify",
    );
    (button.props.onClick as () => void)();
    expect(actions).toEqual([
      { PressSettingButton: { id: "behavior.soloist_path" } },
    ]);
    expect(page(null, [soloist(true)])).toMatch(
      /<button[^>]*disabled=""[^>]*>Download from Spotify</,
    );
    expect(page(null, rows)).not.toContain("setting-controls");
  });

  it("has no Updates section without an updater", () => {
    const html = page(null, rows.slice(0, 3));
    expect(html).not.toContain(">Updates</h2>");
    expect(html).not.toContain("latest version");
  });

  it("says it is loading before the rows arrive", () => {
    expect(
      renderToStaticMarkup(
        <Settings
          rows={null}
          update={null}
          version={null}
          send={() => {}}
          onUpdate={() => {}}
        />,
      ),
    ).toContain("Loading…");
  });

  it("shows the settings search once the rows arrive", () => {
    expect(page(null)).toContain('aria-label="Search settings"');
  });

  it("hides the settings search while the rows are loading", () => {
    expect(
      renderToStaticMarkup(
        <Settings
          rows={null}
          update={null}
          version={null}
          send={() => {}}
          onUpdate={() => {}}
        />,
      ),
    ).not.toContain('aria-label="Search settings"');
  });

  it("Escape in the settings search empties it, and leaves a second Escape to the page", () => {
    const typed: string[] = [];
    const escape = (query: string) => {
      const preventDefault = vi.fn();
      const input = byName(
        <SettingsSearch query={query} onQuery={(next) => typed.push(next)} />,
        "Search settings",
      );
      (input.props.onKeyDown as (event: unknown) => void)({
        key: "Escape",
        nativeEvent: { isComposing: false },
        preventDefault,
      });
      return preventDefault;
    };
    expect(escape("disc")).toHaveBeenCalled();
    expect(escape("")).not.toHaveBeenCalled();
    expect(typed).toEqual([""]);
  });

  it("leaves Escape to the IME while the settings search is composing", () => {
    const onQuery = vi.fn();
    const preventDefault = vi.fn();
    const input = byName(
      <SettingsSearch query="disc" onQuery={onQuery} />,
      "Search settings",
    );
    (input.props.onKeyDown as (event: unknown) => void)({
      key: "Escape",
      nativeEvent: { isComposing: true },
      preventDefault,
    });
    expect(preventDefault).not.toHaveBeenCalled();
    expect(onQuery).not.toHaveBeenCalled();
  });

  it("a switch click sends the flipped value", () => {
    const actions = sent(rows[2], (control) => control.onClick?.());
    expect(actions).toEqual([
      {
        SetSetting: {
          id: "behavior.enable_discord_rpc",
          value: { Bool: false },
        },
      },
    ]);
  });

  it("a number commit sends the number and an unchanged one sends nothing", () => {
    const actions = sent(rows[0], (control) => {
      control.onCommit?.("7500");
      control.onCommit?.("5000");
    });
    expect(actions).toEqual([
      {
        SetSetting: {
          id: "behavior.seek_milliseconds",
          value: { Number: 7500 },
        },
      },
    ]);
  });

  it("a select change sends the option as text", () => {
    const actions = sent(rows[1], (control) =>
      control.onChange?.({ currentTarget: { value: "160" } }),
    );
    expect(actions).toEqual([
      {
        SetSetting: {
          id: "behavior.streaming_bitrate",
          value: { Text: "160" },
        },
      },
    ]);
  });

  it("a default sort select sends the raw token, not its label", () => {
    const library: SettingRow = {
      id: "behavior.default_sort_playlist_tracks",
      group: "library",
      name: "Default Playlist Sort",
      description: "",
      button: null,
      value: {
        kind: "choice",
        value: "default",
        options: ["default", "date_added:desc"],
        labels: ["Custom order", "Date added (newest first)"],
      },
    };
    const actions = sent(library, (control) =>
      control.onChange?.({ currentTarget: { value: "date_added:desc" } }),
    );
    expect(actions).toEqual([
      {
        SetSetting: {
          id: "behavior.default_sort_playlist_tracks",
          value: { Text: "date_added:desc" },
        },
      },
    ]);
  });

  it("the Updates section shows the action for each state", () => {
    const cases: [UpdateStatus, string, string | null][] = [
      [{ state: "idle" }, "You have the latest version (0.43.0)", "check"],
      [
        { state: "up_to_date" },
        "You have the latest version (0.43.0)",
        "check",
      ],
      [{ state: "available", version: "0.44.0" }, "is available", "download"],
      [
        { state: "downloading", version: "0.44.0", percent: 10 },
        "Downloading version 0.44.0 (10%)",
        null,
      ],
      [{ state: "ready", version: "0.44.0" }, "is ready", "install"],
      [
        { state: "manual", version: "0.44.0" },
        "Download it from the release page",
        null,
      ],
      [{ state: "failed", message: "Network down" }, "Network down", "check"],
    ];
    const labels = {
      check: "Check for updates",
      download: "Download",
      install: "Restart to update",
    };
    for (const [status, text, ask] of cases) {
      expect(page(status)).toContain(text);
      const asks: UpdateAsk[] = [];
      const row = (
        <UpdateRow
          status={status}
          version="0.43.0"
          onUpdate={(next) => asks.push(next)}
        />
      );
      if (ask === null) {
        expect(renderToStaticMarkup(row)).not.toContain("<button");
        continue;
      }
      const label =
        status.state === "failed"
          ? "Try again"
          : labels[ask as keyof typeof labels];
      (byName(row, label).props.onClick as () => void)();
      expect(asks).toEqual([ask]);
    }
    expect(page({ state: "checking" })).toMatch(
      /<button[^>]*disabled=""[^>]*>Check for updates</,
    );
  });
});
