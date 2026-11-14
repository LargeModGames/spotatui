import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { Action } from "./bindings/Action";
import type { DiscoverPayload } from "./bindings/DiscoverPayload";
import type { SettingRow } from "./bindings/SettingRow";
import { Discover } from "./Discover";
import { Home } from "./Home";
import { EN, ZH_HANS, ZH_HANT } from "./i18n";
import { SettingControl, Settings } from "./Settings";
import { SourceBadge } from "./SourceBadge";

const discover: DiscoverPayload = {
  available: true,
  loading: false,
  shelves: [
    {
      title: "中文栏目 {title}",
      playlists: [
        {
          uri: "spotify:playlist:p",
          name: "用户歌单 {name}",
          description: null,
          image_url: null,
        },
      ],
    },
  ],
};

describe("main additions share the active language table", () => {
  it("renders Made For You and its card controls while keeping Spotify shelf and playlist names literal", () => {
    for (const s of [EN, ZH_HANS, ZH_HANT]) {
      const html = renderToStaticMarkup(
        <Discover
          s={s}
          discover={discover}
          connected={false}
          active={false}
          open={() => {}}
          send={() => {}}
        />,
      );
      expect(html).toContain(s.discover_intro);
      expect(html).toContain(s.by_spotify);
      expect(html).toContain("中文栏目 {title}");
      expect(html).toContain("用户歌单 {name}");
      expect(html).toContain(s.play_named.replace("{name}", "用户歌单 {name}"));
      const home = renderToStaticMarkup(
        <Home
          s={s}
          welcome={null}
          changelog={null}
          connected={false}
          active={false}
          announcements={[]}
          askChangelog={() => {}}
          send={() => {}}
        />,
      );
      expect(home).toContain(s.changelog);
      expect(home).toContain(s.loading_changelog);
    }
    expect(EN.discover_intro).toBe("Made For You, from Spotify.");
  });

  it("localizes Sources and password placeholders without exposing a saved password or changing submitted text", () => {
    for (const s of [EN, ZH_HANS, ZH_HANT]) {
      for (const set of [false, true]) {
        const row: SettingRow = {
          id: "behavior.subsonic_password",
          group: "sources",
          name: "Subsonic",
          description: "",
          button: null,
          value: { kind: "secret", set },
        };
        const html = renderToStaticMarkup(
          <Settings
            s={s}
            rows={[row]}
            update={null}
            version={null}
            send={() => {}}
            onUpdate={() => {}}
          />,
        );
        expect(html).toContain(`>${s.sources}</h2>`);
        expect(html).toContain(
          `placeholder="${set ? s.secret_saved : s.secret_not_set}"`,
        );
        expect(html).not.toMatch(/type="password"[^>]*value=/);
        const actions: Action[] = [];
        const control = SettingControl({
          s,
          row,
          send: (action) => actions.push(action),
        });
        (control.props.onCommit as (draft: string) => void)("{secret} 原样");
        expect(actions).toEqual([
          { SetSetting: { id: row.id, value: { Text: "{secret} 原样" } } },
        ]);
      }
      const badge = renderToStaticMarkup(<SourceBadge s={s} source="Local" />);
      expect(badge).toContain('class="source-icon local"');
      expect(badge).toContain(s.source_local_badge);
    }
  });
});
