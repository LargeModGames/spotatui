import {
  type CSSProperties,
  useCallback,
  useEffect,
  useMemo,
  useState,
} from "react";
import {
  applyLocalCommand,
  asRecord,
  clamp,
  createMockSnapshot,
  type CoverStyle,
  type DesktopSnapshot,
  type Device,
  formatDuration,
  normalizeSnapshot,
  type PlaybackCommand,
  type PlaybackPayload,
  type Playlist,
  searchTracks,
  type Track,
} from "./data";
import { invokeCommand } from "./tauri";

type ViewId = "now" | "search" | "library";

type BridgeState = {
  mode: "connecting" | "live" | "mock";
  message: string;
};

const refreshMs = 8000;

export default function App() {
  const [snapshot, setSnapshot] = useState<DesktopSnapshot>(() => createMockSnapshot());
  const [activeView, setActiveView] = useState<ViewId>("now");
  const [selectedPlaylistId, setSelectedPlaylistId] = useState<string>(
    snapshot.playlists[0]?.id ?? "",
  );
  const [searchQuery, setSearchQuery] = useState("night");
  const [bridge, setBridge] = useState<BridgeState>({
    mode: "connecting",
    message: "Connecting to Spotatui",
  });

  const refreshSnapshot = useCallback(async () => {
    const result = await invokeCommand<unknown>("get_snapshot");

    if (result.ok) {
      const statusMessage = snapshotStatusMessage(result.value);
      const hasBackendTrack = hasPlaybackTrack(result.value);
      setSnapshot((previous) => normalizeSnapshot(result.value, previous));
      setBridge({
        // If the backend returned a track or a non-"not connected" status,
        // treat it as live.  The stub backend always includes a status message
        // mentioning "not connected" and has no playback track.
        mode: hasBackendTrack || !statusMessage?.toLowerCase().includes("not connected")
          ? "live"
          : "mock",
        message: statusMessage ?? (hasBackendTrack ? "Playing" : "No track loaded"),
      });
      return;
    }

    setSnapshot((previous) => ({ ...previous, connection: "mock" }));
    setBridge({
      mode: "mock",
      message:
        result.reason === "missing"
          ? "Local mock session"
          : "Backend unavailable",
    });
  }, []);

  useEffect(() => {
    void refreshSnapshot();
    const timer = window.setInterval(() => void refreshSnapshot(), refreshMs);

    return () => window.clearInterval(timer);
  }, [refreshSnapshot]);

  useEffect(() => {
    if (snapshot.playlists.some((playlist) => playlist.id === selectedPlaylistId)) {
      return;
    }

    setSelectedPlaylistId(snapshot.playlists[0]?.id ?? "");
  }, [selectedPlaylistId, snapshot.playlists]);

  useEffect(() => {
    if (!snapshot.isPlaying) {
      return undefined;
    }

    const timer = window.setInterval(() => {
      setSnapshot((previous) => {
        if (!previous.isPlaying) {
          return previous;
        }

        const progressMs = clamp(
          previous.track.progressMs + 1000,
          0,
          previous.track.durationMs,
        );

        return {
          ...previous,
          isPlaying: progressMs < previous.track.durationMs,
          track: {
            ...previous.track,
            progressMs,
          },
        };
      });
    }, 1000);

    return () => window.clearInterval(timer);
  }, [snapshot.isPlaying]);

  const dispatchPlaybackCommand = useCallback(
    async (command: PlaybackCommand, payload: PlaybackPayload = {}) => {
      setSnapshot((previous) => applyLocalCommand(previous, command, payload));

      const result = await invokeCommand("dispatch_command", {
        command: toBackendCommand(command, payload),
      });

      if (result.ok) {
        setBridge({
          mode: "live",
          message: "Command sent",
        });
        window.setTimeout(() => void refreshSnapshot(), 240);
        return;
      }

      setBridge({
        mode: "mock",
        message:
          result.reason === "missing"
            ? "Local controls active"
            : "Backend command failed",
      });
    },
    [refreshSnapshot],
  );

  const searchResults = useMemo(
    () => searchTracks(snapshot, searchQuery),
    [searchQuery, snapshot.library],
  );

  const selectedPlaylist = useMemo(() => {
    return (
      snapshot.playlists.find((playlist) => playlist.id === selectedPlaylistId) ??
      snapshot.playlists[0]
    );
  }, [selectedPlaylistId, snapshot.playlists]);

  const playlistTracks = useMemo(() => {
    return rotateTracks(snapshot.library, selectedPlaylist?.id ?? "");
  }, [selectedPlaylist?.id, snapshot.library]);

  const activeDevice =
    snapshot.devices.find((device) => device.isActive) ?? snapshot.devices[0];

  return (
    <div className="desktop-shell">
      <Sidebar
        activeView={activeView}
        onViewChange={setActiveView}
        playlists={snapshot.playlists}
        selectedPlaylistId={selectedPlaylist?.id ?? ""}
        onPlaylistSelect={(playlistId) => {
          setSelectedPlaylistId(playlistId);
          setActiveView("library");
        }}
      />

      <main className="workspace">
        <TopBar
          activeDevice={activeDevice}
          bridge={bridge}
          searchQuery={searchQuery}
          onSearchQueryChange={(value) => {
            setSearchQuery(value);
            setActiveView("search");
          }}
        />

        {activeView === "now" && (
          <NowPlayingView
            snapshot={snapshot}
            onCommand={dispatchPlaybackCommand}
          />
        )}

        {activeView === "search" && (
          <SearchView
            query={searchQuery}
            results={searchResults}
            onQueryChange={setSearchQuery}
            onPlayTrack={(track) =>
              dispatchPlaybackCommand("play_track", {
                trackId: track.id,
                uri: track.uri,
              })
            }
          />
        )}

        {activeView === "library" && selectedPlaylist && (
          <LibraryView
            playlist={selectedPlaylist}
            tracks={playlistTracks}
            onPlayTrack={(track) =>
              dispatchPlaybackCommand("play_track", {
                trackId: track.id,
                uri: track.uri,
              })
            }
          />
        )}
      </main>

      <RightRail
        bridge={bridge}
        devices={snapshot.devices}
        queue={snapshot.queue}
        activeDevice={activeDevice}
        onPlayTrack={(track) =>
          dispatchPlaybackCommand("play_track", {
            trackId: track.id,
            uri: track.uri,
          })
        }
      />

      <PlayerBar
        snapshot={snapshot}
        onCommand={dispatchPlaybackCommand}
      />
    </div>
  );
}

type SidebarProps = {
  activeView: ViewId;
  onViewChange: (view: ViewId) => void;
  playlists: Playlist[];
  selectedPlaylistId: string;
  onPlaylistSelect: (playlistId: string) => void;
};

function Sidebar({
  activeView,
  onViewChange,
  playlists,
  selectedPlaylistId,
  onPlaylistSelect,
}: SidebarProps) {
  return (
    <aside className="sidebar" aria-label="Library navigation">
      <div className="brand-lockup">
        <div className="brand-mark" aria-hidden="true">
          S
        </div>
        <div>
          <strong>Spotatui</strong>
          <span>Desktop</span>
        </div>
      </div>

      <nav className="primary-nav" aria-label="Primary">
        <NavButton
          active={activeView === "now"}
          icon="home"
          label="Now Playing"
          onClick={() => onViewChange("now")}
        />
        <NavButton
          active={activeView === "search"}
          icon="search"
          label="Search"
          onClick={() => onViewChange("search")}
        />
        <NavButton
          active={activeView === "library"}
          icon="library"
          label="Your Library"
          onClick={() => onViewChange("library")}
        />
      </nav>

      <div className="sidebar-section">
        <div className="section-kicker">Playlists</div>
        <div className="playlist-list">
          {playlists.map((playlist) => (
            <button
              className={`playlist-button ${
                playlist.id === selectedPlaylistId ? "is-active" : ""
              }`}
              key={playlist.id}
              onClick={() => onPlaylistSelect(playlist.id)}
              type="button"
            >
              <CoverArt cover={playlist.cover} size="xs" />
              <span>
                <strong>{playlist.name}</strong>
                <small>{playlist.trackCount} tracks</small>
              </span>
            </button>
          ))}
        </div>
      </div>
    </aside>
  );
}

type NavButtonProps = {
  active: boolean;
  icon: "home" | "search" | "library";
  label: string;
  onClick: () => void;
};

function NavButton({ active, icon, label, onClick }: NavButtonProps) {
  return (
    <button
      className={`nav-button ${active ? "is-active" : ""}`}
      onClick={onClick}
      type="button"
    >
      <span className={`ui-icon ui-icon-${icon}`} aria-hidden="true" />
      <span>{label}</span>
    </button>
  );
}

type TopBarProps = {
  activeDevice?: Device;
  bridge: BridgeState;
  searchQuery: string;
  onSearchQueryChange: (value: string) => void;
};

function TopBar({
  activeDevice,
  bridge,
  searchQuery,
  onSearchQueryChange,
}: TopBarProps) {
  return (
    <header className="topbar">
      <label className="search-box">
        <span className="ui-icon ui-icon-search" aria-hidden="true" />
        <input
          aria-label="Search library"
          onChange={(event) => onSearchQueryChange(event.currentTarget.value)}
          placeholder="Search tracks, artists, albums"
          type="search"
          value={searchQuery}
        />
      </label>

      <div className="topbar-status">
        <span className={`bridge-pill is-${bridge.mode}`}>
          <span className="status-dot" aria-hidden="true" />
          {bridge.message}
        </span>
        {activeDevice && (
          <span className="device-pill">
            <span className="ui-icon ui-icon-device" aria-hidden="true" />
            {activeDevice.name}
          </span>
        )}
      </div>
    </header>
  );
}

type NowPlayingViewProps = {
  snapshot: DesktopSnapshot;
  onCommand: (command: PlaybackCommand, payload?: PlaybackPayload) => void;
};

function NowPlayingView({ snapshot, onCommand }: NowPlayingViewProps) {
  const progressPercent =
    snapshot.track.durationMs > 0
      ? Math.round((snapshot.track.progressMs / snapshot.track.durationMs) * 100)
      : 0;

  return (
    <section className="view now-view" aria-label="Now playing">
      <div className="now-hero">
        <CoverArt cover={snapshot.track.cover} size="hero" />
        <div className="now-copy">
          <div className="section-kicker">Now playing</div>
          <h1>{snapshot.track.title}</h1>
          <p>
            {snapshot.track.artist}
            <span aria-hidden="true"> / </span>
            {snapshot.track.album}
          </p>

          <div className="hero-actions">
            <button
              className="primary-action"
              onClick={() => onCommand("toggle_playback")}
              type="button"
            >
              <span
                className={`transport-icon ${
                  snapshot.isPlaying ? "icon-pause" : "icon-play"
                }`}
                aria-hidden="true"
              />
              {snapshot.isPlaying ? "Pause" : "Play"}
            </button>
            <button
              className={`mode-chip ${snapshot.shuffle ? "is-active" : ""}`}
              onClick={() => onCommand("toggle_shuffle")}
              type="button"
            >
              Shuffle
            </button>
            <button
              className={`mode-chip ${snapshot.repeat !== "off" ? "is-active" : ""}`}
              onClick={() => onCommand("toggle_repeat")}
              type="button"
            >
              Repeat {snapshot.repeat === "track" ? "1" : ""}
            </button>
          </div>

          <div className="hero-meter" aria-label="Track progress">
            <div className="meter-header">
              <span>{formatDuration(snapshot.track.progressMs)}</span>
              <strong>{progressPercent}%</strong>
              <span>{formatDuration(snapshot.track.durationMs)}</span>
            </div>
            <div className="meter-track">
              <span style={{ width: `${progressPercent}%` }} />
            </div>
          </div>
        </div>
      </div>

      <div className="content-grid">
        <section className="track-panel" aria-labelledby="queue-heading">
          <PanelHeading
            eyebrow="Queue"
            title="Up next"
            value={`${snapshot.queue.length} tracks`}
          />
          <TrackList
            tracks={snapshot.queue}
            variant="queue"
            onPlayTrack={(track) =>
              onCommand("play_track", { trackId: track.id, uri: track.uri })
            }
          />
        </section>

        <section className="track-panel" aria-labelledby="recent-heading">
          <PanelHeading
            eyebrow="History"
            title="Recently played"
            value={`${snapshot.recentlyPlayed.length} tracks`}
          />
          <TrackList
            tracks={snapshot.recentlyPlayed}
            variant="compact"
            onPlayTrack={(track) =>
              onCommand("play_track", { trackId: track.id, uri: track.uri })
            }
          />
        </section>
      </div>
    </section>
  );
}

type SearchViewProps = {
  query: string;
  results: Track[];
  onQueryChange: (query: string) => void;
  onPlayTrack: (track: Track) => void;
};

function SearchView({
  query,
  results,
  onQueryChange,
  onPlayTrack,
}: SearchViewProps) {
  return (
    <section className="view list-view" aria-label="Search">
      <div className="view-header">
        <div>
          <div className="section-kicker">Search</div>
          <h1>Find music</h1>
        </div>
        <label className="inline-search">
          <span className="ui-icon ui-icon-search" aria-hidden="true" />
          <input
            aria-label="Search tracks"
            onChange={(event) => onQueryChange(event.currentTarget.value)}
            placeholder="Track, artist, album"
            type="search"
            value={query}
          />
        </label>
      </div>

      <TrackTable
        emptyLabel="No local matches"
        onPlayTrack={onPlayTrack}
        tracks={results}
      />
    </section>
  );
}

type LibraryViewProps = {
  playlist: Playlist;
  tracks: Track[];
  onPlayTrack: (track: Track) => void;
};

function LibraryView({ playlist, tracks, onPlayTrack }: LibraryViewProps) {
  const likedCount = tracks.filter((track) => track.isLiked).length;

  return (
    <section className="view list-view" aria-label="Library">
      <div className="library-header">
        <CoverArt cover={playlist.cover} size="lg" />
        <div>
          <div className="section-kicker">Playlist</div>
          <h1>{playlist.name}</h1>
          <p>{playlist.description}</p>
          <div className="library-stats">
            <span>{playlist.trackCount} tracks</span>
            <span>{likedCount} saved</span>
            <span>{formatDuration(totalDuration(tracks))}</span>
          </div>
        </div>
      </div>

      <TrackTable
        emptyLabel="This playlist is empty"
        onPlayTrack={onPlayTrack}
        tracks={tracks}
      />
    </section>
  );
}

type RightRailProps = {
  bridge: BridgeState;
  devices: Device[];
  queue: Track[];
  activeDevice?: Device;
  onPlayTrack: (track: Track) => void;
};

function RightRail({
  bridge,
  devices,
  queue,
  activeDevice,
  onPlayTrack,
}: RightRailProps) {
  return (
    <aside className="right-rail" aria-label="Playback status">
      <section className="rail-section">
        <PanelHeading
          eyebrow="Status"
          title="Playback"
          value={bridge.mode === "live" ? "Live" : "Mock"}
        />
        <div className="status-card">
          <div>
            <span className={`status-dot large is-${bridge.mode}`} />
          </div>
          <div>
            <strong>{bridge.message}</strong>
            <span>
              {activeDevice
                ? `${activeDevice.name} at ${activeDevice.volume}%`
                : "No active device"}
            </span>
          </div>
        </div>
      </section>

      <section className="rail-section">
        <PanelHeading
          eyebrow="Devices"
          title="Connect"
          value={`${devices.length}`}
        />
        <div className="device-list">
          {devices.map((device) => (
            <div
              className={`device-row ${device.isActive ? "is-active" : ""}`}
              key={device.id}
            >
              <span className="ui-icon ui-icon-device" aria-hidden="true" />
              <div>
                <strong>{device.name}</strong>
                <span>{device.type}</span>
              </div>
              <small>{device.volume}%</small>
            </div>
          ))}
        </div>
      </section>

      <section className="rail-section rail-queue">
        <PanelHeading
          eyebrow="Queue"
          title="Next"
          value={`${queue.length}`}
        />
        <TrackList
          tracks={queue}
          variant="rail"
          onPlayTrack={onPlayTrack}
        />
      </section>
    </aside>
  );
}

type PlayerBarProps = {
  snapshot: DesktopSnapshot;
  onCommand: (command: PlaybackCommand, payload?: PlaybackPayload) => void;
};

function PlayerBar({ snapshot, onCommand }: PlayerBarProps) {
  return (
    <footer className="player-bar" aria-label="Player controls">
      <div className="player-track">
        <CoverArt cover={snapshot.track.cover} size="sm" />
        <div>
          <strong>{snapshot.track.title}</strong>
          <span>{snapshot.track.artist}</span>
        </div>
      </div>

      <div className="transport">
        <div className="transport-controls">
          <button
            aria-label="Shuffle"
            className={`icon-button ${snapshot.shuffle ? "is-active" : ""}`}
            onClick={() => onCommand("toggle_shuffle")}
            type="button"
          >
            <span className="transport-icon icon-shuffle" aria-hidden="true" />
          </button>
          <button
            aria-label="Previous track"
            className="icon-button"
            onClick={() => onCommand("previous_track")}
            type="button"
          >
            <span className="transport-icon icon-prev" aria-hidden="true" />
          </button>
          <button
            aria-label={snapshot.isPlaying ? "Pause" : "Play"}
            className="play-button"
            onClick={() => onCommand("toggle_playback")}
            type="button"
          >
            <span
              className={`transport-icon ${
                snapshot.isPlaying ? "icon-pause" : "icon-play"
              }`}
              aria-hidden="true"
            />
          </button>
          <button
            aria-label="Next track"
            className="icon-button"
            onClick={() => onCommand("next_track")}
            type="button"
          >
            <span className="transport-icon icon-next" aria-hidden="true" />
          </button>
          <button
            aria-label="Repeat"
            className={`icon-button ${snapshot.repeat !== "off" ? "is-active" : ""}`}
            onClick={() => onCommand("toggle_repeat")}
            type="button"
          >
            <span className="transport-icon icon-repeat" aria-hidden="true" />
          </button>
        </div>

        <div className="progress-row">
          <span>{formatDuration(snapshot.track.progressMs)}</span>
          <input
            aria-label="Seek"
            max={snapshot.track.durationMs}
            min={0}
            onChange={(event) =>
              onCommand("seek", {
                progressMs: Number(event.currentTarget.value),
              })
            }
            style={rangeFillStyle(snapshot.track.progressMs, snapshot.track.durationMs)}
            type="range"
            value={snapshot.track.progressMs}
          />
          <span>{formatDuration(snapshot.track.durationMs)}</span>
        </div>
      </div>

      <div className="volume-control">
        <span className="ui-icon ui-icon-volume" aria-hidden="true" />
        <input
          aria-label="Volume"
          max={100}
          min={0}
          onChange={(event) =>
            onCommand("set_volume", {
              volume: Number(event.currentTarget.value),
            })
          }
          style={rangeFillStyle(snapshot.volume, 100)}
          type="range"
          value={snapshot.volume}
        />
      </div>
    </footer>
  );
}

type TrackTableProps = {
  tracks: Track[];
  emptyLabel: string;
  onPlayTrack: (track: Track) => void;
};

function TrackTable({ tracks, emptyLabel, onPlayTrack }: TrackTableProps) {
  if (tracks.length === 0) {
    return <div className="empty-state">{emptyLabel}</div>;
  }

  return (
    <div className="track-table" role="table" aria-label="Tracks">
      <div className="track-table-head" role="row">
        <span>#</span>
        <span>Title</span>
        <span>Album</span>
        <span>Time</span>
      </div>
      {tracks.map((track, index) => (
        <button
          className="track-table-row"
          key={`${track.id}-${index}`}
          onClick={() => onPlayTrack(track)}
          role="row"
          type="button"
        >
          <span className="track-index">{index + 1}</span>
          <span className="table-track-cell">
            <CoverArt cover={track.cover} size="xs" />
            <span>
              <strong>{track.title}</strong>
              <small>{track.artist}</small>
            </span>
          </span>
          <span>{track.album}</span>
          <span>{formatDuration(track.durationMs)}</span>
        </button>
      ))}
    </div>
  );
}

type TrackListProps = {
  tracks: Track[];
  variant: "queue" | "compact" | "rail";
  onPlayTrack: (track: Track) => void;
};

function TrackList({ tracks, variant, onPlayTrack }: TrackListProps) {
  if (tracks.length === 0) {
    return <div className="empty-state compact">Queue is clear</div>;
  }

  return (
    <div className={`track-list is-${variant}`}>
      {tracks.map((track) => (
        <button
          className="track-row"
          key={`${variant}-${track.id}`}
          onClick={() => onPlayTrack(track)}
          type="button"
        >
          <CoverArt cover={track.cover} size="xs" />
          <span className="track-row-copy">
            <strong>{track.title}</strong>
            <small>
              {track.artist}
              {variant !== "rail" ? ` / ${track.album}` : ""}
            </small>
          </span>
          {variant !== "rail" && <span>{formatDuration(track.durationMs)}</span>}
        </button>
      ))}
    </div>
  );
}

type PanelHeadingProps = {
  eyebrow: string;
  title: string;
  value: string;
};

function PanelHeading({ eyebrow, title, value }: PanelHeadingProps) {
  return (
    <div className="panel-heading">
      <div>
        <span className="section-kicker">{eyebrow}</span>
        <h2>{title}</h2>
      </div>
      <strong>{value}</strong>
    </div>
  );
}

type CoverArtProps = {
  cover: CoverStyle;
  size: "xs" | "sm" | "lg" | "hero";
};

function CoverArt({ cover, size }: CoverArtProps) {
  return (
    <span
      className={`cover-art cover-${size}`}
      data-label={cover.label}
      style={coverStyle(cover)}
    />
  );
}

function coverStyle(cover: CoverStyle): CSSProperties {
  return {
    "--cover-from": cover.from,
    "--cover-via": cover.via,
    "--cover-to": cover.to,
  } as CSSProperties;
}

function rangeFillStyle(value: number, max: number): CSSProperties {
  const fill = max > 0 ? clamp((value / max) * 100, 0, 100) : 0;

  return {
    "--range-fill": `${fill}%`,
  } as CSSProperties;
}

function rotateTracks(tracks: Track[], seed: string): Track[] {
  if (tracks.length === 0) {
    return [];
  }

  const offset =
    seed.split("").reduce((total, character) => total + character.charCodeAt(0), 0) %
    tracks.length;

  return [...tracks.slice(offset), ...tracks.slice(0, offset)];
}

function totalDuration(tracks: Track[]): number {
  return tracks.reduce((total, track) => total + track.durationMs, 0);
}

function snapshotStatusMessage(snapshot: unknown): string | null {
  const source = asRecord(snapshot);
  const status = asRecord(source?.status);
  const message = status?.message;

  return typeof message === "string" && message.trim() ? message : null;
}

function hasPlaybackTrack(snapshot: unknown): boolean {
  const source = asRecord(snapshot);
  const playback = asRecord(source?.playback);
  const track = asRecord(playback?.track);

  if (!track) {
    return false;
  }

  // A track is considered present if it has a non-empty title.
  const title = track.title;
  return typeof title === "string" && title.trim().length > 0;
}

function toBackendCommand(
  command: PlaybackCommand,
  payload: PlaybackPayload,
): Record<string, unknown> {
  switch (command) {
    case "toggle_playback":
      return { type: "toggle_playback" };
    case "previous_track":
      return { type: "previous_track" };
    case "next_track":
      return { type: "next_track" };
    case "seek":
      return { type: "seek", position_ms: Math.round(payload.progressMs ?? 0) };
    case "set_volume":
      return {
        type: "change_volume",
        volume_percent: Math.round(clamp(payload.volume ?? 0, 0, 100)),
      };
    case "toggle_shuffle":
      return { type: "toggle_shuffle" };
    case "toggle_repeat":
      return { type: "toggle_repeat" };
    case "play_track":
      return {
        type: "play_track",
        track_id: payload.trackId,
        uri: payload.uri,
      };
    default:
      return { type: command };
  }
}
