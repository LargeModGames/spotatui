import { type CSSProperties, useCallback, useEffect, useMemo, useState } from "react";
import { invokeCommand } from "./tauri";

type ViewId =
  | "home"
  | "search"
  | "track_table"
  | "queue"
  | "recently_played"
  | "albums"
  | "artists"
  | "podcasts"
  | "lyrics"
  | "discover"
  | "settings"
  | "party"
  | "create_playlist";

type GuiSnapshot = {
  playback?: GuiPlayback;
  devices?: GuiDevice[];
  status?: GuiStatus;
  user?: GuiUser | null;
  library?: GuiLibrary;
  playlists?: GuiPlaylist[];
  playlist_folders?: GuiPlaylistFolderEntry[];
  track_table?: GuiTrackTable;
  queue?: GuiTrack[];
  recently_played?: GuiTrack[];
  search?: GuiSearchResults;
  albums?: GuiAlbumList;
  artists?: GuiArtistList;
  podcasts?: GuiPodcastList;
  lyrics?: GuiLyrics;
  discover?: GuiDiscover;
  settings?: GuiSettings;
  dialog?: GuiDialog;
  sort?: GuiSort;
  party?: GuiParty;
  create_playlist?: GuiCreatePlaylist;
};

type GuiPlayback = {
  track?: GuiTrack | null;
  progress_ms?: number;
  is_playing?: boolean;
  shuffle?: boolean;
  repeat?: string | null;
  volume_percent?: number;
  device_id?: string | null;
  device_name?: string | null;
};

type GuiTrack = {
  id?: string | null;
  uri?: string | null;
  item_type?: string;
  title?: string;
  artists?: string[];
  album?: string | null;
  image_url?: string | null;
  duration_ms?: number;
};

type GuiDevice = {
  id?: string | null;
  name?: string;
  device_type?: string;
  is_active?: boolean;
  is_restricted?: boolean;
  volume_percent?: number | null;
};

type GuiStatus = {
  is_loading?: boolean;
  message?: string | null;
  error?: string | null;
  route?: string;
  route_id?: string;
  active_block?: string;
  is_streaming_active?: boolean;
};

type GuiUser = {
  id: string;
  display_name?: string | null;
};

type GuiLibrary = {
  options?: string[];
  selected_index?: number;
};

type GuiPlaylist = {
  id: string;
  uri?: string;
  name: string;
  owner?: string;
  description?: string | null;
  image_url?: string | null;
  track_count?: number;
  editable?: boolean;
  selected?: boolean;
};

type GuiPlaylistFolderEntry = {
  kind: string;
  id?: string | null;
  name: string;
  index: number;
  selected?: boolean;
};

type GuiTrackTable = {
  context?: string | null;
  selected_index?: number;
  tracks?: GuiTrack[];
  page?: GuiPageInfo;
  playlist_id?: string | null;
  playlist_name?: string | null;
};

type GuiPageInfo = {
  offset?: number;
  limit?: number;
  total?: number;
  page_index?: number;
  page_count?: number;
  has_previous?: boolean;
  has_next?: boolean;
};

type GuiSearchResults = {
  query?: string;
  selected_block?: string;
  tracks?: GuiTrack[];
  albums?: GuiAlbum[];
  artists?: GuiArtist[];
  playlists?: GuiPlaylist[];
  shows?: GuiShow[];
};

type GuiAlbumList = {
  selected_index?: number;
  albums?: GuiAlbum[];
};

type GuiArtistList = {
  selected_index?: number;
  artists?: GuiArtist[];
};

type GuiPodcastList = {
  selected_index?: number;
  shows?: GuiShow[];
};

type GuiAlbum = {
  id?: string | null;
  uri?: string | null;
  name: string;
  artists?: string[];
  image_url?: string | null;
  release_date?: string | null;
  total_tracks?: number | null;
};

type GuiArtist = {
  id?: string | null;
  uri?: string | null;
  name: string;
  image_url?: string | null;
  followers?: number | null;
};

type GuiShow = {
  id?: string | null;
  uri?: string | null;
  name: string;
  publisher?: string | null;
  description?: string | null;
  image_url?: string | null;
};

type GuiLyrics = {
  status?: string;
  lines?: Array<{ timestamp_ms: number; text: string }>;
};

type GuiDiscover = {
  selected_index?: number;
  time_range?: string;
  loading?: boolean;
  top_tracks?: GuiTrack[];
  artists_mix?: GuiTrack[];
};

type GuiSettings = {
  category?: string;
  selected_index?: number;
  edit_mode?: boolean;
  edit_buffer?: string;
  unsaved_prompt_visible?: boolean;
  items?: Array<{ id: string; name: string; description: string; value: string; value_type: string }>;
};

type GuiDialog = {
  kind?: string | null;
  message?: string | null;
  confirm?: boolean;
  pending_track_name?: string | null;
  playlist_name?: string | null;
};

type GuiSort = {
  visible?: boolean;
  selected_index?: number;
  context?: string | null;
};

type GuiParty = {
  status?: string;
  role?: string | null;
  code?: string | null;
  host_name?: string | null;
  guests?: string[];
  control_mode?: string | null;
};

type GuiCreatePlaylist = {
  name?: string;
  stage?: string;
  focus?: string;
  search_input?: string;
  selected_result?: number;
  tracks?: GuiTrack[];
  search_results?: GuiTrack[];
};

type BridgeState = {
  mode: "connecting" | "live" | "demo" | "error";
  message: string;
};

const refreshMs = 1200;

const fallbackSnapshot: GuiSnapshot = {
  playback: {
    track: {
      title: "No backend connected",
      artists: ["Spotatui"],
      album: "Browser demo",
      duration_ms: 180000,
      progress_ms: 0,
    } as GuiTrack,
    progress_ms: 0,
    is_playing: false,
    shuffle: false,
    repeat: "off",
    volume_percent: 60,
  },
  status: {
    route_id: "home",
    message: "Browser-only demo. Open the Tauri app for live Spotify state.",
  },
  devices: [],
  playlists: [],
  track_table: { tracks: [], selected_index: 0 },
  queue: [],
  recently_played: [],
  search: { tracks: [], albums: [], artists: [], playlists: [], shows: [] },
};

export default function App() {
  const [snapshot, setSnapshot] = useState<GuiSnapshot>(fallbackSnapshot);
  const [bridge, setBridge] = useState<BridgeState>({
    mode: "connecting",
    message: "Connecting to Spotatui",
  });
  const [searchDraft, setSearchDraft] = useState("");

  const refreshSnapshot = useCallback(async () => {
    const result = await invokeCommand<GuiSnapshot>("get_snapshot");

    if (result.ok) {
      setSnapshot(normalizeSnapshot(result.value));
      const status = result.value.status;
      setBridge({
        mode: status?.error ? "error" : status?.is_loading ? "connecting" : "live",
        message: status?.error ?? status?.message ?? (status?.is_loading ? "Starting backend" : "Live backend"),
      });
      return;
    }

    setSnapshot(fallbackSnapshot);
    setBridge({
      mode: result.reason === "missing" ? "demo" : "error",
      message: result.reason === "missing" ? "Browser-only demo" : "Backend unavailable",
    });
  }, []);

  useEffect(() => {
    void refreshSnapshot();
    const timer = window.setInterval(() => void refreshSnapshot(), refreshMs);
    return () => window.clearInterval(timer);
  }, [refreshSnapshot]);

  const dispatchAction = useCallback(
    async (action: Record<string, unknown>) => {
      const result = await invokeCommand("dispatch_action", { action });
      if (!result.ok) {
        setBridge({
          mode: result.reason === "missing" ? "demo" : "error",
          message: result.reason === "missing" ? "Browser-only demo" : "Action failed",
        });
        return;
      }
      window.setTimeout(() => void refreshSnapshot(), 120);
    },
    [refreshSnapshot],
  );

  const activeView = routeToView(snapshot.status?.route_id);
  const playback = snapshot.playback ?? fallbackSnapshot.playback!;
  const devices = snapshot.devices ?? [];
  const activeDevice =
    devices.find((device) => device.is_active) ??
    devices.find((device) => device.id === playback.device_id);

  return (
    <div className="desktop-shell">
      <Sidebar
        activeView={activeView}
        library={snapshot.library}
        playlists={snapshot.playlists ?? []}
        folders={snapshot.playlist_folders ?? []}
        onAction={dispatchAction}
      />

      <main className="workspace">
        <TopBar
          activeDevice={activeDevice}
          bridge={bridge}
          searchDraft={searchDraft}
          onSearchDraftChange={setSearchDraft}
          onSearchSubmit={() => {
            void dispatchAction({ type: "search", query: searchDraft });
          }}
        />

        {activeView === "home" && (
          <NowPlayingView snapshot={snapshot} onAction={dispatchAction} />
        )}
        {activeView === "search" && (
          <SearchView
            search={snapshot.search}
            query={searchDraft || snapshot.search?.query || ""}
            onQueryChange={setSearchDraft}
            onSubmit={() => void dispatchAction({ type: "search", query: searchDraft })}
            onPlayTrack={(index) => {
              void dispatchAction({ type: "select_track", index });
              void dispatchAction({ type: "play_selected_track" });
            }}
          />
        )}
        {activeView === "track_table" && (
          <TrackTableView table={snapshot.track_table} onAction={dispatchAction} />
        )}
        {activeView === "queue" && (
          <SimpleTracksView title="Queue" tracks={snapshot.queue ?? []} emptyLabel="Queue is clear" />
        )}
        {activeView === "recently_played" && (
          <SimpleTracksView title="Recently played" tracks={snapshot.recently_played ?? []} emptyLabel="No recent tracks loaded" />
        )}
        {activeView === "albums" && <AlbumsView albums={snapshot.albums?.albums ?? []} />}
        {activeView === "artists" && <ArtistsView artists={snapshot.artists?.artists ?? []} />}
        {activeView === "podcasts" && <PodcastsView shows={snapshot.podcasts?.shows ?? []} />}
        {activeView === "lyrics" && <LyricsView lyrics={snapshot.lyrics} />}
        {activeView === "discover" && <DiscoverView discover={snapshot.discover} />}
        {activeView === "settings" && <SettingsView settings={snapshot.settings} />}
        {activeView === "party" && <PartyView party={snapshot.party} onAction={dispatchAction} />}
        {activeView === "create_playlist" && (
          <CreatePlaylistView createPlaylist={snapshot.create_playlist} />
        )}
      </main>

      <RightRail
        bridge={bridge}
        devices={devices}
        queue={snapshot.queue ?? []}
        activeDevice={activeDevice}
        onAction={dispatchAction}
      />

      <PlayerBar snapshot={snapshot} onAction={dispatchAction} />
    </div>
  );
}

function Sidebar({
  activeView,
  library,
  playlists,
  folders,
  onAction,
}: {
  activeView: ViewId;
  library?: GuiLibrary;
  playlists: GuiPlaylist[];
  folders: GuiPlaylistFolderEntry[];
  onAction: (action: Record<string, unknown>) => void;
}) {
  const playlistRows =
    folders.length > 0
      ? folders
      : playlists.map((playlist, index) => ({
          kind: "playlist",
          id: playlist.id,
          name: playlist.name,
          index,
          selected: playlist.selected,
        }));

  return (
    <aside className="sidebar" aria-label="Library navigation">
      <div className="brand-lockup">
        <div className="brand-mark" aria-hidden="true">S</div>
        <div>
          <strong>Spotatui</strong>
          <span>Desktop</span>
        </div>
      </div>

      <nav className="primary-nav" aria-label="Primary">
        <NavButton active={activeView === "home"} icon="home" label="Now Playing" onClick={() => onAction({ type: "open_home" })} />
        <NavButton active={activeView === "search"} icon="search" label="Search" onClick={() => onAction({ type: "open_search", query: null })} />
        <NavButton active={activeView === "settings"} icon="library" label="Settings" onClick={() => onAction({ type: "open_settings" })} />
        <NavButton active={activeView === "party"} icon="device" label="Party" onClick={() => onAction({ type: "open_party" })} />
      </nav>

      <div className="sidebar-section">
        <div className="section-kicker">Library</div>
        <div className="playlist-list">
          {(library?.options ?? []).map((option, index) => (
            <button
              className={`playlist-button ${library?.selected_index === index ? "is-active" : ""}`}
              key={option}
              onClick={() => onAction({ type: "open_library_item", index })}
              type="button"
            >
              <CoverArt label={option.slice(0, 2).toUpperCase()} imageUrl={null} size="xs" />
              <span>
                <strong>{option}</strong>
                <small>Shared TUI state</small>
              </span>
            </button>
          ))}
        </div>
      </div>

      <div className="sidebar-section">
        <div className="section-kicker">Playlists</div>
        <div className="playlist-list">
          {playlistRows.map((row) => (
            <button
              className={`playlist-button ${row.selected ? "is-active" : ""}`}
              key={`${row.kind}-${row.id ?? row.index}`}
              onClick={() => {
                if (row.kind === "playlist" && row.id) {
                  onAction({ type: "open_playlist", playlist_id: row.id });
                }
              }}
              type="button"
            >
              <CoverArt label={row.kind === "folder" ? ">" : row.name.slice(0, 2).toUpperCase()} imageUrl={null} size="xs" />
              <span>
                <strong>{row.name}</strong>
                <small>{row.kind}</small>
              </span>
            </button>
          ))}
        </div>
      </div>
    </aside>
  );
}

function NavButton({
  active,
  icon,
  label,
  onClick,
}: {
  active: boolean;
  icon: "home" | "search" | "library" | "device";
  label: string;
  onClick: () => void;
}) {
  return (
    <button className={`nav-button ${active ? "is-active" : ""}`} onClick={onClick} type="button">
      <span className={`ui-icon ui-icon-${icon}`} aria-hidden="true" />
      <span>{label}</span>
    </button>
  );
}

function TopBar({
  activeDevice,
  bridge,
  searchDraft,
  onSearchDraftChange,
  onSearchSubmit,
}: {
  activeDevice?: GuiDevice;
  bridge: BridgeState;
  searchDraft: string;
  onSearchDraftChange: (value: string) => void;
  onSearchSubmit: () => void;
}) {
  return (
    <header className="topbar">
      <form
        className="search-box"
        onSubmit={(event) => {
          event.preventDefault();
          onSearchSubmit();
        }}
      >
        <span className="ui-icon ui-icon-search" aria-hidden="true" />
        <input
          aria-label="Search Spotify"
          onChange={(event) => onSearchDraftChange(event.currentTarget.value)}
          placeholder="Search tracks, artists, albums"
          type="search"
          value={searchDraft}
        />
      </form>

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

function NowPlayingView({
  snapshot,
  onAction,
}: {
  snapshot: GuiSnapshot;
  onAction: (action: Record<string, unknown>) => void;
}) {
  const playback = snapshot.playback ?? {};
  const track = playback.track;
  const progress = playback.progress_ms ?? 0;
  const duration = track?.duration_ms ?? 0;
  const progressPercent = duration > 0 ? Math.round((progress / duration) * 100) : 0;

  return (
    <section className="view now-view" aria-label="Now playing">
      <div className="now-hero">
        <CoverArt imageUrl={track?.image_url ?? null} label={trackInitials(track)} size="hero" />
        <div className="now-copy">
          <div className="section-kicker">Now playing</div>
          <h1>{track?.title ?? "Nothing playing"}</h1>
          <p>{artistText(track)}{track?.album ? ` / ${track.album}` : ""}</p>

          <div className="hero-actions">
            <button className="primary-action" onClick={() => onAction({ type: "toggle_playback" })} type="button">
              <span className={`transport-icon ${playback.is_playing ? "icon-pause" : "icon-play"}`} aria-hidden="true" />
              {playback.is_playing ? "Pause" : "Play"}
            </button>
            <button className={`mode-chip ${playback.shuffle ? "is-active" : ""}`} onClick={() => onAction({ type: "toggle_shuffle" })} type="button">
              Shuffle
            </button>
            <button className={`mode-chip ${playback.repeat && playback.repeat !== "off" ? "is-active" : ""}`} onClick={() => onAction({ type: "toggle_repeat" })} type="button">
              Repeat {playback.repeat === "track" ? "1" : ""}
            </button>
          </div>

          <div className="hero-meter" aria-label="Track progress">
            <div className="meter-header">
              <span>{formatDuration(progress)}</span>
              <strong>{progressPercent}%</strong>
              <span>{formatDuration(duration)}</span>
            </div>
            <div className="meter-track"><span style={{ width: `${progressPercent}%` }} /></div>
          </div>
        </div>
      </div>

      <div className="content-grid">
        <section className="track-panel">
          <PanelHeading eyebrow="Queue" title="Up next" value={`${snapshot.queue?.length ?? 0} tracks`} />
          <TrackList tracks={snapshot.queue ?? []} variant="queue" />
        </section>
        <section className="track-panel">
          <PanelHeading eyebrow="History" title="Recently played" value={`${snapshot.recently_played?.length ?? 0} tracks`} />
          <TrackList tracks={snapshot.recently_played ?? []} variant="compact" />
        </section>
      </div>
    </section>
  );
}

function SearchView({
  search,
  query,
  onQueryChange,
  onSubmit,
  onPlayTrack,
}: {
  search?: GuiSearchResults;
  query: string;
  onQueryChange: (query: string) => void;
  onSubmit: () => void;
  onPlayTrack: (index: number) => void;
}) {
  return (
    <section className="view list-view" aria-label="Search">
      <div className="view-header">
        <div>
          <div className="section-kicker">Search</div>
          <h1>Find music</h1>
        </div>
        <form
          className="inline-search"
          onSubmit={(event) => {
            event.preventDefault();
            onSubmit();
          }}
        >
          <span className="ui-icon ui-icon-search" aria-hidden="true" />
          <input aria-label="Search tracks" onChange={(event) => onQueryChange(event.currentTarget.value)} placeholder="Track, artist, album" type="search" value={query} />
        </form>
      </div>

      <TrackTable emptyLabel="No search results loaded" onPlayTrack={onPlayTrack} tracks={search?.tracks ?? []} />
    </section>
  );
}

function TrackTableView({
  table,
  onAction,
}: {
  table?: GuiTrackTable;
  onAction: (action: Record<string, unknown>) => void;
}) {
  const title = table?.playlist_name ?? table?.context ?? "Tracks";
  return (
    <section className="view list-view" aria-label="Tracks">
      <div className="view-header">
        <div>
          <div className="section-kicker">{table?.context ?? "Track table"}</div>
          <h1>{title}</h1>
        </div>
        <div className="hero-actions">
          <button className="mode-chip" onClick={() => onAction({ type: "track_table_previous_page" })} type="button">Previous</button>
          <button className="mode-chip" onClick={() => onAction({ type: "track_table_next_page" })} type="button">Next</button>
          <button className="mode-chip" onClick={() => onAction({ type: "queue_selected_track" })} type="button">Queue</button>
          <button className="mode-chip" onClick={() => onAction({ type: "toggle_save_selected_track" })} type="button">Save</button>
        </div>
      </div>
      <TrackTable
        emptyLabel="No tracks loaded"
        selectedIndex={table?.selected_index ?? 0}
        onPlayTrack={(index) => {
          onAction({ type: "select_track", index });
          onAction({ type: "play_selected_track" });
        }}
        tracks={table?.tracks ?? []}
      />
    </section>
  );
}

function SimpleTracksView({ title, tracks, emptyLabel }: { title: string; tracks: GuiTrack[]; emptyLabel: string }) {
  return (
    <section className="view list-view" aria-label={title}>
      <div className="view-header">
        <div>
          <div className="section-kicker">Spotify</div>
          <h1>{title}</h1>
        </div>
      </div>
      <TrackTable emptyLabel={emptyLabel} tracks={tracks} />
    </section>
  );
}

function AlbumsView({ albums }: { albums: GuiAlbum[] }) {
  return <CollectionView title="Albums" emptyLabel="No albums loaded" items={albums.map((album) => ({ title: album.name, subtitle: (album.artists ?? []).join(", "), imageUrl: album.image_url }))} />;
}

function ArtistsView({ artists }: { artists: GuiArtist[] }) {
  return <CollectionView title="Artists" emptyLabel="No artists loaded" items={artists.map((artist) => ({ title: artist.name, subtitle: artist.followers ? `${artist.followers} followers` : "", imageUrl: artist.image_url }))} />;
}

function PodcastsView({ shows }: { shows: GuiShow[] }) {
  return <CollectionView title="Podcasts" emptyLabel="No podcasts loaded" items={shows.map((show) => ({ title: show.name, subtitle: show.publisher ?? "", imageUrl: show.image_url }))} />;
}

function CollectionView({ title, emptyLabel, items }: { title: string; emptyLabel: string; items: Array<{ title: string; subtitle?: string; imageUrl?: string | null }> }) {
  return (
    <section className="view list-view" aria-label={title}>
      <div className="view-header">
        <div>
          <div className="section-kicker">Library</div>
          <h1>{title}</h1>
        </div>
      </div>
      {items.length === 0 ? <div className="empty-state">{emptyLabel}</div> : (
        <div className="track-list">
          {items.map((item) => (
            <div className="track-row" key={`${item.title}-${item.subtitle}`}>
              <CoverArt imageUrl={item.imageUrl ?? null} label={item.title.slice(0, 2).toUpperCase()} size="xs" />
              <span className="track-row-copy">
                <strong>{item.title}</strong>
                <small>{item.subtitle}</small>
              </span>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}

function LyricsView({ lyrics }: { lyrics?: GuiLyrics }) {
  return (
    <section className="view list-view" aria-label="Lyrics">
      <div className="view-header">
        <div>
          <div className="section-kicker">{lyrics?.status ?? "Lyrics"}</div>
          <h1>Lyrics</h1>
        </div>
      </div>
      <div className="track-list">
        {(lyrics?.lines ?? []).map((line) => (
          <div className="track-row" key={`${line.timestamp_ms}-${line.text}`}>
            <span className="track-index">{formatDuration(line.timestamp_ms)}</span>
            <span className="track-row-copy"><strong>{line.text}</strong></span>
          </div>
        ))}
      </div>
    </section>
  );
}

function DiscoverView({ discover }: { discover?: GuiDiscover }) {
  const tracks = [...(discover?.top_tracks ?? []), ...(discover?.artists_mix ?? [])];
  return <SimpleTracksView title={`Discover ${discover?.time_range ?? ""}`} tracks={tracks} emptyLabel={discover?.loading ? "Loading discover data" : "No discover data loaded"} />;
}

function SettingsView({ settings }: { settings?: GuiSettings }) {
  return (
    <section className="view list-view" aria-label="Settings">
      <div className="view-header">
        <div>
          <div className="section-kicker">{settings?.category ?? "Settings"}</div>
          <h1>Settings</h1>
        </div>
      </div>
      <div className="track-table">
        {(settings?.items ?? []).map((item, index) => (
          <div className={`track-table-row ${settings?.selected_index === index ? "is-active" : ""}`} key={item.id}>
            <span className="track-index">{index + 1}</span>
            <span className="table-track-cell"><span><strong>{item.name}</strong><small>{item.description}</small></span></span>
            <span>{item.value_type}</span>
            <span>{item.value}</span>
          </div>
        ))}
      </div>
    </section>
  );
}

function PartyView({ party, onAction }: { party?: GuiParty; onAction: (action: Record<string, unknown>) => void }) {
  return (
    <section className="view list-view" aria-label="Party">
      <div className="view-header">
        <div>
          <div className="section-kicker">{party?.status ?? "Disconnected"}</div>
          <h1>Listening Party</h1>
        </div>
        <div className="hero-actions">
          <button className="mode-chip" onClick={() => onAction({ type: "start_party", control_mode: "host_only" })} type="button">Host</button>
          <button className="mode-chip" onClick={() => onAction({ type: "leave_party" })} type="button">Leave</button>
        </div>
      </div>
      <div className="status-card">
        <span className="status-dot large" />
        <div>
          <strong>{party?.code ? `Code ${party.code}` : "No active party"}</strong>
          <span>{party?.guests?.length ? `${party.guests.length} guests` : party?.control_mode ?? "Disconnected"}</span>
        </div>
      </div>
    </section>
  );
}

function CreatePlaylistView({ createPlaylist }: { createPlaylist?: GuiCreatePlaylist }) {
  return (
    <section className="view list-view" aria-label="Create playlist">
      <div className="view-header">
        <div>
          <div className="section-kicker">{createPlaylist?.stage ?? "Name"}</div>
          <h1>{createPlaylist?.name || "Create Playlist"}</h1>
        </div>
      </div>
      <TrackTable emptyLabel="No tracks added" tracks={createPlaylist?.tracks ?? []} />
    </section>
  );
}

function RightRail({
  bridge,
  devices,
  queue,
  activeDevice,
  onAction,
}: {
  bridge: BridgeState;
  devices: GuiDevice[];
  queue: GuiTrack[];
  activeDevice?: GuiDevice;
  onAction: (action: Record<string, unknown>) => void;
}) {
  return (
    <aside className="right-rail" aria-label="Playback status">
      <section className="rail-section">
        <PanelHeading eyebrow="Status" title="Playback" value={bridge.mode} />
        <div className="status-card">
          <span className={`status-dot large is-${bridge.mode}`} />
          <div>
            <strong>{bridge.message}</strong>
            <span>{activeDevice ? `${activeDevice.name} at ${activeDevice.volume_percent ?? 0}%` : "No active device"}</span>
          </div>
        </div>
      </section>

      <section className="rail-section">
        <PanelHeading eyebrow="Devices" title="Connect" value={`${devices.length}`} />
        <div className="device-list">
          {devices.map((device) => (
            <button
              className={`device-row ${device.is_active ? "is-active" : ""}`}
              key={device.id ?? device.name}
              onClick={() => device.id && onAction({ type: "transfer_playback", device_id: device.id, play: true })}
              type="button"
            >
              <span className="ui-icon ui-icon-device" aria-hidden="true" />
              <div>
                <strong>{device.name}</strong>
                <span>{device.device_type}</span>
              </div>
              <small>{device.volume_percent ?? 0}%</small>
            </button>
          ))}
        </div>
      </section>

      <section className="rail-section rail-queue">
        <PanelHeading eyebrow="Queue" title="Next" value={`${queue.length}`} />
        <TrackList tracks={queue} variant="rail" />
      </section>
    </aside>
  );
}

function PlayerBar({
  snapshot,
  onAction,
}: {
  snapshot: GuiSnapshot;
  onAction: (action: Record<string, unknown>) => void;
}) {
  const playback = snapshot.playback ?? {};
  const track = playback.track;
  const progress = playback.progress_ms ?? 0;
  const duration = track?.duration_ms ?? 0;

  return (
    <footer className="player-bar" aria-label="Player controls">
      <div className="player-track">
        <CoverArt imageUrl={track?.image_url ?? null} label={trackInitials(track)} size="sm" />
        <div>
          <strong>{track?.title ?? "Nothing playing"}</strong>
          <span>{artistText(track)}</span>
        </div>
      </div>

      <div className="transport">
        <div className="transport-controls">
          <button aria-label="Shuffle" className={`icon-button ${playback.shuffle ? "is-active" : ""}`} onClick={() => onAction({ type: "toggle_shuffle" })} type="button"><span className="transport-icon icon-shuffle" /></button>
          <button aria-label="Previous track" className="icon-button" onClick={() => onAction({ type: "previous_track" })} type="button"><span className="transport-icon icon-prev" /></button>
          <button aria-label={playback.is_playing ? "Pause" : "Play"} className="play-button" onClick={() => onAction({ type: "toggle_playback" })} type="button"><span className={`transport-icon ${playback.is_playing ? "icon-pause" : "icon-play"}`} /></button>
          <button aria-label="Next track" className="icon-button" onClick={() => onAction({ type: "next_track" })} type="button"><span className="transport-icon icon-next" /></button>
          <button aria-label="Repeat" className={`icon-button ${playback.repeat && playback.repeat !== "off" ? "is-active" : ""}`} onClick={() => onAction({ type: "toggle_repeat" })} type="button"><span className="transport-icon icon-repeat" /></button>
        </div>
        <div className="progress-row">
          <span>{formatDuration(progress)}</span>
          <input aria-label="Seek" max={duration} min={0} onChange={(event) => onAction({ type: "seek", position_ms: Number(event.currentTarget.value) })} style={rangeFillStyle(progress, duration)} type="range" value={Math.min(progress, duration)} />
          <span>{formatDuration(duration)}</span>
        </div>
      </div>

      <div className="volume-control">
        <span className="ui-icon ui-icon-volume" aria-hidden="true" />
        <input aria-label="Volume" max={100} min={0} onChange={(event) => onAction({ type: "change_volume", volume_percent: Number(event.currentTarget.value) })} style={rangeFillStyle(playback.volume_percent ?? 0, 100)} type="range" value={playback.volume_percent ?? 0} />
      </div>
    </footer>
  );
}

function TrackTable({
  tracks,
  emptyLabel,
  selectedIndex,
  onPlayTrack,
}: {
  tracks: GuiTrack[];
  emptyLabel: string;
  selectedIndex?: number;
  onPlayTrack?: (index: number) => void;
}) {
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
        <button className={`track-table-row ${selectedIndex === index ? "is-active" : ""}`} key={`${track.uri ?? track.id ?? track.title}-${index}`} onClick={() => onPlayTrack?.(index)} role="row" type="button">
          <span className="track-index">{index + 1}</span>
          <span className="table-track-cell">
            <CoverArt imageUrl={track.image_url ?? null} label={trackInitials(track)} size="xs" />
            <span>
              <strong>{track.title ?? "Unknown"}</strong>
              <small>{artistText(track)}</small>
            </span>
          </span>
          <span>{track.album ?? ""}</span>
          <span>{formatDuration(track.duration_ms ?? 0)}</span>
        </button>
      ))}
    </div>
  );
}

function TrackList({ tracks, variant }: { tracks: GuiTrack[]; variant: "queue" | "compact" | "rail" }) {
  if (tracks.length === 0) {
    return <div className="empty-state compact">No tracks loaded</div>;
  }

  return (
    <div className={`track-list is-${variant}`}>
      {tracks.map((track, index) => (
        <div className="track-row" key={`${variant}-${track.uri ?? track.id ?? index}`}>
          <CoverArt imageUrl={track.image_url ?? null} label={trackInitials(track)} size="xs" />
          <span className="track-row-copy">
            <strong>{track.title ?? "Unknown"}</strong>
            <small>{artistText(track)}{variant !== "rail" && track.album ? ` / ${track.album}` : ""}</small>
          </span>
          {variant !== "rail" && <span>{formatDuration(track.duration_ms ?? 0)}</span>}
        </div>
      ))}
    </div>
  );
}

function PanelHeading({ eyebrow, title, value }: { eyebrow: string; title: string; value: string }) {
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

function CoverArt({
  imageUrl,
  label,
  size,
}: {
  imageUrl?: string | null;
  label: string;
  size: "xs" | "sm" | "hero";
}) {
  const style = imageUrl
    ? ({ backgroundImage: `url("${cssUrl(imageUrl)}")` } as CSSProperties)
    : coverGradient(label);
  return <span className={`cover-art cover-${size} ${imageUrl ? "has-image" : ""}`} data-label={imageUrl ? "" : label} style={style} />;
}

function normalizeSnapshot(snapshot: GuiSnapshot): GuiSnapshot {
  return {
    ...fallbackSnapshot,
    ...snapshot,
    playback: { ...fallbackSnapshot.playback, ...snapshot.playback },
    devices: snapshot.devices ?? [],
    playlists: snapshot.playlists ?? [],
    queue: snapshot.queue ?? [],
    recently_played: snapshot.recently_played ?? [],
    track_table: { ...fallbackSnapshot.track_table, ...snapshot.track_table },
    search: { ...fallbackSnapshot.search, ...snapshot.search },
  };
}

function routeToView(route?: string): ViewId {
  switch (route) {
    case "search":
      return "search";
    case "track_table":
    case "recommendations":
      return "track_table";
    case "queue":
      return "queue";
    case "recently_played":
      return "recently_played";
    case "albums":
    case "album_tracks":
      return "albums";
    case "artists":
    case "artist":
      return "artists";
    case "podcasts":
    case "podcast_episodes":
      return "podcasts";
    case "lyrics":
      return "lyrics";
    case "discover":
      return "discover";
    case "settings":
      return "settings";
    case "party":
      return "party";
    case "create_playlist":
      return "create_playlist";
    default:
      return "home";
  }
}

function artistText(track?: GuiTrack | null): string {
  return track?.artists?.filter(Boolean).join(", ") || "Unknown artist";
}

function trackInitials(track?: GuiTrack | null): string {
  const title = track?.title ?? "ST";
  return title
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0])
    .join("")
    .toUpperCase();
}

function formatDuration(ms: number): string {
  const safeMs = Number.isFinite(ms) ? Math.max(0, ms) : 0;
  const totalSeconds = Math.floor(safeMs / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}

function rangeFillStyle(value: number, max: number): CSSProperties {
  const fill = max > 0 ? Math.min(100, Math.max(0, (value / max) * 100)) : 0;
  return { "--range-fill": `${fill}%` } as CSSProperties;
}

function coverGradient(seed: string): CSSProperties {
  const hash = seed.split("").reduce((total, char) => total + char.charCodeAt(0), 0);
  const colors = [
    ["#1db954", "#7afad0", "#101011"],
    ["#00d4ff", "#7c7cff", "#10141c"],
    ["#ffb000", "#ff5c33", "#15110f"],
    ["#68727f", "#c4cad2", "#101112"],
  ];
  const [from, via, to] = colors[hash % colors.length];
  return {
    "--cover-from": from,
    "--cover-via": via,
    "--cover-to": to,
  } as CSSProperties;
}

function cssUrl(value: string): string {
  return value.replaceAll("\\", "\\\\").replaceAll('"', '\\"');
}
