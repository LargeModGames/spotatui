import { renderToStaticMarkup } from "react-dom/server";
import type { KeyboardEvent } from "react";
import { describe, expect, it } from "vitest";
import type { Action } from "./bindings/Action";
import type { SoloistOffer } from "./bindings/SoloistOffer";
import { SoloistOfferModal } from "./SoloistOfferDialog";
import { byName, hostElements } from "./testTree";

const offer: SoloistOffer = {
  title: "Play through Soloist?",
  body: "Soloist can play them. In Settings:",
  steps: [
    "1. Soloist Path: Download from Spotify.",
    "2. Soloist API Key: Get a key, then enter it.",
    "3. Playback Engine: Soloist.",
  ],
  set_up: "Set up Soloist",
  not_now: "Not now",
};

const dialog = (
  value: SoloistOffer | null,
  sent: Action[] = [],
  opened: string[] = [],
) => (
  <SoloistOfferModal
    offer={value}
    send={(action) => sent.push(action)}
    onSetUp={() => opened.push("settings")}
  />
);

describe("SoloistOfferDialog", () => {
  it("renders nothing without an offer", () => {
    expect(renderToStaticMarkup(dialog(null))).toBe("");
  });

  it("lists the setup steps in a labelled modal", () => {
    const html = renderToStaticMarkup(dialog(offer));
    expect(html).toContain('aria-labelledby="soloist-offer-title"');
    expect(html).toContain("2. Soloist API Key: Get a key, then enter it.");
  });

  it("opens Settings on Set up, and only answers on Not now and Escape", () => {
    const sent: Action[] = [];
    const opened: string[] = [];
    const tree = dialog(offer, sent, opened);
    (byName(tree, "Set up Soloist").props.onClick as () => void)();
    (byName(tree, "Not now").props.onClick as () => void)();
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
      { AnswerSoloistOffer: { set_up: true } },
      { AnswerSoloistOffer: { set_up: false } },
      { AnswerSoloistOffer: { set_up: false } },
    ]);
    expect(opened).toEqual(["settings"]);
  });
});
