import { renderToStaticMarkup } from "react-dom/server";
import type { KeyboardEvent } from "react";
import { describe, expect, it } from "vitest";
import {
  AnnouncementDialog,
  AnnouncementLevelIcon,
} from "./AnnouncementDialog";
import type { Action } from "./bindings/Action";
import type { ActiveAnnouncement } from "./bindings/ActiveAnnouncement";
import { EN } from "./i18n";
import { byName, hostElements } from "./testTree";

const active = (
  patch: Partial<ActiveAnnouncement["entry"]> = {},
  position = 1,
  total = 1,
): ActiveAnnouncement => ({
  entry: {
    id: "v1-notice",
    title: "New release",
    body: "spotatui 1.0 is out.",
    level: "info",
    url: "https://spotatui.com",
    ...patch,
  },
  position,
  total,
});

const dialog = (value: ActiveAnnouncement | null, sent: Action[] = []) => (
  <AnnouncementDialog active={value} send={(action) => sent.push(action)} />
);

const click = (tree: ReturnType<typeof dialog>, name: string) =>
  (byName(tree, name).props.onClick as () => void)();

describe("AnnouncementDialog", () => {
  it("renders nothing without an active announcement", () => {
    expect(renderToStaticMarkup(dialog(null))).toBe("");
  });

  it("is a labelled modal that focuses Got it", () => {
    const html = renderToStaticMarkup(dialog(active()));
    expect(html).toContain('role="dialog"');
    expect(html).toContain('aria-modal="true"');
    expect(html).toContain('aria-labelledby="announcement-title"');
    expect(html).toContain('<h2 id="announcement-title">New release</h2>');
    const gotIt = byName(dialog(active()), EN.announcement_got_it);
    expect(gotIt.props.autoFocus).toBe(true);
  });

  it("sends the id with Got it, Open and Escape", () => {
    const sent: Action[] = [];
    const tree = dialog(active(), sent);
    click(tree, EN.announcement_got_it);
    click(tree, EN.announcement_open);
    const modal = hostElements(tree).find(
      (element) => element.props.role === "dialog",
    )!;
    const escape = {
      key: "Escape",
      preventDefault: () => {},
      stopPropagation: () => {},
    } as unknown as KeyboardEvent<HTMLDivElement>;
    (modal.props.onKeyDown as (event: KeyboardEvent<HTMLDivElement>) => void)(
      escape,
    );
    expect(sent).toEqual([
      { DismissAnnouncement: { id: "v1-notice" } },
      { OpenAnnouncementUrl: { id: "v1-notice" } },
      { DismissAnnouncement: { id: "v1-notice" } },
    ]);
  });

  it("offers no Open without a url and no counter for a batch of one", () => {
    const html = renderToStaticMarkup(dialog(active({ url: null })));
    expect(html).not.toContain(EN.announcement_open);
    expect(html).not.toContain("announcement-position");
  });

  it("counts its place in a batch", () => {
    expect(renderToStaticMarkup(dialog(active({}, 2, 2)))).toContain(
      '<span class="announcement-position">2 of 2</span>',
    );
  });

  it("makes a critical announcement an alert dialog in its own tone", () => {
    const html = renderToStaticMarkup(dialog(active({ level: "critical" })));
    expect(html).toContain('role="alertdialog"');
    expect(html).toContain('class="dialog announcement-dialog critical"');
  });

  it("gives each level its own icon class", () => {
    expect(
      renderToStaticMarkup(<AnnouncementLevelIcon level="warning" />),
    ).toContain('class="announcement-icon warning"');
  });
});
