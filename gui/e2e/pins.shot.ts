import { expect, test, type Page, type WebSocketRoute } from "@playwright/test";
import type { ClientMessage } from "../src/bindings/ClientMessage";
import type { ServerMessage } from "../src/bindings/ServerMessage";
import { shot } from "./bridge";
import { idle, libraryFolders, spotifyLoggedOut } from "./fixtures";

/** Answers pin intents with a server-ordered library push, retaining all client messages. */
async function openPins(
  page: Page,
  {
    loggedOut = false,
    pinnedUri = null,
    autoPush = true,
  }: {
    loggedOut?: boolean;
    pinnedUri?: string | null;
    autoPush?: boolean;
  } = {},
) {
  const sent: ClientMessage[] = [];
  let socket: WebSocketRoute;
  let revision = 1;
  const messages = (uri: string | null): ServerMessage[] =>
    libraryFolders.map((message) =>
      message.kind === "library"
        ? {
            ...message,
            rev: revision,
            payload: {
              ...message.payload,
              pinned: uri ? [uri] : [],
              folders: uri
                ? [
                    { kind: "playlist", uri },
                    ...(message.payload.folders ?? []).filter(
                      (node) => node.kind !== "playlist" || node.uri !== uri,
                    ),
                  ]
                : message.payload.folders,
            },
          }
        : message,
    );
  const pushPin = (uri: string | null) => {
    revision += 1;
    for (const update of messages(uri))
      if (update.kind === "library") socket.send(JSON.stringify(update));
  };
  await page.routeWebSocket("**/ws", (ws) => {
    socket = ws;
    ws.onMessage((text) => {
      const message = JSON.parse(String(text)) as ClientMessage;
      sent.push(message);
      if (
        message.type !== "action" ||
        typeof message.action !== "object" ||
        !("SetPlaylistPinned" in message.action)
      )
        return;
      if (autoPush) {
        const { uri, pinned } = message.action.SetPlaylistPinned;
        pushPin(pinned ? uri : null);
      }
    });
    for (const message of [...idle, ...messages(pinnedUri)])
      ws.send(JSON.stringify(message));
    if (loggedOut)
      for (const message of spotifyLoggedOut)
        if (message.kind === "liked")
          ws.send(JSON.stringify({ ...message, rev: 2 }));
  });
  await page.goto("/#code=shot");
  return { sent, pushPin };
}

test("the More menu pins and unpins a folder playlist without a removal confirmation", async ({
  page,
}) => {
  const { sent } = await openPins(page);
  const library = page.getByRole("navigation", { name: "Your Library" });
  await library.getByRole("button", { name: /Chill & focus/ }).click();
  const copies = library.locator("button.library-row", {
    hasText: "Film scores",
  });
  await copies.focus();
  await copies.evaluate((row) => row.setAttribute("data-folder-copy", "yes"));
  await copies.hover();
  await copies.locator(".library-more").click();
  const menu = page.getByRole("menu", { name: "Film scores" });
  await expect(menu.getByRole("menuitem").first()).toHaveText("Pin to top");
  await menu.getByRole("menuitem", { name: "Pin to top", exact: true }).click();
  await expect
    .poll(() => sent)
    .toContainEqual({
      type: "action",
      action: {
        SetPlaylistPinned: { uri: "spotify:playlist:1", pinned: true },
      },
    });
  await expect(page.getByRole("alertdialog")).toHaveCount(0);
  await expect(copies).toHaveCount(2);
  await expect(
    library.getByRole("img", { name: "Pinned", exact: true }),
  ).toHaveCount(2);
  const inside = library.locator('[data-folder-copy="yes"]');
  await expect(inside).toBeFocused();
  await shot(page, "library-playlist-pinned-folder");

  await inside.focus();
  await inside.hover();
  await inside.locator(".library-more").click();
  await expect(menu.getByRole("menuitem").first()).toHaveText("Unpin");
  await shot(page, "library-playlist-unpin-menu");
  await menu.getByRole("menuitem", { name: "Unpin", exact: true }).click();
  await expect
    .poll(() => sent)
    .toContainEqual({
      type: "action",
      action: {
        SetPlaylistPinned: { uri: "spotify:playlist:1", pinned: false },
      },
    });
  await expect(copies).toHaveCount(1);
  await expect(inside).toBeFocused();
  await expect(
    library.getByRole("img", { name: "Pinned", exact: true }),
  ).toHaveCount(0);
  await expect(page.getByRole("alertdialog")).toHaveCount(0);
  expect(
    sent.filter(
      (message) =>
        message.type === "action" &&
        typeof message.action === "object" &&
        ("UnfollowPlaylist" in message.action ||
          "DeletePlaylist" in message.action),
    ),
  ).toEqual([]);
});

test("a cached Spotify playlist can be pinned locally while logged out with no removal entry", async ({
  page,
}) => {
  const { sent } = await openPins(page, { loggedOut: true });
  const library = page.getByRole("navigation", { name: "Your Library" });
  await library
    .getByRole("button", { name: "Log in to Spotify", exact: true })
    .waitFor();
  await library.getByRole("button", { name: /Chill & focus/ }).click();
  const playlist = library.locator("button.library-row", {
    hasText: "Film scores",
  });
  await playlist.focus();
  await playlist.press("Shift+F10");
  const items = page.getByRole("menuitem");
  await expect(items).toHaveCount(1);
  await expect(items).toHaveText(["Pin to top"]);
  await shot(page, "library-playlist-pin-logged-out");
  await expect(items).toBeFocused();
  await items.press("Enter");
  await expect
    .poll(() => sent)
    .toContainEqual({
      type: "action",
      action: {
        SetPlaylistPinned: { uri: "spotify:playlist:1", pinned: true },
      },
    });
  await expect(
    library.getByRole("img", { name: "Pinned", exact: true }),
  ).toHaveCount(2);
  await expect(page.getByRole("alertdialog")).toHaveCount(0);
});

test("unpinning a root playlist keeps keyboard focus after the library push moves its row", async ({
  page,
}) => {
  const uri = "spotify:playlist:4";
  const { sent, pushPin } = await openPins(page, {
    pinnedUri: uri,
    autoPush: false,
  });
  const library = page.getByRole("navigation", { name: "Your Library" });
  const row = library.locator(`[data-playlist-uri="${uri}"]`);
  const titles = library.locator(".library-row-title");
  await expect(titles).toHaveText([
    "Liked Songs",
    "Recently played",
    "Gym: EDM",
    "Chill & focus",
    "Kygo & friends",
    "Discover Weekly",
    "Library health",
  ]);
  await row.focus();
  await row.evaluate((node) => node.setAttribute("data-original-row", "yes"));
  await row.press("Shift+F10");
  const unpin = page.getByRole("menuitem", { name: "Unpin", exact: true });
  await expect(unpin).toBeFocused();
  await unpin.press("Enter");
  await expect
    .poll(() => sent)
    .toContainEqual({
      type: "action",
      action: { SetPlaylistPinned: { uri, pinned: false } },
    });
  await expect(page.getByRole("menu")).toHaveCount(0);
  await expect(row).toBeFocused();
  // The intent has left the page, but its row has not changed yet.
  await expect(
    library.getByRole("img", { name: "Pinned", exact: true }),
  ).toHaveCount(1);
  pushPin(null);
  await expect(titles).toHaveText([
    "Liked Songs",
    "Recently played",
    "Chill & focus",
    "Kygo & friends",
    "Gym: EDM",
    "Discover Weekly",
    "Library health",
  ]);
  await expect(row).toHaveAttribute("data-original-row", "yes");
  await expect(row).toBeFocused();
  await expect(page.getByRole("alertdialog")).toHaveCount(0);
  await shot(page, "library-playlist-unpinned-root-focus");
});

for (const expanded of [true, false]) {
  test(`unpinning a folder shortcut restores focus to ${expanded ? "its visible folder copy" : "the adjacent closed folder"} after the library push`, async ({
    page,
  }) => {
    const uri = "spotify:playlist:1";
    const { sent, pushPin } = await openPins(page, {
      pinnedUri: uri,
      autoPush: false,
    });
    const library = page.getByRole("navigation", { name: "Your Library" });
    const folder = library.getByRole("button", { name: /Chill & focus/ });
    if (expanded) await folder.click();
    const copies = library.locator(`[data-playlist-uri="${uri}"]`);
    await expect(copies).toHaveCount(expanded ? 2 : 1);
    const shortcut = copies.first();
    await shortcut.focus();
    await shortcut.press("Shift+F10");
    const unpin = page.getByRole("menuitem", { name: "Unpin", exact: true });
    await expect(unpin).toBeFocused();
    await unpin.press("Enter");
    await expect
      .poll(() => sent)
      .toContainEqual({
        type: "action",
        action: { SetPlaylistPinned: { uri, pinned: false } },
      });
    await expect(page.getByRole("menu")).toHaveCount(0);
    await expect(shortcut).toBeFocused();
    pushPin(null);
    await expect(copies).toHaveCount(expanded ? 1 : 0);
    await expect(expanded ? copies : folder).toBeFocused();
    await expect(
      library.getByRole("img", { name: "Pinned", exact: true }),
    ).toHaveCount(0);
    await expect(page.getByRole("alertdialog")).toHaveCount(0);
    await shot(
      page,
      expanded
        ? "library-playlist-unpinned-folder-copy-focus"
        : "library-playlist-unpinned-neighbor-focus",
    );
  });
}

test("a pin push leaves focus alone after the user moves to another control", async ({
  page,
}) => {
  const uri = "spotify:playlist:1";
  const { sent, pushPin } = await openPins(page, {
    pinnedUri: uri,
    autoPush: false,
  });
  const library = page.getByRole("navigation", { name: "Your Library" });
  const shortcut = library.locator(`[data-playlist-uri="${uri}"]`);
  await shortcut.focus();
  await shortcut.press("Shift+F10");
  await page
    .getByRole("menuitem", { name: "Unpin", exact: true })
    .press("Enter");
  await expect
    .poll(() => sent)
    .toContainEqual({
      type: "action",
      action: { SetPlaylistPinned: { uri, pinned: false } },
    });
  const search = library.getByRole("searchbox", {
    name: "Filter Your Library",
  });
  await search.focus();
  pushPin(null);
  await expect(shortcut).toHaveCount(0);
  await expect(search).toBeFocused();
});
