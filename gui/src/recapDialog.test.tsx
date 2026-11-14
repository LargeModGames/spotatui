import { renderToStaticMarkup } from "react-dom/server";
import type { KeyboardEvent } from "react";
import { describe, expect, it } from "vitest";
import type { Action } from "./bindings/Action";
import type { RecapPrompt } from "./bindings/RecapPrompt";
import { RecapModal } from "./RecapDialog";
import { byName, hostElements } from "./testTree";

const prompt: RecapPrompt = {
  title: "2026 Recap",
  ready: "Your 2026 listening recap is ready! 🎉",
  listens: "321 listens made it into your 2026 recap.",
  browser: "Open it in the browser to view and download your share card.",
  open: "Open",
  later: "Later",
  disable: "Don't show this again",
};

const dialog = (value: RecapPrompt | null, sent: Action[] = []) => (
  <RecapModal prompt={value} send={(action) => sent.push(action)} />
);

describe("RecapDialog", () => {
  it("renders nothing without a recap that waits", () => {
    expect(renderToStaticMarkup(dialog(null))).toBe("");
  });

  it("is a labelled modal with the recap lines", () => {
    const html = renderToStaticMarkup(dialog(prompt));
    expect(html).toContain('aria-modal="true"');
    expect(html).toContain('<h2 id="recap-title">2026 Recap</h2>');
    expect(html).toContain("321 listens made it into your 2026 recap.");
  });

  it("answers Open, Later and Don't show this again, and Escape answers Later", () => {
    const sent: Action[] = [];
    const tree = dialog(prompt, sent);
    for (const name of ["Open", "Later", "Don't show this again"]) {
      (byName(tree, name).props.onClick as () => void)();
    }
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
      "OpenRecap",
      "DismissRecapPrompt",
      "DisableRecapPrompt",
      "DismissRecapPrompt",
    ]);
  });
});
