import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Action } from "./bindings/Action";
import type { EpisodeInfo } from "./bindings/EpisodeInfo";
import type { PodcastPage as Podcast } from "./bindings/PodcastPage";
import type { ShowPayload } from "./bindings/ShowPayload";
import { EpisodeRow, PodcastPage, ShowHeader } from "./PodcastPage";
import { byName } from "./testTree";

const episode = (
  id: string,
  resume: EpisodeInfo["resume_point"] = null,
): EpisodeInfo => ({
  id,
  uri: `spotify:episode:${id}`,
  name: `Episode ${id}`,
  duration_ms: 3_900_000,
  show_name: "",
  description: `About ${id}`,
  release_date: "2026-10-01",
  is_playable: true,
  resume_point: resume,
  image_url: null,
});

const podcast: Podcast = {
  show: {
    id: "s",
    uri: "spotify:show:s",
    name: "Lex Fridman Podcast",
    description: "Conversations about AI.",
    publisher: "Lex Fridman",
    image_url: null,
  },
  saved: false,
  loaded: true,
  episodes: [
    episode("a", { fully_played: true, resume_position_ms: 0 }),
    episode("b", { fully_played: false, resume_position_ms: 975_000 }),
    episode("c"),
  ],
  total: 400,
  has_more: true,
};

const render = (id: string, show: ShowPayload | null, playingUri = "") =>
  renderToStaticMarkup(
    <PodcastPage
      view={{}}
      id={id}
      name="Lex Fridman Podcast"
      show={show}
      statusRev={null}
      statusError={false}
      playingUri={playingUri || null}
      active
      open={() => {}}
      send={() => {}}
    />,
  );

const event = { stopPropagation() {} };

describe("PodcastPage", () => {
  it("shows a loading header with the name before the show landed", () => {
    const html = render("s", null);
    expect(html).toContain("<h1>Lex Fridman Podcast</h1>");
    expect(html).toContain("Loading episodes…");
    expect(html).not.toContain('role="listbox"');
  });

  it("shows a loading header while another show is still loaded", () => {
    const other = { ...podcast, show: { ...podcast.show, id: "x" } };
    const html = render("s", { page: other });
    expect(html).toContain("Loading episodes…");
    expect(html).not.toContain("Episode a");
  });

  it("heads the page with the kind, the publisher, the count and the description", () => {
    const html = render("s", { page: podcast });
    expect(html).toContain(">Podcast<");
    expect(html).toContain("<h1>Lex Fridman Podcast</h1>");
    expect(html).toContain("Lex Fridman");
    expect(html).toContain("400 episodes");
    expect(html).toContain("Conversations about AI.");
    expect(html).toMatch(/aria-pressed="false"[^>]*>Follow</);
    expect(html).toContain('aria-label="Copy link"');
    expect(html).toContain('aria-label="Play the newest episode"');
  });

  it("lists the episodes with their date, length and progress", () => {
    const html = render("s", { page: podcast }, "spotify:episode:c");
    expect(html).toMatch(
      /role="listbox"[^>]*aria-label="Episodes of Lex Fridman Podcast"[^>]*aria-activedescendant="episode-0"[^>]*data-focus/,
    );
    expect(html).toContain("Oct 1, 2026");
    expect(html).toContain("1 hr 5 min");
    expect(html).toContain("About a");
    expect(html).toMatch(/id="episode-0"[^]*?>Played</);
    expect(html).toMatch(/aria-valuenow="25"/);
    expect(html).toContain("49 min left");
    expect(html).toMatch(
      /class="episode-row now"[^]*?Playing: <\/span>Episode c/,
    );
    expect(html).toContain('aria-label="Copy episode link for Episode b"');
  });

  it("offers Show more only for a description the clamp cuts", () => {
    expect(render("s", { page: podcast })).not.toContain("Show more");
    const lines = {
      ...podcast,
      show: { ...podcast.show, description: "One\nTwo\nThree\nFour" },
    };
    expect(render("s", { page: lines })).toMatch(
      /aria-expanded="false"[^>]*>Show more</,
    );
  });

  it("says when a show has no episodes", () => {
    const html = render("s", { page: { ...podcast, episodes: [] } });
    expect(html).toContain("This show has no episodes yet.");
    expect(html).not.toContain("Loading episodes…");
  });

  it("shows the header and loads episodes until the first page landed", () => {
    const html = render("s", {
      page: { ...podcast, loaded: false, episodes: [] },
    });
    expect(html).toContain("400 episodes");
    expect(html).toContain("Loading episodes…");
    expect(html).not.toContain("This show has no episodes yet.");
  });
});

describe("ShowHeader", () => {
  afterEach(() => vi.unstubAllGlobals());

  const header = (page: Podcast, sent: Action[]) =>
    ShowHeader({
      page,
      expanded: false,
      onExpand: () => {},
      send: (action) => sent.push(action),
    });

  it("follows and unfollows the show", () => {
    const sent: Action[] = [];
    (byName(header(podcast, sent), "Follow").props.onClick as () => void)();
    (
      byName(header({ ...podcast, saved: true }, sent), "Following").props
        .onClick as () => void
    )();
    expect(sent).toEqual([{ SaveShow: "s" }, { UnsaveShow: "s" }]);
  });

  it("plays the newest episode with the rest after it", () => {
    const sent: Action[] = [];
    const play = byName(header(podcast, sent), "Play the newest episode");
    (play.props.onClick as (e: typeof event) => void)(event);
    expect(sent).toEqual([
      {
        PlayUris: {
          uris: ["spotify:episode:a", "spotify:episode:b", "spotify:episode:c"],
          offset: 0,
        },
      },
    ]);
  });

  it("copies the show link", async () => {
    const writeText = vi.fn(() => Promise.resolve());
    vi.stubGlobal("navigator", { clipboard: { writeText } });
    const sent: Action[] = [];
    (byName(header(podcast, sent), "Copy link").props.onClick as () => void)();
    await Promise.resolve();
    expect(writeText).toHaveBeenCalledWith("https://open.spotify.com/show/s");
    expect(sent).toEqual([{ Notify: ["Copied the show link.", 3] }]);
  });
});

describe("EpisodeRow", () => {
  afterEach(() => vi.unstubAllGlobals());

  const row = (sent: Action[], played: number[]) => (
    <EpisodeRow
      episode={episode("b")}
      index={1}
      selected={false}
      playing={false}
      onPick={() => {}}
      onPlay={(index) => played.push(index)}
      send={(action) => sent.push(action)}
    />
  );

  it("plays the row from its play button", () => {
    const played: number[] = [];
    const button = byName(row([], played), "Play Episode b");
    (button.props.onClick as (e: typeof event) => void)(event);
    expect(played).toEqual([1]);
  });

  it("copies the episode link", async () => {
    const writeText = vi.fn(() => Promise.resolve());
    vi.stubGlobal("navigator", { clipboard: { writeText } });
    const sent: Action[] = [];
    const button = byName(row(sent, []), "Copy episode link for Episode b");
    (button.props.onClick as (e: typeof event) => void)(event);
    await Promise.resolve();
    expect(writeText).toHaveBeenCalledWith(
      "https://open.spotify.com/episode/b",
    );
    expect(sent).toEqual([{ Notify: ["Copied the episode link.", 3] }]);
  });
});
