import { test } from "@playwright/test";
import { openWith, shot } from "./bridge";
import {
  hello,
  idle,
  library,
  libraryUnavailable,
  onboarding,
  playing,
  search,
  stats,
} from "./fixtures";

test("expired", async ({ page }) => {
  await page.goto("/");
  await page.getByText("no longer connected").waitFor();
  await shot(page, "expired");
});

test("onboarding", async ({ page }) => {
  await openWith(page, [hello, onboarding]);
  await page.getByText("Choose your music sources").waitFor();
  await shot(page, "onboarding");
});

test("playing", async ({ page }) => {
  await openWith(page, [...playing, ...library]);
  await page.getByText("Wake Me Up").waitFor();
  await shot(page, "playing");
});

test("playing at 800 px on a long device name", async ({ page }) => {
  await page.setViewportSize({ width: 800, height: 600 });
  await openWith(page, [
    ...playing.map((message) =>
      message.kind === "playback"
        ? {
            ...message,
            payload: {
              ...message.payload,
              device: "Living Room Speaker Group",
            },
          }
        : message,
    ),
    ...library,
  ]);
  await page.getByText("Wake Me Up").waitFor();
  await shot(page, "playing-narrow");
});

test("idle", async ({ page }) => {
  await openWith(page, idle);
  await page.getByText("Nothing is queued").waitFor();
  await shot(page, "idle");
});

test("library without a Spotify session", async ({ page }) => {
  await openWith(page, [...idle, ...libraryUnavailable]);
  await page.getByText("needs a Spotify session").waitFor();
  await shot(page, "library-unavailable");
});

test("status error", async ({ page }) => {
  await openWith(page, [
    ...playing,
    ...library,
    {
      kind: "status",
      rev: 1,
      payload: {
        message: null,
        is_error: false,
        api_error: "Spotify refused the request: 403 Forbidden",
      },
    },
  ]);
  await page.getByRole("alert").waitFor();
  await shot(page, "status-error");
});

test("queue drawer", async ({ page }) => {
  await openWith(page, [...playing, ...library]);
  await page.getByText("Wake Me Up").waitFor();
  await page.keyboard.press("Shift+Q");
  await page.getByRole("dialog", { name: "Queue" }).waitFor();
  await shot(page, "queue-drawer");
});

test("search", async ({ page }) => {
  await openWith(page, [...playing, ...library, ...search]);
  await page.getByText("Wake Me Up").waitFor();
  await page.keyboard.press("2");
  await page.getByText("Play Kygo").waitFor();
  await page.keyboard.type("kygo");
  await shot(page, "search");
});

test("search at 800 px", async ({ page }) => {
  await page.setViewportSize({ width: 800, height: 600 });
  await openWith(page, [...playing, ...library, ...search]);
  await page.getByText("Wake Me Up").waitFor();
  await page.keyboard.press("2");
  await page.getByText("Play Kygo").waitFor();
  await page.keyboard.type("kygo");
  await shot(page, "search-narrow");
});

test("command mode", async ({ page }) => {
  await openWith(page, [...playing, ...library]);
  await page.getByText("Wake Me Up").waitFor();
  await page.keyboard.press(":");
  await page.getByRole("dialog", { name: "Command mode" }).waitFor();
  await page.keyboard.type("play adele");
  await shot(page, "command-mode");
});

test("stats", async ({ page }) => {
  await openWith(page, [...playing, ...library, ...stats]);
  await page.getByText("Wake Me Up").waitFor();
  await page.keyboard.press("5");
  await page.getByText("THIS WEEK,").waitFor();
  await shot(page, "stats");
});

test("stats at 800 px", async ({ page }) => {
  await page.setViewportSize({ width: 800, height: 600 });
  await openWith(page, [...playing, ...library, ...stats]);
  await page.getByText("Wake Me Up").waitFor();
  await page.keyboard.press("5");
  await page.getByText("THIS WEEK,").waitFor();
  await shot(page, "stats-narrow");
});
