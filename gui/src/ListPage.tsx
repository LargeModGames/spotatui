import {
  useCallback,
  useDeferredValue,
  useMemo,
  useState,
  type KeyboardEvent,
} from "react";
import type { Action } from "./bindings/Action";
import type { LikedSongs } from "./bindings/LikedSongs";
import type { SourcePayload } from "./bindings/SourcePayload";
import type { SourcePlaylists } from "./bindings/SourcePlaylists";
import type { TrackInfo } from "./bindings/TrackInfo";
import type { TrackTablePayload } from "./bindings/TrackTablePayload";
import { Glyph } from "./Icon";
import { clampCursor, playlistImage, songCount, step } from "./libraryModel";
import "./ListPage.css";
import type { OpenView, View } from "./nav";
import {
  filterTracks,
  isDefaultSort,
  parseSort,
  removeAction,
  shownPlayRequest,
  SORT_OPTIONS,
  sortTracks,
  sortValue,
} from "./sortModel";
import { SourceBadge } from "./SourceBadge";
import { TrackTable } from "./TrackTable";
import type { RowContext } from "./trackMenuModel";
import { PageHeader, PlayButton } from "./ui";

const NO_TRACKS: TrackInfo[] = [];
const NO_URIS: string[] = [];

/** Liked Songs or an opened playlist: the header, the play row and the track table. */
export function ListPage({
  view,
  liked,
  table,
  playlists,
  playingUri,
  statusRev,
  statusError,
  open,
  send,
  defaultSort,
}: {
  view: Extract<View, { page: "liked" | "playlist" }>;
  liked: LikedSongs | null;
  /** The library lists, for the opened playlist's cover. */
  playlists: SourcePlaylists | null;
  table: TrackTablePayload | null;
  source: SourcePayload | null;
  playingUri: string | null;
  statusRev: number | null;
  statusError: boolean;
  active: boolean;
  open: OpenView;
  send: (action: Action) => void;
  /** The sort a list opens with, `field[:desc]`. */
  defaultSort?: string | null;
}) {
  const playlist = view.page === "playlist" ? view : null;
  // The shell hands Liked Songs a fresh object each render, a playlist the view it opened.
  const shown = playlist ?? "liked";
  // The wanted row; a `j` past the end lands on the first row the next page brings.
  const [want, setWant] = useState(0);
  // The status revision when the list was opened: a later error means the open failed.
  const [openedAt, setOpenedAt] = useState(statusRev);
  // The playlist whose open failed; it stays failed after the error expires, until it is opened again.
  const [failedUri, setFailedUri] = useState<string | null>(null);
  const initialSort = sortValue(parseSort(defaultSort));
  const [sortBy, setSortBy] = useState(initialSort);
  const [filter, setFilter] = useState("");
  const [shownFor, setShownFor] = useState<typeof shown>(shown);
  const [sortDefault, setSortDefault] = useState(initialSort);
  if (shownFor !== shown) {
    setShownFor(shown);
    setWant(0);
    setOpenedAt(statusRev);
    setFailedUri(null);
    setSortBy(initialSort);
    setFilter("");
  }
  if (sortDefault !== initialSort) {
    setSortDefault(initialSort);
    setSortBy(initialSort);
    setWant(0);
  }
  const landed = playlist !== null && table?.uri === playlist.uri;
  // While the list shows, an error is old news; one after a reload starts means the reload failed.
  if (landed && (openedAt !== statusRev || failedUri !== null)) {
    setOpenedAt(statusRev);
    setFailedUri(null);
  }
  if (
    shownFor === shown &&
    playlist !== null &&
    !landed &&
    statusError &&
    statusRev !== openedAt &&
    failedUri !== playlist.uri
  )
    setFailedUri(playlist.uri);
  const failed = playlist !== null && !landed && failedUri === playlist.uri;
  const tracks = playlist
    ? landed
      ? table.tracks
      : NO_TRACKS
    : (liked?.tracks ?? NO_TRACKS);
  const hasMore = playlist
    ? landed && table.has_more
    : (liked?.has_more ?? false);
  const sorted = useMemo(
    () => sortTracks(tracks, parseSort(sortBy)),
    [tracks, sortBy],
  );
  const shownFilter = useDeferredValue(filter);
  const rows = useMemo(
    () => filterTracks(sorted, shownFilter),
    [sorted, shownFilter],
  );
  const filtering = shownFilter.trim() !== "";
  const reordered = filtering || !isDefaultSort(parseSort(sortBy));
  const shownTracks = useMemo(
    () => (reordered ? rows.map((row) => row.track) : tracks),
    [reordered, rows, tracks],
  );
  const cursor = clampCursor(want, shownTracks.length);

  const play = useCallback(
    (index: number) => {
      const action = shownPlayRequest(playlist, reordered, shownTracks, index);
      if (action) send(action);
    },
    [playlist, reordered, shownTracks, send],
  );

  const editable = playlists?.editable ?? NO_URIS;
  const positions = landed ? table.positions : null;
  const rowContext = useCallback(
    (track: TrackInfo, index: number): RowContext => {
      const original = rows[index]?.index ?? index;
      const remove = removeAction(
        playlist,
        editable,
        positions,
        track,
        original,
      );
      return remove ? { remove } : {};
    },
    [playlist, editable, positions, rows],
  );

  const onKeyDown = useCallback(
    (event: KeyboardEvent<HTMLDivElement>) => {
      if (event.ctrlKey || event.metaKey || event.altKey) return;
      if (event.key === "Enter") {
        play(cursor);
        event.preventDefault();
        return;
      }
      if (event.key === "q") {
        const track = shownTracks[cursor];
        // A held key repeats; one press queues the row once.
        if (track && !event.repeat) send({ QueueTrack: track });
        event.preventDefault();
        return;
      }
      const move = step(event.key, cursor, shownTracks.length, hasMore);
      if (!move) return;
      event.preventDefault();
      setWant(move.next);
      if (move.loadMore)
        send({ LoadMore: playlist ? "PlaylistTracks" : "SavedTracks" });
    },
    [cursor, shownTracks, hasMore, playlist, play, send],
  );

  const title = playlist ? playlist.name : "Liked Songs";
  const available = liked?.available ?? false;
  const loaded = liked?.loaded ?? false;
  const total = Math.max(liked?.total ?? 0, tracks.length);
  const meta = playlist ? (
    <>
      {playlist.source !== "Spotify" && (
        <SourceBadge source={playlist.source} />
      )}
      {landed && (
        <span>
          {hasMore ? `${tracks.length}+ songs` : songCount(tracks.length)}
        </span>
      )}
    </>
  ) : (
    loaded && (
      <span>
        {songCount(total)}
        {tracks.length < total && `, ${tracks.length} loaded`}
      </span>
    )
  );
  const first = shownTracks.findIndex((track) => track.uri);

  return (
    <section className="list-page" aria-label={title}>
      <PageHeader
        kind="Playlist"
        title={title}
        image={
          playlist &&
          (playlistImage(playlists, playlist.uri) ?? playlist.image ?? null)
        }
        tone={playlist ? "playlist" : "liked"}
        meta={meta}
      />
      <div className="list-actions">
        <PlayButton
          label={`Play ${title}`}
          large
          disabled={first < 0}
          onClick={() => play(first)}
        />
        <button
          type="button"
          className="list-shuffle"
          aria-label="Shuffle"
          disabled
        >
          <Glyph name="shuffle" size={28} />
        </button>
        <div className="list-tools">
          <span className="list-count" aria-live="polite">
            {filtering &&
              tracks.length > 0 &&
              `${rows.length} of ${tracks.length}`}
          </span>
          <input
            type="search"
            className="list-filter"
            aria-label="Filter"
            placeholder="Filter"
            autoComplete="off"
            value={filter}
            onChange={(event) => {
              setFilter(event.target.value);
              setWant(0);
            }}
          />
          <select
            className="list-sort"
            aria-label="Sort by"
            value={sortBy}
            onChange={(event) => {
              setSortBy(event.target.value);
              setWant(0);
            }}
          >
            {SORT_OPTIONS.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </div>
      </div>
      {tracks.length > 0 && shownTracks.length === 0 ? (
        <p className="list-empty">No songs match “{shownFilter.trim()}”.</p>
      ) : tracks.length === 0 ? (
        <p className="list-empty">
          {playlist
            ? landed
              ? "This playlist is empty."
              : failed
                ? `Could not load ${playlist.name}. Choose it again to retry.`
                : `Loading ${playlist.name}…`
            : !available
              ? "Liked Songs needs a Spotify session."
              : loaded
                ? "No liked songs yet."
                : "Loading Liked Songs…"}
        </p>
      ) : (
        <TrackTable
          label={title}
          idPrefix={playlist ? "playlist" : "liked"}
          tracks={shownTracks}
          cursor={cursor}
          playingUri={playingUri}
          onKeyDown={onKeyDown}
          onPick={setWant}
          onPlay={play}
          open={open}
          rowContext={rowContext}
        />
      )}
    </section>
  );
}
