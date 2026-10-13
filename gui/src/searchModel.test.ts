import { describe, expect, it } from "vitest";
import type { SearchPayload } from "./bindings/SearchPayload";
import {
  otherSources,
  scopeLabel,
  showView,
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
  shows: [],
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

  it("disables a tab with no results but never All", () => {
    expect(tabHasResults(empty, "Albums")).toBe(false);
    expect(tabHasResults(empty, "All")).toBe(true);
  });

  it("offers the Podcasts tab only for a show it can open", () => {
    const show = {
      id: null,
      uri: null,
      name: "Lex",
      description: "",
      publisher: "Lex Fridman",
      image_url: null,
    };
    expect(tabHasResults({ ...empty, shows: [show] }, "Podcasts")).toBe(false);
    expect(
      tabHasResults({ ...empty, shows: [{ ...show, id: "s" }] }, "Podcasts"),
    ).toBe(true);
  });

  it("opens a show by id and skips one without an id", () => {
    const show = {
      id: "s",
      uri: "spotify:show:s",
      name: "Lex",
      description: "",
      publisher: "Lex Fridman",
      image_url: null,
    };
    expect(showView(show)).toEqual({ page: "show", id: "s", name: "Lex" });
    expect(showView({ ...show, id: null })).toBeNull();
  });
});
