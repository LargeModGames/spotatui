import type { NowPlaying } from "../src/bindings/NowPlaying";
import type { PlaylistInfo } from "../src/bindings/PlaylistInfo";
import type { ServerMessage } from "../src/bindings/ServerMessage";
import type { Source } from "../src/bindings/Source";
import type { TrackInfo } from "../src/bindings/TrackInfo";

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
};

export const hello: ServerMessage = {
  kind: "hello",
  payload: { version: "0.0.0-shot", token: "shot", revisions },
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
      radio: [],
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
      radio: [],
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
      native: [
        track("Firestone", 272, "spotify:track:k1", "Kygo"),
        track("Stay", 239, "spotify:track:k2", "Kygo"),
        track("Suite", 602, "file:///music/powell/01.flac", "John Powell"),
      ].map((entry, index) => ({
        ...entry,
        album: index < 2 ? "Cloud Nine" : "Film Suites, Vol. 1",
      })),
      spotify: { currently_playing: null, items: [] },
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
