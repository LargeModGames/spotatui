import {
  startTransition,
  type CSSProperties,
  type ReactNode,
  useEffect,
  useEffectEvent,
  useState,
} from "react";
import { invokeCommand } from "./tauri";
import {
  fallbackSnapshot,
  getContentRouteId,
  getOverlayRouteId,
  getVisibleRouteId,
  normalizeSnapshot,
  routeTitles,
  type DeepPartial,
  type GuiAlbum,
  type GuiAnnouncement,
  type GuiArtist,
  type GuiCreatePlaylist,
  type GuiDevice,
  type GuiDialog,
  type GuiDiscover,
  type GuiEpisode,
  type GuiHelp,
  type GuiHome,
  type GuiParty,
  type GuiPlayback,
  type GuiPlaylist,
  type GuiPlaylistFolderEntry,
  type GuiPodcastEpisodes,
  type GuiSearchResults,
  type GuiSettings,
  type GuiShow,
  type GuiSnapshot,
  type GuiSort,
  type GuiStatus,
  type GuiTrack,
  type GuiTrackTable,
  type RouteId,
} from "./snapshot";

type BridgeState = {
  mode: "connecting" | "live" | "demo" | "error";
  message: string;
};

type ActionPayload = {
  type: string;
  [key: string]: unknown;
};

const refreshMs = 1200;

export default function App() {
  const [snapshot, setSnapshot] = useState<GuiSnapshot>(fallbackSnapshot);
  const [bridge, setBridge] = useState<BridgeState>({
    mode: "connecting",
    message: "Connecting to Spotatui",
  });
  const [searchDraft, setSearchDraft] = useState("");
  const [partyCodeDraft, setPartyCodeDraft] = useState("");
  const [partyNameDraft, setPartyNameDraft] = useState("");

  const refreshSnapshot = useEffectEvent(async () => {
    const result = await invokeCommand<DeepPartial<GuiSnapshot>>("get_snapshot");

    if (result.ok) {
      const nextSnapshot = normalizeSnapshot(result.value);
      startTransition(() => {
        setSnapshot(nextSnapshot);
        setBridge({
          mode: nextSnapshot.status.error
            ? "error"
            : nextSnapshot.status.is_loading
              ? "connecting"
              : "live",
          message:
            nextSnapshot.status.error ??
            nextSnapshot.status.message ??
            (nextSnapshot.status.is_loading ? "Starting backend" : "Live backend"),
        });
      });
      return;
    }

    startTransition(() => {
      setSnapshot(fallbackSnapshot);
      setBridge({
        mode: result.reason === "missing" ? "demo" : "error",
        message: result.reason === "missing" ? "Browser-only demo" : "Backend unavailable",
      });
    });
  });

  useEffect(() => {
    void refreshSnapshot();
    const timer = window.setInterval(() => void refreshSnapshot(), refreshMs);
    return () => window.clearInterval(timer);
  }, []);

  useEffect(() => {
    setSearchDraft((current) => current || snapshot.search.query);
  }, [snapshot.search.query]);

  useEffect(() => {
    setPartyCodeDraft((current) => current || snapshot.party.code_input);
    setPartyNameDraft((current) => current || snapshot.party.join_name);
  }, [snapshot.party.code_input, snapshot.party.join_name]);

  async function dispatchAction(action: ActionPayload) {
    const result = await invokeCommand("dispatch_action", { action });
    if (!result.ok) {
      startTransition(() => {
        setBridge({
          mode: result.reason === "missing" ? "demo" : "error",
          message: result.reason === "missing" ? "Browser-only demo" : "Action failed",
        });
      });
      return;
    }

    window.setTimeout(() => void refreshSnapshot(), 120);
  }

  async function selectAndPlayTrack(index: number) {
    await dispatchAction({ type: "select_track", index });
    await dispatchAction({ type: "play_selected_track" });
  }

  const activeRoute = getContentRouteId(snapshot);
  const visibleRoute = getVisibleRouteId(snapshot);
  const overlayRoute = getOverlayRouteId(snapshot);
  const playback = snapshot.playback;
  const activeDevice =
    snapshot.devices.find((device) => device.is_active) ??
    snapshot.devices.find((device) => device.id === playback.device_id);

  return (
    <div className="app-shell">
      <div className="body-shell">
        <Sidebar
          activeRoute={activeRoute}
          libraryOptions={snapshot.library.options}
          librarySelectedIndex={snapshot.library.selected_index}
          playback={playback}
          playlistFolders={snapshot.playlist_folders}
          playlists={snapshot.playlists}
          onAction={dispatchAction}
        />

        <main className="main-pane">
          <AppHeader
            activeDevice={activeDevice}
            bridge={bridge}
            route={activeRoute}
            snapshot={snapshot}
            visibleRoute={visibleRoute}
            onAction={dispatchAction}
          />

          <section className="content-pane">
            <RouteView
              activeRoute={activeRoute}
              partyCodeDraft={partyCodeDraft}
              partyNameDraft={partyNameDraft}
              searchDraft={searchDraft}
              snapshot={snapshot}
              onAction={dispatchAction}
              onPartyCodeDraftChange={setPartyCodeDraft}
              onPartyNameDraftChange={setPartyNameDraft}
              onSearchDraftChange={setSearchDraft}
              onSearchSubmit={() => void dispatchAction({ type: "search", query: searchDraft })}
              onTrackTablePlay={selectAndPlayTrack}
            />
          </section>
        </main>
      </div>

      <PlayerBar
        activeDevice={activeDevice}
        bridge={bridge}
        playback={playback}
        status={snapshot.status}
        onAction={dispatchAction}
      />

      <OverlayStack
        announcement={snapshot.announcement}
        dialog={snapshot.dialog}
        overlayRoute={overlayRoute}
        sort={snapshot.sort}
        status={snapshot.status}
        onAction={dispatchAction}
      />
    </div>
  );
}

function Sidebar({
  activeRoute,
  libraryOptions,
  librarySelectedIndex,
  playback,
  playlistFolders,
  playlists,
  onAction,
}: {
  activeRoute: RouteId;
  libraryOptions: string[];
  librarySelectedIndex: number;
  playback: GuiPlayback;
  playlistFolders: GuiPlaylistFolderEntry[];
  playlists: GuiPlaylist[];
  onAction: (action: ActionPayload) => void;
}) {
  const playlistRows =
    playlistFolders.length > 0
      ? playlistFolders
      : playlists.map((playlist, index) => ({
          kind: "playlist",
          id: playlist.id,
          name: playlist.name,
          index,
          depth: 0,
          selected: playlist.selected,
        }));

  const libraryRoutes = [
    { label: "Home", route: "home" as const, action: { type: "open_home" } },
    { label: "Recently Played", route: "recently_played" as const, action: { type: "open_library_item", index: 1 } },
    { label: "Liked Songs", route: "track_table" as const, action: { type: "open_saved_tracks" } },
    { label: "Artists", route: "artists" as const, action: { type: "open_library_item", index: 4 } },
    { label: "Albums", route: "albums" as const, action: { type: "open_library_item", index: 3 } },
    { label: "Podcasts", route: "podcasts" as const, action: { type: "open_library_item", index: 5 } },
    { label: "Devices", route: "devices" as const, action: { type: "open_devices" } },
    { label: "Queue", route: "queue" as const, action: { type: "open_queue" } },
  ];

  return (
    <aside className="sidebar">
      <div className="brand-row">
        <span className="brand-mark" aria-hidden="true" />
        <div className="brand-copy">
          <strong>Spotatui</strong>
          <span>{playback.track?.title ? "Desktop TUI parity" : "Desktop fallback"}</span>
        </div>
      </div>

      <div className="tool-stack">
        <ToolButton label="Search" active={activeRoute === "search"} onClick={() => onAction({ type: "open_search", query: null })} />
        <ToolButton label="Help" active={activeRoute === "help"} onClick={() => onAction({ type: "open_help" })} />
        <ToolButton label="Settings" active={activeRoute === "settings"} onClick={() => onAction({ type: "open_settings" })} />
      </div>

      <SidebarSection title="Library">
        {libraryRoutes.map((item) => (
          <SidebarButton
            key={item.label}
            active={activeRoute === item.route}
            label={item.label}
            onClick={() => onAction(item.action)}
          />
        ))}
      </SidebarSection>

      <SidebarSection title="Views">
        {libraryOptions.map((option, index) => (
          <SidebarButton
            key={option}
            active={librarySelectedIndex === index}
            label={option}
            onClick={() => onAction({ type: "open_library_item", index })}
            suffix={index === librarySelectedIndex ? "active" : undefined}
          />
        ))}
      </SidebarSection>

      <SidebarSection title="Playlists">
        <div className="playlist-list">
          {playlistRows.length === 0 ? (
            <div className="sidebar-empty">No playlists loaded</div>
          ) : (
            playlistRows.map((row) => (
              <button
                className={`playlist-row ${row.selected ? "is-active" : ""}`}
                key={`${row.kind}-${row.id ?? row.index}`}
                onClick={() => {
                  if (row.kind === "playlist" && row.id) {
                    onAction({ type: "open_playlist", playlist_id: row.id });
                  }
                }}
                style={{ "--playlist-depth": row.depth } as CSSProperties}
                type="button"
              >
                <span className={`playlist-kind playlist-kind-${row.kind}`} />
                <span className="playlist-name">{row.name}</span>
              </button>
            ))
          )}
        </div>
      </SidebarSection>
    </aside>
  );
}

function ToolButton({
  active,
  label,
  onClick,
}: {
  active: boolean;
  label: string;
  onClick: () => void;
}) {
  return (
    <button className={`tool-button ${active ? "is-active" : ""}`} onClick={onClick} type="button">
      <span>{label}</span>
    </button>
  );
}

function SidebarSection({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="sidebar-section">
      <div className="section-label">{title}</div>
      {children}
    </section>
  );
}

function SidebarButton({
  active,
  label,
  onClick,
  suffix,
}: {
  active: boolean;
  label: string;
  onClick: () => void;
  suffix?: string;
}) {
  return (
    <button className={`sidebar-button ${active ? "is-active" : ""}`} onClick={onClick} type="button">
      <span>{label}</span>
      {suffix ? <small>{suffix}</small> : null}
    </button>
  );
}

function AppHeader({
  activeDevice,
  bridge,
  route,
  snapshot,
  visibleRoute,
  onAction,
}: {
  activeDevice?: GuiDevice;
  bridge: BridgeState;
  route: RouteId;
  snapshot: GuiSnapshot;
  visibleRoute: RouteId;
  onAction: (action: ActionPayload) => void;
}) {
  return (
    <header className="app-header">
      <div className="header-copy">
        <div className="eyebrow-row">
          <span className="route-tag">{routeTitles[route]}</span>
          {visibleRoute !== route ? <span className="overlay-tag">overlay: {routeTitles[visibleRoute]}</span> : null}
          <span className={`bridge-tag is-${bridge.mode}`}>{bridge.message}</span>
        </div>
        <h1>{routeTitles[route]}</h1>
        <p>
          {snapshot.status.message ??
            snapshot.status.error ??
            `${snapshot.status.active_block} / ${snapshot.status.hovered_block}`}
        </p>
      </div>

      <div className="header-actions">
        {snapshot.user?.display_name ? <span className="header-chip">{snapshot.user.display_name}</span> : null}
        {activeDevice ? <span className="header-chip">{activeDevice.name}</span> : null}
        <button className="header-button" onClick={() => onAction({ type: "refresh_playback" })} type="button">
          Refresh
        </button>
        <button className="header-button" onClick={() => onAction({ type: "back" })} type="button">
          Back
        </button>
      </div>
    </header>
  );
}

function RouteView({
  activeRoute,
  partyCodeDraft,
  partyNameDraft,
  searchDraft,
  snapshot,
  onAction,
  onPartyCodeDraftChange,
  onPartyNameDraftChange,
  onSearchDraftChange,
  onSearchSubmit,
  onTrackTablePlay,
}: {
  activeRoute: RouteId;
  partyCodeDraft: string;
  partyNameDraft: string;
  searchDraft: string;
  snapshot: GuiSnapshot;
  onAction: (action: ActionPayload) => void;
  onPartyCodeDraftChange: (value: string) => void;
  onPartyNameDraftChange: (value: string) => void;
  onSearchDraftChange: (value: string) => void;
  onSearchSubmit: () => void;
  onTrackTablePlay: (index: number) => Promise<void>;
}) {
  switch (activeRoute) {
    case "home":
      return <HomeView home={snapshot.home} status={snapshot.status} />;
    case "search":
      return (
        <SearchView
          search={snapshot.search}
          searchDraft={searchDraft}
          onAction={onAction}
          onSearchDraftChange={onSearchDraftChange}
          onSearchSubmit={onSearchSubmit}
        />
      );
    case "track_table":
    case "recommendations":
      return (
        <TrackTableView
          route={activeRoute}
          table={snapshot.track_table}
          onAction={onAction}
          onTrackTablePlay={onTrackTablePlay}
        />
      );
    case "queue":
      return <QueueView snapshot={snapshot} onAction={onAction} />;
    case "recently_played":
      return <RecentlyPlayedView tracks={snapshot.recently_played} onAction={onAction} />;
    case "albums":
      return <AlbumsView albums={snapshot.albums.albums} selectedIndex={snapshot.albums.selected_index} onAction={onAction} />;
    case "album_tracks":
      return <AlbumTracksView albumTracks={snapshot.album_tracks} onAction={onAction} />;
    case "artists":
      return <ArtistsView artists={snapshot.artists.artists} selectedIndex={snapshot.artists.selected_index} onAction={onAction} />;
    case "artist":
      return <ArtistDetailView detail={snapshot.artist_detail} onAction={onAction} />;
    case "podcasts":
      return <PodcastsView shows={snapshot.podcasts.shows} selectedIndex={snapshot.podcasts.selected_index} onAction={onAction} />;
    case "podcast_episodes":
      return <PodcastEpisodesView podcastEpisodes={snapshot.podcast_episodes} onAction={onAction} />;
    case "lyrics":
      return <LyricsView lyrics={snapshot.lyrics} />;
    case "discover":
      return <DiscoverView discover={snapshot.discover} onAction={onAction} />;
    case "devices":
      return <DevicesView devices={snapshot.devices} playback={snapshot.playback} onAction={onAction} />;
    case "help":
      return <HelpView help={snapshot.help} />;
    case "analysis":
      return <AnalysisView snapshot={snapshot} onAction={onAction} />;
    case "cover_art":
      return <CoverArtView snapshot={snapshot} />;
    case "settings":
      return <SettingsView settings={snapshot.settings} onAction={onAction} />;
    case "party":
      return (
        <PartyView
          party={snapshot.party}
          partyCodeDraft={partyCodeDraft}
          partyNameDraft={partyNameDraft}
          onAction={onAction}
          onPartyCodeDraftChange={onPartyCodeDraftChange}
          onPartyNameDraftChange={onPartyNameDraftChange}
        />
      );
    case "create_playlist":
      return <CreatePlaylistView createPlaylist={snapshot.create_playlist} onAction={onAction} />;
    case "error":
      return <ErrorView status={snapshot.status} />;
    case "dialog":
    case "announcement":
    case "exit":
      return <HomeView home={snapshot.home} status={snapshot.status} />;
  }
}

function HomeView({ home, status }: { home: GuiHome; status: GuiStatus }) {
  return (
    <div className="route-grid route-grid-home">
      <Panel eyebrow="Status" title="Runtime" meta={status.is_streaming_active ? "Native streaming" : "Spotify API"}>
        <div className="banner-block">
          {home.banner.map((line, index) => (
            <div className="banner-line" key={`${line}-${index}`}>
              {line}
            </div>
          ))}
        </div>
        <div className="detail-copy">
          <strong>{home.counter_message || "Global counter unavailable."}</strong>
          <p>{status.message ?? "Desktop route parity shell connected to the shared TUI state."}</p>
        </div>
      </Panel>

      <Panel eyebrow="Changelog" title="Recent Notes" meta={`${home.changelog_lines.length}`}>
        <ListBlock
          emptyLabel="No changelog entries available."
          items={home.changelog_lines.map((line) => (
            <div className="simple-line" key={line}>
              {line}
            </div>
          ))}
        />
      </Panel>

      <Panel eyebrow="Paths" title="Runtime Files">
        <dl className="meta-list">
          <div>
            <dt>Log path</dt>
            <dd>{home.log_path || "Unavailable"}</dd>
          </div>
          <div>
            <dt>Issue report</dt>
            <dd>
              {home.report_url ? (
                <a href={home.report_url} rel="noreferrer" target="_blank">
                  {home.report_url}
                </a>
              ) : (
                "Unavailable"
              )}
            </dd>
          </div>
          <div>
            <dt>Active block</dt>
            <dd>{status.active_block}</dd>
          </div>
          <div>
            <dt>Hovered block</dt>
            <dd>{status.hovered_block}</dd>
          </div>
        </dl>
      </Panel>
    </div>
  );
}

function SearchView({
  search,
  searchDraft,
  onAction,
  onSearchDraftChange,
  onSearchSubmit,
}: {
  search: GuiSearchResults;
  searchDraft: string;
  onAction: (action: ActionPayload) => void;
  onSearchDraftChange: (value: string) => void;
  onSearchSubmit: () => void;
}) {
  return (
    <div className="route-grid route-grid-search">
      <Panel eyebrow="Search" title="Spotify Search" meta={search.query || "No query"}>
        <form
          className="search-toolbar"
          onSubmit={(event) => {
            event.preventDefault();
            onSearchSubmit();
          }}
        >
          <input
            className="text-input"
            onChange={(event) => onSearchDraftChange(event.currentTarget.value)}
            placeholder="Search tracks, artists, albums, playlists, podcasts"
            type="search"
            value={searchDraft}
          />
          <button className="primary-button" type="submit">
            Search
          </button>
        </form>
      </Panel>

      <SearchTrackPanel search={search} onAction={onAction} />
      <SearchArtistsPanel search={search} onAction={onAction} />
      <SearchAlbumsPanel search={search} onAction={onAction} />
      <SearchPlaylistsPanel search={search} onAction={onAction} />
      <SearchShowsPanel search={search} onAction={onAction} />
    </div>
  );
}

function SearchTrackPanel({
  search,
  onAction,
}: {
  search: GuiSearchResults;
  onAction: (action: ActionPayload) => void;
}) {
  return (
    <Panel eyebrow="Songs" title="Songs" meta={String(search.tracks.length)}>
      <TrackTableBlock
        block="search_tracks"
        tracks={search.tracks}
        selectedIndex={search.selected_track_index ?? -1}
        onAction={onAction}
      />
    </Panel>
  );
}

function SearchArtistsPanel({
  search,
  onAction,
}: {
  search: GuiSearchResults;
  onAction: (action: ActionPayload) => void;
}) {
  return (
    <Panel eyebrow="Artists" title="Artists" meta={String(search.artists.length)}>
      <ArtistRows
        artists={search.artists}
        selectedIndex={search.selected_artist_index ?? -1}
        onOpen={(index) => onAction({ type: "open_indexed_item", block: "search_artists", index })}
        onRecommend={(index) => onAction({ type: "recommend_indexed_item", block: "search_artists", index })}
        onSave={(index) => onAction({ type: "toggle_save_indexed_item", block: "search_artists", index })}
      />
    </Panel>
  );
}

function SearchAlbumsPanel({
  search,
  onAction,
}: {
  search: GuiSearchResults;
  onAction: (action: ActionPayload) => void;
}) {
  return (
    <Panel eyebrow="Albums" title="Albums" meta={String(search.albums.length)}>
      <AlbumRows
        albums={search.albums}
        selectedIndex={search.selected_album_index ?? -1}
        onOpen={(index) => onAction({ type: "open_indexed_item", block: "search_albums", index })}
        onSave={(index) => onAction({ type: "toggle_save_indexed_item", block: "search_albums", index })}
      />
    </Panel>
  );
}

function SearchPlaylistsPanel({
  search,
  onAction,
}: {
  search: GuiSearchResults;
  onAction: (action: ActionPayload) => void;
}) {
  return (
    <Panel eyebrow="Playlists" title="Playlists" meta={String(search.playlists.length)}>
      <PlaylistRows
        playlists={search.playlists}
        selectedIndex={search.selected_playlist_index ?? -1}
        onOpen={(index) => onAction({ type: "open_indexed_item", block: "search_playlists", index })}
      />
    </Panel>
  );
}

function SearchShowsPanel({
  search,
  onAction,
}: {
  search: GuiSearchResults;
  onAction: (action: ActionPayload) => void;
}) {
  return (
    <Panel eyebrow="Podcasts" title="Podcasts" meta={String(search.shows.length)}>
      <ShowRows
        shows={search.shows}
        selectedIndex={search.selected_show_index ?? -1}
        onOpen={(index) => onAction({ type: "open_indexed_item", block: "search_shows", index })}
        onSave={(index) => onAction({ type: "toggle_save_indexed_item", block: "search_shows", index })}
      />
    </Panel>
  );
}

function TrackTableView({
  route,
  table,
  onAction,
  onTrackTablePlay,
}: {
  route: RouteId;
  table: GuiTrackTable;
  onAction: (action: ActionPayload) => void;
  onTrackTablePlay: (index: number) => Promise<void>;
}) {
  return (
    <div className="route-grid">
      <Panel
        eyebrow={table.context ?? route}
        title={table.playlist_name ?? routeTitles[route]}
        meta={pageMeta(table.page)}
        actions={
          <>
            <button className="panel-button" onClick={() => onAction({ type: "track_table_previous_page" })} type="button">
              Prev
            </button>
            <button className="panel-button" onClick={() => onAction({ type: "track_table_next_page" })} type="button">
              Next
            </button>
            <button className="panel-button" onClick={() => onAction({ type: "open_sort_menu", context: mapSortContext(table.context) })} type="button">
              Sort
            </button>
            <button className="panel-button" onClick={() => onAction({ type: "play_random_track" })} type="button">
              Random
            </button>
          </>
        }
      >
        <TrackTableBlock
          block="track_table"
          selectedIndex={table.selected_index}
          tracks={table.tracks}
          onAction={onAction}
          onPlay={onTrackTablePlay}
        />
      </Panel>
    </div>
  );
}

function QueueView({
  snapshot,
  onAction,
}: {
  snapshot: GuiSnapshot;
  onAction: (action: ActionPayload) => void;
}) {
  return (
    <div className="route-grid route-grid-two">
      <Panel eyebrow="Now Playing" title="Current Item">
        {snapshot.queue_view.current ? (
          <NowCard track={snapshot.queue_view.current} />
        ) : (
          <EmptyState label="Nothing is currently queued as playing." />
        )}
      </Panel>

      <Panel eyebrow="Queue" title="Up Next" meta={String(snapshot.queue_view.items.length)}>
        <TrackTableBlock
          block="queue"
          selectedIndex={snapshot.queue_view.selected_index}
          tracks={snapshot.queue_view.items}
          onAction={onAction}
        />
      </Panel>
    </div>
  );
}

function RecentlyPlayedView({
  tracks,
  onAction,
}: {
  tracks: GuiTrack[];
  onAction: (action: ActionPayload) => void;
}) {
  return (
    <div className="route-grid">
      <Panel eyebrow="History" title="Recently Played" meta={String(tracks.length)}>
        <TrackTableBlock block="recently_played" selectedIndex={-1} tracks={tracks} onAction={onAction} />
      </Panel>
    </div>
  );
}

function AlbumsView({
  albums,
  selectedIndex,
  onAction,
}: {
  albums: GuiAlbum[];
  selectedIndex: number;
  onAction: (action: ActionPayload) => void;
}) {
  return (
    <div className="route-grid">
      <Panel eyebrow="Library" title="Saved Albums" meta={String(albums.length)}>
        <AlbumRows
          albums={albums}
          selectedIndex={selectedIndex}
          onOpen={(index) => onAction({ type: "open_indexed_item", block: "saved_albums", index })}
          onSave={(index) => onAction({ type: "toggle_save_indexed_item", block: "saved_albums", index })}
        />
      </Panel>
    </div>
  );
}

function AlbumTracksView({
  albumTracks,
  onAction,
}: {
  albumTracks: GuiTrackTable | { album: GuiAlbum | null; context: string; tracks: GuiTrack[]; selected_index: number; page: { offset: number; limit: number; total: number; page_index: number; page_count: number; has_previous: boolean; has_next: boolean } };
  onAction: (action: ActionPayload) => void;
}) {
  const current = albumTracks as {
    album: GuiAlbum | null;
    context: string;
    tracks: GuiTrack[];
    selected_index: number;
    page: { offset: number; limit: number; total: number; page_index: number; page_count: number; has_previous: boolean; has_next: boolean };
  };

  return (
    <div className="route-grid">
      <Panel eyebrow={current.context} title={current.album?.name || "Album Tracks"} meta={pageMeta(current.page)}>
        {current.album ? <AlbumHero album={current.album} /> : null}
        <TrackTableBlock
          block="album_tracks"
          selectedIndex={current.selected_index}
          tracks={current.tracks}
          onAction={onAction}
        />
      </Panel>
    </div>
  );
}

function ArtistsView({
  artists,
  selectedIndex,
  onAction,
}: {
  artists: GuiArtist[];
  selectedIndex: number;
  onAction: (action: ActionPayload) => void;
}) {
  return (
    <div className="route-grid">
      <Panel eyebrow="Library" title="Followed Artists" meta={String(artists.length)}>
        <ArtistRows
          artists={artists}
          selectedIndex={selectedIndex}
          onOpen={(index) => onAction({ type: "open_indexed_item", block: "saved_artists", index })}
          onRecommend={(index) => onAction({ type: "recommend_indexed_item", block: "saved_artists", index })}
          onSave={(index) => onAction({ type: "toggle_save_indexed_item", block: "saved_artists", index })}
        />
      </Panel>
    </div>
  );
}

function ArtistDetailView({
  detail,
  onAction,
}: {
  detail: GuiSnapshot["artist_detail"];
  onAction: (action: ActionPayload) => void;
}) {
  return (
    <div className="route-grid route-grid-three">
      <Panel eyebrow="Artist" title={detail.artist?.name || "Artist Detail"} meta={detail.selected_block}>
        {detail.artist ? <ArtistHero artist={detail.artist} /> : <EmptyState label="No artist selected." />}
      </Panel>

      <Panel eyebrow="Top Tracks" title="Top Tracks" meta={String(detail.top_tracks.length)}>
        <TrackTableBlock
          block="artist_top_tracks"
          selectedIndex={detail.selected_top_track_index}
          tracks={detail.top_tracks}
          onAction={onAction}
        />
      </Panel>

      <Panel eyebrow="Albums" title="Albums" meta={String(detail.albums.length)}>
        <AlbumRows
          albums={detail.albums}
          selectedIndex={detail.selected_album_index}
          onOpen={(index) => onAction({ type: "open_indexed_item", block: "artist_albums", index })}
          onSave={(index) => onAction({ type: "toggle_save_indexed_item", block: "artist_albums", index })}
        />
      </Panel>

      <Panel eyebrow="Related Artists" title="Related Artists" meta={String(detail.related_artists.length)}>
        <ArtistRows
          artists={detail.related_artists}
          selectedIndex={detail.selected_related_artist_index}
          onOpen={(index) => onAction({ type: "open_indexed_item", block: "artist_related_artists", index })}
          onRecommend={(index) => onAction({ type: "recommend_indexed_item", block: "artist_related_artists", index })}
          onSave={(index) => onAction({ type: "toggle_save_indexed_item", block: "artist_related_artists", index })}
        />
      </Panel>
    </div>
  );
}

function PodcastsView({
  shows,
  selectedIndex,
  onAction,
}: {
  shows: GuiShow[];
  selectedIndex: number;
  onAction: (action: ActionPayload) => void;
}) {
  return (
    <div className="route-grid">
      <Panel eyebrow="Library" title="Saved Podcasts" meta={String(shows.length)}>
        <ShowRows
          shows={shows}
          selectedIndex={selectedIndex}
          onOpen={(index) => onAction({ type: "open_indexed_item", block: "saved_podcasts", index })}
          onSave={(index) => onAction({ type: "toggle_save_indexed_item", block: "saved_podcasts", index })}
        />
      </Panel>
    </div>
  );
}

function PodcastEpisodesView({
  podcastEpisodes,
  onAction,
}: {
  podcastEpisodes: GuiPodcastEpisodes;
  onAction: (action: ActionPayload) => void;
}) {
  return (
    <div className="route-grid">
      <Panel
        eyebrow={podcastEpisodes.context}
        title={podcastEpisodes.show?.name || "Podcast Episodes"}
        meta={pageMeta(podcastEpisodes.page)}
        actions={
          <button className="panel-button" onClick={() => onAction({ type: "toggle_save_indexed_item", block: "podcast_episodes", index: podcastEpisodes.selected_index })} type="button">
            Save Show
          </button>
        }
      >
        {podcastEpisodes.show ? <ShowHero show={podcastEpisodes.show} /> : null}
        <EpisodeRows
          episodes={podcastEpisodes.episodes}
          selectedIndex={podcastEpisodes.selected_index}
          onPlay={(index) => onAction({ type: "play_indexed_item", block: "podcast_episodes", index })}
          onQueue={(index) => onAction({ type: "queue_indexed_item", block: "podcast_episodes", index })}
        />
      </Panel>
    </div>
  );
}

function LyricsView({ lyrics }: { lyrics: GuiSnapshot["lyrics"] }) {
  return (
    <div className="route-grid">
      <Panel eyebrow={lyrics.status} title="Lyrics" meta={String(lyrics.lines.length)}>
        <ListBlock
          emptyLabel="No synced lyrics available."
          items={lyrics.lines.map((line) => (
            <div className="lyric-row" key={`${line.timestamp_ms}-${line.text}`}>
              <span>{formatDuration(line.timestamp_ms)}</span>
              <strong>{line.text}</strong>
            </div>
          ))}
        />
      </Panel>
    </div>
  );
}

function DiscoverView({
  discover,
  onAction,
}: {
  discover: GuiDiscover;
  onAction: (action: ActionPayload) => void;
}) {
  return (
    <div className="route-grid route-grid-two">
      <Panel eyebrow={discover.time_range} title="Top Tracks" meta={discover.loading ? "Loading" : String(discover.top_tracks.length)}>
        <TrackTableBlock
          block="discover_top_tracks"
          selectedIndex={discover.selected_index}
          tracks={discover.top_tracks}
          onAction={onAction}
        />
      </Panel>

      <Panel eyebrow="Mix" title="Artists Mix" meta={String(discover.artists_mix.length)}>
        <TrackTableBlock
          block="discover_artists_mix"
          selectedIndex={discover.selected_index}
          tracks={discover.artists_mix}
          onAction={onAction}
        />
      </Panel>
    </div>
  );
}

function DevicesView({
  devices,
  playback,
  onAction,
}: {
  devices: GuiDevice[];
  playback: GuiPlayback;
  onAction: (action: ActionPayload) => void;
}) {
  return (
    <div className="route-grid">
      <Panel eyebrow="Devices" title="Playback Targets" meta={String(devices.length)}>
        <ListBlock
          emptyLabel="No devices available."
          items={devices.map((device) => (
            <button
              className={`device-card ${device.is_active ? "is-active" : ""}`}
              key={device.id ?? device.name}
              onClick={() => {
                if (device.id) {
                  onAction({ type: "transfer_playback", device_id: device.id, play: playback.is_playing });
                }
              }}
              type="button"
            >
              <div>
                <strong>{device.name}</strong>
                <span>{device.device_type}</span>
              </div>
              <small>{device.volume_percent ?? 0}%</small>
            </button>
          ))}
        />
      </Panel>
    </div>
  );
}

function HelpView({ help }: { help: GuiHelp }) {
  return (
    <div className="route-grid">
      <Panel eyebrow={`Page ${help.page + 1}`} title="Help" meta={`${help.docs.length} bindings`}>
        <ListBlock
          emptyLabel="Help index unavailable."
          items={help.docs.map((item) => (
            <div className="help-row" key={`${item.binding}-${item.description}`}>
              <strong>{item.description}</strong>
              <span>{item.context}</span>
              <code>{item.binding}</code>
            </div>
          ))}
        />
      </Panel>
    </div>
  );
}

function AnalysisView({
  snapshot,
  onAction,
}: {
  snapshot: GuiSnapshot;
  onAction: (action: ActionPayload) => void;
}) {
  return (
    <div className="route-grid route-grid-two">
      <Panel
        eyebrow={snapshot.analysis.audio_capture_active ? "Capture Active" : "Capture Idle"}
        title="Audio Analysis"
        meta={snapshot.analysis.visualizer_style}
        actions={
          <button className="panel-button" onClick={() => onAction({ type: "cycle_visualizer_style" })} type="button">
            Cycle Style
          </button>
        }
      >
        <div className="analysis-metrics">
          <div>
            <dt>Tick rate</dt>
            <dd>{snapshot.analysis.tick_rate_ms} ms</dd>
          </div>
          <div>
            <dt>Peak</dt>
            <dd>{snapshot.analysis.peak?.toFixed(3) ?? "n/a"}</dd>
          </div>
          <div>
            <dt>Bars</dt>
            <dd>{snapshot.analysis.bands.length}</dd>
          </div>
        </div>
      </Panel>

      <Panel eyebrow="Spectrum" title="Bands" meta={String(snapshot.analysis.bands.length)}>
        <SpectrumBars bands={snapshot.analysis.bands} />
      </Panel>
    </div>
  );
}

function CoverArtView({ snapshot }: { snapshot: GuiSnapshot }) {
  return (
    <div className="route-grid route-grid-two">
      <Panel eyebrow={snapshot.cover_art.mode} title="Cover Art" meta={snapshot.cover_art.device_name ?? "No device"}>
        <div className="cover-stage">
          <CoverArt imageUrl={snapshot.cover_art.image_url} label={trackInitials(snapshot.cover_art.track)} size="xl" />
        </div>
      </Panel>

      <Panel eyebrow={snapshot.cover_art.enabled ? "Enabled" : "Disabled"} title={snapshot.cover_art.track?.title || "No track"}>
        <div className="detail-copy">
          <strong>{artistLine(snapshot.cover_art.track)}</strong>
          <p>{snapshot.cover_art.track?.album || "No album metadata available."}</p>
        </div>
        <dl className="meta-list">
          <div>
            <dt>Mode</dt>
            <dd>{snapshot.cover_art.mode}</dd>
          </div>
          <div>
            <dt>Forced</dt>
            <dd>{snapshot.cover_art.forced ? "Yes" : "No"}</dd>
          </div>
          <div>
            <dt>Device</dt>
            <dd>{snapshot.cover_art.device_name || "Unavailable"}</dd>
          </div>
        </dl>
      </Panel>
    </div>
  );
}

function SettingsView({
  settings,
  onAction,
}: {
  settings: GuiSettings;
  onAction: (action: ActionPayload) => void;
}) {
  const selectedItem = settings.items[settings.selected_index];

  return (
    <div className="route-grid route-grid-settings">
      <Panel eyebrow="Categories" title="Settings" meta={settings.category}>
        <div className="category-list">
          {settings.categories.map((category, index) => (
            <button
              className={`category-pill ${index === settings.category_index ? "is-active" : ""}`}
              key={category}
              onClick={() => onAction({ type: "select_settings_category", index })}
              type="button"
            >
              {category}
            </button>
          ))}
        </div>

        <div className="settings-list">
          {settings.items.map((item, index) => (
            <button
              className={`settings-row ${index === settings.selected_index ? "is-active" : ""}`}
              key={item.id}
              onClick={() => onAction({ type: "select_settings_item", index })}
              type="button"
            >
              <strong>{item.name}</strong>
              <span>{item.value}</span>
            </button>
          ))}
        </div>
      </Panel>

      <Panel
        eyebrow={settings.edit_mode ? "Edit Mode" : selectedItem?.value_type || "Setting"}
        title={selectedItem?.name || "Select a setting"}
        actions={
          <>
            <button className="panel-button" onClick={() => onAction({ type: "activate_setting" })} type="button">
              Activate
            </button>
            <button className="panel-button" onClick={() => onAction({ type: "save_settings" })} type="button">
              Save
            </button>
          </>
        }
      >
        {selectedItem ? (
          <>
            <div className="detail-copy">
              <strong>{selectedItem.value}</strong>
              <p>{selectedItem.description}</p>
            </div>
            {settings.edit_mode ? (
              <div className="editor-stack">
                <input
                  className="text-input"
                  onChange={(event) => onAction({ type: "update_settings_edit_buffer", value: event.currentTarget.value })}
                  type="text"
                  value={settings.edit_buffer}
                />
                <div className="button-row">
                  <button className="primary-button" onClick={() => onAction({ type: "commit_settings_edit" })} type="button">
                    Commit
                  </button>
                  <button className="panel-button" onClick={() => onAction({ type: "cancel_settings_edit" })} type="button">
                    Cancel
                  </button>
                </div>
              </div>
            ) : null}
          </>
        ) : (
          <EmptyState label="No setting selected." />
        )}

        {settings.unsaved_prompt_visible ? (
          <div className="prompt-banner">
            <span>Unsaved changes detected.</span>
            <div className="button-row">
              <button
                className={`panel-button ${settings.unsaved_prompt_save_selected ? "is-primary-alt" : ""}`}
                onClick={() => onAction({ type: "resolve_settings_unsaved_prompt", save: true })}
                type="button"
              >
                Save
              </button>
              <button
                className={`panel-button ${!settings.unsaved_prompt_save_selected ? "is-primary-alt" : ""}`}
                onClick={() => onAction({ type: "resolve_settings_unsaved_prompt", save: false })}
                type="button"
              >
                Discard
              </button>
            </div>
          </div>
        ) : null}
      </Panel>
    </div>
  );
}

function PartyView({
  party,
  partyCodeDraft,
  partyNameDraft,
  onAction,
  onPartyCodeDraftChange,
  onPartyNameDraftChange,
}: {
  party: GuiParty;
  partyCodeDraft: string;
  partyNameDraft: string;
  onAction: (action: ActionPayload) => void;
  onPartyCodeDraftChange: (value: string) => void;
  onPartyNameDraftChange: (value: string) => void;
}) {
  return (
    <div className="route-grid route-grid-two">
      <Panel
        eyebrow={party.status}
        title="Listening Party"
        meta={party.code || "No room"}
        actions={
          <>
            <button className="panel-button" onClick={() => onAction({ type: "leave_party" })} type="button">
              Leave
            </button>
            <button className="panel-button" onClick={() => onAction({ type: "set_party_control_mode", control_mode: "shared_control" })} type="button">
              Shared
            </button>
          </>
        }
      >
        <div className="party-grid">
          <button className="primary-button" onClick={() => onAction({ type: "start_party", control_mode: "host_only" })} type="button">
            Host Only
          </button>
          <button className="panel-button" onClick={() => onAction({ type: "start_party", control_mode: "shared_control" })} type="button">
            Host Shared
          </button>
        </div>
        <dl className="meta-list">
          <div>
            <dt>Role</dt>
            <dd>{party.role || "None"}</dd>
          </div>
          <div>
            <dt>Host</dt>
            <dd>{party.host_name || "Unavailable"}</dd>
          </div>
          <div>
            <dt>Control</dt>
            <dd>{party.control_mode || "Unavailable"}</dd>
          </div>
          <div>
            <dt>Guests</dt>
            <dd>{party.guests.length}</dd>
          </div>
        </dl>
      </Panel>

      <Panel eyebrow="Join" title="Join Room" meta="Relay">
        <div className="editor-stack">
          <input
            className="text-input"
            onChange={(event) => onPartyCodeDraftChange(event.currentTarget.value)}
            placeholder="Room code"
            type="text"
            value={partyCodeDraft}
          />
          <input
            className="text-input"
            onChange={(event) => onPartyNameDraftChange(event.currentTarget.value)}
            placeholder="Display name"
            type="text"
            value={partyNameDraft}
          />
          <div className="button-row">
            <button
              className="primary-button"
              onClick={() => onAction({ type: "join_party", code: partyCodeDraft, name: partyNameDraft })}
              type="button"
            >
              Join
            </button>
            <button className="panel-button" onClick={() => onAction({ type: "party_playback_command", action: { play: null } })} disabled type="button">
              Sync via route
            </button>
          </div>
        </div>

        <ListBlock
          emptyLabel="No guests connected."
          items={party.guests.map((guest) => (
            <div className="simple-line" key={guest}>
              {guest}
            </div>
          ))}
        />
      </Panel>
    </div>
  );
}

function CreatePlaylistView({
  createPlaylist,
  onAction,
}: {
  createPlaylist: GuiCreatePlaylist;
  onAction: (action: ActionPayload) => void;
}) {
  return (
    <div className="route-grid route-grid-two">
      <Panel eyebrow={createPlaylist.stage} title={createPlaylist.name || "Create Playlist"} meta={createPlaylist.focus}>
        <TrackTableBlock block="track_table" selectedIndex={-1} tracks={createPlaylist.tracks} onAction={onAction} />
      </Panel>

      <Panel eyebrow="Search" title="Candidate Tracks" meta={createPlaylist.search_input || "No query"}>
        <TrackTableBlock block="track_table" selectedIndex={createPlaylist.selected_result} tracks={createPlaylist.search_results} onAction={onAction} />
      </Panel>
    </div>
  );
}

function ErrorView({ status }: { status: GuiStatus }) {
  return (
    <div className="route-grid">
      <Panel eyebrow="Error" title="Backend Error">
        <div className="detail-copy">
          <strong>{status.error || "Unknown error"}</strong>
          <p>{status.message || "The backend reported an error while serving the snapshot."}</p>
        </div>
      </Panel>
    </div>
  );
}

function OverlayStack({
  announcement,
  dialog,
  overlayRoute,
  sort,
  status,
  onAction,
}: {
  announcement: GuiAnnouncement;
  dialog: GuiDialog;
  overlayRoute: RouteId | null;
  sort: GuiSort;
  status: GuiStatus;
  onAction: (action: ActionPayload) => void;
}) {
  const showDialog = overlayRoute === "dialog" && dialog.kind;
  const showAnnouncement = overlayRoute === "announcement" && announcement.active;
  const showExit = overlayRoute === "exit";

  if (!showDialog && !showAnnouncement && !showExit && !sort.visible) {
    return null;
  }

  return (
    <div className="overlay-root">
      {showDialog ? <DialogOverlay dialog={dialog} onAction={onAction} /> : null}
      {showAnnouncement ? <AnnouncementOverlay announcement={announcement} onAction={onAction} /> : null}
      {showExit ? <ExitOverlay status={status} onAction={onAction} /> : null}
      {sort.visible ? <SortOverlay sort={sort} onAction={onAction} /> : null}
    </div>
  );
}

function DialogOverlay({
  dialog,
  onAction,
}: {
  dialog: GuiDialog;
  onAction: (action: ActionPayload) => void;
}) {
  return (
    <div className="overlay-card">
      <div className="overlay-eyebrow">{dialog.kind || "dialog"}</div>
      <h2>{dialog.title || "Dialog"}</h2>
      <p>{dialog.message || "A TUI dialog is active."}</p>

      {dialog.pending_track_name ? <div className="overlay-note">Track: {dialog.pending_track_name}</div> : null}
      {dialog.playlist_name ? <div className="overlay-note">Playlist: {dialog.playlist_name}</div> : null}

      {dialog.playlist_options.length > 0 ? (
        <div className="overlay-list">
          {dialog.playlist_options.map((option, index) => (
            <button
              className={`overlay-row ${index === dialog.selected_index ? "is-active" : ""}`}
              key={option.id || `${option.label}-${index}`}
              onClick={() => onAction({ type: "dialog_select_index", index })}
              type="button"
            >
              <strong>{option.label}</strong>
              <span>{option.description || option.id}</span>
            </button>
          ))}
        </div>
      ) : null}

      <div className="button-row">
        <button className="primary-button" onClick={() => onAction({ type: "dialog_confirm" })} type="button">
          {dialog.confirm_label || "Confirm"}
        </button>
        <button className="panel-button" onClick={() => onAction({ type: "dialog_cancel" })} type="button">
          {dialog.cancel_label || "Cancel"}
        </button>
      </div>
    </div>
  );
}

function AnnouncementOverlay({
  announcement,
  onAction,
}: {
  announcement: GuiAnnouncement;
  onAction: (action: ActionPayload) => void;
}) {
  const active = announcement.active;
  if (!active) {
    return null;
  }

  return (
    <div className="overlay-card">
      <div className="overlay-eyebrow">{active.level}</div>
      <h2>{active.title}</h2>
      <p>{active.body}</p>
      {active.url ? (
        <a className="overlay-link" href={active.url} rel="noreferrer" target="_blank">
          {active.url}
        </a>
      ) : null}
      <div className="overlay-note">{announcement.pending.length} pending announcements in queue.</div>
      <div className="button-row">
        <button className="primary-button" onClick={() => onAction({ type: "dismiss_announcement" })} type="button">
          Dismiss
        </button>
      </div>
    </div>
  );
}

function ExitOverlay({
  status,
  onAction,
}: {
  status: GuiStatus;
  onAction: (action: ActionPayload) => void;
}) {
  return (
    <div className="overlay-card">
      <div className="overlay-eyebrow">exit prompt</div>
      <h2>Exit Prompt Active</h2>
      <p>{status.message || "The TUI backend has opened its exit prompt. Return to the previous content route to continue."}</p>
      <div className="button-row">
        <button className="primary-button" onClick={() => onAction({ type: "back" })} type="button">
          Return
        </button>
      </div>
    </div>
  );
}

function SortOverlay({
  sort,
  onAction,
}: {
  sort: GuiSort;
  onAction: (action: ActionPayload) => void;
}) {
  return (
    <div className="overlay-card is-compact">
      <div className="overlay-eyebrow">{sort.context || "sort"}</div>
      <h2>{sort.title || "Sort Menu"}</h2>
      <p>
        {sort.current_field || "No field"} / {sort.current_order || "No order"}
      </p>
      <div className="overlay-list">
        {sort.options.map((option, index) => (
          <button
            className={`overlay-row ${option.active ? "is-active" : ""}`}
            key={`${option.field}-${option.label}`}
            onClick={() => onAction({ type: "apply_sort_option", index })}
            type="button"
          >
            <strong>{option.label}</strong>
            <span>{option.shortcut || option.field}</span>
          </button>
        ))}
      </div>
      <div className="button-row">
        <button className="primary-button" onClick={() => onAction({ type: "close_sort_menu" })} type="button">
          Close
        </button>
      </div>
    </div>
  );
}

function PlayerBar({
  activeDevice,
  bridge,
  playback,
  status,
  onAction,
}: {
  activeDevice?: GuiDevice;
  bridge: BridgeState;
  playback: GuiPlayback;
  status: GuiStatus;
  onAction: (action: ActionPayload) => void;
}) {
  const progressPercent = toPercent(playback.progress_ms, playback.track?.duration_ms ?? 0);

  return (
    <footer className="player-bar">
      <div className="playbar-track">
        <CoverArt imageUrl={playback.track?.image_url} label={trackInitials(playback.track)} size="md" />
        <div className="playbar-copy">
          <strong>{playback.track?.title || "Nothing playing"}</strong>
          <span>{artistLine(playback.track)}</span>
          <small>{playback.track?.album || bridge.message}</small>
        </div>
      </div>

      <div className="playbar-center">
        <div className="transport-row">
          <button className={`transport-button ${playback.shuffle ? "is-active" : ""}`} onClick={() => onAction({ type: "toggle_shuffle" })} type="button">
            Shuffle
          </button>
          <button className="transport-icon-button" onClick={() => onAction({ type: "previous_track" })} type="button">
            <span className="glyph glyph-prev" />
          </button>
          <button className="transport-play" onClick={() => onAction({ type: "toggle_playback" })} type="button">
            <span className={`glyph ${playback.is_playing ? "glyph-pause" : "glyph-play"}`} />
          </button>
          <button className="transport-icon-button" onClick={() => onAction({ type: "next_track" })} type="button">
            <span className="glyph glyph-next" />
          </button>
          <button
            className={`transport-button ${playback.repeat && playback.repeat !== "off" ? "is-active" : ""}`}
            onClick={() => onAction({ type: "toggle_repeat" })}
            type="button"
          >
            Repeat
          </button>
        </div>

        <div className="seek-row">
          <span>{formatDuration(playback.progress_ms)}</span>
          <input
            aria-label="Seek position"
            className="range-input"
            max={playback.track?.duration_ms ?? 0}
            min={0}
            onChange={(event) => onAction({ type: "seek", position_ms: Number(event.currentTarget.value) })}
            style={{ "--range-fill": `${progressPercent}%` } as CSSProperties}
            type="range"
            value={Math.min(playback.progress_ms, playback.track?.duration_ms ?? 0)}
          />
          <span>{formatDuration(playback.track?.duration_ms ?? 0)}</span>
        </div>
      </div>

      <div className="playbar-meta">
        <div className="meta-pill-row">
          <button
            className={`meta-pill ${playback.track?.saved ? "is-active" : ""}`}
            onClick={() => {
              if (playback.track?.uri) {
                onAction({ type: "toggle_save_track", uri: playback.track.uri });
              }
            }}
            type="button"
          >
            Save
          </button>
          <button className="meta-pill" onClick={() => onAction({ type: "open_devices" })} type="button">
            {activeDevice?.name || "Devices"}
          </button>
          <span className={`meta-pill is-static is-${bridge.mode}`}>{status.message || bridge.message}</span>
        </div>
        <div className="volume-row">
          <span>{playback.volume_percent}%</span>
          <input
            aria-label="Volume"
            className="range-input"
            max={100}
            min={0}
            onChange={(event) => onAction({ type: "change_volume", volume_percent: Number(event.currentTarget.value) })}
            style={{ "--range-fill": `${playback.volume_percent}%` } as CSSProperties}
            type="range"
            value={playback.volume_percent}
          />
        </div>
      </div>
    </footer>
  );
}

function Panel({
  eyebrow,
  title,
  meta,
  actions,
  children,
}: {
  eyebrow: string;
  title: string;
  meta?: string;
  actions?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="panel">
      <div className="panel-header">
        <div>
          <div className="panel-eyebrow">{eyebrow}</div>
          <h2>{title}</h2>
        </div>
        <div className="panel-header-right">
          {meta ? <span className="panel-meta">{meta}</span> : null}
          {actions ? <div className="panel-actions">{actions}</div> : null}
        </div>
      </div>
      <div className="panel-body">{children}</div>
    </section>
  );
}

function TrackTableBlock({
  block,
  tracks,
  selectedIndex,
  onAction,
  onPlay,
}: {
  block: string;
  tracks: GuiTrack[];
  selectedIndex: number;
  onAction: (action: ActionPayload) => void;
  onPlay?: (index: number) => Promise<void>;
}) {
  if (tracks.length === 0) {
    return <EmptyState label="No tracks available." />;
  }

  return (
    <div className="data-table">
      <div className="data-head track-grid">
        <span>#</span>
        <span>Title</span>
        <span>Artist</span>
        <span>Album</span>
        <span>Time</span>
        <span className="data-actions-head">Actions</span>
      </div>

      {tracks.map((track, index) => (
        <div className={`data-row track-grid ${selectedIndex === index ? "is-active" : ""}`} key={`${track.uri ?? track.id ?? track.title}-${index}`}>
          <button
            className="row-main track-grid"
            onClick={() => {
              if (block === "track_table") {
                onAction({ type: "select_track", index });
              } else {
                onAction({ type: "open_indexed_item", block, index });
              }
            }}
            type="button"
          >
            <span>{index + 1}</span>
            <span className="row-title-cell">
              <CoverArt imageUrl={track.image_url} label={trackInitials(track)} size="smol" />
              <strong>{track.title || "Unknown"}</strong>
            </span>
            <span>{artistLine(track)}</span>
            <span>{track.album || "-"}</span>
            <span>{formatDuration(track.duration_ms)}</span>
          </button>
          <div className="row-actions">
            <button className="table-action" onClick={() => (onPlay ? void onPlay(index) : onAction({ type: "play_indexed_item", block, index }))} type="button">
              Play
            </button>
            <button className="table-action" onClick={() => onAction({ type: "queue_indexed_item", block, index })} type="button">
              Queue
            </button>
            <button className={`table-action ${track.saved ? "is-active" : ""}`} onClick={() => onAction({ type: "toggle_save_indexed_item", block, index })} type="button">
              Save
            </button>
          </div>
        </div>
      ))}
    </div>
  );
}

function AlbumRows({
  albums,
  selectedIndex,
  onOpen,
  onSave,
}: {
  albums: GuiAlbum[];
  selectedIndex: number;
  onOpen: (index: number) => void;
  onSave: (index: number) => void;
}) {
  return (
    <MediaList
      emptyLabel="No albums available."
      items={albums.map((album, index) => (
        <MediaRow
          key={`${album.id ?? album.name}-${index}`}
          active={selectedIndex === index}
          cover={<CoverArt imageUrl={album.image_url} label={album.name.slice(0, 2).toUpperCase()} size="smol" />}
          meta={album.release_date || ""}
          subtitle={album.artists.join(", ")}
          title={album.name}
          tools={
            <button className={`table-action ${album.saved ? "is-active" : ""}`} onClick={() => onSave(index)} type="button">
              Save
            </button>
          }
          onClick={() => onOpen(index)}
        />
      ))}
    />
  );
}

function ArtistRows({
  artists,
  selectedIndex,
  onOpen,
  onRecommend,
  onSave,
}: {
  artists: GuiArtist[];
  selectedIndex: number;
  onOpen: (index: number) => void;
  onRecommend: (index: number) => void;
  onSave: (index: number) => void;
}) {
  return (
    <MediaList
      emptyLabel="No artists available."
      items={artists.map((artist, index) => (
        <MediaRow
          key={`${artist.id ?? artist.name}-${index}`}
          active={selectedIndex === index}
          cover={<CoverArt imageUrl={artist.image_url} label={artist.name.slice(0, 2).toUpperCase()} size="smol" />}
          meta={artist.followers ? formatNumber(artist.followers) : ""}
          subtitle={artist.followers ? "followers" : ""}
          title={artist.name}
          tools={
            <>
              <button className="table-action" onClick={() => onRecommend(index)} type="button">
                Mix
              </button>
              <button className={`table-action ${artist.saved ? "is-active" : ""}`} onClick={() => onSave(index)} type="button">
                Save
              </button>
            </>
          }
          onClick={() => onOpen(index)}
        />
      ))}
    />
  );
}

function PlaylistRows({
  playlists,
  selectedIndex,
  onOpen,
}: {
  playlists: GuiPlaylist[];
  selectedIndex: number;
  onOpen: (index: number) => void;
}) {
  return (
    <MediaList
      emptyLabel="No playlists available."
      items={playlists.map((playlist, index) => (
        <MediaRow
          key={`${playlist.id}-${index}`}
          active={selectedIndex === index}
          cover={<CoverArt imageUrl={playlist.image_url} label={playlist.name.slice(0, 2).toUpperCase()} size="smol" />}
          meta={String(playlist.track_count)}
          subtitle={playlist.owner}
          title={playlist.name}
          onClick={() => onOpen(index)}
        />
      ))}
    />
  );
}

function ShowRows({
  shows,
  selectedIndex,
  onOpen,
  onSave,
}: {
  shows: GuiShow[];
  selectedIndex: number;
  onOpen: (index: number) => void;
  onSave: (index: number) => void;
}) {
  return (
    <MediaList
      emptyLabel="No podcasts available."
      items={shows.map((show, index) => (
        <MediaRow
          key={`${show.id ?? show.name}-${index}`}
          active={selectedIndex === index}
          cover={<CoverArt imageUrl={show.image_url} label={show.name.slice(0, 2).toUpperCase()} size="smol" />}
          meta={show.publisher || ""}
          subtitle={show.description || ""}
          title={show.name}
          tools={
            <button className={`table-action ${show.saved ? "is-active" : ""}`} onClick={() => onSave(index)} type="button">
              Save
            </button>
          }
          onClick={() => onOpen(index)}
        />
      ))}
    />
  );
}

function EpisodeRows({
  episodes,
  selectedIndex,
  onPlay,
  onQueue,
}: {
  episodes: GuiEpisode[];
  selectedIndex: number;
  onPlay: (index: number) => void;
  onQueue: (index: number) => void;
}) {
  return (
    <MediaList
      emptyLabel="No episodes available."
      items={episodes.map((episode, index) => (
        <MediaRow
          key={`${episode.id ?? episode.title}-${index}`}
          active={selectedIndex === index}
          cover={<CoverArt imageUrl={episode.image_url} label={episode.title.slice(0, 2).toUpperCase()} size="smol" />}
          meta={formatDuration(episode.duration_ms)}
          subtitle={`${episode.show}${episode.release_date ? ` / ${episode.release_date}` : ""}`}
          title={episode.title}
          tools={
            <>
              <button className="table-action" onClick={() => onPlay(index)} type="button">
                Play
              </button>
              <button className="table-action" onClick={() => onQueue(index)} type="button">
                Queue
              </button>
            </>
          }
          onClick={() => onPlay(index)}
        />
      ))}
    />
  );
}

function MediaList({
  emptyLabel,
  items,
}: {
  emptyLabel: string;
  items: ReactNode[];
}) {
  return <div className="media-list">{items.length > 0 ? items : <EmptyState label={emptyLabel} />}</div>;
}

function MediaRow({
  active,
  cover,
  meta,
  subtitle,
  title,
  tools,
  onClick,
}: {
  active: boolean;
  cover: ReactNode;
  meta?: string;
  subtitle?: string;
  title: string;
  tools?: ReactNode;
  onClick: () => void;
}) {
  return (
    <div className={`media-row ${active ? "is-active" : ""}`}>
      <button className="media-row-main" onClick={onClick} type="button">
        {cover}
        <span className="media-row-copy">
          <strong>{title}</strong>
          <span>{subtitle || meta || ""}</span>
        </span>
        {meta ? <small>{meta}</small> : null}
      </button>
      {tools ? <div className="row-actions">{tools}</div> : null}
    </div>
  );
}

function ListBlock({ emptyLabel, items }: { emptyLabel: string; items: ReactNode[] }) {
  return <div className="stack-list">{items.length > 0 ? items : <EmptyState label={emptyLabel} />}</div>;
}

function EmptyState({ label }: { label: string }) {
  return <div className="empty-state">{label}</div>;
}

function NowCard({ track }: { track: GuiTrack }) {
  return (
    <div className="hero-card">
      <CoverArt imageUrl={track.image_url} label={trackInitials(track)} size="lg" />
      <div className="detail-copy">
        <strong>{track.title}</strong>
        <p>{artistLine(track)}</p>
        <p>{track.album || "No album metadata available."}</p>
      </div>
    </div>
  );
}

function AlbumHero({ album }: { album: GuiAlbum }) {
  return (
    <div className="hero-card is-compact">
      <CoverArt imageUrl={album.image_url} label={album.name.slice(0, 2).toUpperCase()} size="lg" />
      <div className="detail-copy">
        <strong>{album.name}</strong>
        <p>{album.artists.join(", ")}</p>
        <p>{album.release_date || "Release date unavailable"}</p>
      </div>
    </div>
  );
}

function ArtistHero({ artist }: { artist: GuiArtist }) {
  return (
    <div className="hero-card is-compact">
      <CoverArt imageUrl={artist.image_url} label={artist.name.slice(0, 2).toUpperCase()} size="lg" />
      <div className="detail-copy">
        <strong>{artist.name}</strong>
        <p>{artist.followers ? `${formatNumber(artist.followers)} followers` : "Follower count unavailable"}</p>
      </div>
    </div>
  );
}

function ShowHero({ show }: { show: GuiShow }) {
  return (
    <div className="hero-card is-compact">
      <CoverArt imageUrl={show.image_url} label={show.name.slice(0, 2).toUpperCase()} size="lg" />
      <div className="detail-copy">
        <strong>{show.name}</strong>
        <p>{show.publisher || "Publisher unavailable"}</p>
        <p>{show.description || "No description available."}</p>
      </div>
    </div>
  );
}

function SpectrumBars({ bands }: { bands: number[] }) {
  if (bands.length === 0) {
    return <EmptyState label="No analysis bands captured yet." />;
  }

  return (
    <div className="spectrum-bars">
      {bands.map((band, index) => (
        <span
          className="spectrum-bar"
          key={`${band}-${index}`}
          style={{ "--bar-height": `${Math.max(6, Math.min(100, band * 100))}%` } as CSSProperties}
        />
      ))}
    </div>
  );
}

function CoverArt({
  imageUrl,
  label,
  size,
}: {
  imageUrl: string | null | undefined;
  label: string;
  size: "smol" | "md" | "lg" | "xl";
}) {
  const style = imageUrl
    ? ({ backgroundImage: `url("${cssUrl(imageUrl)}")` } as CSSProperties)
    : coverGradient(label);
  return <span className={`cover-art cover-${size} ${imageUrl ? "has-image" : ""}`} data-label={imageUrl ? "" : label} style={style} />;
}

function pageMeta(page: { page_index: number; page_count: number; total: number }): string {
  const currentPage = page.page_count > 0 ? page.page_index + 1 : 0;
  return `${currentPage}/${page.page_count || 0} pages • ${page.total} total`;
}

function mapSortContext(context: string | null): string {
  switch (context) {
    case "saved_albums":
      return "saved_albums";
    case "artists":
      return "saved_artists";
    case "recently_played":
      return "recently_played";
    default:
      return "playlist_tracks";
  }
}

function artistLine(track?: GuiTrack | null): string {
  return track?.artists.join(", ") || "Unknown artist";
}

function trackInitials(track?: GuiTrack | null): string {
  const title = track?.title || "ST";
  return title
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0])
    .join("")
    .toUpperCase();
}

function formatDuration(ms: number): string {
  const safe = Number.isFinite(ms) ? Math.max(0, ms) : 0;
  const totalSeconds = Math.floor(safe / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}

function formatNumber(value: number): string {
  return new Intl.NumberFormat().format(value);
}

function toPercent(value: number, total: number): number {
  if (total <= 0) {
    return 0;
  }
  return Math.max(0, Math.min(100, (value / total) * 100));
}

function coverGradient(seed: string): CSSProperties {
  const palette = [
    ["#0f2615", "#1db954", "#0a0f0b"],
    ["#173022", "#3de17c", "#0a0e0c"],
    ["#1f2c1d", "#86e370", "#0b0d0b"],
    ["#172726", "#5cc98a", "#0a0d0d"],
  ];
  const index =
    seed.split("").reduce((total, character) => total + character.charCodeAt(0), 0) %
    palette.length;
  const [from, via, to] = palette[index];
  return {
    "--cover-from": from,
    "--cover-via": via,
    "--cover-to": to,
  } as CSSProperties;
}

function cssUrl(value: string): string {
  return value.replaceAll("\\", "\\\\").replaceAll('"', '\\"');
}
