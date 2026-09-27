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
};

/** Adele's 21 from song 5, paused at 1:40, so the frame does not move between runs; one queued song per other source. */
export const playing: ServerMessage[] = [
  hello,
  { kind: "route", rev: 1, payload: "Home" },
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
  { kind: "route", rev: 1, payload: "Home" },
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
