import { describe, expect, it } from "vitest";
import type { PartyRoom } from "./bindings/PartyRoom";
import {
  headline,
  joinRequest,
  listeners,
  normalizeCode,
  relaying,
} from "./partyModel";

const room = (patch: Partial<PartyRoom> = {}): PartyRoom => ({
  host: true,
  code: "ABC123",
  host_name: "Host",
  guests: ["Alex", "Sam"],
  shared_control: false,
  ...patch,
});

describe("partyModel", () => {
  it("keeps six ASCII letters and digits of a code", () => {
    expect(normalizeCode("ab-c1 23xyz")).toBe("ABC123");
    expect(normalizeCode("äbc")).toBe("BC");
  });

  it("joins only with a full code and a name", () => {
    expect(joinRequest("ABC123", " Sam ")).toEqual({
      JoinParty: { code: "ABC123", name: "Sam" },
    });
    expect(joinRequest("ABC12", "Sam")).toBeNull();
    expect(joinRequest("ABC123", "  ")).toBeNull();
  });

  it("counts the host in the headline and names the room a guest follows", () => {
    expect(headline({ phase: "hosting", room: room(), available: true })).toBe(
      "You host · 3 listening",
    );
    expect(
      headline({
        phase: "joined",
        room: room({ host: false, host_name: "", guests: [] }),
        available: true,
      }),
    ).toBe("You follow the host · room ABC123");
  });

  it("lists you first, then the host for a guest", () => {
    const names = listeners(room({ host: false, guests: ["Sam"] })).map(
      (listener) => `${listener.name}:${listener.role}`,
    );
    expect(names).toEqual(["You:guest", "Host:host", "Sam:guest"]);
  });

  it("relays only a Spotify track outside the native queue", () => {
    expect(relaying(room(), false, "spotify:track:1")).toBe(true);
    expect(relaying(room(), true, "spotify:track:1")).toBe(false);
    expect(relaying(room(), false, "spotify:local:a:b:c:1")).toBe(false);
    expect(relaying(room(), false, "file:///a.flac")).toBe(false);
    expect(relaying(room({ host: false }), false, "spotify:track:1")).toBe(
      false,
    );
  });
});
