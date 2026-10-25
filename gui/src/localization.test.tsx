import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { Action } from "./bindings/Action";
import type { SettingRow } from "./bindings/SettingRow";
import strings from "./bindings/strings.json";
import { EN, ZH_HANS, ZH_HANT, fmt } from "./i18n";
import { Onboarding } from "./Onboarding";
import { SettingControl } from "./Settings";
import { SourceBadge } from "./SourceBadge";
import { byName } from "./testTree";
import { TopBar } from "./TopBar";
import { songCount } from "./libraryModel";
import { sourceOf } from "./format";
import { parse } from "./commandModel";

describe("shared browser strings", () => {
  it("generates exactly the Rust-serialized tables, including English punctuation and spaces", () => {
    expect(EN).toEqual(strings.EN.gui);
    expect(ZH_HANS).toEqual(strings.ZH_HANS.gui);
    expect(ZH_HANT).toEqual(strings.ZH_HANT.gui);
    expect(EN.no_search_results).toBe("No results found for “{query}”.");
    expect(EN.rank_climbed).toBe(
      "{name} climbed from #{from} all time to {highlight} {period}.",
    );
    expect(EN.update_checks_description).toBe(
      "Skip the checks at launch and every 6 hours; check, download and install updates from this page yourself",
    );
  });

  it("preserves every placeholder in the generated translations", () => {
    const keys = (text: string) =>
      [...text.matchAll(/\{([^{}]+)\}/g)].map((match) => match[1]).sort();
    for (const key of Object.keys(EN) as (keyof typeof EN)[]) {
      expect(keys(ZH_HANS[key]), key).toEqual(keys(EN[key]));
      expect(keys(ZH_HANT[key]), key).toEqual(keys(EN[key]));
    }
  });

  it("never replaces placeholder-looking text inside a user value", () => {
    expect(fmt("{name}: {count}", { name: "{count}", count: 2 })).toBe(
      "{count}: 2",
    );
    expect(fmt("{unknown}", {})).toBe("{unknown}");
  });

  it("renders active accessibility text and unchanged product and command ids", () => {
    for (const s of [ZH_HANS, ZH_HANT]) {
      const html = renderToStaticMarkup(
        <TopBar
          s={s}
          page="home"
          canBack={false}
          canForward={false}
          onBack={() => {}}
          onForward={() => {}}
          onPage={() => {}}
          query=""
          onQuery={() => {}}
          onRun={() => {}}
          device={null}
          connected={true}
          update={null}
          onUpdate={() => {}}
        />,
      );
      expect(html).toContain(`aria-label="${s.search}"`);
      expect(html).toContain(`placeholder="${s.search_placeholder}"`);
      expect(html).toContain(`aria-label="${s.settings}"`);
      expect(
        renderToStaticMarkup(<SourceBadge s={s} source="Local" />),
      ).toContain(s.source_local_badge);
      expect(
        renderToStaticMarkup(<SourceBadge s={s} source="Spotify" />),
      ).toContain("SPOTIFY");
      expect(songCount(2, s)).toBe(fmt(s.songs_count, { count: 2 }));
    }
    expect(sourceOf("file:///a.flac")).toBe("Local");
    expect(parse("source Local")).toEqual({
      verb: "source",
      kind: null,
      arg: "Local",
    });
  });

  it("can show the first question with only the Hello table, before Settings exists", () => {
    const html = renderToStaticMarkup(
      <Onboarding
        s={strings.ZH_HANT.gui}
        connected
        view={{
          transcript: "Spotify",
          pending: { seq: 1, ask: { kind: "PickSources", options: [] } },
        }}
        onReply={() => {}}
      />,
    );
    expect(html).toContain(strings.ZH_HANT.gui.choose_sources);
    expect(html).toContain(strings.ZH_HANT.gui.continue_button);
    expect(html).toContain("Spotify");
  });

  it("translates a choice label while submitting the raw option", () => {
    const actions: Action[] = [];
    const row: SettingRow = {
      id: "behavior.language",
      group: "interface",
      name: "語言",
      description: "",
      button: null,
      value: {
        kind: "choice",
        value: "en",
        options: ["en", "zh-Hant"],
        labels: ["English", "繁體中文"],
      },
    };
    const control = SettingControl({
      row,
      send: (action) => actions.push(action),
    });
    expect(renderToStaticMarkup(control)).toContain('value="zh-Hant">繁體中文');
    expect(byName(control, "繁體中文").props.value).toBe("zh-Hant");
    (
      control.props.onChange as (event: {
        currentTarget: { value: string };
      }) => void
    )({ currentTarget: { value: "zh-Hant" } });
    expect(actions).toEqual([
      { SetSetting: { id: "behavior.language", value: { Text: "zh-Hant" } } },
    ]);
  });
});
