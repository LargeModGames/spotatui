import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { PartyPayload } from "./bindings/PartyPayload";
import { Party } from "./Party";

const render = (party: PartyPayload | null, connected = true) =>
  renderToStaticMarkup(
    <Party
      party={party}
      connected={connected}
      item={null}
      position={null}
      queue={null}
      active={false}
      send={() => {}}
    />,
  );

const hosting: PartyPayload = {
  phase: "hosting",
  available: true,
  room: {
    host: true,
    code: "ABC123",
    host_name: "Host",
    guests: ["Alex", "Sam"],
    shared_control: false,
  },
};

describe("Party", () => {
  it("offers host and join, disabled without Spotify", () => {
    const html = render({
      phase: "disconnected",
      room: null,
      available: false,
    });
    expect(html).toContain("Start a party");
    expect(html).toContain("Join a party");
    expect(html).toContain("needs a Spotify session");
    expect(html).toMatch(/class="primary" disabled=""/);
  });

  it("waits for the socket before it offers host or join", () => {
    const party: PartyPayload = {
      phase: "disconnected",
      room: null,
      available: true,
    };
    expect(render(party)).not.toMatch(/class="primary" disabled=""/);
    expect(render(party, false)).toMatch(/class="primary" disabled=""/);
  });

  it("shows the code, the listeners and the guest control to the host", () => {
    const html = render(hosting);
    expect(html).toContain("You host · 3 listening");
    expect(html).toContain("ABC123");
    expect(html).toContain("<b>Alex</b>");
    expect(html).toContain("Guests can control playback");
    expect(html).toContain("End party");
  });

  it("tells a guest the host picks the songs and offers no control", () => {
    const html = render({
      ...hosting,
      phase: "joined",
      room: { ...hosting.room!, host: false, guests: [] },
    });
    expect(html).toContain("The host picks the songs.");
    expect(html).toContain("Leave party");
    expect(html).not.toContain("Guests can control playback");
  });
});
