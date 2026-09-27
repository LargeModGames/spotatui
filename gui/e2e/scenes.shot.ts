import { test } from "@playwright/test";
import { openWith, shot } from "./bridge";
import { hello, idle, onboarding, playing } from "./fixtures";

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
  await openWith(page, playing);
  await page.getByText("He Won't Go").waitFor();
  await shot(page, "playing");
});

test("playing at 800 px on a long device name", async ({ page }) => {
  await page.setViewportSize({ width: 800, height: 600 });
  await openWith(
    page,
    playing.map((message) =>
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
  );
  await page.getByText("He Won't Go").waitFor();
  await shot(page, "playing-narrow");
});

test("idle", async ({ page }) => {
  await openWith(page, idle);
  await page.getByText("Nothing is queued").waitFor();
  await shot(page, "idle");
});
