import { expect, test, type Page } from "@playwright/test";
import type { ServerMessage } from "../src/bindings/ServerMessage";
import type { TrackInfo } from "../src/bindings/TrackInfo";
import { hello, library, playing } from "./fixtures";

/** Opens the page and keeps the socket, so a test can deliver messages later. */
async function openLive(
  page: Page,
  messages: ServerMessage[],
): Promise<(late: ServerMessage[]) => void> {
  let socket: { send: (data: string) => void } | null = null;
  await page.routeWebSocket("**/ws", (ws) => {
    socket = ws;
    for (const message of messages) ws.send(JSON.stringify(message));
  });
  await page.goto("/#code=shot");
  return (late: ServerMessage[]) => {
    for (const message of late) socket?.send(JSON.stringify(message));
  };
}

function lateTrack(name: string, id: string, artist: string): TrackInfo {
  return {
    uri: `spotify:track:${id}`,
    name,
    artists: [artist],
    album: "Late Album",
    duration_ms: 200_000,
    id,
    album_id: "late-album",
    artist_refs: [],
    is_playable: true,
    is_local: false,
    track_number: 1,
    explicit: false,
    image_url: null,
    release_date: null,
  };
}

/** The liked list with its tracks withheld: available, not yet loaded. */
const likedPending: ServerMessage = {
  kind: "liked",
  rev: 1,
  payload: {
    tracks: [],
    total: 0,
    has_more: false,
    available: true,
    loaded: false,
  },
};

/** The liked list arriving after navigation, with a later revision. */
const likedArrived: ServerMessage = {
  kind: "liked",
  rev: 2,
  payload: {
    tracks: [
      lateTrack("Late Arrival", "late-1", "Adele"),
      lateTrack("Second Late Arrival", "late-2", "Avicii"),
    ],
    total: 2,
    has_more: false,
    available: true,
    loaded: true,
  },
};

test("a sidebar click keeps focus on the control until the list lands, then hands the keyboard to it", async ({
  page,
}) => {
  const send = await openLive(page, [
    hello,
    ...playing,
    ...library,
    likedPending,
  ]);
  const liked = page.locator("button.library-row", { hasText: "Liked Songs" });
  await liked.waitFor();

  await liked.click();
  await expect(liked).toBeFocused();
  await expect(page.getByRole("listbox")).toHaveCount(0);

  send([likedArrived]);
  await page.getByRole("listbox").waitFor();
  await expect(page.getByRole("listbox")).toBeFocused();

  await page.keyboard.press("j");
  await expect(page.getByRole("listbox")).toHaveAttribute(
    "aria-activedescendant",
    "liked-1",
  );
  await page.keyboard.press("k");
  await expect(page.getByRole("listbox")).toHaveAttribute(
    "aria-activedescendant",
    "liked-0",
  );
});

test("a list that lands after focus moved elsewhere never steals the keyboard", async ({
  page,
}) => {
  const send = await openLive(page, [
    hello,
    ...playing,
    ...library,
    likedPending,
  ]);
  const liked = page.locator("button.library-row", { hasText: "Liked Songs" });
  await liked.waitFor();

  await liked.click();

  // The user tabs to the next control while the list is still loading.
  await page.keyboard.press("Tab");
  await expect(liked).not.toBeFocused();

  send([likedArrived]);
  await page.getByRole("listbox").waitFor();
  await expect(page.getByRole("listbox")).not.toBeFocused();
});
