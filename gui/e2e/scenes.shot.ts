import { test } from "@playwright/test";
import { openWith, shot } from "./bridge";
import {
  artist,
  discover,
  filmScores,
  hello,
  idle,
  library,
  libraryUnavailable,
  onboarding,
  playing,
  partyHosting,
  partyNone,
  recentlyPlayed,
  playlistSync,
  room,
  search,
  session,
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
  await page.getByText("Set Fire to the Rain").first().waitFor();
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
  await page.getByText("Set Fire to the Rain").first().waitFor();
  await shot(page, "playing-narrow");
});

test("a liked Spotify song at 800 px", async ({ page }) => {
  await page.setViewportSize({ width: 800, height: 600 });
  await openWith(page, [
    ...playing.map((message) =>
      message.kind === "playback" && message.payload.item
        ? {
            ...message,
            payload: {
              ...message.payload,
              item: { ...message.payload.item, uri: "spotify:track:21" },
            },
          }
        : message,
    ),
    ...library,
  ]);
  await page.getByText("Set Fire to the Rain").first().waitFor();
  await shot(page, "playing-spotify-narrow");
});

test("idle", async ({ page }) => {
  await openWith(page, idle);
  await page.getByText("Nothing is playing").first().waitFor();
  await shot(page, "idle");
});

test("library playlist", async ({ page }) => {
  await openWith(page, [...playing, ...library, ...filmScores]);
  await page
    .getByRole("navigation", { name: "Your Library" })
    .getByRole("button", { name: "Film scores" })
    .click();
  await page
    .getByRole("listbox", { name: "Film scores" })
    .getByText("Cornfield Chase")
    .waitFor();
  await shot(page, "library-playlist");
});

test("library without a Spotify session", async ({ page }) => {
  await openWith(page, [...idle, ...libraryUnavailable]);
  await page
    .getByRole("navigation", { name: "Your Library" })
    .getByRole("button", { name: "Liked Songs" })
    .click();
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

test("queue panel", async ({ page }) => {
  await openWith(page, [...playing, ...library]);
  await page.getByText("Set Fire to the Rain").first().waitFor();
  await page.keyboard.press("Shift+Q");
  await page.getByRole("region", { name: "Queue" }).waitFor();
  await shot(page, "queue-panel");
});

test("search", async ({ page }) => {
  await openWith(page, [...playing, ...library, ...search]);
  await page.getByText("Set Fire to the Rain").first().waitFor();
  await page.keyboard.press("2");
  await page.getByRole("button", { name: "Play Kygo" }).first().waitFor();
  await page.keyboard.type("kygo");
  await shot(page, "search");
});

test("search at 800 px", async ({ page }) => {
  await page.setViewportSize({ width: 800, height: 600 });
  await openWith(page, [...playing, ...library, ...search]);
  await page.getByText("Set Fire to the Rain").first().waitFor();
  await page.keyboard.press("2");
  await page.getByRole("button", { name: "Play Kygo" }).first().waitFor();
  await page.keyboard.type("kygo");
  await shot(page, "search-narrow");
});

test("command mode", async ({ page }) => {
  await openWith(page, [...playing, ...library]);
  await page.getByText("Set Fire to the Rain").first().waitFor();
  await page.keyboard.press(":");
  await page.getByRole("dialog", { name: "Command mode" }).waitFor();
  await page.keyboard.type("play adele");
  await shot(page, "command-mode");
});

test("stats", async ({ page }) => {
  await openWith(page, [...playing, ...library, ...stats]);
  await page.getByText("Set Fire to the Rain").first().waitFor();
  await page.keyboard.press("5");
  await page.getByText("THIS WEEK,").waitFor();
  await shot(page, "stats");
});

test("stats at 800 px", async ({ page }) => {
  await page.setViewportSize({ width: 800, height: 600 });
  await openWith(page, [...playing, ...library, ...stats]);
  await page.getByText("Set Fire to the Rain").first().waitFor();
  await page.keyboard.press("5");
  await page.getByText("THIS WEEK,").waitFor();
  await shot(page, "stats-narrow");
});

test("party start", async ({ page }) => {
  await openWith(page, [...playing, ...library, ...partyNone]);
  await page.getByText("Set Fire to the Rain").first().waitFor();
  await page.keyboard.press("6");
  await page.getByText("Join a party").waitFor();
  await shot(page, "party-start");
});

test("party hosting", async ({ page }) => {
  await openWith(page, [...playing, ...library, ...partyHosting]);
  await page.getByText("Set Fire to the Rain").first().waitFor();
  await page.keyboard.press("6");
  await page.getByText("You host").waitFor();
  await shot(page, "party-hosting");
});

test("party hosting at 800 px", async ({ page }) => {
  await page.setViewportSize({ width: 800, height: 600 });
  await openWith(page, [...playing, ...library, ...partyHosting]);
  await page.getByText("Set Fire to the Rain").first().waitFor();
  await page.keyboard.press("6");
  await page.getByText("You host").waitFor();
  await shot(page, "party-hosting-narrow");
});

test("room", async ({ page }) => {
  await openWith(page, [...playing, ...library, ...room]);
  await page.getByText("Set Fire to the Rain").first().waitFor();
  await page.keyboard.press("r");
  await page.getByText("SIDE A").waitFor();
  await shot(page, "room");
});

test("lyrics", async ({ page }) => {
  await openWith(page, [...playing, ...library, ...room]);
  await page.getByText("Set Fire to the Rain").first().waitFor();
  await page.keyboard.press("l");
  await page.getByText("But I set fire to the rain").waitFor();
  await shot(page, "lyrics");
});

test("room at 800 px", async ({ page }) => {
  await page.setViewportSize({ width: 800, height: 600 });
  await openWith(page, [...playing, ...library, ...room]);
  await page.getByText("Set Fire to the Rain").first().waitFor();
  await page.keyboard.press("r");
  await page.getByText("SIDE A").waitFor();
  await shot(page, "room-narrow");
});

test("session", async ({ page }) => {
  await openWith(page, [...playing, ...library, ...session]);
  await page.getByText("Set Fire to the Rain").first().waitFor();
  await page.keyboard.press("4");
  await page.getByText("This session").waitFor();
  await shot(page, "session");
});

test("session at 800 px", async ({ page }) => {
  await page.setViewportSize({ width: 800, height: 600 });
  await openWith(page, [...playing, ...library, ...session]);
  await page.getByText("Set Fire to the Rain").first().waitFor();
  await page.keyboard.press("4");
  await page.getByText("This session").waitFor();
  await shot(page, "session-narrow");
});

test("discover", async ({ page }) => {
  await openWith(page, [...playing, ...library, ...discover]);
  await page.getByText("Set Fire to the Rain").first().waitFor();
  await page.keyboard.press("3");
  await page
    .getByRole("listbox", { name: /Your top tracks/ })
    .getByText("Lose Somebody")
    .waitFor();
  await shot(page, "discover");
});

test("library health", async ({ page }) => {
  await openWith(page, [...playing, ...library, ...playlistSync]);
  await page.getByText("Set Fire to the Rain").first().waitFor();
  await page
    .getByRole("navigation", { name: "Your Library" })
    .getByRole("button", { name: /Library health/ })
    .click();
  await page.getByText("LINKED PLAYLIST").waitFor();
  await shot(page, "library-health");
});

test("home", async ({ page }) => {
  await openWith(page, [
    ...playing,
    ...library,
    ...recentlyPlayed,
    ...discover,
  ]);
  await page.getByText("Set Fire to the Rain").first().waitFor();
  await page.getByRole("region", { name: "Recently played" }).waitFor();
  await shot(page, "home");
});

test("album", async ({ page }) => {
  await openWith(page, [...playing, ...library, ...recentlyPlayed, ...room]);
  await page
    .getByRole("region", { name: "Recently played" })
    .getByRole("button", { name: /21/ })
    .first()
    .click();
  await page.getByRole("listbox", { name: "21" }).waitFor();
  await shot(page, "album");
});

test("artist", async ({ page }) => {
  await openWith(page, [
    ...playing,
    ...library,
    ...recentlyPlayed,
    ...room,
    ...artist,
  ]);
  await page
    .getByRole("region", { name: "Recently played" })
    .getByRole("button", { name: /21/ })
    .first()
    .click();
  await page
    .getByRole("button", { name: "Adele", exact: true })
    .first()
    .click();
  await page.getByText("Fans also like").waitFor();
  await shot(page, "artist");
});

test("track menu", async ({ page }) => {
  await openWith(page, [...playing, ...library, ...filmScores]);
  await page
    .getByRole("navigation", { name: "Your Library" })
    .getByRole("button", { name: "Film scores" })
    .click();
  await page
    .getByRole("option", { name: /Cornfield Chase/ })
    .click({ button: "right" });
  await page.getByRole("menuitem", { name: "Add to playlist" }).hover();
  await page.getByRole("menu", { name: "Add to playlist" }).waitFor();
  await shot(page, "track-menu");
});
