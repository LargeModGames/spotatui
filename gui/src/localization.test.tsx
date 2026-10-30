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
import { headline as partyHeadline } from "./partyModel";
import { songCount } from "./libraryModel";
import { sourceOf } from "./format";
import { parse } from "./commandModel";
import { sentence } from "./statsModel";

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

  it("places the period before the Chinese ranking sentence and keeps literal artist braces", () => {
    expect(
      sentence(
        { name: "{period} 五月天", kind: "climb", from: 9, to: 4 },
        "30d",
        ZH_HANS,
      ).join(""),
    ).toBe("在最近 30 天，{period} 五月天 从总榜第 9 升至#4。");
    expect(
      sentence(
        { name: "五月天", kind: "new", from: null, to: 4 },
        "30d",
        ZH_HANT,
      ).join(""),
    ).toBe("在最近 30 天，五月天 首次進入你的前十名。");
  });

  it("never replaces placeholder-looking text inside a user value", () => {
    expect(fmt(ZH_HANS.play_named, { name: "{count} <&> 五月天" })).toBe(
      "播放 {count} <&> 五月天",
    );
    expect(fmt("{name}: {count}", { name: "{count}", count: 2 })).toBe(
      "{count}: 2",
    );
    expect(fmt("{unknown}", {})).toBe("{unknown}");
  });

  it("renders both scripts, localized accessibility text, and unchanged product and command ids", () => {
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
      expect(songCount(2, s)).toBe("2 首歌");
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
    expect(html).toContain("選擇音源");
    expect(html).toContain("繼續");
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

  it("keeps a room host's literal braces and code intact", () => {
    const line = partyHeadline(
      {
        phase: "joined",
        available: true,
        room: {
          host: false,
          host_name: "{code}",
          code: "ABC123",
          guests: [],
          shared_control: false,
        },
      },
      ZH_HANT,
    );
    expect(line).toBe("你正在跟隨 {code} · 房間 ABC123");
  });
});
