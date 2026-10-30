import { expect, test, type WebSocketRoute } from "@playwright/test";
import type { ClientMessage } from "../src/bindings/ClientMessage";
import type { ServerMessage } from "../src/bindings/ServerMessage";
import strings from "../src/bindings/strings.json" with { type: "json" };
import { openWith, shot } from "./bridge";
import {
  DEFAULT_LOOK,
  hello,
  onboarding,
  playing,
  library,
  discover,
  settings,
} from "./fixtures";

function settingsFor(
  language: "en" | "zh-Hans" | "zh-Hant",
  rev: number,
): ServerMessage {
  const table =
    language === "zh-Hans"
      ? strings.ZH_HANS
      : language === "zh-Hant"
        ? strings.ZH_HANT
        : strings.EN;
  return {
    kind: "settings",
    rev,
    payload: {
      language,
      strings: table,
      visualizer: DEFAULT_LOOK,
      rows: [
        {
          id: "behavior.language",
          name:
            language === "en"
              ? "Language"
              : language === "zh-Hans"
                ? "语言"
                : "語言",
          description: "",
          group: "interface",
          value: {
            kind: "choice",
            value: language,
            options: ["auto", "en", "zh-Hans", "zh-Hant"],
            labels: [
              table.settings.option_auto,
              table.settings.option_en,
              table.settings.option_zh_hans,
              table.settings.option_zh_hant,
            ],
          },
          button: null,
        },
      ],
    },
  };
}

test("the Hello table localizes onboarding before App or Settings exists", async ({
  page,
}) => {
  if (hello.kind !== "hello") throw new Error("hello fixture");
  await openWith(page, [
    {
      ...hello,
      payload: {
        ...hello.payload,
        language: "zh-Hant",
        strings: strings.ZH_HANT,
      },
    },
    onboarding,
  ]);
  await expect(page.getByRole("heading", { name: "選擇來源" })).toBeVisible();
  await expect(page.getByRole("button", { name: "繼續" })).toBeVisible();
  await expect(page.locator("html")).toHaveAttribute("lang", "zh-Hant");
  await shot(page, "onboarding-zh-Hant");
});

test("a Settings push updates mounted memoized pages and keeps input and raw choices", async ({
  page,
}) => {
  let socket: WebSocketRoute | undefined;
  const sent: ClientMessage[] = [];
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.routeWebSocket("**/ws", (ws) => {
    socket = ws;
    ws.onMessage((message) =>
      sent.push(JSON.parse(String(message)) as ClientMessage),
    );
    for (const message of [
      ...playing,
      ...library,
      ...discover,
      ...settings,
      settingsFor("en", 2),
    ])
      ws.send(JSON.stringify(message));
  });
  await page.goto("/#code=shot");
  await page
    .getByRole("searchbox", { name: "Search", exact: true })
    .fill("{name} 五月天");
  await page.getByRole("button", { name: "Discover", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Discover", exact: true }),
  ).toBeVisible();
  socket!.send(JSON.stringify(settingsFor("zh-Hans", 3)));
  await expect(
    page.getByRole("heading", { name: "发现", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("searchbox", { name: "搜索", exact: true }),
  ).toHaveValue("{name} 五月天");
  await expect(
    page.getByRole("button", { name: "随机播放", exact: true }),
  ).toBeVisible();
  await expect(page.locator("html")).toHaveAttribute("lang", "zh-Hans");
  await shot(page, "discover-zh-Hans");
  // A stale Settings message cannot roll the language or the table back.
  socket!.send(JSON.stringify(settingsFor("en", 1)));
  // The page handles messages in order: once this later shelf shows, the stale
  // Settings message before it has been handled too.
  const shelves = discover[0];
  if (shelves.kind !== "discover" || !shelves.payload.shelves)
    throw new Error("discover fixture");
  socket!.send(
    JSON.stringify({
      ...shelves,
      rev: 2,
      payload: {
        ...shelves.payload,
        shelves: [
          { ...shelves.payload.shelves[0], title: "After the stale message" },
          ...shelves.payload.shelves.slice(1),
        ],
      },
    } satisfies ServerMessage),
  );
  await expect(page.getByText("After the stale message")).toBeVisible();
  await expect(page.locator("html")).toHaveAttribute("lang", "zh-Hans");
  await expect(
    page.getByRole("heading", { name: "发现", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "设置", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "设置", exact: true }),
  ).toBeVisible();
  socket!.send(JSON.stringify(settingsFor("zh-Hant", 4)));
  await expect(
    page.getByRole("heading", { name: "設定", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("searchbox", { name: "搜尋", exact: true }),
  ).toHaveValue("{name} 五月天");
  await page
    .getByRole("combobox", { name: "語言", exact: true })
    .selectOption("en");
  await expect
    .poll(() => sent)
    .toContainEqual({
      type: "action",
      action: {
        SetSetting: { id: "behavior.language", value: { Text: "en" } },
      },
    });
  await expect(page.locator("html")).toHaveAttribute("lang", "zh-Hant");
  await shot(page, "settings-zh-Hant");
  expect(errors).toEqual([]);
});
