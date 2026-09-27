import { test, type Page } from "@playwright/test";
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

/** Saves the page as `shots/<browser>/<name>.png` once its fonts have loaded. */
export async function shot(page: Page, name: string): Promise<void> {
  await page.evaluate(() => document.fonts.ready);
  await page.screenshot({
    path: `shots/${test.info().project.name}/${name}.png`,
  });
}
