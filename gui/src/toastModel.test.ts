import { describe, expect, it } from "vitest";
import type { StatusPayload } from "./bindings/StatusPayload";
import { toastView } from "./toastModel";

const status = (
  api_error: string | null,
  message: string | null = null,
): StatusPayload => ({ api_error, message, is_error: message !== null });

describe("toastView", () => {
  it("keeps a dismissed error hidden through the handoff, then shows it again", () => {
    // Dismissed while it is the api error.
    let view = toastView(status("No device"), "No device");
    expect(view).toEqual({ held: "No device", error: null, message: null });
    // The expiring error hands the same text to the status line.
    view = toastView(status(null, "No device"), view.held);
    expect(view).toEqual({ held: "No device", error: null, message: null });
    // Both expired: the dismissal ends.
    view = toastView(status(null), view.held);
    expect(view.held).toBeNull();
    // The same failure later shows again.
    expect(toastView(status("No device"), view.held).error).toBe("No device");
  });

  it("shows a different text while an earlier one is dismissed", () => {
    expect(toastView(status("Rate limited", "No device"), "No device")).toEqual(
      {
        held: "No device",
        error: "Rate limited",
        message: null,
      },
    );
  });
});
