import { test } from "@playwright/test";
import { openWith, shot } from "./bridge";
import { hello, onboarding, playing } from "./fixtures";

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
