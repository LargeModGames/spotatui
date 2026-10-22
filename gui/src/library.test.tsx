import { EN } from "./i18n";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { Action } from "./bindings/Action";
import type { LikedSongs } from "./bindings/LikedSongs";
import type { PlaylistSyncPayload } from "./bindings/PlaylistSyncPayload";
import type { SourcePayload } from "./bindings/SourcePayload";
import type { SourcePlaylists } from "./bindings/SourcePlaylists";
import type { TrackInfo } from "./bindings/TrackInfo";
import type { TrackTablePayload } from "./bindings/TrackTablePayload";
import { LibraryPanel, SpotifyLogin } from "./LibraryPanel";
import { ListPage } from "./ListPage";
import type { View } from "./nav";
import { byName } from "./testTree";
import { TrackTable } from "./TrackTable";

const track = (name: string, uri: string): TrackInfo => ({
  uri,
  name,
  artists: ["Adele"],
  album: "21",
  duration_ms: 243_000,
  id: null,
  album_id: "a21",
  artist_refs: [{ id: "adele", name: "Adele" }],
  is_playable: true,
  is_local: false,
  track_number: 0,
  explicit: false,
  image_url: null,
  release_date: null,
});

const local: SourcePayload = {
  active: "Local",
  compiled: ["Spotify", "Local"],
};

const liked: LikedSongs = {
  tracks: [
    track("Rolling in the Deep", "spotify:track:1"),
    track("Set Fire to the Rain", "file:///21/05.flac"),
  ],
  total: 40,
  has_more: true,
  available: true,
  loaded: true,
};

const playlists: SourcePlaylists = {
  spotify: [],
  local: [
    {
      uri: "file:///rips",
      name: "Soundtrack CD rips",
      owner: "",
      track_count: 318,
      id: null,
      owner_id: null,
      collaborative: false,
      public: null,
      image_url: null,
    },
  ],
  subsonic: [],
  qobuz: [],
  youtube: [],
  apple_music: [],
  apple_music_uncounted: [],
  radio: [track("Score radio", "radio:https://radio.example")],
  saved_albums: [],
  saved_albums_has_more: false,
  followed_artists: [],
  saved_shows: [],
  saved_shows_has_more: false,
  saved_show_ids: [],
  folders: null,
  editable: [],
};

const sync: PlaylistSyncPayload = {
  running: false,
  last_summary: null,
  last_failed: false,
  links: [
    {
      id: "a",
      source: "Local",
      name: "Soundtrack CD rips",
      last_line: null,
      last_failed: false,
      mirrors: [
        {
          source: "Qobuz",
          matched: 3,
          unmatched: [
            {
              master_key: "k",
              title: "Levels",
              artist: "Avicii",
              reason: "NoCandidate",
            },
          ],
          last_run: null,
        },
      ],
    },
  ],
};

const panel = (
  source: SourcePayload | null,
  view: View = { page: "home" },
  payload: PlaylistSyncPayload | null = null,
  loggedOut = false,
) =>
  renderToStaticMarkup(
    <LibraryPanel
      playlists={playlists}
      artists={null}
      source={source}
      sync={payload}
      loggedOut={loggedOut}
      view={view}
      open={() => {}}
      send={() => {}}
    />,
  );

describe("LibraryPanel", () => {
  it("pins Liked Songs, then lists the active source's playlists with their song counts", () => {
    const html = panel(local);
    expect(html).toContain("Your Library");
    expect(html.indexOf("Liked Songs")).toBeLessThan(
      html.indexOf("Soundtrack CD rips"),
    );
    expect(html).toContain("Playlist · 318 songs");
    expect(html).toContain("Library health");
  });

  it("marks the open playlist as the current page", () => {
    const html = panel(local, {
      page: "playlist",
      uri: "file:///rips",
      name: "Soundtrack CD rips",
      source: "Local",
    });
    expect(html).toMatch(/title="Soundtrack CD rips" aria-current="page"/);
    expect(html).not.toMatch(/title="Liked Songs" aria-current/);
    expect(panel(local, { page: "liked" })).toMatch(
      /title="Liked Songs" aria-current="page"/,
    );
  });

  it("offers the Albums and Artists filters for Spotify only", () => {
    const spotify = panel({ ...local, active: "Spotify" });
    expect(spotify).toContain('aria-pressed="true">Playlists<');
    expect(spotify).toContain('aria-pressed="false">Albums<');
    expect(spotify).toContain('aria-pressed="false">Artists<');
    const other = panel(local);
    expect(other).toContain(">Playlists<");
    expect(other).not.toContain(">Albums<");
    expect(other).not.toContain(">Artists<");
  });

  it("offers the Spotify login only while Spotify is active and logged out", () => {
    const spotifyOnly: SourcePayload = {
      active: "Spotify",
      compiled: ["Spotify"],
    };
    const home: View = { page: "home" };
    expect(panel(spotifyOnly, home, null, true)).toContain("Log in to Spotify");
    expect(panel(spotifyOnly, home, null, false)).not.toContain(
      "Log in to Spotify",
    );
    expect(panel(local, home, null, true)).not.toContain("Log in to Spotify");
  });

  it("the login button asks the server to select Spotify", () => {
    const actions: Action[] = [];
    const login = byName(
      <SpotifyLogin send={(action) => actions.push(action)} />,
      "Log in to Spotify",
    );
    (login.props.onClick as () => void)();
    expect(actions).toEqual([{ SelectSource: "Spotify" }]);
  });

  it("selects the active source when several sources are compiled", () => {
    expect(panel(local)).toContain('aria-label="Source"');
    expect(panel(local)).toMatch(/<option value="Local" selected="">/);
    expect(panel({ active: "Local", compiled: ["Local"] })).not.toContain(
      'aria-label="Source"',
    );
  });

  it("lists stations as radio stations", () => {
    const html = panel({ ...local, active: "Radio" });
    expect(html).toContain("Score radio");
    expect(html).toContain("Radio station");
  });

  it("counts the unmatched tracks on the Library health row", () => {
    expect(panel(local, { page: "home" }, sync)).toContain("1 unmatched");
    expect(panel(local)).toContain("Playlist sync");
  });
});

const page = (
  view: Extract<View, { page: "liked" | "playlist" }>,
  options: {
    songs?: LikedSongs | null;
    table?: TrackTablePayload | null;
    playlists?: SourcePlaylists;
    playingUri?: string | null;
  } = {},
) =>
  renderToStaticMarkup(
    <ListPage
      view={view}
      liked={options.songs === undefined ? liked : options.songs}
      table={options.table ?? null}
      playlists={options.playlists ?? playlists}
      source={local}
      playingUri={options.playingUri ?? null}
      statusRev={null}
      statusError={false}
      active
      open={() => {}}
      send={() => {}}
    />,
  );

const rips: Extract<View, { page: "playlist" }> = {
  page: "playlist",
  uri: "file:///rips",
  name: "Soundtrack CD rips",
  source: "Local",
};

describe("ListPage", () => {
  it("heads an opened playlist with its library cover", () => {
    const covered = {
      ...playlists,
      local: [{ ...playlists.local[0], image_url: "file:///rips.jpg" }],
    };
    expect(page(rips, { playlists: covered })).toContain(
      'src="file:///rips.jpg"',
    );
  });

  it("heads Liked Songs with its song count and a play button", () => {
    const html = page({ page: "liked" });
    expect(html).toContain(">Liked Songs</h1>");
    expect(html).toContain("40 songs, 2 loaded");
    expect(html).toContain('aria-label="Play Liked Songs"');
  });

  it("marks the first row as the cursor and the playing row", () => {
    const html = page({ page: "liked" }, { playingUri: "file:///21/05.flac" });
    expect(html).toContain('aria-activedescendant="liked-0"');
    expect(html).toMatch(/id="liked-0"[^>]*aria-selected="true"/);
    expect(html).toMatch(/id="liked-1"[^>]*class="track-row now"/);
    expect(html).toContain('class="source-icon local"');
    expect(html).toContain(">4:03<");
  });

  it("explains an empty list: no session, still loading, or none liked", () => {
    const none = { ...liked, tracks: [], total: 0, has_more: false };
    expect(
      page({ page: "liked" }, { songs: { ...none, available: false } }),
    ).toContain("Liked Songs needs a Spotify session.");
    expect(
      page({ page: "liked" }, { songs: { ...none, loaded: false } }),
    ).toContain("Loading Liked Songs…");
    expect(page({ page: "liked" }, { songs: none })).toContain(
      "No liked songs yet.",
    );
  });

  it("shows a playlist's rows only once the table holds that playlist", () => {
    const table: TrackTablePayload = {
      uri: "file:///rips",
      tracks: liked.tracks,
      has_more: true,
      positions: null,
      radio: null,
    };
    expect(page(rips, { table: { ...table, uri: "file:///other" } })).toContain(
      "Loading Soundtrack CD rips…",
    );
    const html = page(rips, { table });
    expect(html).toContain(">Soundtrack CD rips</h1>");
    expect(html).toContain("2+ songs");
    expect(html).toContain('id="playlist-1"');
    expect(
      page(rips, { table: { ...table, tracks: [], has_more: false } }),
    ).toContain("This playlist is empty.");
  });
});

describe("TrackTable", () => {
  const table = (
    props: Partial<{
      showAlbum: boolean;
      showCover: boolean;
      open: (view: View) => void;
    }>,
  ) =>
    renderToStaticMarkup(
      <TrackTable
        label="Songs"
        idPrefix="album"
        tracks={[track("Hello", "spotify:track:h")]}
        cursor={0}
        playingUri={null}
        onKeyDown={() => {}}
        onPick={() => {}}
        onPlay={() => {}}
        {...props}
      />,
    );

  it("links the artist and the album only when it can open them", () => {
    const linked = table({ open: () => {} });
    expect(linked).toContain('class="link" tabindex="-1">Adele<');
    expect(linked).toContain('class="link" tabindex="-1">21<');
    expect(table({})).not.toContain('class="link"');
  });

  it("links no artist or album of a row from another source, ids or not", () => {
    const html = renderToStaticMarkup(
      <TrackTable
        label="Songs"
        idPrefix="sub"
        tracks={[track("Hello", "subsonic:track:h")]}
        cursor={0}
        playingUri={null}
        onKeyDown={() => {}}
        onPick={() => {}}
        onPlay={() => {}}
        open={() => {}}
      />,
    );
    expect(html).not.toContain('class="link"');
  });

  it("drops the album column and the cover when asked", () => {
    const full = table({});
    expect(full).toContain(">Album<");
    expect(full).toContain("cover-art");
    const bare = table({ showAlbum: false, showCover: false });
    expect(bare).not.toContain(">Album<");
    expect(bare).not.toContain("cover-art");
    expect(bare).toContain('aria-activedescendant="album-0"');
  });

  it("marks only the explicit rows with an E badge, whatever the source", () => {
    const html = renderToStaticMarkup(
      <TrackTable
        label="Songs"
        idPrefix="mix"
        tracks={[
          { ...track("Rude", "spotify:track:r"), explicit: true },
          { ...track("Hello", "qobuz:track:h"), explicit: true },
          track("Clean", "spotify:track:c"),
        ]}
        cursor={0}
        playingUri={null}
        onKeyDown={() => {}}
        onPick={() => {}}
        onPlay={() => {}}
        open={() => {}}
      />,
    );
    expect(html.match(/class="explicit"/g)).toHaveLength(2);
    expect(html).toContain('<span class="sr-only">Explicit</span>');
  });
});

it("the explicit badge translates its title and accessible text while keeping its E glyph", () => {
  const html = renderToStaticMarkup(
    <TrackTable
      s={{ ...EN, explicit: "alternate explicit label" }}
      label="Songs"
      idPrefix="explicit"
      tracks={[{ ...track("Rude", "spotify:track:r"), explicit: true }]}
      cursor={0}
      playingUri={null}
      onKeyDown={() => {}}
      onPick={() => {}}
      onPlay={() => {}}
      open={() => {}}
    />,
  );
  expect(html).toContain('title="alternate explicit label"');
  expect(html).toContain(
    '<span class="sr-only">alternate explicit label</span>',
  );
  expect(html).toContain('aria-hidden="true">E</span>');
  expect(html).not.toContain('title="Explicit"');
});
