import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { DeviceInfo } from "./bindings/DeviceInfo";
import { DevicesPopover } from "./DevicesPopover";

const device = (
  name: string,
  id: string | null,
  is_active = false,
): DeviceInfo => ({
  id,
  name,
  kind: "speaker",
  is_active,
  volume_percent: 50,
});

const popover = (devices: DeviceInfo[], current: string | null) =>
  renderToStaticMarkup(
    <DevicesPopover
      devices={devices}
      current={current}
      send={() => {}}
      onClose={() => {}}
    />,
  );

describe("DevicesPopover", () => {
  it("is a dialog that shows the current device before the others", () => {
    const html = popover(
      [device("Kitchen", "k"), device("This PC", "pc", true)],
      "This PC",
    );
    expect(html).toMatch(/role="dialog" aria-label="Devices"/);
    expect(html).toContain("Connect to a device");
    const current = html.indexOf("Current device");
    expect(current).toBeGreaterThan(-1);
    expect(html.indexOf("This PC")).toBeGreaterThan(current);
    expect(html.indexOf("Kitchen")).toBeGreaterThan(html.indexOf("This PC"));
    expect(html.match(/This PC/g)).toHaveLength(1);
  });

  it("disables a device that has no id to transfer to", () => {
    const html = popover([device("Ghost", null)], null);
    expect(html).toMatch(/<button[^>]*disabled=""[^>]*>.*Ghost/);
    expect(html).not.toContain("Current device");
  });

  it("says so when there is no other device", () => {
    const html = popover([device("This PC", "pc", true)], "This PC");
    expect(html).toContain("No other devices found.");
  });
});
