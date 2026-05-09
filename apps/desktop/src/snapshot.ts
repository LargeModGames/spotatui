export type RouteId =
  | "analysis"
  | "album_tracks"
  | "albums"
  | "artist"
  | "lyrics"
  | "cover_art"
  | "error"
  | "home"
  | "recently_played"
  | "search"
  | "devices"
  | "track_table"
  | "discover"
  | "artists"
  | "podcasts"
  | "podcast_episodes"
  | "recommendations"
  | "dialog"
  | "announcement"
  | "exit"
  | "settings"
  | "help"
  | "queue"
  | "party"
  | "create_playlist";

export type DeepPartial<T> = T extends Array<infer U>
  ? Array<DeepPartial<U>>
  : T extends object
    ? { [K in keyof T]?: DeepPartial<T[K]> }
    : T;

export type GuiTrack = {
  id: string | null;
  uri: string | null;
  item_type: string;
  title: string;
  artists: string[];
  album: string | null;
  image_url: string | null;
  duration_ms: number;
  saved: boolean;
};

export type GuiPlayback = {
  track: GuiTrack | null;
  progress_ms: number;
  is_playing: boolean;
  shuffle: boolean;
  repeat: string | null;
  volume_percent: number;
  device_id: string | null;
  device_name: string | null;
};

export type GuiDevice = {
  id: string | null;
  name: string;
  device_type: string;
  is_active: boolean;
  is_restricted: boolean;
  volume_percent: number | null;
};

export type GuiStatus = {
  is_loading: boolean;
  message: string | null;
  error: string | null;
  route: string;
  active_block: string;
  is_streaming_active: boolean;
  route_id: RouteId;
  content_route_id: RouteId;
  hovered_block: string;
};

export type GuiUser = {
  id: string;
  display_name: string | null;
  country: string | null;
};

export type GuiPageInfo = {
  offset: number;
  limit: number;
  total: number;
  page_index: number;
  page_count: number;
  has_previous: boolean;
  has_next: boolean;
};

export type GuiCursorInfo = {
  page_index: number;
  page_count: number;
  has_previous: boolean;
  has_next: boolean;
};

export type GuiLibrary = {
  options: string[];
  selected_index: number;
  saved_tracks: GuiPageInfo;
  saved_albums: GuiPageInfo;
  saved_artists: GuiCursorInfo;
  saved_shows: GuiPageInfo;
};

export type GuiPlaylist = {
  id: string;
  uri: string;
  name: string;
  owner: string;
  description: string | null;
  image_url: string | null;
  track_count: number;
  collaborative: boolean;
  editable: boolean;
  selected: boolean;
};

export type GuiPlaylistFolderEntry = {
  kind: string;
  id: string | null;
  name: string;
  index: number;
  depth: number;
  selected: boolean;
};

export type GuiTrackTable = {
  context: string | null;
  selected_index: number;
  tracks: GuiTrack[];
  page: GuiPageInfo;
  playlist_id: string | null;
  playlist_name: string | null;
};

export type GuiAlbum = {
  id: string | null;
  uri: string | null;
  name: string;
  artists: string[];
  image_url: string | null;
  release_date: string | null;
  total_tracks: number | null;
  saved: boolean;
};

export type GuiArtist = {
  id: string | null;
  uri: string | null;
  name: string;
  image_url: string | null;
  followers: number | null;
  saved: boolean;
};

export type GuiShow = {
  id: string | null;
  uri: string | null;
  name: string;
  publisher: string | null;
  description: string | null;
  image_url: string | null;
  saved: boolean;
};

export type GuiEpisode = {
  id: string | null;
  uri: string | null;
  title: string;
  show: string;
  description: string | null;
  release_date: string | null;
  image_url: string | null;
  duration_ms: number;
};

export type GuiSearchResults = {
  query: string;
  selected_block: string;
  hovered_block: string;
  selected_track_index: number | null;
  selected_album_index: number | null;
  selected_artist_index: number | null;
  selected_playlist_index: number | null;
  selected_show_index: number | null;
  tracks: GuiTrack[];
  albums: GuiAlbum[];
  artists: GuiArtist[];
  playlists: GuiPlaylist[];
  shows: GuiShow[];
};

export type GuiHome = {
  banner: string[];
  counter_message: string;
  changelog_lines: string[];
  scroll: number;
  log_path: string;
  report_url: string;
};

export type GuiArtistDetail = {
  artist: GuiArtist | null;
  selected_block: string;
  hovered_block: string;
  top_tracks: GuiTrack[];
  selected_top_track_index: number;
  albums: GuiAlbum[];
  selected_album_index: number;
  related_artists: GuiArtist[];
  selected_related_artist_index: number;
};

export type GuiAlbumTracks = {
  album: GuiAlbum | null;
  context: string;
  tracks: GuiTrack[];
  selected_index: number;
  page: GuiPageInfo;
};

export type GuiAlbumList = {
  selected_index: number;
  albums: GuiAlbum[];
};

export type GuiArtistList = {
  selected_index: number;
  artists: GuiArtist[];
};

export type GuiPodcastList = {
  selected_index: number;
  shows: GuiShow[];
};

export type GuiPodcastEpisodes = {
  show: GuiShow | null;
  context: string;
  episodes: GuiEpisode[];
  selected_index: number;
  page: GuiPageInfo;
};

export type GuiQueueView = {
  current: GuiTrack | null;
  items: GuiTrack[];
  selected_index: number;
};

export type GuiHelpItem = {
  description: string;
  binding: string;
  context: string;
};

export type GuiHelp = {
  docs: GuiHelpItem[];
  page: number;
  offset: number;
  page_size: number;
};

export type GuiAnalysis = {
  audio_capture_active: boolean;
  visualizer_style: string;
  tick_rate_ms: number;
  peak: number | null;
  bands: number[];
};

export type GuiCoverArt = {
  track: GuiTrack | null;
  device_name: string | null;
  mode: string;
  enabled: boolean;
  forced: boolean;
  image_url: string | null;
};

export type GuiLyricLine = {
  timestamp_ms: number;
  text: string;
};

export type GuiLyrics = {
  status: string;
  lines: GuiLyricLine[];
};

export type GuiDiscover = {
  selected_index: number;
  time_range: string;
  loading: boolean;
  top_tracks: GuiTrack[];
  artists_mix: GuiTrack[];
};

export type GuiSettingItem = {
  id: string;
  name: string;
  description: string;
  value: string;
  value_type: string;
};

export type GuiSettings = {
  category: string;
  category_index: number;
  categories: string[];
  selected_index: number;
  edit_mode: boolean;
  edit_buffer: string;
  unsaved_prompt_visible: boolean;
  unsaved_prompt_save_selected: boolean;
  items: GuiSettingItem[];
};

export type GuiDialogOption = {
  id: string;
  label: string;
  description: string | null;
};

export type GuiDialog = {
  kind: string | null;
  title: string | null;
  message: string | null;
  confirm: boolean;
  confirm_label: string | null;
  cancel_label: string | null;
  pending_track_name: string | null;
  playlist_name: string | null;
  playlist_options: GuiDialogOption[];
  selected_index: number;
  effective_open_settings_key: string | null;
};

export type GuiSortOption = {
  field: string;
  label: string;
  shortcut: string | null;
  selected: boolean;
  active: boolean;
};

export type GuiSort = {
  visible: boolean;
  selected_index: number;
  context: string | null;
  title: string | null;
  current_field: string | null;
  current_order: string | null;
  options: GuiSortOption[];
};

export type GuiParty = {
  status: string;
  role: string | null;
  code: string | null;
  host_name: string | null;
  guests: string[];
  control_mode: string | null;
  code_input: string;
  join_name: string;
};

export type GuiAnnouncementItem = {
  id: string;
  title: string;
  body: string;
  level: string;
  url: string | null;
};

export type GuiAnnouncement = {
  active: GuiAnnouncementItem | null;
  pending: GuiAnnouncementItem[];
};

export type GuiCreatePlaylist = {
  name: string;
  stage: string;
  focus: string;
  search_input: string;
  selected_result: number;
  tracks: GuiTrack[];
  search_results: GuiTrack[];
};

export type GuiSnapshot = {
  playback: GuiPlayback;
  devices: GuiDevice[];
  status: GuiStatus;
  user: GuiUser | null;
  library: GuiLibrary;
  playlists: GuiPlaylist[];
  playlist_folders: GuiPlaylistFolderEntry[];
  track_table: GuiTrackTable;
  queue: GuiTrack[];
  queue_view: GuiQueueView;
  recently_played: GuiTrack[];
  search: GuiSearchResults;
  home: GuiHome;
  artist_detail: GuiArtistDetail;
  album_tracks: GuiAlbumTracks;
  albums: GuiAlbumList;
  artists: GuiArtistList;
  podcasts: GuiPodcastList;
  podcast_episodes: GuiPodcastEpisodes;
  lyrics: GuiLyrics;
  discover: GuiDiscover;
  help: GuiHelp;
  analysis: GuiAnalysis;
  cover_art: GuiCoverArt;
  settings: GuiSettings;
  dialog: GuiDialog;
  sort: GuiSort;
  party: GuiParty;
  announcement: GuiAnnouncement;
  create_playlist: GuiCreatePlaylist;
};

export const routeIds: RouteId[] = [
  "analysis",
  "album_tracks",
  "albums",
  "artist",
  "lyrics",
  "cover_art",
  "error",
  "home",
  "recently_played",
  "search",
  "devices",
  "track_table",
  "discover",
  "artists",
  "podcasts",
  "podcast_episodes",
  "recommendations",
  "dialog",
  "announcement",
  "exit",
  "settings",
  "help",
  "queue",
  "party",
  "create_playlist",
];

export const routeTitles: Record<RouteId, string> = {
  analysis: "Audio Analysis",
  album_tracks: "Album Tracks",
  albums: "Albums",
  artist: "Artist Detail",
  lyrics: "Lyrics",
  cover_art: "Cover Art",
  error: "Error",
  home: "Home",
  recently_played: "Recently Played",
  search: "Search",
  devices: "Devices",
  track_table: "Track Table",
  discover: "Discover",
  artists: "Artists",
  podcasts: "Podcasts",
  podcast_episodes: "Podcast Episodes",
  recommendations: "Recommendations",
  dialog: "Dialog",
  announcement: "Announcement",
  exit: "Exit Prompt",
  settings: "Settings",
  help: "Help",
  queue: "Queue",
  party: "Listening Party",
  create_playlist: "Create Playlist",
};

const overlayRoutes = new Set<RouteId>(["dialog", "announcement", "exit"]);

const defaultPageInfo: GuiPageInfo = {
  offset: 0,
  limit: 0,
  total: 0,
  page_index: 0,
  page_count: 0,
  has_previous: false,
  has_next: false,
};

const defaultCursorInfo: GuiCursorInfo = {
  page_index: 0,
  page_count: 0,
  has_previous: false,
  has_next: false,
};

const defaultTrack: GuiTrack = {
  id: null,
  uri: null,
  item_type: "track",
  title: "",
  artists: [],
  album: null,
  image_url: null,
  duration_ms: 0,
  saved: false,
};

const defaultAlbum: GuiAlbum = {
  id: null,
  uri: null,
  name: "",
  artists: [],
  image_url: null,
  release_date: null,
  total_tracks: null,
  saved: false,
};

const defaultArtist: GuiArtist = {
  id: null,
  uri: null,
  name: "",
  image_url: null,
  followers: null,
  saved: false,
};

const defaultShow: GuiShow = {
  id: null,
  uri: null,
  name: "",
  publisher: null,
  description: null,
  image_url: null,
  saved: false,
};

export const fallbackSnapshot: GuiSnapshot = {
  playback: {
    track: {
      ...defaultTrack,
      title: "Desktop fallback",
      artists: ["Spotatui"],
      album: "Browser demo",
      duration_ms: 215000,
    },
    progress_ms: 0,
    is_playing: false,
    shuffle: false,
    repeat: "off",
    volume_percent: 58,
    device_id: null,
    device_name: "Browser demo",
  },
  devices: [],
  status: {
    is_loading: false,
    message: "Browser-only demo. Open the Tauri app for live Spotify state.",
    error: null,
    route: "Home",
    active_block: "Home",
    is_streaming_active: false,
    route_id: "home",
    content_route_id: "home",
    hovered_block: "Home",
  },
  user: {
    id: "demo",
    display_name: "Spotatui",
    country: null,
  },
  library: {
    options: ["Discover", "Recently Played", "Liked Songs", "Albums", "Artists", "Podcasts"],
    selected_index: 0,
    saved_tracks: defaultPageInfo,
    saved_albums: defaultPageInfo,
    saved_artists: defaultCursorInfo,
    saved_shows: defaultPageInfo,
  },
  playlists: [],
  playlist_folders: [],
  track_table: {
    context: "saved_tracks",
    selected_index: 0,
    tracks: [],
    page: defaultPageInfo,
    playlist_id: null,
    playlist_name: null,
  },
  queue: [],
  queue_view: {
    current: null,
    items: [],
    selected_index: 0,
  },
  recently_played: [],
  search: {
    query: "",
    selected_block: "song_search",
    hovered_block: "song_search",
    selected_track_index: null,
    selected_album_index: null,
    selected_artist_index: null,
    selected_playlist_index: null,
    selected_show_index: null,
    tracks: [],
    albums: [],
    artists: [],
    playlists: [],
    shows: [],
  },
  home: {
    banner: ["Spotatui Desktop", "TUI parity shell"],
    counter_message: "Global song counter unavailable in browser fallback.",
    changelog_lines: ["Connect the Tauri bridge to view real changelog and report paths."],
    scroll: 0,
    log_path: "logs/spotatui.log",
    report_url: "https://github.com/spotatui/spotatui/issues",
  },
  artist_detail: {
    artist: null,
    selected_block: "top_tracks",
    hovered_block: "top_tracks",
    top_tracks: [],
    selected_top_track_index: 0,
    albums: [],
    selected_album_index: 0,
    related_artists: [],
    selected_related_artist_index: 0,
  },
  album_tracks: {
    album: null,
    context: "full",
    tracks: [],
    selected_index: 0,
    page: defaultPageInfo,
  },
  albums: {
    selected_index: 0,
    albums: [],
  },
  artists: {
    selected_index: 0,
    artists: [],
  },
  podcasts: {
    selected_index: 0,
    shows: [],
  },
  podcast_episodes: {
    show: null,
    context: "full",
    episodes: [],
    selected_index: 0,
    page: defaultPageInfo,
  },
  lyrics: {
    status: "idle",
    lines: [],
  },
  discover: {
    selected_index: 0,
    time_range: "medium_term",
    loading: false,
    top_tracks: [],
    artists_mix: [],
  },
  help: {
    docs: [],
    page: 0,
    offset: 0,
    page_size: 0,
  },
  analysis: {
    audio_capture_active: false,
    visualizer_style: "Bars",
    tick_rate_ms: 16,
    peak: null,
    bands: [],
  },
  cover_art: {
    track: null,
    device_name: null,
    mode: "spotify",
    enabled: false,
    forced: false,
    image_url: null,
  },
  settings: {
    category: "Behavior",
    category_index: 0,
    categories: ["Behavior", "Keybindings", "Theme"],
    selected_index: 0,
    edit_mode: false,
    edit_buffer: "",
    unsaved_prompt_visible: false,
    unsaved_prompt_save_selected: true,
    items: [],
  },
  dialog: {
    kind: null,
    title: null,
    message: null,
    confirm: true,
    confirm_label: null,
    cancel_label: null,
    pending_track_name: null,
    playlist_name: null,
    playlist_options: [],
    selected_index: 0,
    effective_open_settings_key: null,
  },
  sort: {
    visible: false,
    selected_index: 0,
    context: null,
    title: null,
    current_field: null,
    current_order: null,
    options: [],
  },
  party: {
    status: "Disconnected",
    role: null,
    code: null,
    host_name: null,
    guests: [],
    control_mode: null,
    code_input: "",
    join_name: "",
  },
  announcement: {
    active: null,
    pending: [],
  },
  create_playlist: {
    name: "",
    stage: "Name",
    focus: "name",
    search_input: "",
    selected_result: 0,
    tracks: [],
    search_results: [],
  },
};

function routeIdOrDefault(value: string | null | undefined, fallback: RouteId = "home"): RouteId {
  return routeIds.includes(value as RouteId) ? (value as RouteId) : fallback;
}

function mergePageInfo(page?: DeepPartial<GuiPageInfo>): GuiPageInfo {
  return {
    ...defaultPageInfo,
    ...page,
  };
}

function mergeCursorInfo(cursor?: DeepPartial<GuiCursorInfo>): GuiCursorInfo {
  return {
    ...defaultCursorInfo,
    ...cursor,
  };
}

function mergeTrack(track?: DeepPartial<GuiTrack> | null): GuiTrack | null {
  if (!track) {
    return null;
  }

  return {
    ...defaultTrack,
    ...track,
    artists: track.artists?.map((artist) => artist ?? "") ?? [],
  };
}

function mergeAlbum(album?: DeepPartial<GuiAlbum> | null): GuiAlbum | null {
  if (!album) {
    return null;
  }

  return {
    ...defaultAlbum,
    ...album,
    artists: album.artists?.map((artist) => artist ?? "") ?? [],
  };
}

function mergeArtist(artist?: DeepPartial<GuiArtist> | null): GuiArtist | null {
  if (!artist) {
    return null;
  }

  return {
    ...defaultArtist,
    ...artist,
  };
}

function mergeShow(show?: DeepPartial<GuiShow> | null): GuiShow | null {
  if (!show) {
    return null;
  }

  return {
    ...defaultShow,
    ...show,
  };
}

function mergeEpisode(episode?: DeepPartial<GuiEpisode>): GuiEpisode {
  return {
    id: null,
    uri: null,
    title: "",
    show: "",
    description: null,
    release_date: null,
    image_url: null,
    duration_ms: 0,
    ...episode,
  };
}

export function normalizeSnapshot(snapshot?: DeepPartial<GuiSnapshot>): GuiSnapshot {
  const next = snapshot ?? {};

  const routeId = routeIdOrDefault(next.status?.route_id, fallbackSnapshot.status.route_id);
  const contentRouteId = routeIdOrDefault(
    next.status?.content_route_id,
    overlayRoutes.has(routeId) ? fallbackSnapshot.status.content_route_id : routeId,
  );

  return {
    playback: {
      ...fallbackSnapshot.playback,
      ...next.playback,
      track: mergeTrack(next.playback?.track) ?? fallbackSnapshot.playback.track,
    },
    devices:
      next.devices?.map((device) => ({
        id: null,
        name: "",
        device_type: "",
        is_active: false,
        is_restricted: false,
        volume_percent: null,
        ...device,
      })) ?? fallbackSnapshot.devices,
    status: {
      ...fallbackSnapshot.status,
      ...next.status,
      route_id: routeId,
      content_route_id: contentRouteId,
    },
    user: next.user
      ? {
          id: next.user.id ?? "",
          display_name: next.user.display_name ?? null,
          country: next.user.country ?? null,
        }
      : fallbackSnapshot.user,
    library: {
      ...fallbackSnapshot.library,
      ...next.library,
      options: next.library?.options?.map((option) => option ?? "") ?? fallbackSnapshot.library.options,
      saved_tracks: mergePageInfo(next.library?.saved_tracks),
      saved_albums: mergePageInfo(next.library?.saved_albums),
      saved_artists: mergeCursorInfo(next.library?.saved_artists),
      saved_shows: mergePageInfo(next.library?.saved_shows),
    },
    playlists:
      next.playlists?.map((playlist) => ({
        id: playlist.id ?? "",
        uri: playlist.uri ?? "",
        name: playlist.name ?? "",
        owner: playlist.owner ?? "",
        description: playlist.description ?? null,
        image_url: playlist.image_url ?? null,
        track_count: playlist.track_count ?? 0,
        collaborative: playlist.collaborative ?? false,
        editable: playlist.editable ?? false,
        selected: playlist.selected ?? false,
      })) ?? fallbackSnapshot.playlists,
    playlist_folders:
      next.playlist_folders?.map((entry) => ({
        kind: entry.kind ?? "playlist",
        id: entry.id ?? null,
        name: entry.name ?? "",
        index: entry.index ?? 0,
        depth: entry.depth ?? 0,
        selected: entry.selected ?? false,
      })) ?? fallbackSnapshot.playlist_folders,
    track_table: {
      ...fallbackSnapshot.track_table,
      ...next.track_table,
      tracks: next.track_table?.tracks?.map((track) => mergeTrack(track) ?? defaultTrack) ?? fallbackSnapshot.track_table.tracks,
      page: mergePageInfo(next.track_table?.page),
    },
    queue: next.queue?.map((track) => mergeTrack(track) ?? defaultTrack) ?? fallbackSnapshot.queue,
    queue_view: {
      ...fallbackSnapshot.queue_view,
      ...next.queue_view,
      current: mergeTrack(next.queue_view?.current),
      items: next.queue_view?.items?.map((track) => mergeTrack(track) ?? defaultTrack) ?? fallbackSnapshot.queue_view.items,
    },
    recently_played:
      next.recently_played?.map((track) => mergeTrack(track) ?? defaultTrack) ?? fallbackSnapshot.recently_played,
    search: {
      ...fallbackSnapshot.search,
      ...next.search,
      tracks: next.search?.tracks?.map((track) => mergeTrack(track) ?? defaultTrack) ?? fallbackSnapshot.search.tracks,
      albums: next.search?.albums?.map((album) => mergeAlbum(album) ?? defaultAlbum) ?? fallbackSnapshot.search.albums,
      artists: next.search?.artists?.map((artist) => mergeArtist(artist) ?? defaultArtist) ?? fallbackSnapshot.search.artists,
      playlists:
        next.search?.playlists?.map((playlist) => ({
          id: playlist.id ?? "",
          uri: playlist.uri ?? "",
          name: playlist.name ?? "",
          owner: playlist.owner ?? "",
          description: playlist.description ?? null,
          image_url: playlist.image_url ?? null,
          track_count: playlist.track_count ?? 0,
          collaborative: playlist.collaborative ?? false,
          editable: playlist.editable ?? false,
          selected: playlist.selected ?? false,
        })) ?? fallbackSnapshot.search.playlists,
      shows: next.search?.shows?.map((show) => mergeShow(show) ?? defaultShow) ?? fallbackSnapshot.search.shows,
    },
    home: {
      ...fallbackSnapshot.home,
      ...next.home,
      banner: next.home?.banner?.map((line) => line ?? "") ?? fallbackSnapshot.home.banner,
      changelog_lines: next.home?.changelog_lines?.map((line) => line ?? "") ?? fallbackSnapshot.home.changelog_lines,
    },
    artist_detail: {
      ...fallbackSnapshot.artist_detail,
      ...next.artist_detail,
      artist: mergeArtist(next.artist_detail?.artist),
      top_tracks:
        next.artist_detail?.top_tracks?.map((track) => mergeTrack(track) ?? defaultTrack) ??
        fallbackSnapshot.artist_detail.top_tracks,
      albums:
        next.artist_detail?.albums?.map((album) => mergeAlbum(album) ?? defaultAlbum) ?? fallbackSnapshot.artist_detail.albums,
      related_artists:
        next.artist_detail?.related_artists?.map((artist) => mergeArtist(artist) ?? defaultArtist) ??
        fallbackSnapshot.artist_detail.related_artists,
    },
    album_tracks: {
      ...fallbackSnapshot.album_tracks,
      ...next.album_tracks,
      album: mergeAlbum(next.album_tracks?.album),
      tracks:
        next.album_tracks?.tracks?.map((track) => mergeTrack(track) ?? defaultTrack) ?? fallbackSnapshot.album_tracks.tracks,
      page: mergePageInfo(next.album_tracks?.page),
    },
    albums: {
      ...fallbackSnapshot.albums,
      ...next.albums,
      albums: next.albums?.albums?.map((album) => mergeAlbum(album) ?? defaultAlbum) ?? fallbackSnapshot.albums.albums,
    },
    artists: {
      ...fallbackSnapshot.artists,
      ...next.artists,
      artists:
        next.artists?.artists?.map((artist) => mergeArtist(artist) ?? defaultArtist) ?? fallbackSnapshot.artists.artists,
    },
    podcasts: {
      ...fallbackSnapshot.podcasts,
      ...next.podcasts,
      shows: next.podcasts?.shows?.map((show) => mergeShow(show) ?? defaultShow) ?? fallbackSnapshot.podcasts.shows,
    },
    podcast_episodes: {
      ...fallbackSnapshot.podcast_episodes,
      ...next.podcast_episodes,
      show: mergeShow(next.podcast_episodes?.show),
      episodes:
        next.podcast_episodes?.episodes?.map((episode) => mergeEpisode(episode)) ?? fallbackSnapshot.podcast_episodes.episodes,
      page: mergePageInfo(next.podcast_episodes?.page),
    },
    lyrics: {
      ...fallbackSnapshot.lyrics,
      ...next.lyrics,
      lines:
        next.lyrics?.lines?.map((line) => ({
          timestamp_ms: line.timestamp_ms ?? 0,
          text: line.text ?? "",
        })) ?? fallbackSnapshot.lyrics.lines,
    },
    discover: {
      ...fallbackSnapshot.discover,
      ...next.discover,
      top_tracks:
        next.discover?.top_tracks?.map((track) => mergeTrack(track) ?? defaultTrack) ?? fallbackSnapshot.discover.top_tracks,
      artists_mix:
        next.discover?.artists_mix?.map((track) => mergeTrack(track) ?? defaultTrack) ??
        fallbackSnapshot.discover.artists_mix,
    },
    help: {
      ...fallbackSnapshot.help,
      ...next.help,
      docs:
        next.help?.docs?.map((item) => ({
          description: item.description ?? "",
          binding: item.binding ?? "",
          context: item.context ?? "",
        })) ?? fallbackSnapshot.help.docs,
    },
    analysis: {
      ...fallbackSnapshot.analysis,
      ...next.analysis,
      peak: next.analysis?.peak ?? fallbackSnapshot.analysis.peak,
      bands: next.analysis?.bands?.map((band) => band ?? 0) ?? fallbackSnapshot.analysis.bands,
    },
    cover_art: {
      ...fallbackSnapshot.cover_art,
      ...next.cover_art,
      track: mergeTrack(next.cover_art?.track),
    },
    settings: {
      ...fallbackSnapshot.settings,
      ...next.settings,
      categories:
        next.settings?.categories?.map((category) => category ?? "") ?? fallbackSnapshot.settings.categories,
      items:
        next.settings?.items?.map((item) => ({
          id: item.id ?? "",
          name: item.name ?? "",
          description: item.description ?? "",
          value: item.value ?? "",
          value_type: item.value_type ?? "",
        })) ?? fallbackSnapshot.settings.items,
    },
    dialog: {
      ...fallbackSnapshot.dialog,
      ...next.dialog,
      playlist_options:
        next.dialog?.playlist_options?.map((option) => ({
          id: option.id ?? "",
          label: option.label ?? "",
          description: option.description ?? null,
        })) ?? fallbackSnapshot.dialog.playlist_options,
    },
    sort: {
      ...fallbackSnapshot.sort,
      ...next.sort,
      options:
        next.sort?.options?.map((option) => ({
          field: option.field ?? "",
          label: option.label ?? "",
          shortcut: option.shortcut ?? null,
          selected: option.selected ?? false,
          active: option.active ?? false,
        })) ?? fallbackSnapshot.sort.options,
    },
    party: {
      ...fallbackSnapshot.party,
      ...next.party,
      guests: next.party?.guests?.map((guest) => guest ?? "") ?? fallbackSnapshot.party.guests,
    },
    announcement: {
      active: next.announcement?.active
        ? {
            id: next.announcement.active.id ?? "",
            title: next.announcement.active.title ?? "",
            body: next.announcement.active.body ?? "",
            level: next.announcement.active.level ?? "",
            url: next.announcement.active.url ?? null,
          }
        : fallbackSnapshot.announcement.active,
      pending:
        next.announcement?.pending?.map((item) => ({
          id: item.id ?? "",
          title: item.title ?? "",
          body: item.body ?? "",
          level: item.level ?? "",
          url: item.url ?? null,
        })) ?? fallbackSnapshot.announcement.pending,
    },
    create_playlist: {
      ...fallbackSnapshot.create_playlist,
      ...next.create_playlist,
      tracks:
        next.create_playlist?.tracks?.map((track) => mergeTrack(track) ?? defaultTrack) ??
        fallbackSnapshot.create_playlist.tracks,
      search_results:
        next.create_playlist?.search_results?.map((track) => mergeTrack(track) ?? defaultTrack) ??
        fallbackSnapshot.create_playlist.search_results,
    },
  };
}

export function getVisibleRouteId(snapshot: GuiSnapshot): RouteId {
  return routeIdOrDefault(snapshot.status.route_id, "home");
}

export function getContentRouteId(snapshot: GuiSnapshot): RouteId {
  return routeIdOrDefault(snapshot.status.content_route_id, getVisibleRouteId(snapshot));
}

export function getOverlayRouteId(snapshot: GuiSnapshot): RouteId | null {
  const route = getVisibleRouteId(snapshot);
  return overlayRoutes.has(route) ? route : null;
}

export function isOverlayRoute(route: RouteId): boolean {
  return overlayRoutes.has(route);
}
