import { useCallback, useState, type KeyboardEvent } from "react";
import type { Action } from "./bindings/Action";
import type { LikedSongs } from "./bindings/LikedSongs";
import type { SourcePayload } from "./bindings/SourcePayload";
import type { SourcePlaylists } from "./bindings/SourcePlaylists";
import type { TrackInfo } from "./bindings/TrackInfo";
import type { TrackTablePayload } from "./bindings/TrackTablePayload";
import { Glyph } from "./Icon";
import {
  clampCursor,
  listPlayRequest,
  playlistImage,
  playRequest,
  songCount,
  step,
} from "./libraryModel";
import "./ListPage.css";
import type { OpenView, View } from "./nav";
import { SourceBadge } from "./SourceBadge";
import { TrackTable } from "./TrackTable";
import { PageHeader, PlayButton } from "./ui";

const NO_TRACKS: TrackInfo[] = [];

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
  const [shownFor, setShownFor] = useState<typeof shown>(shown);
  if (shownFor !== shown) {
    setShownFor(shown);
    setWant(0);
    setOpenedAt(statusRev);
    setFailedUri(null);
  }
  const landed = playlist !== null && table?.uri === playlist.uri;
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
  const cursor = clampCursor(want, tracks.length);

  const play = useCallback(
    (index: number) => {
      const action = playlist
        ? listPlayRequest(playlist.source, playlist.uri, tracks, index)
        : playRequest(tracks, index);
      if (action) send(action);
    },
    [playlist, tracks, send],
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
        send({ LoadMore: playlist ? "PlaylistTracks" : "SavedTracks" });
    },
    [cursor, tracks, hasMore, playlist, play, send],
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
  const first = tracks.findIndex((track) => track.uri);

  return (
    <section className="list-page" aria-label={title}>
      <PageHeader
        kind="Playlist"
        title={title}
        image={playlist && playlistImage(playlists, playlist.uri)}
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
      </div>
      {tracks.length === 0 ? (
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
          tracks={tracks}
          cursor={cursor}
          playingUri={playingUri}
          onKeyDown={onKeyDown}
          onPick={setWant}
          onPlay={play}
          open={open}
        />
      )}
    </section>
  );
}
