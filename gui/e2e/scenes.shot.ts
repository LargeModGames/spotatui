import { expect, test, type Page } from "@playwright/test";
import { openWith, openWithVisualizer, shot } from "./bridge";
import {
  announcement,
  artist,
  DEFAULT_LOOK,
  discover,
  filmScores,
  filmScoresLongExplicit,
  history,
  hello,
  idle,
  library,
  libraryFolders,
  libraryUnavailable,
  onboarding,
  playing,
  podcast,
  podcastSearch,
  partyHosting,
  partyNone,
  recentlyPlayed,
  playlistSync,
  spotifyLoggedOut,
  room,
  search,
  session,
  settings,
  songRadio,
  spectrumFrame,
  stats,
  stripedCover,
  visualizerSetting,
  waveFrame,
  welcome,
} from "./fixtures";

/** Opens the album 21 from its link in a Recently played row. */
async function openAlbum21(page: Page): Promise<void> {
  await page
    .getByRole("navigation", { name: "Your Library" })
    .getByRole("button", { name: /Recently played/ })
    .click();
  await page
    .getByRole("listbox", { name: "Recently played" })
    .getByRole("button", { name: "21" })
    .first()
    .click();
}

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

test("a long playlist with explicit songs keeps the page from scrolling", async ({
  page,
}) => {
  await openWith(page, [...playing, ...library, ...filmScoresLongExplicit]);
  await page
    .getByRole("navigation", { name: "Your Library" })
    .getByRole("button", { name: "Film scores" })
    .click();
  // Wait for a below-the-fold explicit row to be in the table before
  // measuring: the hidden sr-only text next to its E badge is what grew the
  // document (#878).
  await page
    .getByRole("listbox", { name: "Film scores" })
    .getByText("Long cut 32")
    .waitFor();
  const pageHeight = await page.evaluate(
    () => document.documentElement.scrollHeight,
  );
  const innerHeight = await page.evaluate(() => window.innerHeight);
  expect(pageHeight).toBe(innerHeight);
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

test("Spotify login after a failed first login", async ({ page }) => {
  await openWith(page, [...idle, ...spotifyLoggedOut]);
  await page
    .getByRole("navigation", { name: "Your Library" })
    .getByRole("button", { name: "Log in to Spotify" })
    .waitFor();
  await shot(page, "library-spotify-login");
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
        soloist_pairing: null,
        soloist_offer: null,
      },
    },
  ]);
  await page.getByRole("alert").waitFor();
  await shot(page, "status-error");
});

test("soloist pairing", async ({ page }) => {
  await openWith(page, [
    ...playing,
    ...library,
    {
      kind: "status",
      rev: 1,
      payload: {
        message: null,
        is_error: false,
        api_error: null,
        soloist_pairing: {
          title: "Pair spotatui with Spotify",
          steps: [
            "1. Open Spotify on a computer on this network.",
            "2. In its device menu, pick 'spotatui'.",
          ],
          waiting: "Waiting for Spotify…",
          close: "Close",
        },
        soloist_offer: null,
      },
    },
  ]);
  await page.getByRole("dialog").waitFor();
  await shot(page, "soloist-pairing");
});

test("soloist offer", async ({ page }) => {
  await openWith(page, [
    ...playing,
    ...library,
    {
      kind: "status",
      rev: 1,
      payload: {
        message: null,
        is_error: false,
        api_error: null,
        soloist_pairing: null,
        soloist_offer: {
          title: "Play through Soloist?",
          body: "Spotify refuses librespot the audio keys for these tracks, as it does for some accounts. Soloist, Spotify's own player, can play them. It needs Premium. In Settings:",
          steps: [
            "1. Soloist Path: Download from Spotify.",
            "2. Soloist API Key: Get a key, then enter it.",
            "3. Playback Engine: Soloist.",
          ],
          set_up: "Set up Soloist",
          not_now: "Not now",
        },
      },
    },
  ]);
  await page.getByRole("dialog").waitFor();
  await shot(page, "soloist-offer");
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
    .getByRole("region", { name: "Your Daily Mixes" })
    .getByText("Daily Mix 2")
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
  await openWith(page, [...playing, ...library, ...welcome]);
  await page.getByText("Set Fire to the Rain").first().waitFor();
  await page.getByRole("heading", { name: "Announcements" }).waitFor();
  await page.getByRole("heading", { name: "[v0.46.0] 2026-10-14" }).waitFor();
  await shot(page, "home");
});

test("album", async ({ page }) => {
  await openWith(page, [...playing, ...library, ...recentlyPlayed, ...room]);
  await openAlbum21(page);
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
  await openAlbum21(page);
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

test("settings", async ({ page }) => {
  await openWith(page, [...playing, ...library, ...settings]);
  await page.getByText("Set Fire to the Rain").first().waitFor();
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByText("Version 0.44.0 is ready").scrollIntoViewIfNeeded();
  await shot(page, "settings");
});

test("settings search", async ({ page }) => {
  await openWith(page, [...playing, ...library, ...settings]);
  await page.getByText("Set Fire to the Rain").first().waitFor();
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("searchbox", { name: "Search settings" }).fill("stream");
  await shot(page, "settings-search");
});

test("visualizer spectrum", async ({ page }) => {
  await openWithVisualizer(
    page,
    [...playing, ...library, visualizerSetting("Spectrum")],
    spectrumFrame,
  );
  await shot(page, "visualizer-spectrum");
});

test("visualizer waveform", async ({ page }) => {
  await openWithVisualizer(
    page,
    [...playing, ...library, visualizerSetting("Waveform")],
    waveFrame,
  );
  await shot(page, "visualizer-waveform");
});

for (const mode of ["Spectrum", "Waveform"] as const) {
  test(`visualizer ${mode.toLowerCase()} cover gradient`, async ({ page }) => {
    await page.route("**/cover?**", (route) =>
      route.fulfill({ contentType: "image/svg+xml", body: stripedCover }),
    );
    await openWithVisualizer(
      page,
      [
        ...playing,
        ...library,
        visualizerSetting(mode, { ...DEFAULT_LOOK, fade_ms: 0 }),
      ],
      mode === "Spectrum" ? spectrumFrame : waveFrame,
    );
    // The bass end takes the cover's darkest colour, navy, in place of the amber accent.
    await page.waitForFunction(() => {
      const canvas =
        document.querySelector<HTMLCanvasElement>(".playerbar-viz");
      const pixels = canvas
        ?.getContext("2d")
        ?.getImageData(0, 0, Math.ceil(canvas.width / 10), canvas.height).data;
      let [red, blue] = [0, 0];
      for (let at = 0; pixels && at < pixels.length; at += 4) {
        red += pixels[at] * pixels[at + 3];
        blue += pixels[at + 2] * pixels[at + 3];
      }
      return blue > red;
    });
    await shot(page, `visualizer-${mode.toLowerCase()}-cover`);
  });
}

test("library folders", async ({ page }) => {
  await openWith(page, [...playing, ...libraryFolders]);
  const panel = page.getByRole("navigation", { name: "Your Library" });
  await panel.getByRole("button", { name: "Create playlist" }).waitFor();
  await panel.getByRole("button", { name: /Chill & focus/ }).click();
  await panel.getByRole("button", { name: /Film scores/ }).waitFor();
  await page.mouse.move(0, 0);
  // The chevron turns on a short transition.
  await page.evaluate(() =>
    Promise.all(document.getAnimations().map((motion) => motion.finished)),
  );
  await shot(page, "library-folders");
});

test("library filter", async ({ page }) => {
  await openWith(page, [...playing, ...libraryFolders]);

  const panel = page.getByRole("navigation", {
    name: "Your Library",
  });
  const input = panel.getByRole("searchbox", {
    name: "Filter Your Library",
    exact: true,
  });
  const folder = panel.getByRole("button", {
    name: /Chill & focus/,
  });
  const film = panel.getByRole("button", {
    name: /Film scores/,
  });

  // A playlist inside a closed folder must still be found.
  await folder.waitFor();
  await input.fill("film");
  await film.waitFor();
  await folder.waitFor({ state: "hidden" });
  await page.mouse.move(0, 0);
  await shot(page, "library-filter");

  // Clearing the query restores the closed folder.
  await input.fill("");
  await folder.waitFor();
  await film.waitFor({ state: "hidden" });

  // An open folder also stays open after filtering and clearing.
  await folder.click();
  await film.waitFor();
  await input.fill("film");
  await folder.waitFor({ state: "hidden" });
  await input.fill("");
  await folder.waitFor();
  await film.waitFor();
});

test("playlist menu", async ({ page }) => {
  await openWith(page, [...playing, ...library]);
  await page
    .getByRole("navigation", { name: "Your Library" })
    .getByRole("button", { name: /Discover Weekly/ })
    .click({ button: "right" });
  await page
    .getByRole("menuitem", { name: "Remove from Your Library" })
    .waitFor();
  await shot(page, "playlist-menu");
});

test("list sort and filter", async ({ page }) => {
  await openWith(page, [...playing, ...library, ...filmScores]);
  await page
    .getByRole("navigation", { name: "Your Library" })
    .getByRole("button", { name: "Film scores" })
    .click();
  const list = page.getByRole("region", { name: "Film scores" });
  await list.getByRole("listbox", { name: "Film scores" }).waitFor();
  await list.getByRole("searchbox", { name: "Filter" }).fill("zimmer");
  await list
    .getByRole("combobox", { name: "Sort by" })
    .selectOption("duration:desc");
  await list.getByText("3 of 4").waitFor();
  await shot(page, "list-sort-filter");
});

test("podcast", async ({ page }) => {
  await openWith(page, [...playing, ...library, ...podcastSearch, ...podcast]);
  await page.getByText("Set Fire to the Rain").first().waitFor();
  await page.keyboard.press("2");
  await page
    .getByRole("button", { name: /Score Notes/ })
    .first()
    .click();
  await page
    .getByRole("listbox", { name: "Episodes of Score Notes" })
    .waitFor();
  await page
    .getByRole("option", { name: /John Powell/ })
    .scrollIntoViewIfNeeded();
  await shot(page, "podcast");
});

test("search podcasts", async ({ page }) => {
  await openWith(page, [...playing, ...library, ...podcastSearch]);
  await page.getByText("Set Fire to the Rain").first().waitFor();
  await page.keyboard.press("2");
  await page.keyboard.type("film score");
  await page
    .getByRole("group", { name: "Result type" })
    .getByRole("button", { name: "Podcasts" })
    .click();
  await page.mouse.move(0, 0);
  await page.getByRole("button", { name: /Leitmotif Hour/ }).waitFor();
  await shot(page, "search-podcasts");
});

test("song radio", async ({ page }) => {
  await openWith(page, [...playing, ...library, ...history, ...songRadio]);
  await page
    .getByRole("navigation", { name: "Your Library" })
    .getByRole("button", { name: /Recently played/ })
    .click();
  await page
    .getByRole("listbox", { name: "Recently played" })
    .getByRole("option", { name: /Cornfield Chase/ })
    .click({ button: "right" });
  await page.getByRole("menuitem", { name: "Go to song radio" }).click();
  await page
    .getByRole("listbox", { name: "Cornfield Chase Radio" })
    .getByText("Arrival of the Birds")
    .waitFor();
  await page.mouse.move(0, 0);
  await shot(page, "radio");
});

test("recently played", async ({ page }) => {
  await openWith(page, [...playing, ...library, ...history]);
  await page
    .getByRole("navigation", { name: "Your Library" })
    .getByRole("button", { name: /Recently played/ })
    .click();
  await page
    .getByRole("listbox", { name: "Recently played" })
    .getByText("Nuvole Bianche")
    .waitFor();
  await shot(page, "recently-played");
});

test("saved album", async ({ page }) => {
  await openWith(page, [
    ...playing,
    ...library,
    ...recentlyPlayed,
    ...room.map((message) =>
      message.kind === "album"
        ? { ...message, payload: { ...message.payload, saved: true } }
        : message,
    ),
  ]);
  await openAlbum21(page);
  await page.getByRole("listbox", { name: "21" }).waitFor();
  await page
    .getByRole("button", { name: "Remove from Your Library" })
    .waitFor();
  await shot(page, "album-saved");
});

test("mini player", async ({ page }) => {
  await openWith(page, [...playing, ...library]);
  await page.getByText("Set Fire to the Rain").first().waitFor();
  await page.keyboard.press("Shift+T");
  // The desktop shell shrinks its window once the page is in the mini mode.
  await page.setViewportSize({ width: 360, height: 420 });
  await page.getByRole("region", { name: "Mini player" }).waitFor();
  await shot(page, "mini-player");
});

test("announcement modal", async ({ page }) => {
  await openWith(page, [
    ...playing,
    ...library,
    announcement(
      "warning",
      "Spotify changes its login on 1 November. Update spotatui before then to keep native playback.",
      1,
      2,
    ),
  ]);
  await page.getByRole("dialog", { name: "Spotify login change" }).waitFor();
  await shot(page, "announcement-modal");
});

test("announcement modal 2 of 2", async ({ page }) => {
  await openWith(page, [
    ...playing,
    ...library,
    announcement(
      "info",
      "spotatui now has a community Discord: install help, feature ideas and listening parties.",
      2,
      2,
    ),
  ]);
  await page.getByText("2 of 2").waitFor();
  await shot(page, "announcement-modal-second");
});

test("critical announcement modal at 800 px with a long body", async ({
  page,
}) => {
  await page.setViewportSize({ width: 800, height: 600 });
  await openWith(page, [
    ...playing,
    ...library,
    announcement(
      "critical",
      Array.from(
        { length: 6 },
        () =>
          "Native playback fails for accounts created after 1 October because Spotify blocks their audio keys. Switch the playback device to the official app until a fixed spotatui release is out.",
      ).join("\n\n"),
    ),
  ]);
  await page.getByRole("alertdialog").waitFor();
  await shot(page, "announcement-modal-critical-800");
});
