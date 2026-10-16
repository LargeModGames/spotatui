import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import type { AnnouncementEntry } from "./bindings/AnnouncementEntry";
import type { WelcomePayload } from "./bindings/WelcomePayload";
import { AnnouncementHistory, Home, WelcomeView } from "./Home";
import { EN } from "./i18n";
import { byName } from "./testTree";

const welcome = (over: Partial<WelcomePayload> = {}): WelcomePayload => ({
  streak: { days: "3-day listening streak", note: " · 25m today" },
  counter: "Global songs played with spotatui: 42",
  discord: "Join the community on Discord: https://spotatui.com/discord",
  banner: "\n ███\n ╚═╝\n",
  banner_gradient: true,
  ...over,
});

const changelog = [1, 2, 3, 4, 5]
  .map(
    (n) =>
      `## [v0.${n}.0] 2026-0${n}-01\n\n### Added\n\n- **Feature ${n}** ([#${n}](https://github.com/LargeModGames/spotatui/issues/${n})).\n`,
  )
  .reverse()
  .join("\n");

const render = (
  props: Partial<Parameters<typeof WelcomeView>[0]> = {},
): string =>
  renderToStaticMarkup(
    <WelcomeView
      s={EN}
      welcome={welcome()}
      animate
      releases={null}
      shown={3}
      onOlder={() => {}}
      onLink={() => {}}
      {...props}
    />,
  );

describe("Home", () => {
  it("needs no Spotify data: the banner and lines come from the welcome message", () => {
    const html = renderToStaticMarkup(
      <Home
        welcome={welcome()}
        changelog={null}
        connected={false}
        active={false}
        announcements={[]}
        askChangelog={() => {}}
        send={() => {}}
      />,
    );
    expect(html).toMatch(/<section[^>]*data-focus="true"[^>]*tabindex="-1"/);
    expect(html).toContain('aria-label="spotatui"');
    expect(html).toContain("Loading the changelog…");
  });

  it("draws the banner without its outer blank lines and animates only when asked", () => {
    expect(render()).toContain(
      '<h1 class="welcome-banner animated" aria-label="spotatui"> ███\n ╚═╝</h1>',
    );
    expect(render({ animate: false })).toContain(
      '<h1 class="welcome-banner" aria-label="spotatui">',
    );
  });

  it("shows the streak, the counter and the Discord link under the banner", () => {
    const html = render();
    expect(html).toContain("3-day listening streak");
    expect(html).toContain(" · 25m today");
    expect(html).toContain("Global songs played with spotatui: 42");
    expect(html).toContain(
      '<a href="https://spotatui.com/discord">https://spotatui.com/discord</a>',
    );
  });

  it("leaves the streak out without one", () => {
    expect(render({ welcome: welcome({ streak: null }) })).not.toContain(
      "welcome-streak",
    );
  });

  it("shows the newest three releases and a Show older button", () => {
    const html = render({ releases: changelog.split(/\n(?=## )/) });
    expect(html).toContain("<h3>[v0.5.0] 2026-05-01</h3>");
    expect(html).toContain("<h3>[v0.3.0] 2026-03-01</h3>");
    expect(html).not.toContain("v0.2.0");
    expect(html).toContain("<h4>Added</h4>");
    expect(html).toContain("<strong>Feature 5</strong>");
    expect(html).toContain(">Show older</button>");
  });

  it("drops the Show older button when every release shows", () => {
    const html = render({
      releases: changelog.split(/\n(?=## )/),
      shown: 6,
    });
    expect(html).toContain("v0.1.0");
    expect(html).not.toContain("Show older");
  });

  it("asks for older releases and sends a link to the server instead of opening it", () => {
    const onOlder = vi.fn();
    const onLink = vi.fn();
    const tree = (
      <WelcomeView
        s={EN}
        welcome={welcome()}
        animate={false}
        releases={changelog.split(/\n(?=## )/)}
        shown={3}
        onOlder={onOlder}
        onLink={onLink}
      />
    );
    (byName(tree, "Show older").props.onClick as () => void)();
    expect(onOlder).toHaveBeenCalledOnce();

    const preventDefault = vi.fn();
    (
      byName(tree, "#5").props.onClick as (event: {
        preventDefault: () => void;
      }) => void
    )({ preventDefault });
    expect(preventDefault).toHaveBeenCalledOnce();
    expect(onLink).toHaveBeenCalledWith(
      "https://github.com/LargeModGames/spotatui/issues/5",
    );
  });
});

const entry = (
  id: string,
  level: AnnouncementEntry["level"],
  url: string | null,
): AnnouncementEntry => ({
  id,
  title: `Title ${id}`,
  body: `Body ${id}\nsecond line`,
  level,
  url,
});

describe("AnnouncementHistory", () => {
  const history = (
    entries: AnnouncementEntry[],
    shown = 3,
    onOlder = () => {},
    onOpen = (id: string) => void id,
  ) => (
    <AnnouncementHistory
      s={EN}
      entries={entries}
      shown={shown}
      onOlder={onOlder}
      onOpen={onOpen}
    />
  );

  it("is hidden without past announcements", () => {
    expect(renderToStaticMarkup(history([]))).toBe("");
  });

  it("shows each level icon, title, full body and an Open only with a link", () => {
    const html = renderToStaticMarkup(
      history([
        entry("a", "critical", "https://spotatui.com/news"),
        entry("b", "info", null),
      ]),
    );
    expect(html).toContain('<h2 class="welcome-heading">Announcements</h2>');
    expect(html).toContain('class="announcement-icon critical"');
    expect(html).toContain('class="announcement-icon info"');
    expect(html).toContain("Title a");
    expect(html).toContain("Body b\nsecond line");
    expect(html.split(">Open</button>").length).toBe(2);
  });

  it("keeps the newest three until Show older and opens a link by id only", () => {
    const onOlder = vi.fn();
    const onOpen = vi.fn();
    const entries = ["a", "b", "c", "d"].map((id) =>
      entry(id, "warning", `https://spotatui.com/${id}`),
    );
    const tree = history(entries, 3, onOlder, onOpen);
    const html = renderToStaticMarkup(tree);
    expect(html).toContain("Title c");
    expect(html).not.toContain("Title d");
    expect(renderToStaticMarkup(history(entries, 6))).not.toContain(
      "Show older",
    );

    (byName(tree, "Show older").props.onClick as () => void)();
    expect(onOlder).toHaveBeenCalledOnce();
    (byName(tree, "Open").props.onClick as () => void)();
    expect(onOpen).toHaveBeenCalledWith("a");
  });

  it("sits between the lines under the banner and the changelog", () => {
    const html = renderToStaticMarkup(
      <WelcomeView
        s={EN}
        welcome={welcome()}
        animate={false}
        releases={null}
        shown={3}
        onOlder={() => {}}
        onLink={() => {}}
        announcements={history([entry("a", "info", null)])}
      />,
    );
    expect(html.indexOf("spotatui.com/discord")).toBeLessThan(
      html.indexOf("Announcements"),
    );
    expect(html.indexOf("Announcements")).toBeLessThan(
      html.indexOf(">Changelog<"),
    );
  });
});
