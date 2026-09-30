import {
  memo,
  useCallback,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";
import type { Action } from "./bindings/Action";
import type { LikedSongs } from "./bindings/LikedSongs";
import type { PlaylistSyncPayload } from "./bindings/PlaylistSyncPayload";
import type { SourcePayload } from "./bindings/SourcePayload";
import type { SourcePlaylists } from "./bindings/SourcePlaylists";
import type { TrackInfo } from "./bindings/TrackInfo";
import type { TrackTablePayload } from "./bindings/TrackTablePayload";
import { KeyHints } from "./KeyHints";
import { unmatchedTotal } from "./healthModel";
import {
  clampCursor,
  listPlayRequest,
  openRow,
  playlistsFor,
  playRequest,
  step,
  type SidebarRow,
} from "./libraryModel";
import "./Library.css";
import { LibraryHealth } from "./LibraryHealth";
import { LibrarySidebar } from "./LibrarySidebar";
import { TrackTable } from "./TrackTable";
import { UpNext } from "./UpNext";

const NO_TRACKS: TrackInfo[] = [];

/** The Library screen: the sections, Liked Songs or an opened playlist with the source chips, and the queue aside. */
export const Library = memo(function Library({
  playlists,
  liked,
  table,
  source,
  playingUri,
  upNext,
  sync,
  statusRev,
  statusError,
  send,
}: {
  playlists: SourcePlaylists | null;
  liked: LikedSongs | null;
  table: TrackTablePayload | null;
  source: SourcePayload | null;
  playingUri: string | null;
  upNext: TrackInfo[];
  sync: PlaylistSyncPayload | null;
  statusRev: number | null;
  statusError: boolean;
  send: (action: Action) => void;
}) {
  // The health sub-page replaces the grid; the Liked Songs cursor stays in this state.
  const [health, setHealth] = useState(false);
  // Closing the sub-page hands the keyboard back to Liked Songs.
  const leaveHealth = useCallback(() => {
    setHealth(false);
    requestAnimationFrame(() =>
      document
        .querySelector<HTMLElement>('[data-area="library"] [data-focus]')
        ?.focus(),
    );
  }, []);
  const active = source?.active ?? null;
  const rows = playlists && active ? playlistsFor(playlists, active) : [];
  // The wanted row; a `j` past the end lands on the first row the next page brings.
  const [want, setWant] = useState(0);
  // The opened sidebar row; none shows Liked Songs, and a source switch goes back to it.
  const [openUri, setOpenUri] = useState<string | null>(null);
  // The status revision when the row was opened: a later error means the open failed.
  const [openedAt, setOpenedAt] = useState<number | null>(null);
  // The row whose open failed; it stays failed after the error expires, until it is opened again.
  const [failedUri, setFailedUri] = useState<string | null>(null);
  const [openFor, setOpenFor] = useState(active);
  if (openFor !== active) {
    setOpenFor(active);
    setOpenUri(null);
    setWant(0);
  }
  const opened = rows.find((row) => row.uri === openUri) ?? null;
  const landed = opened !== null && table?.uri === opened.uri;
  if (
    opened !== null &&
    !landed &&
    statusError &&
    statusRev !== openedAt &&
    failedUri !== opened.uri
  )
    setFailedUri(opened.uri);
  const failed = opened !== null && !landed && failedUri === opened.uri;
  const tracks = opened
    ? landed
      ? table.tracks
      : NO_TRACKS
    : (liked?.tracks ?? NO_TRACKS);
  const hasMore = opened
    ? landed && table.has_more
    : (liked?.has_more ?? false);
  const cursor = clampCursor(want, tracks.length);

  // Liked Songs load on demand, once per page load, as the terminal does on Enter.
  const requested = useRef(false);
  const openLiked = useCallback(() => {
    requested.current = true;
    send({ OpenLibrary: "LikedSongs" });
  }, [send]);
  const available = liked?.available ?? false;
  const loaded = liked?.loaded ?? false;
  useEffect(() => {
    if (available && !loaded && !requested.current) openLiked();
  }, [available, loaded, openLiked]);

  const showLiked = useCallback(() => {
    setOpenUri(null);
    setWant(0);
    openLiked();
  }, [openLiked]);
  const onOpenRow = useCallback(
    (row: SidebarRow) => {
      if (!active) return;
      const action = openRow(active, row);
      if (!action) return;
      send(action);
      if (active === "Radio") return;
      setOpenUri(row.uri);
      setOpenedAt(statusRev);
      setFailedUri(null);
      setWant(0);
    },
    [active, statusRev, send],
  );

  const play = useCallback(
    (index: number) => {
      const action =
        opened && active
          ? listPlayRequest(active, opened.uri, tracks, index)
          : playRequest(tracks, index);
      if (action) send(action);
    },
    [opened, active, tracks, send],
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
        const track = tracks[cursor];
        // A held key repeats; one press queues the row once.
        if (track && !event.repeat) send({ QueueTrack: track });
        event.preventDefault();
        return;
      }
      const move = step(event.key, cursor, tracks.length, hasMore);
      if (!move) return;
      event.preventDefault();
      setWant(move.next);
      if (move.loadMore)
        send({ LoadMore: opened ? "PlaylistTracks" : "SavedTracks" });
    },
    [cursor, tracks, hasMore, opened, play, send],
  );

  if (health)
    return <LibraryHealth sync={sync} send={send} onBack={leaveHealth} />;
  return (
    <div className="library">
      <LibrarySidebar
        source={active}
        playlists={rows}
        openUri={opened?.uri ?? null}
        unmatched={unmatchedTotal(sync?.links ?? [])}
        onOpenLiked={showLiked}
        onOpenRow={onOpenRow}
        onOpenHealth={() => setHealth(true)}
      />
      <section className="liked">
        <div className="liked-head">
          <div>
            <span className="eyebrow">
              {opened && active
                ? `LIBRARY / ${active.toUpperCase()}`
                : "LIBRARY / LIKED SONGS"}
            </span>
            <h1>{opened ? opened.name : "Liked Songs"}</h1>
            <div className="chips" role="group" aria-label="Browse source">
              {(source?.compiled ?? []).map((chip) => (
                <button
                  key={chip}
                  type="button"
                  aria-pressed={chip === active}
                  onClick={() => send({ SelectSource: chip })}
                >
                  {chip}
                </button>
              ))}
            </div>
          </div>
          <button type="button" className="play-all" disabled>
            Play
          </button>
          <button type="button" className="shuffle-all" disabled>
            Shuffle
          </button>
        </div>
        {tracks.length === 0 ? (
          <p className="empty">
            {opened
              ? landed
                ? "This playlist is empty."
                : failed
                  ? `Could not load ${opened.name}. Choose it again to retry.`
                  : `Loading ${opened.name}…`
              : !available
                ? "Liked Songs needs a Spotify session."
                : loaded
                  ? "No liked songs yet."
                  : "Loading Liked Songs…"}
          </p>
        ) : (
          <TrackTable
            label={opened ? opened.name : "Liked Songs"}
            tracks={tracks}
            cursor={cursor}
            playingUri={playingUri}
            onKeyDown={onKeyDown}
            onPick={setWant}
            onPlay={play}
          />
        )}
        <KeyHints hints={["enter play", "q add to queue", "j k move"]}>
          {!opened && liked && liked.total > tracks.length && (
            <span>
              {tracks.length} of {liked.total}
            </span>
          )}
        </KeyHints>
      </section>
      <UpNext tracks={upNext} />
    </div>
  );
});
