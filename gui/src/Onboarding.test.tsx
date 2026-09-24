import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { OnboardingAsk } from "./bindings/OnboardingAsk";
import { Onboarding } from "./Onboarding";

const render = (ask: OnboardingAsk, transcript = "") =>
  renderToStaticMarkup(
    <Onboarding
      view={{ transcript, pending: { seq: 0, ask } }}
      onReply={() => {}}
    />,
  );

describe("Onboarding", () => {
  it("masks a secret prompt", () => {
    const html = render({ kind: "Line", prompt: "Password: ", secret: true });
    expect(html).toContain('type="password"');
    expect(html).toContain("Password: ");
  });

  it("shows the transcript and a yes/no question", () => {
    const html = render(
      {
        kind: "Confirm",
        title: "Song counter",
        body: "Count plays?",
        question: "Join?",
      },
      "Welcome\n",
    );
    expect(html).toContain("Welcome");
    expect(html).toContain("Join?");
    expect(html).toContain(">Yes<");
    expect(html).toContain(">No<");
  });

  it("lists every offered source as a checkbox", () => {
    const html = render({ kind: "PickSources", options: ["Spotify", "Local"] });
    expect(html.match(/type="checkbox"/g)).toHaveLength(2);
    expect(html).toContain("Local");
  });
});
