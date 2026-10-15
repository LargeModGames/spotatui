import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { Action } from "./bindings/Action";
import { LyricText } from "./LyricText";
import { byName } from "./testTree";

const line = { at_ms: 4_000, text: "Reaching a fever pitch" };

const click = (connected: boolean) => {
  const sent: Action[] = [];
  const button = byName(
    <LyricText
      line={line}
      synced
      tabStop
      connected={connected}
      send={(action) => sent.push(action)}
    />,
    "Reaching a fever pitch",
  );
  (button.props.onClick as () => void)();
  return sent;
};

describe("LyricText", () => {
  it("seeks to the start of a synced line when you click it", () => {
    expect(click(true)).toEqual([{ SeekTo: 4_000 }]);
  });

  it("sends nothing while the connection is down", () => {
    expect(click(false)).toEqual([]);
  });

  it("leaves an estimated line as plain text", () => {
    const html = renderToStaticMarkup(
      <LyricText
        line={line}
        synced={false}
        tabStop
        connected
        send={() => {}}
      />,
    );
    expect(html).toBe("Reaching a fever pitch");
  });
});
