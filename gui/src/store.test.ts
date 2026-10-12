import { describe, expect, it } from "vitest";
import { applyChannel, type ChannelMessage } from "./store";

const route = (
  rev: number,
  name = "home",
): Extract<ChannelMessage, { kind: "route" }> => ({
  kind: "route",
  rev,
  payload: name,
});

describe("applyChannel", () => {
  it("applies the first message of a channel at any rev", () => {
    expect(applyChannel({}, route(7)).route?.rev).toBe(7);
  });

  it("applies a message at the same or a higher rev", () => {
    const start = { route: route(3, "home") };
    expect(applyChannel(start, route(3, "queue")).route?.payload).toBe("queue");
    expect(applyChannel(start, route(9, "search")).route?.payload).toBe(
      "search",
    );
  });

  it("keeps a null update payload, the browser build's no updater", () => {
    const start = applyChannel({}, { kind: "update", rev: 2, payload: null });
    expect(start.update?.payload).toBeNull();
    expect(
      applyChannel(start, {
        kind: "update",
        rev: 1,
        payload: { state: "ready", version: "1.0.0" },
      }),
    ).toBe(start);
  });

  it("ignores a message below the stored rev", () => {
    const start = { route: route(3) };
    expect(applyChannel(start, route(2))).toBe(start);
  });
});
