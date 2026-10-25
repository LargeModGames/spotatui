import { renderToStaticMarkup } from "react-dom/server";
import type { KeyboardEvent } from "react";
import { describe, expect, it } from "vitest";
import type { Action } from "./bindings/Action";
import type { SoloistPairingGuide } from "./bindings/SoloistPairingGuide";
import { SoloistPairingModal } from "./SoloistPairingDialog";
import { byName, hostElements } from "./testTree";

const guide: SoloistPairingGuide = {
  title: "Pair spotatui with Spotify",
  steps: [
    "1. Open Spotify on a computer on this network.",
    "2. In its device menu, pick 'spotatui'.",
  ],
  waiting: "Waiting for Spotify…",
  close: "Close",
};

const dialog = (value: SoloistPairingGuide | null, sent: Action[] = []) => (
  <SoloistPairingModal guide={value} send={(action) => sent.push(action)} />
);

describe("SoloistPairingDialog", () => {
  it("renders nothing while Soloist needs no pairing", () => {
    expect(renderToStaticMarkup(dialog(null))).toBe("");
  });

  it("shows the steps and the live wait in a labelled modal", () => {
    const html = renderToStaticMarkup(dialog(guide));
    expect(html).toContain('aria-labelledby="soloist-pairing-title"');
    expect(html).toContain("pick &#x27;spotatui&#x27;");
    expect(html).toContain('<p role="status">Waiting for Spotify…</p>');
  });

  it("closes with Close and with Escape", () => {
    const sent: Action[] = [];
    const tree = dialog(guide, sent);
    (byName(tree, "Close").props.onClick as () => void)();
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
    expect(sent).toEqual(["DismissSoloistPairing", "DismissSoloistPairing"]);
  });
});
