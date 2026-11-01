import { test, type Page } from "@playwright/test";
import type { ClientMessage } from "../src/bindings/ClientMessage";
import type { ServerMessage } from "../src/bindings/ServerMessage";

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

/** Like `openWith`, and while the page asks for visualizer frames, sends it `frame` again and again. */
export async function openWithVisualizer(
  page: Page,
  messages: ServerMessage[],
  frame: ServerMessage,
): Promise<void> {
  await page.routeWebSocket("**/ws", (ws) => {
    let frames: ReturnType<typeof setInterval> | undefined;
    ws.onMessage((text) => {
      const message = JSON.parse(String(text)) as ClientMessage;
      if (message.type !== "visualizer") return;
      clearInterval(frames);
      if (message.on)
        frames = setInterval(() => ws.send(JSON.stringify(frame)), 30);
    });
    ws.onClose(() => clearInterval(frames));
    for (const message of messages) ws.send(JSON.stringify(message));
  });
  await page.goto("/#code=shot");
  // Drawn once the first frame lands on the canvas.
  await page.waitForFunction(() => {
    const canvas = document.querySelector<HTMLCanvasElement>(".playerbar-viz");
    const pixels = canvas
      ?.getContext("2d")
      ?.getImageData(0, 0, canvas.width, canvas.height).data;
    return (
      pixels?.some((value, index) => index % 4 === 3 && value > 0) ?? false
    );
  });
}

/** Opens the visualizer screen in `style` and answers each ask with `frame` at the band count it asked for. */
export async function openScreen(
  page: Page,
  messages: ServerMessage[],
  style: string,
  frame: (bars: number) => ServerMessage,
): Promise<void> {
  await page.addInitScript(
    (value) => localStorage.setItem("spotatui.visualizer_style", value),
    style,
  );
  await page.routeWebSocket("**/ws", (ws) => {
    let frames: ReturnType<typeof setInterval> | undefined;
    ws.onMessage((text) => {
      const message = JSON.parse(String(text)) as ClientMessage;
      if (message.type !== "visualizer_screen") return;
      clearInterval(frames);
      if (message.on) {
        const next = JSON.stringify(frame(message.bars));
        frames = setInterval(() => ws.send(next), 30);
      }
    });
    ws.onClose(() => clearInterval(frames));
    for (const message of messages) ws.send(JSON.stringify(message));
  });
  await page.goto("/#code=shot");
  await page.getByRole("button", { name: "Visualizer", exact: true }).click();
  await page.waitForFunction(() => {
    const canvas =
      document.querySelector<HTMLCanvasElement>(".viz-screen-canvas");
    const pixels = canvas
      ?.getContext("2d")
      ?.getImageData(0, 0, canvas.width, canvas.height).data;
    return (
      pixels?.some((value, index) => index % 4 === 3 && value > 0) ?? false
    );
  });
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
