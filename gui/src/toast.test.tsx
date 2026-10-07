import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { StatusPayload } from "./bindings/StatusPayload";
import { Toast } from "./Toast";

const toast = (status: StatusPayload | null) =>
  renderToStaticMarkup(<Toast status={status} route="home" send={() => {}} />);

describe("Toast", () => {
  it("shows the api error before the status message and announces it", () => {
    const html = toast({
      message: "Added to queue",
      is_error: false,
      api_error: "Spotify refused the request",
    });
    expect(html).toContain(
      '<div class="sr-only" role="alert">Spotify refused the request</div>',
    );
    expect(html).toContain('class="toast error"');
    expect(html).not.toContain("Added to queue");
  });

  it("marks an error status message and keeps empty live regions without one", () => {
    expect(
      toast({ message: "No device", is_error: true, api_error: null }),
    ).toContain('<div class="toast error"><span>No device</span>');
    const empty = toast(null);
    expect(empty).toContain('role="status"></div>');
    expect(empty).not.toContain("toast");
  });
});
