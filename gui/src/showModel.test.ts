import { describe, expect, it } from "vitest";
import type { EpisodeInfo } from "./bindings/EpisodeInfo";
import type { PodcastPage } from "./bindings/PodcastPage";
import {
  episodeDate,
  episodeLength,
  episodeLink,
  episodePager,
  episodePlay,
  longDescription,
  newestEpisode,
  resumeOf,
  heldShowId,
  showLink,
} from "./showModel";

const episode = (
  uri: string | null,
  resume: EpisodeInfo["resume_point"] = null,
): EpisodeInfo => ({
  id: uri?.split(":")[2] ?? null,
  uri,
  name: uri ?? "?",
  duration_ms: 3_600_000,
  show_name: "",
  description: "",
  release_date: "2026-10-01",
  is_playable: true,
  resume_point: resume,
  image_url: null,
});

const page = (has_more: boolean, count: number): PodcastPage => ({
  show: {
    id: "s",
    uri: "spotify:show:s",
    name: "Lex",
    description: "",
    publisher: "Lex Fridman",
    image_url: null,
  },
  saved: false,
  loaded: true,
  episodes: Array.from({ length: count }, (_, index) =>
    episode(`spotify:episode:${index}`),
  ),
  total: 100,
  has_more,
});

describe("episodePlay", () => {
  it("plays every episode with a uri and counts the offset among them", () => {
    const episodes = [
      episode("spotify:episode:a"),
      episode(null),
      episode("spotify:episode:c"),
    ];
    expect(episodePlay(episodes, 2)).toEqual({
      PlayUris: {
        uris: ["spotify:episode:a", "spotify:episode:c"],
        offset: 1,
      },
    });
    expect(episodePlay(episodes, 1)).toBeNull();
  });

  it("finds the newest episode that can play", () => {
    expect(newestEpisode([episode(null), episode("spotify:episode:b")])).toBe(
      1,
    );
    expect(newestEpisode([episode(null)])).toBe(-1);
  });
});

describe("episodeLength", () => {
  it("speaks in minutes and hours", () => {
    expect(episodeLength(45 * 60_000)).toBe("45 min");
    expect(episodeLength(65 * 60_000)).toBe("1 hr 5 min");
    expect(episodeLength(120 * 60_000)).toBe("2 hr");
    expect(episodeLength(20_000)).toBe("20 sec");
    expect(episodeLength(0)).toBe("");
  });
});

describe("episodeDate", () => {
  it("writes a full date short and keeps a partial one", () => {
    expect(episodeDate("2026-10-01")).toBe("Oct 1, 2026");
    expect(episodeDate("2026")).toBe("2026");
  });
});

describe("resumeOf", () => {
  it("marks a played episode and the time left on a started one", () => {
    expect(
      resumeOf(
        episode("spotify:episode:a", {
          fully_played: true,
          resume_position_ms: 0,
        }),
      ),
    ).toEqual({ played: true });
    expect(
      resumeOf(
        episode("spotify:episode:a", {
          fully_played: false,
          resume_position_ms: 900_000,
        }),
      ),
    ).toEqual({ played: false, percent: 25, left: "45 min left" });
  });

  it("is null for an episode not started or without a resume point", () => {
    expect(
      resumeOf(
        episode("spotify:episode:a", {
          fully_played: false,
          resume_position_ms: 0,
        }),
      ),
    ).toBeNull();
    expect(resumeOf(episode("spotify:episode:a"))).toBeNull();
  });
});

describe("episodePager", () => {
  it("asks once for each landed page", () => {
    const pager = episodePager();
    expect(pager.ask(page(true, 50))).toBe(true);
    expect(pager.ask(page(true, 50))).toBe(false);
    pager.land("s", 100);
    expect(pager.ask(page(true, 100))).toBe(true);
    expect(pager.ask(page(true, 100))).toBe(false);
  });

  it("asks again after a refetch brings back a list of the same length", () => {
    const pager = episodePager();
    expect(pager.ask(page(true, 50))).toBe(true);
    pager.land("s", 100);
    pager.land("s", 50);
    expect(pager.ask(page(true, 50))).toBe(true);
  });

  it("asks again after another show was on screen", () => {
    const pager = episodePager();
    expect(pager.ask(page(true, 50))).toBe(true);
    pager.land("other", 50);
    pager.land("s", 50);
    expect(pager.ask(page(true, 50))).toBe(true);
  });

  it("asks for nothing once the show has no more", () => {
    expect(episodePager().ask(page(false, 50))).toBe(false);
  });
});

describe("longDescription", () => {
  it("is long past 240 characters or three lines", () => {
    expect(longDescription("short")).toBe(false);
    expect(longDescription("x".repeat(241))).toBe(true);
    expect(longDescription("a\nb\nc")).toBe(false);
    expect(longDescription("a\nb\nc\nd")).toBe(true);
  });
});

describe("links", () => {
  it("builds the open.spotify.com show and episode links", () => {
    expect(showLink("s")).toBe("https://open.spotify.com/show/s");
    expect(episodeLink("e")).toBe("https://open.spotify.com/episode/e");
  });
});

describe("heldShowId", () => {
  it("holds a show only once its episodes landed", () => {
    expect(heldShowId(page(false, 3))).toBe("s");
    expect(heldShowId({ ...page(false, 0), loaded: false })).toBe(null);
    expect(heldShowId(null)).toBe(null);
  });
});
