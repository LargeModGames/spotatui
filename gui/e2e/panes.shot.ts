import { expect, test, type Page } from "@playwright/test";
import type { ClientMessage } from "../src/bindings/ClientMessage";
import type { GuiLayout } from "../src/bindings/GuiLayout";
import type { PaneKeys } from "../src/bindings/PaneKeys";
import type { ServerMessage } from "../src/bindings/ServerMessage";
import defaults from "../src/bindings/default_layout.json" with { type: "json" };
import { shot } from "./bridge";
import { library, playing, room } from "./fixtures";

const DEFAULT_LAYOUT = defaults as unknown as GuiLayout;

const alt = (code: string) => ({ code, alt: true, ctrl: false, shift: false });

const paneLayout = (
  layout: GuiLayout,
  keys: Partial<PaneKeys> = {},
): ServerMessage => ({
  kind: "pane_layout",
  rev: 2,
  payload: {
    layout,
    keys: {
      detach: alt("KeyD"),
      close: alt("KeyX"),
      focus_left: alt("KeyH"),
      focus_down: alt("KeyJ"),
      focus_up: alt("KeyK"),
      focus_right: alt("KeyL"),
      reset: alt("Digit0"),
      ...keys,
    },
  },
});

/** Opens the page on `layout` and collects the layout messages it sends. */
async function openPanes(
  page: Page,
  layout: GuiLayout,
  keys: Partial<PaneKeys> = {},
): Promise<(GuiLayout | null)[]> {
  const sent: (GuiLayout | null)[] = [];
  await page.routeWebSocket("**/ws", (ws) => {
    ws.onMessage((text) => {
      const message = JSON.parse(String(text)) as ClientMessage;
      if (message.type === "pane_layout") sent.push(message.layout);
    });
    for (const message of [
      ...playing,
      ...library,
      ...room,
      paneLayout(layout, keys),
    ])
      ws.send(JSON.stringify(message));
  });
  await page.goto("/#code=shot");
  await page.getByText("Set Fire to the Rain").first().waitFor();
  return sent;
}

test("lyrics beside the page with the player bar on the left", async ({
  page,
}) => {
  await openPanes(page, {
    tree: {
      axis: "horizontal",
      percent: 67,
      first: "main",
      second: {
        axis: "vertical",
        percent: 60,
        first: "lyrics",
        second: "now_playing",
      },
    },
    floating: [],
    bars: [
      { bar: "top", edge: "top" },
      { bar: "player", edge: "left" },
    ],
  });
  await page.getByText("But I set fire to the rain").waitFor();
  await shot(page, "panes-lyrics-player-left");
});

test("a floating queue over the page with the top bar as a rail", async ({
  page,
}) => {
  await openPanes(page, {
    tree: "main",
    floating: [{ pane: "queue", x: 600, y: 80, width: 300, height: 700 }],
    bars: [
      { bar: "top", edge: "right" },
      { bar: "player", edge: "bottom" },
    ],
  });
  await page.getByRole("region", { name: "Queue" }).waitFor();
  await shot(page, "panes-floating-rail");
});

test("dragging a pane shows the tile it becomes and sends one layout on drop", async ({
  page,
}) => {
  const sent = await openPanes(page, DEFAULT_LAYOUT);
  const head = page.locator('[data-pane="now_playing"] .panel-head h2');
  const from = (await head.boundingBox())!;
  const area = (await page.locator(".pane-area").boundingBox())!;
  await page.mouse.move(from.x + 10, from.y + 10);
  await page.mouse.down();
  // Over the top edge of Main: the pane will lie across the top.
  await page.mouse.move(area.x + 400, area.y + 60, { steps: 8 });
  await expect(page.locator('.drop-preview[data-show="true"]')).toBeVisible();
  await expect(page.locator(".pane-ghost")).toBeVisible();
  await shot(page, "panes-drag-preview");
  await page.mouse.up();

  await expect(page.locator(".drop-preview")).toHaveAttribute(
    "data-show",
    "false",
  );
  expect(sent).toHaveLength(1);
  expect(sent[0]?.tree).toMatchObject({
    axis: "vertical",
    first: "now_playing",
    second: "main",
  });
  const moved = (await page
    .locator('[data-pane="now_playing"]')
    .boundingBox())!;
  expect(moved.width).toBeGreaterThan(area.width * 0.9);
});

test("a drop in the middle of a pane floats it, and Escape cancels a drag", async ({
  page,
}) => {
  const sent = await openPanes(page, DEFAULT_LAYOUT);
  const head = page.locator('[data-pane="now_playing"] .panel-head h2');
  const from = (await head.boundingBox())!;
  const area = (await page.locator(".pane-area").boundingBox())!;

  await page.mouse.move(from.x + 10, from.y + 10);
  await page.mouse.down();
  await page.mouse.move(area.x + 300, area.y + 300, { steps: 8 });
  await page.keyboard.press("Escape");
  await page.mouse.up();
  expect(sent).toHaveLength(0);

  await page.mouse.move(from.x + 10, from.y + 10);
  await page.mouse.down();
  await page.mouse.move(area.x + 300, area.y + 250, { steps: 8 });
  await page.mouse.up();
  expect(sent).toHaveLength(1);
  expect(sent[0]?.tree).toBe("main");
  expect(sent[0]?.floating.map((entry) => entry.pane)).toEqual(["now_playing"]);
  await expect(page.locator('[data-pane="now_playing"]')).toHaveClass(
    /floating/,
  );
});

test("a divider snaps to a third and Alt+0 resets the layout", async ({
  page,
}) => {
  const sent = await openPanes(page, DEFAULT_LAYOUT);
  const divider = page.getByRole("separator");
  const box = (await divider.boundingBox())!;
  const area = (await page.locator(".pane-area").boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + 200);
  await page.mouse.down();
  await page.mouse.move(area.x + area.width * 0.66, box.y + 200, { steps: 6 });
  await page.mouse.up();
  expect(sent).toHaveLength(1);
  expect(sent[0]?.tree).toMatchObject({ percent: 67 });

  await page.keyboard.press("Alt+0");
  expect(sent).toHaveLength(2);
  expect(sent[1]).toBeNull();
});

test("the player bar moves to the edge it is dragged to", async ({ page }) => {
  const sent = await openPanes(page, DEFAULT_LAYOUT);
  const grip = page.locator(".playerbar .bar-grip");
  await page.locator(".playerbar").hover();
  const box = (await grip.boundingBox())!;
  await page.mouse.move(box.x + 5, box.y + 5);
  await page.mouse.down();
  await page.mouse.move(1430, 450, { steps: 8 });
  await expect(page.locator('.bar-preview[data-show="true"]')).toBeVisible();
  await page.mouse.up();
  expect(sent).toHaveLength(1);
  expect(sent[0]?.bars).toEqual([
    { bar: "top", edge: "top" },
    { bar: "player", edge: "right" },
  ]);
  await expect(page.locator(".playerbar")).toHaveAttribute(
    "data-edge",
    "right",
  );
  await shot(page, "panes-player-right");
});

test("the top bar hides Main and opening a page shows it again", async ({
  page,
}) => {
  const sent = await openPanes(page, DEFAULT_LAYOUT);
  const area = (await page.locator(".pane-area").boundingBox())!;
  await page.getByRole("button", { name: "Hide main view" }).click();
  expect(sent).toHaveLength(1);
  expect(sent[0]?.tree).toBe("now_playing");
  await expect(page.locator('[data-pane="main"]')).toBeHidden();
  const alone = (await page
    .locator('[data-pane="now_playing"]')
    .boundingBox())!;
  expect(alone.width).toBeGreaterThan(area.width * 0.9);
  await shot(page, "panes-main-hidden");

  await page.getByRole("button", { name: "Home", exact: true }).click();
  expect(sent).toHaveLength(2);
  expect(sent[1]?.tree).toEqual({
    axis: "horizontal",
    percent: 60,
    first: "main",
    second: "now_playing",
  });
  await expect(page.locator('[data-pane="main"]')).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Hide main view" }),
  ).toBeVisible();
});

test("a layout change made while the socket is closed goes out when it opens again", async ({
  page,
}) => {
  const sent: { socket: number; layout: GuiLayout | null }[] = [];
  const sockets: {
    close: () => Promise<void>;
    send: (text: string) => void;
  }[] = [];
  const greet = (socket: { send: (text: string) => void }) => {
    for (const message of [
      ...playing,
      ...library,
      ...room,
      paneLayout(DEFAULT_LAYOUT),
    ])
      socket.send(JSON.stringify(message));
  };
  await page.routeWebSocket("**/ws", (ws) => {
    const socket = sockets.length;
    sockets.push(ws);
    ws.onMessage((text) => {
      const message = JSON.parse(String(text)) as ClientMessage;
      if (message.type === "pane_layout")
        sent.push({ socket, layout: message.layout });
    });
    // The second socket holds its hello, so the page stays disconnected until the test greets it.
    if (socket === 0) greet(ws);
  });
  await page.goto("/#code=shot");
  await page.getByText("Set Fire to the Rain").first().waitFor();

  await sockets[0]!.close();
  await expect.poll(() => sockets.length).toBe(2);
  await page.getByRole("button", { name: "Hide main view" }).click();
  await expect(page.locator('[data-pane="main"]')).toBeHidden();
  expect(sent).toEqual([]);

  greet(sockets[1]!);
  await expect.poll(() => sent.map((entry) => entry.socket)).toEqual([1]);
  expect(sent[0]?.layout?.tree).toBe("now_playing");
  await expect(page.locator('[data-pane="main"]')).toBeHidden();
});

test("the side-pane buttons open the overlay when the panes do not fit", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1200, height: 480 });
  const sent = await openPanes(page, {
    tree: {
      axis: "horizontal",
      percent: 60,
      first: "main",
      second: {
        axis: "vertical",
        percent: 50,
        first: "now_playing",
        second: {
          axis: "vertical",
          percent: 50,
          first: "lyrics",
          second: "visualizer",
        },
      },
    },
    floating: [],
    bars: DEFAULT_LAYOUT.bars,
  });
  await expect(page.locator('[data-pane="now_playing"]')).toHaveCount(0);
  await page.getByRole("button", { name: "Queue", exact: true }).click();
  await expect(
    page.getByRole("complementary").getByRole("region", { name: "Queue" }),
  ).toBeVisible();
  expect(sent).toEqual([]);
});

test("a pane key on / leaves search to the shell while the panes are hidden", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1000, height: 800 });
  await openPanes(page, DEFAULT_LAYOUT, {
    detach: { code: "Slash", alt: false, ctrl: false, shift: false },
  });
  await page.keyboard.press("/");
  await expect(page.locator("#top-search")).toBeFocused();
});
