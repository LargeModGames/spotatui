import { expect, test, type WebSocketRoute } from "@playwright/test";
import strings from "../src/bindings/strings.json" with { type: "json" };
import { playing, library, filmScores, DEFAULT_LOOK } from "./fixtures";
import type { ClientMessage } from "../src/bindings/ClientMessage";

const alternate = {
  ...strings.EN,
  gui: {
    ...strings.EN.gui,
    add_queue: "alternate queue",
    copied_song_link: "alternate copied link",
    copy_link_failed: "alternate copy failure",
  },
};
function changeLanguage(socket: WebSocketRoute) {
  socket.send(
    JSON.stringify({
      kind: "settings",
      rev: 100,
      payload: {
        language: "zh-Hant",
        strings: alternate,
        rows: [],
        visualizer: DEFAULT_LOOK,
      },
    }),
  );
}
async function openMenu(page: import("@playwright/test").Page) {
  await page.goto("/#code=shot");
  await page
    .getByRole("navigation", { name: "Your Library" })
    .getByRole("button", { name: "Film scores" })
    .click();
  await page
    .getByRole("option", { name: /Cornfield Chase/ })
    .click({ button: "right" });
}

test("a focused plain menu button stays mounted when the language changes", async ({
  page,
}) => {
  let socket!: WebSocketRoute;
  await page.routeWebSocket("**/ws", (ws) => {
    socket = ws;
    for (const message of [...playing, ...library, ...filmScores])
      ws.send(JSON.stringify(message));
  });
  await openMenu(page);
  const button = page.getByRole("menuitem", {
    name: "Add to queue",
    exact: true,
  });
  await button.focus();
  await button.evaluate((node) =>
    node.setAttribute("data-original-button", "yes"),
  );
  changeLanguage(socket);
  const current = page.getByRole("menuitem", {
    name: "alternate queue",
    exact: true,
  });
  await expect(current).toBeFocused();
  await expect(current).toHaveAttribute("data-original-button", "yes");
});

for (const failure of [false, true]) {
  test(`a closed track menu uses the current language when copying ${failure ? "fails" : "finishes"}`, async ({
    page,
  }) => {
    let socket!: WebSocketRoute;
    const sent: ClientMessage[] = [];
    await page.addInitScript(() => {
      Object.defineProperty(navigator, "clipboard", {
        value: {
          writeText: () =>
            new Promise<void>((resolve, reject) => {
              Object.assign(window, {
                finishCopy: (fail: boolean) =>
                  fail ? reject(new Error("denied")) : resolve(),
              });
            }),
        },
      });
    });
    await page.routeWebSocket("**/ws", (ws) => {
      socket = ws;
      ws.onMessage((message) =>
        sent.push(JSON.parse(String(message)) as ClientMessage),
      );
      for (const message of [...playing, ...library, ...filmScores])
        ws.send(JSON.stringify(message));
    });
    await openMenu(page);
    await page.getByRole("menuitem", { name: "Share", exact: true }).hover();
    await page
      .getByRole("menuitem", { name: "Copy song link", exact: true })
      .click();
    await expect(page.getByRole("menu")).toHaveCount(0);
    changeLanguage(socket);
    await expect(page.locator("html")).toHaveAttribute("lang", "zh-Hant");
    await page.evaluate(
      (fail) =>
        (
          window as unknown as { finishCopy: (failure: boolean) => void }
        ).finishCopy(fail),
      failure,
    );
    await expect
      .poll(() => sent)
      .toContainEqual({
        type: "action",
        action: failure
          ? { NotifyError: ["alternate copy failure", 4] }
          : { Notify: ["alternate copied link", 3] },
      });
  });
}
