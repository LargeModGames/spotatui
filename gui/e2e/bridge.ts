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

/**
 * Like `openWith`, and sends the page the frame `answer` gives for each of its
 * asks again and again: `undefined` leaves a message alone, `null` stops.
 */
async function openStreaming(
  page: Page,
  messages: ServerMessage[],
  answer: (message: ClientMessage) => ServerMessage | null | undefined,
): Promise<void> {
  await page.routeWebSocket("**/ws", (ws) => {
    let frames: ReturnType<typeof setInterval> | undefined;
    ws.onMessage((text) => {
      const frame = answer(JSON.parse(String(text)) as ClientMessage);
      if (frame === undefined) return;
      clearInterval(frames);
      if (frame) {
        const next = JSON.stringify(frame);
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
  await openStreaming(page, messages, (message) =>
    message.type !== "visualizer" ? undefined : message.on ? frame : null,
  );
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
  await openStreaming(page, messages, (message) =>
    message.type !== "visualizer_screen"
      ? undefined
      : message.bars === null
        ? null
        : frame(message.bars),
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
