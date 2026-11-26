import { expect, test, type WebSocketRoute } from "@playwright/test";
import type { ServerMessage } from "../src/bindings/ServerMessage";
import { hello, library, playing, podcastSearch } from "./fixtures";

function emptySearch(rev: number, query: string): ServerMessage {
  return {
    kind: "search",
    rev,
    payload: {
      ran: true,
      query,
      tracks: [],
      artists: [],
      albums: [],
      playlists: [],
      shows: [],
      liked: [],
    },
  };
}

test("an empty search updates the mounted status node in place", async ({
  page,
}) => {
  let socket!: WebSocketRoute;
  await page.routeWebSocket("**/ws", (ws) => {
    socket = ws;
    for (const message of [hello, ...playing, ...library])
      ws.send(JSON.stringify(message));
  });
  await page.goto("/#code=shot");
  await page.getByText("Set Fire to the Rain").first().waitFor();

  const searchBox = page.getByRole("searchbox", {
    name: "Search",
    exact: true,
  });
  await searchBox.fill("zzzzqqqq");
  await searchBox.press("Enter");

  const status = page.locator('.search-page [role="status"]');
  await expect(status).toHaveCount(1);
  await expect(status).toHaveText("Searching…");
  await expect(status).toHaveClass("search-note");
  await status.evaluate((node) => node.setAttribute("data-status-node", "yes"));

  socket.send(JSON.stringify(emptySearch(1, "zzzzqqqq")));
  await expect(status).toHaveCount(1);
  await expect(status).toHaveText("No results found for “zzzzqqqq”.");
  await expect(status).toHaveClass("search-note");
  await expect(status).toHaveAttribute("data-status-node", "yes");

  await searchBox.fill("film score");
  await searchBox.press("Enter");
  await expect(status).toHaveText("Searching…");
  await expect(status).toHaveAttribute("data-status-node", "yes");

  const populated = podcastSearch[0];
  if (populated.kind !== "search") throw new Error("search fixture");
  socket.send(JSON.stringify({ ...populated, rev: 2 }));
  await expect(status).toHaveText("");
  await expect(status).toHaveClass("sr-only");
  await expect(status).toHaveAttribute("data-status-node", "yes");
});
