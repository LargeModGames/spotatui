export type CoverStyle = {
  from: string;
  via: string;
  to: string;
  label: string;
};

export type Track = {
  id: string;
  title: string;
  artist: string;
  album: string;
  durationMs: number;
  progressMs: number;
  uri?: string;
  cover: CoverStyle;
  isLiked: boolean;
};

export type Playlist = {
  id: string;
  name: string;
  description: string;
  trackCount: number;
  cover: CoverStyle;
};

export type Device = {
  id: string;
  name: string;
  type: string;
  isActive: boolean;
  volume: number;
};

export type DesktopSnapshot = {
  track: Track;
  isPlaying: boolean;
  shuffle: boolean;
  repeat: "off" | "track" | "context";
  volume: number;
  connection: "connected" | "mock";
  playlists: Playlist[];
  library: Track[];
  queue: Track[];
  recentlyPlayed: Track[];
  devices: Device[];
  generatedAt: string;
};

export type PlaybackCommand =
  | "toggle_playback"
  | "previous_track"
  | "next_track"
  | "seek"
  | "set_volume"
  | "play_track"
  | "toggle_shuffle"
  | "toggle_repeat";

export type PlaybackPayload = {
  progressMs?: number;
  volume?: number;
  trackId?: string;
  uri?: string;
};

const covers = {
  neon: {
    from: "#1db954",
    via: "#7afad0",
    to: "#101011",
    label: "NW",
  },
  violet: {
    from: "#b84cff",
    via: "#ff7aca",
    to: "#151316",
    label: "GL",
  },
  amber: {
    from: "#ffb000",
    via: "#ff5c33",
    to: "#15110f",
    label: "RS",
  },
  cyan: {
    from: "#00d4ff",
    via: "#7c7cff",
    to: "#10141c",
    label: "CT",
  },
  graphite: {
    from: "#68727f",
    via: "#c4cad2",
    to: "#101112",
    label: "MT",
  },
};

const mockTracks: Track[] = [
  {
    id: "night-work",
    title: "Night Work",
    artist: "The Interface",
    album: "Terminal Lights",
    durationMs: 226000,
    progressMs: 84000,
    uri: "spotify:track:night-work",
    cover: covers.neon,
    isLiked: true,
  },
  {
    id: "glass-lane",
    title: "Glass Lane",
    artist: "Soft Focus",
    album: "Nocturne Routes",
    durationMs: 198000,
    progressMs: 0,
    uri: "spotify:track:glass-lane",
    cover: covers.violet,
    isLiked: false,
  },
  {
    id: "radio-signal",
    title: "Radio Signal",
    artist: "Analog State",
    album: "Low Orbit",
    durationMs: 252000,
    progressMs: 0,
    uri: "spotify:track:radio-signal",
    cover: covers.amber,
    isLiked: true,
  },
  {
    id: "cold-terminal",
    title: "Cold Terminal",
    artist: "Northline",
    album: "Blue Hour",
    durationMs: 214000,
    progressMs: 0,
    uri: "spotify:track:cold-terminal",
    cover: covers.cyan,
    isLiked: false,
  },
  {
    id: "metro-tape",
    title: "Metro Tape",
    artist: "Archive Eight",
    album: "Late Transfers",
    durationMs: 184000,
    progressMs: 0,
    uri: "spotify:track:metro-tape",
    cover: covers.graphite,
    isLiked: true,
  },
];

const mockPlaylists: Playlist[] = [
  {
    id: "daily-stack",
    name: "Daily Stack",
    description: "42 tracks updated from recent listening",
    trackCount: 42,
    cover: covers.neon,
  },
  {
    id: "deep-work",
    name: "Deep Work",
    description: "Focus tracks and long-form electronic",
    trackCount: 88,
    cover: covers.violet,
  },
  {
    id: "liked-songs",
    name: "Liked Songs",
    description: "Saved tracks from Spotify",
    trackCount: 316,
    cover: covers.cyan,
  },
  {
    id: "release-radar",
    name: "Release Radar",
    description: "Fresh tracks from followed artists",
    trackCount: 30,
    cover: covers.amber,
  },
];

const mockDevices: Device[] = [
  {
    id: "desktop",
    name: "Spotatui Desktop",
    type: "Computer",
    isActive: true,
    volume: 68,
  },
  {
    id: "phone",
    name: "Jay's Phone",
    type: "Phone",
    isActive: false,
    volume: 44,
  },
];

function cloneTrack(track: Track): Track {
  return {
    ...track,
    cover: { ...track.cover },
  };
}

function clonePlaylist(playlist: Playlist): Playlist {
  return {
    ...playlist,
    cover: { ...playlist.cover },
  };
}

function cloneDevice(device: Device): Device {
  return { ...device };
}

export function createMockSnapshot(): DesktopSnapshot {
  return {
    track: cloneTrack(mockTracks[0]),
    isPlaying: true,
    shuffle: false,
    repeat: "context",
    volume: 68,
    connection: "mock",
    playlists: mockPlaylists.map(clonePlaylist),
    library: mockTracks.map(cloneTrack),
    queue: mockTracks.slice(1, 4).map(cloneTrack),
    recentlyPlayed: mockTracks.slice(2).reverse().map(cloneTrack),
    devices: mockDevices.map(cloneDevice),
    generatedAt: new Date().toISOString(),
  };
}

export function formatDuration(ms: number): string {
  const safeMs = Number.isFinite(ms) ? Math.max(0, ms) : 0;
  const totalSeconds = Math.floor(safeMs / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;

  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}

export function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

export function searchTracks(snapshot: DesktopSnapshot, query: string): Track[] {
  const normalizedQuery = query.trim().toLowerCase();
  if (!normalizedQuery) {
    return snapshot.library;
  }

  return snapshot.library.filter((track) => {
    return [track.title, track.artist, track.album]
      .join(" ")
      .toLowerCase()
      .includes(normalizedQuery);
  });
}

export function applyLocalCommand(
  snapshot: DesktopSnapshot,
  command: PlaybackCommand,
  payload: PlaybackPayload = {},
): DesktopSnapshot {
  switch (command) {
    case "toggle_playback":
      return { ...snapshot, isPlaying: !snapshot.isPlaying };
    case "toggle_shuffle":
      return { ...snapshot, shuffle: !snapshot.shuffle };
    case "toggle_repeat": {
      const repeat =
        snapshot.repeat === "off"
          ? "context"
          : snapshot.repeat === "context"
            ? "track"
            : "off";
      return { ...snapshot, repeat };
    }
    case "seek":
      return {
        ...snapshot,
        track: {
          ...snapshot.track,
          progressMs: clamp(payload.progressMs ?? 0, 0, snapshot.track.durationMs),
        },
      };
    case "set_volume": {
      const volume = clamp(payload.volume ?? snapshot.volume, 0, 100);
      return {
        ...snapshot,
        volume,
        devices: snapshot.devices.map((device) =>
          device.isActive ? { ...device, volume } : device,
        ),
      };
    }
    case "play_track": {
      const nextTrack = findTrack(snapshot, payload.trackId, payload.uri);

      if (!nextTrack) {
        return snapshot;
      }

      return {
        ...snapshot,
        track: { ...cloneTrack(nextTrack), progressMs: 0 },
        isPlaying: true,
        recentlyPlayed: [
          { ...snapshot.track, progressMs: 0 },
          ...snapshot.recentlyPlayed.filter((track) => track.id !== snapshot.track.id),
        ].slice(0, 5),
        queue: snapshot.queue.filter((track) => track.id !== nextTrack.id),
      };
    }
    case "previous_track": {
      const previousTrack = snapshot.recentlyPlayed[0];
      if (!previousTrack) {
        return applyLocalCommand(snapshot, "seek", { progressMs: 0 });
      }

      return {
        ...snapshot,
        track: { ...cloneTrack(previousTrack), progressMs: 0 },
        isPlaying: true,
        recentlyPlayed: snapshot.recentlyPlayed.slice(1),
        queue: [{ ...snapshot.track, progressMs: 0 }, ...snapshot.queue].slice(0, 5),
      };
    }
    case "next_track": {
      const [nextTrack, ...restQueue] = snapshot.queue;
      if (!nextTrack) {
        return applyLocalCommand(snapshot, "seek", { progressMs: 0 });
      }

      return {
        ...snapshot,
        track: { ...cloneTrack(nextTrack), progressMs: 0 },
        isPlaying: true,
        recentlyPlayed: [
          { ...snapshot.track, progressMs: 0 },
          ...snapshot.recentlyPlayed,
        ].slice(0, 5),
        queue: restQueue,
      };
    }
    default:
      return snapshot;
  }
}

function findTrack(
  snapshot: DesktopSnapshot,
  trackId?: string,
  uri?: string,
): Track | undefined {
  const matches = (track: Track) => track.id === trackId || track.uri === uri;

  if (matches(snapshot.track)) {
    return snapshot.track;
  }

  for (const track of snapshot.library) {
    if (matches(track)) {
      return track;
    }
  }

  for (const track of snapshot.queue) {
    if (matches(track)) {
      return track;
    }
  }

  for (const track of snapshot.recentlyPlayed) {
    if (matches(track)) {
      return track;
    }
  }

  return undefined;
}

export function normalizeSnapshot(
  rawSnapshot: unknown,
  fallback = createMockSnapshot(),
): DesktopSnapshot {
  const source = asRecord(rawSnapshot);

  if (!source) {
    return fallback;
  }

  // The backend GuiSnapshot shape is:
  //   { playback: { track, progress_ms, is_playing, shuffle, repeat, volume_percent, ... },
  //     devices: [...], status: { message, ... } }
  //
  // We also accept the flat "mock" shape used by createMockSnapshot() for
  // browser-only / fallback mode.
  const playback = firstRecord(
    source.playback,
    source.currentPlayback,
    source.current_playback,
  );

  // Determine the track source. The backend nests it inside playback.track.
  const trackSource = firstRecord(
    source.track,
    source.currentTrack,
    source.current_track,
    playback?.track,
  );

  // Extract progress_ms from playback level (backend) or fall back to the
  // track-level value (mock data).  The backend puts progress on the playback
  // object, not on the individual track.
  const backendProgressMs = numberValue(
    firstValue(playback?.progress_ms, playback?.progressMs),
    undefined as unknown as number,
  );
  const hasBackendProgress =
    typeof backendProgressMs === "number" && Number.isFinite(backendProgressMs) && backendProgressMs >= 0;

  const track = normalizeTrack(trackSource, fallback.track, hasBackendProgress ? backendProgressMs : undefined);
  const hasBackendTrack = trackSource !== null;

  // Playlists, library, queue, recentlyPlayed are not provided by the backend
  // GuiSnapshot (v1).  They fall through to mock/fallback data.
  const playlists = normalizeList(source.playlists, fallback.playlists, normalizePlaylist);
  const library = normalizeList(
    firstArray(source.library, source.tracks, source.savedTracks, source.saved_tracks),
    fallback.library,
    normalizeTrack,
  );
  const queue = normalizeList(source.queue, fallback.queue, normalizeTrack);
  const recentlyPlayed = normalizeList(
    firstArray(source.recentlyPlayed, source.recently_played, source.history),
    fallback.recentlyPlayed,
    normalizeTrack,
  );
  const devices = normalizeList(source.devices, fallback.devices, normalizeDevice);

  return {
    track,
    isPlaying: booleanValue(
      hasBackendTrack
        ? firstValue(
            source.isPlaying,
            source.is_playing,
            source.playing,
            playback?.isPlaying,
            playback?.is_playing,
          )
        : undefined,
      fallback.isPlaying,
    ),
    shuffle: booleanValue(
      hasBackendTrack
        ? firstValue(source.shuffle, source.shuffleState, playback?.shuffle)
        : undefined,
      fallback.shuffle,
    ),
    repeat: repeatValue(
      hasBackendTrack
        ? firstValue(source.repeat, source.repeatState, playback?.repeat)
        : undefined,
      fallback.repeat,
    ),
    volume: numberValue(
      firstValue(
        source.volume,
        source.volumePercent,
        source.volume_percent,
        playback?.volumePercent,
        playback?.volume_percent,
      ),
      fallback.volume,
    ),
    connection: "connected",
    playlists,
    library,
    queue,
    recentlyPlayed,
    devices,
    generatedAt: textValue(source.generatedAt, new Date().toISOString()),
  };
}

function normalizeTrack(rawTrack: unknown, fallback: Track, progressMsOverride?: number): Track {
  const source = asRecord(rawTrack);

  if (!source) {
    return cloneTrack(fallback);
  }

  const durationMs = numberValue(
    firstValue(source.durationMs, source.duration_ms, source.duration),
    fallback.durationMs,
  );
  // When the backend supplies progress_ms on the playback object (rather than
  // on the track), it is passed via progressMsOverride and takes precedence.
  const progressMs = clamp(
    progressMsOverride ?? numberValue(
      firstValue(source.progressMs, source.progress_ms, source.position),
      fallback.progressMs,
    ),
    0,
    durationMs,
  );

  return {
    id: textValue(firstValue(source.id, source.uri), fallback.id),
    title: textValue(firstValue(source.title, source.name), fallback.title),
    artist: artistValue(source.artist, source.artists, fallback.artist),
    album: albumValue(
      firstValue(source.album, source.albumName, source.album_name),
      fallback.album,
    ),
    durationMs,
    progressMs,
    uri: optionalText(source.uri) ?? fallback.uri,
    cover: normalizeCover(source.cover, fallback.cover),
    isLiked: booleanValue(
      firstValue(source.isLiked, source.is_liked, source.liked),
      fallback.isLiked,
    ),
  };
}

function normalizePlaylist(rawPlaylist: unknown, fallback: Playlist): Playlist {
  const source = asRecord(rawPlaylist);

  if (!source) {
    return clonePlaylist(fallback);
  }

  return {
    id: textValue(firstValue(source.id, source.uri), fallback.id),
    name: textValue(firstValue(source.name, source.title), fallback.name),
    description: textValue(source.description, fallback.description),
    trackCount: numberValue(
      firstValue(source.trackCount, source.track_count, source.total),
      fallback.trackCount,
    ),
    cover: normalizeCover(source.cover, fallback.cover),
  };
}

function normalizeDevice(rawDevice: unknown, fallback: Device): Device {
  const source = asRecord(rawDevice);

  if (!source) {
    return cloneDevice(fallback);
  }

  return {
    id: textValue(source.id, fallback.id),
    name: textValue(source.name, fallback.name),
    type: textValue(firstValue(source.type, source.device_type), fallback.type),
    isActive: booleanValue(firstValue(source.isActive, source.is_active), fallback.isActive),
    volume: numberValue(
      firstValue(source.volume, source.volumePercent, source.volume_percent),
      fallback.volume,
    ),
  };
}

function normalizeCover(rawCover: unknown, fallback: CoverStyle): CoverStyle {
  const source = asRecord(rawCover);

  if (!source) {
    return { ...fallback };
  }

  return {
    from: textValue(firstValue(source.from, source.primary), fallback.from),
    via: textValue(firstValue(source.via, source.secondary), fallback.via),
    to: textValue(firstValue(source.to, source.background), fallback.to),
    label: textValue(source.label, fallback.label).slice(0, 3).toUpperCase(),
  };
}

function normalizeList<T>(
  rawList: unknown,
  fallback: T[],
  normalizeItem: (item: unknown, fallbackItem: T) => T,
): T[] {
  const list = Array.isArray(rawList) ? rawList : null;

  if (!list || list.length === 0) {
    return fallback;
  }

  return list.map((item, index) => {
    const fallbackItem = fallback[index] ?? fallback[0];
    return normalizeItem(item, fallbackItem);
  });
}

export function asRecord(value: unknown): Record<string, unknown> | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    return null;
  }

  return value as Record<string, unknown>;
}

function firstRecord(...values: unknown[]): Record<string, unknown> | null {
  for (const value of values) {
    const record = asRecord(value);
    if (record) {
      return record;
    }
  }

  return null;
}

function firstArray(...values: unknown[]): unknown {
  return values.find(Array.isArray);
}

function firstValue(...values: unknown[]): unknown {
  return values.find((value) => value !== undefined && value !== null);
}

function textValue(value: unknown, fallback: string): string {
  if (typeof value === "string" && value.trim()) {
    return value;
  }

  return fallback;
}

function optionalText(value: unknown): string | undefined {
  if (typeof value === "string" && value.trim()) {
    return value;
  }

  return undefined;
}

function numberValue(value: unknown, fallback: number): number {
  if (typeof value === "number" && Number.isFinite(value)) {
    return value;
  }

  if (typeof value === "string") {
    const parsed = Number(value);
    if (Number.isFinite(parsed)) {
      return parsed;
    }
  }

  return fallback;
}

function booleanValue(value: unknown, fallback: boolean): boolean {
  if (typeof value === "boolean") {
    return value;
  }

  return fallback;
}

function repeatValue(value: unknown, fallback: DesktopSnapshot["repeat"]): DesktopSnapshot["repeat"] {
  if (value === "off" || value === "track" || value === "context") {
    return value;
  }

  return fallback;
}

function artistValue(
  artist: unknown,
  artists: unknown,
  fallback: string,
): string {
  if (typeof artist === "string" && artist.trim()) {
    return artist;
  }

  if (Array.isArray(artists) && artists.length > 0) {
    return artists
      .map((item) => {
        if (typeof item === "string") {
          return item;
        }

        return textValue(asRecord(item)?.name, "");
      })
      .filter(Boolean)
      .join(", ");
  }

  return fallback;
}

function albumValue(album: unknown, fallback: string): string {
  if (typeof album === "string" && album.trim()) {
    return album;
  }

  const albumRecord = asRecord(album);
  if (albumRecord) {
    return textValue(albumRecord.name, fallback);
  }

  return fallback;
}
