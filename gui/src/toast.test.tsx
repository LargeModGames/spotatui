import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { StatusPayload } from "./bindings/StatusPayload";
import { Toast } from "./Toast";

const toast = (status: StatusPayload | null) =>
  renderToStaticMarkup(<Toast status={status} route="home" send={() => {}} />);

describe("Toast", () => {
  it("shows the api error before the status message", () => {
    const html = toast({
      message: "Added to queue",
      is_error: false,
      api_error: "Spotify refused the request",
    });
    expect(html).toContain('role="alert"');
    expect(html).toContain("Spotify refused the request");
    expect(html).not.toContain("Added to queue");
  });

  it("marks an error status message and renders nothing without one", () => {
    expect(
      toast({ message: "No device", is_error: true, api_error: null }),
    ).toContain('class="toast error" role="status"');
    expect(toast({ message: null, is_error: false, api_error: null })).toBe("");
    expect(toast(null)).toBe("");
  });
});
