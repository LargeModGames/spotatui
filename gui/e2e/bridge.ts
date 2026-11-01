import { test, type Page } from "@playwright/test";
import type { ClientMessage } from "../src/bindings/ClientMessage";
import type { ServerMessage } from "../src/bindings/ServerMessage";
import { STYLE_KEY } from "../src/screenModel";

/** Opens the page with a launch code and answers its socket with `messages`, in order. */
export async function openWith(
  page: Page,
  messages: ServerMessage[],
): Promise<void> {
  await page.routeWebSocket("**/ws", (ws) => {
    for (const message of messages) ws.send(JSON.stringify(message));
  });
  await page.goto("/#code=shot");
}

/** A page asking for visualizer frames, or for the screen's at a band count. */
type FrameAsk = { on: boolean; bars?: number };

/** Like `openWith`, and while the page asks with `type`, sends it the frame for that ask again and again. */
async function openStreaming(
  page: Page,
  messages: ServerMessage[],
  type: "visualizer" | "visualizer_screen",
  frame: (ask: FrameAsk) => ServerMessage,
): Promise<void> {
  await page.routeWebSocket("**/ws", (ws) => {
    let frames: ReturnType<typeof setInterval> | undefined;
    ws.onMessage((text) => {
      const message = JSON.parse(String(text)) as ClientMessage;
      if (message.type !== type) return;
      clearInterval(frames);
      const ask = message as FrameAsk;
      if (ask.on) {
        const next = JSON.stringify(frame(ask));
        frames = setInterval(() => ws.send(next), 30);
      }
    });
    ws.onClose(() => clearInterval(frames));
    for (const message of messages) ws.send(JSON.stringify(message));
  });
  await page.goto("/#code=shot");
}

/** Waits until the first frame lands on the canvas `selector` names. */
async function painted(page: Page, selector: string): Promise<void> {
  await page.waitForFunction((selector) => {
    const canvas = document.querySelector<HTMLCanvasElement>(selector);
    const pixels = canvas
      ?.getContext("2d")
      ?.getImageData(0, 0, canvas.width, canvas.height).data;
    return (
      pixels?.some((value, index) => index % 4 === 3 && value > 0) ?? false
    );
  }, selector);
}

/** Like `openWith`, and while the page asks for visualizer frames, sends it `frame` again and again. */
export async function openWithVisualizer(
  page: Page,
  messages: ServerMessage[],
  frame: ServerMessage,
): Promise<void> {
  await openStreaming(page, messages, "visualizer", () => frame);
  await painted(page, ".playerbar-viz");
}

/** Opens the visualizer screen in `style` and answers each ask with `frame` at the band count it asked for. */
export async function openScreen(
  page: Page,
  messages: ServerMessage[],
  style: string,
  frame: (bars: number) => ServerMessage,
): Promise<void> {
  await page.addInitScript(
    ([key, value]) => localStorage.setItem(key, value),
    [STYLE_KEY, style],
  );
  await openStreaming(page, messages, "visualizer_screen", (ask) =>
    frame(ask.bars ?? 0),
  );
  await page.getByRole("button", { name: "Visualizer", exact: true }).click();
  await painted(page, ".viz-screen-canvas");
  // Needles, meters and peak caps ease in over a few hundred ms.
  await page.waitForTimeout(800);
}

/** Saves the page as `shots/<browser>/<name>.png` once its fonts have loaded. */
export async function shot(page: Page, name: string): Promise<void> {
  await page.evaluate(() => document.fonts.ready);
  await page.screenshot({
    path: `shots/${test.info().project.name}/${name}.png`,
  });
}
