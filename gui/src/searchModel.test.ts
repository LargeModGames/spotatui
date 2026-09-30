import { describe, expect, it } from "vitest";
import type { SearchPayload } from "./bindings/SearchPayload";
import {
  otherSources,
  queueable,
  scopeLabel,
  tabHasResults,
  year,
} from "./searchModel";

const empty: SearchPayload = {
  ran: true,
  query: "q",
  tracks: [],
  artists: [],
  albums: [],
  playlists: [],
  liked: [],
};

describe("searchModel", () => {
  it("offers the other compiled sources and leaves out Local, which searches Spotify", () => {
    expect(
      otherSources(["Spotify", "Local", "YouTube", "Radio"], "Spotify"),
    ).toEqual(["YouTube", "Radio"]);
    expect(scopeLabel("Local")).toBe("Local has no search · Spotify");
    expect(scopeLabel("Qobuz")).toBe("In Qobuz");
  });

  it("takes the year from a release date and is empty without one", () => {
    expect(year("2016-05-13")).toBe("2016");
    expect(year(null)).toBe("");
  });

  it("disables a tab with no results but never Everything", () => {
    expect(tabHasResults(empty, "Albums")).toBe(false);
    expect(tabHasResults(empty, "Everything")).toBe(true);
  });

  it("refuses to queue a radio station", () => {
    expect(queueable("radio:https://radio.example")).toBe(false);
    expect(queueable("spotify:track:1")).toBe(true);
    expect(queueable(null)).toBe(false);
  });
});
