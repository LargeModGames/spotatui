import type { EpisodeInfo } from "../src/bindings/EpisodeInfo";
import strings from "../src/bindings/strings.json" with { type: "json" };
import type { NowPlaying } from "../src/bindings/NowPlaying";
import type { PlaylistInfo } from "../src/bindings/PlaylistInfo";
import type { ResumePointInfo } from "../src/bindings/ResumePointInfo";
import type { ServerMessage } from "../src/bindings/ServerMessage";
import type { SettingRow } from "../src/bindings/SettingRow";
import type { ShowInfo } from "../src/bindings/ShowInfo";
import type { Source } from "../src/bindings/Source";
import type { SourcePlaylists } from "../src/bindings/SourcePlaylists";
import type { TrackInfo } from "../src/bindings/TrackInfo";
import type { VisualizerLook } from "../src/bindings/VisualizerLook";

const revisions = {
  route: 1,
  status: 1,
  source: 1,
  theme: 1,
  playback: 1,
  party: 1,
  devices: 1,
  search: 1,
  lyrics: 1,
  artist: 1,
  library: 1,
  liked: 1,
  queue: 1,
  stats: 1,
  album: 1,
  session: 1,
  discover: 1,
  playlist_sync: 1,
  track_table: 1,
  recently_played: 1,
  liked_marks: 1,
  settings: 1,
  update: 1,
  show: 1,
  announcement: 1,
};

/** The Rust defaults of `VisualizerLook`, for pages and tests with no server. */
export const DEFAULT_LOOK: VisualizerLook = {
  source: "cover",
  colors: [],
  fallback: [],
  count: 5,
  order: "luminance",
  reverse: false,
  greys: false,
  min_distance: 0.08,
  min_share: 0.01,
  min_saturation: 0,
  luminance_floor: 0.22,
  luminance_ceiling: 0.35,
  interpolation: "srgb",
  fade_ms: 800,
  spectrum: {
    gap: 2,
    cap_height: 2,
    cap_gap: 2,
    glow_side: 2,
    glow_end: 3,
    foot_alpha: 0.28,
    top_alpha: 0.08,
    glow_alpha: 0.12,
    glow_top_alpha: 0.036,
    cap_alpha: 0.7,
    pulse_alpha: 0.35,
    pulse_height: 0.5,
    pulse_bands: 0.125,
    fall_per_second: 3,
    peak_hold: 0.35,
    peak_gravity: 2.4,
  },
  waveform: {
    reach: 0.46,
    fill_alpha: 0.14,
    point_spacing: 8,
    min_points: 75,
    lines: [
      [-0.6, 1, 0.18],
      [1, 9, 0.06],
      [1, 4, 0.16],
      [1, 1.6, 0.85],
    ],
  },
};

export const hello: ServerMessage = {
  kind: "hello",
  payload: {
    version: "0.0.0-shot",
    token: "shot",
    revisions,
    window: false,
    language: "en",
    strings: strings.EN,
  },
};

const sourceChoices: [Source, string, string][] = [
  ["Spotify", "Spotify", "needs login"],
  ["Local", "Local Files", "free"],
  ["Subsonic", "Subsonic", "free, needs a Subsonic/Navidrome server"],
  ["Radio", "Internet Radio", "free"],
  ["YouTube", "YouTube", "free, needs the yt-dlp binary"],
  ["Qobuz", "Qobuz", "paid subscription, logs in through the browser"],
];

export const onboarding: ServerMessage = {
  kind: "onboarding",
  payload: {
    transcript: "Welcome to spotatui.\n",
    pending: {
      seq: 1,
      ask: {
        kind: "PickSources",
        options: sourceChoices.map(([source, label, note]) => ({
          source,
          label,
          note,
        })),
      },
    },
  },
};

function track(
  name: string,
  seconds: number,
  uri = `file:///music/Adele/21/${name}.flac`,
  artist = "Adele",
): TrackInfo {
  return {
    uri,
    name,
    artists: [artist],
    album: "21",
    duration_ms: seconds * 1000,
    id: null,
    album_id: null,
    artist_refs: [],
    is_playable: true,
    is_local: true,
    track_number: 0,
    explicit: false,
    image_url: null,
    release_date: null,
  };
}

const nowPlaying: NowPlaying = {
  title: "Set Fire to the Rain",
  artists: ["Adele"],
  album: "21",
  image_url: null,
  duration_ms: 242973,
  uri: "file:///music/Adele/21/Set Fire to the Rain.flac",
  is_playing: false,
  is_live: false,
  shuffle: false,
  repeat: "off",
  context_uri: null,
  context_name: null,
  quality: null,
};

/** Adele's 21 from song 5, paused at 1:40, so the frame does not move between runs; one queued song per other source. */
export const playing: ServerMessage[] = [
  hello,
  { kind: "route", rev: 1, payload: "home" },
  {
    kind: "playback",
    rev: 1,
    payload: { item: nowPlaying, volume: 72, device: "This PC", liked: true },
  },
  { kind: "tick", payload: 100000 },
  {
    kind: "queue",
    rev: 1,
    payload: {
      now: track("Set Fire to the Rain", 243),
      native: [
        track("He Won't Go", 278),
        track("Take It All", 228),
        track("I'll Be Waiting", 241),
        track("One and Only", 348),
        track("Lovesong", 316),
        track("Someone Like You", 285),
        track("Freeze", 487, "spotify:track:freeze", "Kygo"),
        track("On the Nature of Daylight", 372, "qobuz:track:1", "Max Richter"),
        track("Tum Hi Ho", 262, "subsonic:track:1", "Arijit Singh"),
        track("Cornfield Chase", 126, "youtube:cornfield", "Hans Zimmer"),
        track("Film score radio", 0, "radio:https://radio.example/score", ""),
      ],
      spotify: { currently_playing: null, items: [] },
    },
  },
];

function playlist(
  name: string,
  uri: string,
  track_count: number,
): PlaylistInfo {
  return {
    uri,
    name,
    owner: "jay",
    track_count,
    id: null,
    owner_id: null,
    collaborative: false,
    public: null,
    image_url: null,
  };
}

const likedRows: [string, string, string, number, string?][] = [
  ["Firework", "Katy Perry", "Teenage Dream", 228],
  [
    'Suite from "How to Train Your Dragon"',
    "John Powell",
    "Film Suites, Vol. 1",
    602,
  ],
  ["Freeze", "Kygo", "Freeze", 487],
  ["Set Fire to the Rain", "Adele", "21", 243, nowPlaying.uri ?? undefined],
  [
    "Angels For Each Other",
    "Martin Garrix, Arijit Singh",
    "Angels For Each Other",
    215,
  ],
  [
    "On the Nature of Daylight",
    "Max Richter",
    "The Blue Notebooks (15 Years)",
    372,
  ],
  ["Tum Hi Ho", "Arijit Singh, Mithoon", "Aashiqui 2", 262],
  ["Firestone", "Kygo, Conrad Sewell", "Cloud Nine", 272],
  [
    "Clair-Obscur",
    "Lorien Testard, Alice Duport-Percier",
    "Clair Obscur: Expedition 33",
    219,
  ],
  ["Someone Like You", "Adele", "21", 285],
  ["Faded", "Alan Walker", "Different World", 212],
  ["Nuvole Bianche", "Ludovico Einaudi", "Una Mattina", 358],
  ["Levels", "Avicii", "Levels", 200],
  ["When We Were Young", "Adele", "25", 291],
  ["Wake Me Up", "Avicii", "True", 247],
];

/** The browse scope and the Spotify Liked Songs of a five-source build. */
export const library: ServerMessage[] = [
  {
    kind: "source",
    rev: 1,
    payload: {
      active: "Spotify",
      compiled: ["Spotify", "YouTube", "Subsonic", "Radio", "Local", "Qobuz"],
    },
  },
  {
    kind: "library",
    rev: 1,
    payload: {
      spotify: [
        playlist("Film scores", "spotify:playlist:1", 142),
        playlist("Kygo & friends", "spotify:playlist:2", 87),
        playlist("Bollywood love songs", "spotify:playlist:3", 64),
        playlist("Gym: EDM", "spotify:playlist:4", 51),
        playlist("Discover Weekly", "spotify:playlist:5", 30),
      ],
      local: [playlist("Soundtrack CD rips", "file:///music/rips", 318)],
      subsonic: [],
      qobuz: [],
      youtube: [],
      apple_music: [],
      apple_music_uncounted: [],
      radio: [],
      saved_albums: [],
      saved_albums_has_more: false,
      followed_artists: [],
      followed_artists_has_more: false,
      saved_shows: [],
      saved_shows_has_more: false,
      saved_show_ids: [],
      folders: null,
      editable: [
        "spotify:playlist:1",
        "spotify:playlist:2",
        "spotify:playlist:4",
      ],
    },
  },
  {
    kind: "liked",
    rev: 1,
    payload: {
      tracks: likedRows.map(([name, artist, album, seconds, uri], index) => ({
        ...track(name, seconds, uri ?? `spotify:track:${index}`, artist),
        album,
        is_local: false,
      })),
      total: 1180,
      has_more: true,
      available: true,
      loaded: true,
    },
  },
];

/** No Spotify session: the Liked Songs list cannot load, and the sidebar is bare. */
export const libraryUnavailable: ServerMessage[] = [
  {
    kind: "source",
    rev: 1,
    payload: { active: "Local", compiled: ["Spotify", "Local"] },
  },
  {
    kind: "library",
    rev: 1,
    payload: {
      spotify: [],
      local: [],
      subsonic: [],
      qobuz: [],
      youtube: [],
      apple_music: [],
      apple_music_uncounted: [],
      radio: [],
      saved_albums: [],
      saved_albums_has_more: false,
      followed_artists: [],
      followed_artists_has_more: false,
      saved_shows: [],
      saved_shows_has_more: false,
      saved_show_ids: [],
      folders: null,
      editable: [],
    },
  },
  {
    kind: "liked",
    rev: 1,
    payload: {
      tracks: [],
      total: 0,
      has_more: false,
      available: false,
      loaded: false,
    },
  },
];

/** A Spotify-only build after a failed first login. */
export const spotifyLoggedOut: ServerMessage[] = [
  {
    kind: "source",
    rev: 1,
    payload: { active: "Spotify", compiled: ["Spotify"] },
  },
  ...libraryUnavailable.filter((message) => message.kind !== "source"),
];

/** A booted app with nothing playing, no device and an empty queue. */
export const idle: ServerMessage[] = [
  hello,
  { kind: "route", rev: 1, payload: "home" },
  {
    kind: "playback",
    rev: 1,
    payload: { item: null, volume: 50, device: null, liked: false },
  },
  { kind: "tick", payload: null },
  {
    kind: "queue",
    rev: 1,
    payload: {
      now: null,
      native: [],
      spotify: { currently_playing: null, items: [] },
    },
  },
];

function hit(
  name: string,
  id: string,
  album: string,
  artists: string,
  seconds: number,
): TrackInfo {
  return {
    ...track(name, seconds, `spotify:track:${id}`, artists),
    id,
    album,
    is_local: false,
  };
}

/** A Spotify search for Kygo: an artist, four albums, six songs, two of them liked. */
export const search: ServerMessage[] = [
  {
    kind: "search",
    rev: 1,
    payload: {
      ran: true,
      query: "kygo",
      tracks: [
        hit("Freeze", "f1", "Freeze", "Kygo", 487),
        hit("Firestone", "f2", "Cloud Nine", "Kygo, Conrad Sewell", 272),
        hit("Stay", "f3", "Cloud Nine", "Kygo, Maty Noyes", 239),
        hit("Stole the Show", "f4", "Cloud Nine", "Kygo, Parson James", 223),
        hit("Carry Me", "f5", "Cloud Nine", "Kygo, Julia Michaels", 233),
        hit("Raging", "f6", "Cloud Nine", "Kygo, Kodaline", 224),
      ],
      artists: [
        {
          id: "kygo",
          uri: "spotify:artist:kygo",
          name: "Kygo",
          image_url: null,
        },
      ],
      albums: [
        ["Cloud Nine", "2016-05-13"],
        ["Kids in Love", "2017-11-03"],
        ["Golden Hour", "2020-05-29"],
        ["KYGO", "2024-06-14"],
      ].map(([name, release_date], index) => ({
        id: `a${index}`,
        uri: `spotify:album:a${index}`,
        name,
        artists: [{ id: "kygo", name: "Kygo" }],
        album_type: "album",
        release_date,
        total_tracks: null,
        image_url: null,
        tracks: [],
      })),
      playlists: [],
      shows: [],
      liked: ["f1", "f2"],
    },
  },
];

function statsRow(title: string, artist: string | null = null) {
  return {
    title,
    artist,
    uri: null,
    listened_ms: 1,
    all_time_rank: null,
    new: false,
  };
}

/** The Stats of the canvas: 30 days selected, three movements. */
export const stats: ServerMessage[] = [
  {
    kind: "stats",
    rev: 2,
    payload: {
      period: "30d",
      loading: false,
      loaded: true,
      plays: [
        ["7d", 293],
        ["30d", 1043],
        ["month", 980],
        ["year", 3810],
        ["all", 4324],
      ].map(([period, plays]) => ({
        period: String(period),
        plays: Number(plays),
      })),
      top_artists: (
        [
          ["Kygo", 1],
          ["John Powell", 2],
          ["Martin Garrix", 3],
          ["Alan Walker", 9],
          ["Adele", 8],
          ["Ed Sheeran", 6],
          ["Avicii", 4],
          ["Sasha Alex Sloan", 7],
          ["James Horner", 31],
          ["Dean Lewis", 11],
        ] as const
      ).map(([title, rank], index) => ({
        ...statsRow(title),
        all_time_rank: rank,
        new: index === 8,
      })),
      top_albums: [
        "Cloud Nine",
        "Film Suites, Vol. 1",
        "Golden Hour",
        "Aashiqui 2",
        "How to Train Your Dragon (Original Motion Picture Soundtrack)",
        "True",
        '"Avatar" Music From The Motion Picture',
        "How to Train Your Dragon 2 (Music from the Motion Picture)",
        "25",
        "KYGO",
      ].map((title) => statsRow(title)),
      top_tracks: [
        ['My Heart Will Go On (Love Theme from "Titanic")', "Céline Dion"],
        ["Freeze", "Kygo"],
        ["Life is a Highway", "Rascal Flatts"],
        [
          'Becoming one of "The People" Becoming one with Neytiri',
          "James Horner",
        ],
        ["Firestone", "Kygo"],
        ["Rewrite The Stars", "Zac Efron"],
        ["Einaudi: Experience", "Daniel Hope"],
        ["Superheroes", "The Script"],
        ['Suite from "How to Train Your Dragon"', "John Powell"],
        ["Lose Somebody", "Kygo"],
      ].map(([title, artist]) => statsRow(title, artist)),
      week_tracks: [
        ['Suite from "How to Train Your Dragon"', "John Powell"],
        ["Giorgio by Moroder", "Thomas Bangalter"],
        ["A Million Dreams", "Ziv Zaifman"],
        ["Freeze", "Kygo"],
        ['Becoming one of "The People"', "James Horner"],
      ].map(([title, artist]) => statsRow(title, artist)),
      movements: [
        { name: "Alan Walker", kind: "climb", from: 9, to: 4 },
        { name: "Arijit Singh", kind: "fall", from: 5, to: 14 },
        { name: "James Horner", kind: "new", from: null, to: 9 },
      ],
    },
  },
];

/** No party, with a Spotify session. */
export const partyNone: ServerMessage[] = [
  {
    kind: "party",
    rev: 1,
    payload: { phase: "disconnected", room: null, available: true },
  },
];

/** A hosted room with two guests. */
export const partyHosting: ServerMessage[] = [
  {
    kind: "party",
    rev: 2,
    payload: {
      phase: "hosting",
      available: true,
      room: {
        host: true,
        code: "K7Q2ZD",
        host_name: "Host",
        guests: ["Alex", "Sam"],
        shared_control: false,
      },
    },
  },
];

const adele21 = [
  ["Rolling in the Deep", 228],
  ["Rumour Has It", 223],
  ["Turning Tables", 250],
  ["Don't You Remember", 243],
  ["Set Fire to the Rain", 243],
  ["He Won't Go", 278],
  ["Take It All", 228],
  ["I'll Be Waiting", 241],
  ["One and Only", 348],
  ["Lovesong", 316],
  ["Someone Like You", 285],
] as const;

/** Adele's 21 on Spotify as the play context, song 5, with lyrics and two albums queued after it. */
export const room: ServerMessage[] = [
  {
    kind: "playback",
    rev: 2,
    payload: {
      item: {
        ...nowPlaying,
        uri: "spotify:track:a4",
        context_uri: "spotify:album:21",
      },
      volume: 72,
      device: "This PC",
      liked: true,
    },
  },
  {
    kind: "album",
    rev: 1,
    payload: {
      saved: false,
      album: {
        id: "21",
        uri: "spotify:album:21",
        name: "21",
        artists: [{ id: "adele", name: "Adele" }],
        album_type: "album",
        release_date: "2011-01-24",
        total_tracks: 11,
        image_url: null,
        tracks: adele21.map(([name, seconds], index) => ({
          ...track(name, seconds, `spotify:track:a${index}`),
          album: "",
          is_local: false,
          track_number: index + 1,
        })),
      },
    },
  },
  {
    kind: "lyrics",
    rev: 1,
    payload: {
      status: "found",
      synced: true,
      lines: [
        [0, "I let it fall, my heart"],
        [9000, "And as it fell, you rose to claim it"],
        [18000, "It was dark and I was over"],
        [26000, "Until you kissed my lips and you saved me"],
        [100000, "But I set fire to the rain"],
        [106000, "Watched it pour as I touched your face"],
        [112000, "Well, it burned while I cried"],
        [118000, "'Cause I heard it screaming out your name"],
      ].map(([at_ms, text]) => ({ at_ms: Number(at_ms), text: String(text) })),
    },
  },
  {
    kind: "queue",
    rev: 2,
    payload: {
      now: null,
      native: [],
      spotify: {
        currently_playing: null,
        items: [
          track("Firestone", 272, "spotify:track:k1", "Kygo"),
          track("Stay", 239, "spotify:track:k2", "Kygo"),
          track("Suite", 602, "spotify:track:p1", "John Powell"),
        ].map((entry, index) => ({
          kind: "track" as const,
          track: {
            ...entry,
            album: index < 2 ? "Cloud Nine" : "Film Suites, Vol. 1",
            is_local: false,
          },
          episode: null,
        })),
      },
    },
  },
];

const sessionStart = Date.UTC(2026, 8, 29, 17, 40);

/** Four Spotify plays, a six-minute pause, then Adele's 21 from local files. */
export const session: ServerMessage[] = [
  {
    kind: "session",
    rev: 1,
    payload: (
      [
        ["More Than You Know", "Axwell & Ingrosso", "spotify:track:s1", 0],
        [
          "Harder, Better, Faster, Stronger",
          "Daft Punk",
          "spotify:track:s2",
          4,
        ],
        ["Shallow", "Lady Gaga", "spotify:track:s3", 8],
        ["Firework", "Katy Perry", "spotify:track:s4", 12],
        ["Rolling in the Deep", "Adele", "file:///music/Adele/21/01.flac", 22],
        ["Rumour Has It", "Adele", "file:///music/Adele/21/02.flac", 26],
        ["Turning Tables", "Adele", "file:///music/Adele/21/03.flac", 30],
        ["Don't You Remember", "Adele", "file:///music/Adele/21/04.flac", 34],
      ] as const
    ).map(([title, artist, uri, minute]) => ({
      started_at_ms: sessionStart + minute * 60_000,
      ended_at_ms: sessionStart + (minute + 4) * 60_000,
      listened_ms: 4 * 60_000,
      duration_ms: 4 * 60_000,
      title,
      artists: [artist],
      album: uri.startsWith("file") ? "21" : "",
      uri,
      image_url: null,
    })),
  },
];

/** Spotify's Made For You page: three shelves. */
export const discover: ServerMessage[] = [
  {
    kind: "discover",
    rev: 1,
    payload: {
      available: true,
      loading: false,
      shelves: (
        [
          [
            "Uniquely yours",
            [
              ["daylist", "Your day in a playlist."],
              ["On Repeat", "Songs you love right now"],
              ["Repeat Rewind", "Your past favorites"],
            ],
          ],
          [
            "Your Daily Mixes",
            [
              ["Daily Mix 1", "Martin Garrix, Alan Walker, Avicii and more"],
              ["Daily Mix 2", "SHAUN, Kygo, Jonas Blue and more"],
              [
                "Daily Mix 3",
                "James Horner, John Powell, Hans Zimmer and more",
              ],
            ],
          ],
          [
            "Discover New Music",
            [
              [
                "Release Radar",
                "Catch all the latest music from artists you follow.",
              ],
              ["Discover Weekly", "Your weekly mixtape of fresh music."],
            ],
          ],
        ] as const
      ).map(([title, playlists], shelf) => ({
        title,
        playlists: playlists.map(([name, description], index) => ({
          uri: `spotify:playlist:mfy${shelf}${index}`,
          name,
          description,
          image_url: null,
        })),
      })),
    },
  },
];

/** Two linked playlists: one with unmatched tracks on Qobuz, one never synced. */
export const playlistSync: ServerMessage[] = [
  {
    kind: "playlist_sync",
    rev: 1,
    payload: {
      running: false,
      last_summary: "Playlist sync: 2 links, 3 added, 0 removed, 3 unmatched",
      last_failed: false,
      links: [
        {
          id: "road",
          source: "Spotify",
          name: "Road Trip",
          last_line: "Road Trip: 3 added, 0 removed, 3 unmatched",
          last_failed: false,
          mirrors: [
            {
              source: "Qobuz",
              matched: 61,
              unmatched: [
                ["Levels", "Avicii", "NoCandidate"],
                ["Local demo", "Jay", "NotSyncable"],
                ["Freeze", "Kygo", "NoCandidate"],
              ].map(([title, artist, reason], index) => ({
                master_key: `k${index}`,
                title,
                artist,
                reason: reason as "NoCandidate" | "NotSyncable",
              })),
              last_run: "2026-09-29T10:12:00Z",
            },
            {
              source: "YouTube",
              matched: 64,
              unmatched: [],
              last_run: "2026-09-29T10:12:00Z",
            },
          ],
        },
        {
          id: "focus",
          source: "Qobuz",
          name: "Focus",
          last_line: "Focus: skipped (Subsonic is not configured)",
          last_failed: false,
          mirrors: [
            { source: "Subsonic", matched: 0, unmatched: [], last_run: null },
          ],
        },
      ],
    },
  },
];

/** The rows of the Film scores playlist, landed in the track table. */
export const filmScores: ServerMessage[] = [
  {
    kind: "track_table",
    rev: 1,
    payload: {
      uri: "spotify:playlist:1",
      tracks: [
        ["Time", "Hans Zimmer", 275],
        ["Cornfield Chase", "Hans Zimmer", 126],
        ["Now We Are Free", "Hans Zimmer", 254],
        ["Concerning Hobbits", "Howard Shore", 175],
      ].map(([name, artist, seconds], index) => ({
        ...track(
          name as string,
          seconds as number,
          `spotify:track:f${index}`,
          artist as string,
        ),
        id: `f${index}`,
        album_id: "interstellar",
        artist_refs: [{ id: "zimmer", name: artist as string }],
        is_local: false,
        explicit: index === 1,
      })),
      has_more: false,
      positions: [0, 1, 2, 3],
      radio: null,
    },
  },
  { kind: "liked_marks", rev: 1, payload: ["f1"] },
];

/**
 * A long playlist with explicit songs spread through it, for the page-scroll
 * check: with unanchored `.sr-only` text the document grows past the window
 * whenever an explicit row sits below the fold (#878). Derived from the
 * library's own playlist shape; the shared filmScores fixture stays as is.
 */
export const filmScoresLongExplicit: ServerMessage[] = [
  {
    kind: "track_table",
    rev: 1,
    payload: {
      uri: "spotify:playlist:1",
      tracks: Array.from({ length: 40 }, (_, index) => ({
        ...track(
          `Long cut ${index + 1}`,
          200 + index,
          `spotify:track:g${index}`,
          "Hans Zimmer",
        ),
        id: `g${index}`,
        album_id: "interstellar",
        artist_refs: [{ id: "zimmer", name: "Hans Zimmer" }],
        is_local: false,
        explicit: index % 7 === 3,
      })),
      has_more: false,
      positions: Array.from({ length: 40 }, (_, index) => index),
      radio: null,
    },
  },
];

/** Two songs of Adele's 21 played lately, so Home shows the album. */
export const recentlyPlayed: ServerMessage[] = [
  {
    kind: "recently_played",
    rev: 1,
    payload: adele21.slice(0, 2).map(([name, seconds], index) => ({
      ...track(name, seconds, `spotify:track:a${index}`),
      album: "21",
      album_id: "21",
      artist_refs: [{ id: "adele", name: "Adele" }],
      is_local: false,
    })),
  },
];

const BANNER = `
 ███████╗██████╗  ██████╗ ████████╗ █████╗ ████████╗██╗   ██╗██╗
 ██╔════╝██╔══██╗██╔═══██╗╚══██╔══╝██╔══██╗╚══██╔══╝██║   ██║██║
 ███████╗██████╔╝██║   ██║   ██║   ███████║   ██║   ██║   ██║██║
 ╚════██║██╔═══╝ ██║   ██║   ██║   ██╔══██║   ██║   ██║   ██║██║
 ███████║██║     ╚██████╔╝   ██║   ██║  ██║   ██║   ╚██████╔╝██║
 ╚══════╝╚═╝      ╚═════╝    ╚═╝   ╚═╝  ╚═╝   ╚═╝    ╚═════╝ ╚═╝
`;

/** The Home page: the welcome lines and a changelog of four releases. */
export const welcome: ServerMessage[] = [
  {
    kind: "welcome",
    payload: {
      streak: { days: "12-day listening streak", note: " · 1h 4m today" },
      counter: "Global songs played with spotatui: 1,284,377",
      discord: "Join the community on Discord: https://spotatui.com/discord",
      banner: BANNER,
      banner_gradient: false,
    },
  },
  {
    kind: "announcement",
    rev: 1,
    payload: {
      active: null,
      history: [
        {
          id: "shot-history-login",
          title: "Spotify login change",
          body: "Spotify changes its login on 1 November. Update spotatui before then to keep native playback.",
          level: "warning",
          url: "https://spotatui.com/news",
        },
        {
          id: "shot-history-survey",
          title: "Tell us what you play",
          body: "The source survey is open until the end of the month.",
          level: "info",
          url: null,
        },
      ],
    },
  },
  {
    kind: "changelog",
    payload: `# Changelog

## [v0.47.0] 2026-10-20

### Added

- **A Welcome page in the desktop app**: the spotatui banner, your listening streak and the changelog of the version you run ([#900](https://github.com/LargeModGames/spotatui/issues/900)).

- **Search settings**: filter by section, setting name or description, and press \`Escape\` to clear the search.

### Fixed

- **The playbar no longer jumps back while a song plays**.

## [v0.46.0] 2026-10-14

### Added

- **Play on Cast and Spotify Connect speakers on your network**: Cast groups are not supported yet.
  - Sonos rooms show as one device each.

### Changed

- **Discover shows Spotify's Made For You playlists**.

## [v0.45.0] 2026-10-12

### Fixed

- **Subsonic usernames with special characters can log in** ([#814](https://github.com/LargeModGames/spotatui/issues/814)).

## [v0.44.0] 2026-10-12

### Added

- **Mini player**.
`,
  },
];

/** Adele's artist page: her top songs from 21 and the album. */
export const artist: ServerMessage[] = [
  {
    kind: "artist",
    rev: 1,
    payload: {
      page: {
        id: "adele",
        name: "Adele",
        image_url: null,
        following: false,
        top_tracks: adele21.slice(0, 6).map(([name, seconds], index) => ({
          ...track(name, seconds, `spotify:track:a${index}`),
          album: "21",
          album_id: "21",
          is_local: false,
        })),
        albums: [
          {
            id: "21",
            uri: "spotify:album:21",
            name: "21",
            artists: [{ id: "adele", name: "Adele" }],
            album_type: "album",
            release_date: "2011-01-24",
            total_tracks: 11,
            image_url: null,
            tracks: [],
          },
        ],
        related_artists: [
          { id: "sam", uri: null, name: "Sam Smith", image_url: null },
        ],
      },
    },
  },
];

function setting(
  id: string,
  group: string,
  name: string,
  description: string,
  value: SettingRow["value"],
): SettingRow {
  return {
    id: `behavior.${id}`,
    group,
    name,
    description,
    value,
    button: null,
  };
}

function visualizerRow(mode: "Spectrum" | "Waveform"): SettingRow {
  const options = ["Off", "Spectrum", "Waveform"];
  return setting(
    "player_bar_visualizer",
    "interface",
    "Player Bar Visualizer",
    "What the desktop app draws behind its player bar while this computer plays; a click on empty space in the bar changes it",
    { kind: "choice", value: mode, options, labels: options },
  );
}

/** Only the visualizer row, set to `mode`, with `look`. */
export const visualizerSetting = (
  mode: "Spectrum" | "Waveform",
  look: VisualizerLook = DEFAULT_LOOK,
): ServerMessage => ({
  kind: "settings",
  rev: 1,
  payload: {
    language: "en",
    strings: strings.EN,
    rows: [visualizerRow(mode)],
    visualizer: look,
  },
});

/** A cover in four bands: navy, coral, teal and sand, largest first. */
export const stripedCover = `<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64">
<rect width="26" height="64" fill="#1d3557"/>
<rect x="26" width="19" height="64" fill="#e63946"/>
<rect x="45" width="13" height="64" fill="#2a9d8f"/>
<rect x="58" width="6" height="64" fill="#f4a261"/>
</svg>`;

/** A loud bass and a quieter treble, as most music has. */
export const spectrumFrame: ServerMessage = {
  kind: "visualizer",
  payload: {
    levels: Array.from({ length: 75 }, (_, band) =>
      Math.round(
        255 *
          Math.min(
            1,
            Math.max(0, 0.85 - band / 110 + 0.12 * Math.sin(band / 2.5)),
          ),
      ),
    ),
    wave: [],
  },
};

export const waveFrame: ServerMessage = {
  kind: "visualizer",
  payload: {
    levels: [],
    wave: Array.from({ length: 320 }, (_, point) =>
      Math.round(
        127 * (0.55 * Math.sin(point / 6) + 0.25 * Math.sin(point / 1.7)),
      ),
    ),
  },
};

/** The desktop app's settings rows, with a downloaded update waiting to install. */
export const settings: ServerMessage[] = [
  {
    kind: "settings",
    rev: 1,
    payload: {
      language: "en",
      strings: strings.EN,
      visualizer: DEFAULT_LOOK,
      rows: [
        setting(
          "seek_milliseconds",
          "playback",
          "Seek Duration (ms)",
          "Milliseconds to skip when seeking",
          { kind: "number", value: 5000 },
        ),
        setting(
          "volume_increment",
          "playback",
          "Volume Increment",
          "Volume change per keypress (0-100)",
          { kind: "number", value: 10 },
        ),
        setting(
          "stop_after_current_track",
          "playback",
          "Stop After Current Track",
          "Pause playback when the current track finishes",
          { kind: "bool", value: false },
        ),
        setting(
          "enable_media_keys",
          "playback",
          "Media Key Controls",
          "Let OS media keys, headphone buttons, and remote controls (playerctl, Now Playing) control playback",
          { kind: "bool", value: true },
        ),
        visualizerRow("Spectrum"),
        setting(
          "enable_streaming",
          "streaming",
          "Native Streaming",
          "Play Spotify in spotatui as a Connect device (applies on restart)",
          { kind: "bool", value: true },
        ),
        setting(
          "streaming_device_name",
          "streaming",
          "Streaming Device Name",
          "The Connect device name other Spotify apps show (applies on restart)",
          { kind: "text", value: "spotatui" },
        ),
        setting(
          "streaming_bitrate",
          "streaming",
          "Streaming Bitrate",
          "Native streaming quality in kbps (applies on restart)",
          {
            kind: "choice",
            value: "320",
            options: ["96", "160", "320"],
            labels: ["96", "160", "320"],
          },
        ),
        {
          ...setting(
            "soloist_path",
            "streaming",
            "Soloist Path",
            "The Soloist program the Soloist engine runs",
            { kind: "text", value: "" },
          ),
          button: { label: "Download from Spotify", busy: false },
        },
        setting(
          "enable_discord_rpc",
          "integrations",
          "Discord Rich Presence",
          "Show your current track in Discord",
          { kind: "bool", value: true },
        ),
        setting(
          "subsonic_url",
          "sources",
          "Subsonic Server",
          "Address of your Subsonic or Navidrome server",
          { kind: "text", value: "https://music.example.net" },
        ),
        setting(
          "subsonic_password",
          "sources",
          "Subsonic Password",
          "Saved in config.yml; SPOTATUI_SUBSONIC_PASSWORD overrides it",
          { kind: "secret", set: true },
        ),
        setting(
          "local_music_path",
          "sources",
          "Music Folder",
          "The folder Local Files reads",
          { kind: "text", value: "" },
        ),
        setting(
          "disable_auto_update",
          "updates",
          "Disable Auto-Update",
          "Skip the checks at launch and every 6 hours; check, download and install updates from this page yourself",
          { kind: "bool", value: false },
        ),
        setting(
          "auto_update_delay",
          "updates",
          "Auto-Update Delay",
          "How long a release must be out before it downloads: '0' for at once, or for example '10m', '2h', '7d'",
          { kind: "text", value: "0" },
        ),
      ],
    },
  },
  { kind: "update", rev: 1, payload: { state: "ready", version: "0.44.0" } },
];

function withLibrary(change: Partial<SourcePlaylists>): ServerMessage[] {
  return library.map((message) =>
    message.kind === "library"
      ? { ...message, payload: { ...message.payload, ...change } }
      : message,
  );
}

/** The Spotify playlists with two of them in a closed folder. */
export const libraryFolders: ServerMessage[] = withLibrary({
  folders: [
    {
      kind: "folder",
      name: "Chill & focus",
      uri: "spotify:user:jay:folder:focus",
      children: [
        { kind: "playlist", uri: "spotify:playlist:1" },
        { kind: "playlist", uri: "spotify:playlist:3" },
      ],
    },
    { kind: "playlist", uri: "spotify:playlist:2" },
    { kind: "playlist", uri: "spotify:playlist:4" },
    { kind: "playlist", uri: "spotify:playlist:5" },
  ],
});

function podcastShow(
  id: string,
  name: string,
  publisher: string,
  description = "",
): ShowInfo {
  return {
    id,
    uri: `spotify:show:${id}`,
    name,
    description,
    publisher,
    image_url: null,
  };
}

const shows: ShowInfo[] = [
  podcastShow(
    "s1",
    "Score Notes",
    "Cue Sheet Media",
    "Every week two composers take one film score apart, cue by cue: the themes, the orchestration and the sessions that made it.",
  ),
  podcastShow("s2", "Leitmotif Hour", "Main Title Radio"),
  podcastShow("s3", "Cue by Cue", "Reel Audio"),
  podcastShow("s4", "The Temp Track", "Studio B Podcasts"),
];

/** A Spotify search for film scores: two songs and four podcast shows. */
export const podcastSearch: ServerMessage[] = [
  {
    kind: "search",
    rev: 1,
    payload: {
      ran: true,
      query: "film score",
      tracks: [
        hit("Time", "t1", "Inception", "Hans Zimmer", 275),
        hit("Cornfield Chase", "t2", "Interstellar", "Hans Zimmer", 126),
      ],
      artists: [],
      albums: [],
      playlists: [],
      shows,
      liked: [],
    },
  },
];

function episode(
  id: string,
  name: string,
  minutes: number,
  release_date: string,
  resume_point: ResumePointInfo | null,
  description: string,
): EpisodeInfo {
  return {
    id,
    uri: `spotify:episode:${id}`,
    name,
    duration_ms: minutes * 60_000,
    show_name: "",
    description,
    release_date,
    is_playable: true,
    resume_point,
    image_url: null,
  };
}

/** Score Notes, followed: a new episode, one half played and one played through. */
export const podcast: ServerMessage[] = [
  {
    kind: "show",
    rev: 1,
    payload: {
      page: {
        show: shows[0],
        saved: true,
        loaded: true,
        total: 3,
        has_more: false,
        episodes: [
          episode(
            "e3",
            "The organ of Interstellar",
            57,
            "2026-10-07",
            null,
            "Why a church organ carries a film about space, and how the recording sessions in London shaped it.",
          ),
          episode(
            "e2",
            "Ninety themes for Middle-earth",
            48,
            "2026-09-30",
            { fully_played: false, resume_position_ms: 24 * 60_000 },
            "Every culture gets a theme: a walk through the leitmotifs of the trilogy.",
          ),
          episode(
            "e1",
            "Dragons, bagpipes and John Powell",
            51,
            "2026-09-23",
            { fully_played: true, resume_position_ms: 0 },
            "The score that taught a generation to love Celtic flutes.",
          ),
        ],
      },
    },
  },
];

/** Eight Spotify songs played lately, each with an id, so a row opens its song radio. */
export const history: ServerMessage[] = [
  {
    kind: "recently_played",
    rev: 1,
    payload: (
      [
        ["Freeze", "Kygo", "Freeze", 487],
        ["Cornfield Chase", "Hans Zimmer", "Interstellar", 126],
        ["Rolling in the Deep", "Adele", "21", 228],
        ["Faded", "Alan Walker", "Different World", 212],
        ["Nuvole Bianche", "Ludovico Einaudi", "Una Mattina", 358],
        ["Wake Me Up", "Avicii", "True", 247],
        ["Time", "Hans Zimmer", "Inception", 275],
        ["Firestone", "Kygo, Conrad Sewell", "Cloud Nine", 272],
      ] as const
    ).map(([name, artist, album, seconds], index) =>
      hit(name, `h${index}`, album, artist, seconds),
    ),
  },
];

/** The song radio of Cornfield Chase, the second row of `history`. */
export const songRadio: ServerMessage[] = [
  {
    kind: "track_table",
    rev: 1,
    payload: {
      uri: null,
      tracks: (
        [
          ["Day One", "Hans Zimmer", "Interstellar", 199],
          ["Experience", "Ludovico Einaudi", "In a Time Lapse", 315],
          [
            "Arrival of the Birds",
            "The Cinematic Orchestra",
            "The Crimson Wing",
            158,
          ],
          ["Mountains", "Hans Zimmer", "Interstellar", 219],
          [
            "On the Nature of Daylight",
            "Max Richter",
            "The Blue Notebooks",
            372,
          ],
          ["Una Mattina", "Ludovico Einaudi", "Una Mattina", 202],
        ] as const
      ).map(([name, artist, album, seconds], index) =>
        hit(name, `r${index}`, album, artist, seconds),
      ),
      has_more: false,
      positions: null,
      radio: "h1",
    },
  },
];

export const announcement = (
  level: "info" | "warning" | "critical",
  body: string,
  position = 1,
  total = 1,
): ServerMessage => ({
  kind: "announcement",
  rev: 1,
  payload: {
    active: {
      entry: {
        id: `shot-${level}`,
        title: "Spotify login change",
        body,
        level,
        url: "https://spotatui.com/news",
      },
      position,
      total,
    },
    history: [],
  },
});
