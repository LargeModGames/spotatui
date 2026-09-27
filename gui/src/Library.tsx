import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";
import type { Action } from "./bindings/Action";
import type { LikedSongs } from "./bindings/LikedSongs";
import type { SourcePayload } from "./bindings/SourcePayload";
import type { SourcePlaylists } from "./bindings/SourcePlaylists";
import type { TrackInfo } from "./bindings/TrackInfo";
import { clampCursor, playlistsFor, playRequest, step } from "./libraryModel";
import { LibrarySidebar } from "./LibrarySidebar";
import { TrackTable } from "./TrackTable";
import { UpNext } from "./UpNext";

const NO_TRACKS: TrackInfo[] = [];

/** The Library screen: the sections, Liked Songs with the source chips, and the queue aside. */
export function Library({
  playlists,
  liked,
  source,
  playingUri,
  upNext,
  send,
}: {
  playlists: SourcePlaylists | null;
  liked: LikedSongs | null;
  source: SourcePayload | null;
  playingUri: string | null;
  upNext: TrackInfo[];
  send: (action: Action) => void;
}) {
  const tracks = liked?.tracks ?? NO_TRACKS;
  const hasMore = liked?.has_more ?? false;
  // The wanted row; a `j` past the end lands on the first row the next page brings.
  const [want, setWant] = useState(0);
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

  const play = useCallback(
    (index: number) => {
      const action = playRequest(tracks, index);
      if (action) send(action);
    },
    [tracks, send],
  );

  const onKeyDown = useCallback(
    (event: KeyboardEvent<HTMLDivElement>) => {
      if (event.ctrlKey || event.metaKey || event.altKey) return;
      if (event.key === "Enter") {
        play(cursor);
        event.preventDefault();
        return;
      }
      const move = step(event.key, cursor, tracks.length, hasMore);
      if (!move) return;
      event.preventDefault();
      setWant(move.next);
      if (move.loadMore) send({ LoadMore: "SavedTracks" });
    },
    [cursor, tracks.length, hasMore, play, send],
  );

  const active = source?.active ?? null;
  const rows = playlists && active ? playlistsFor(playlists, active) : [];
  return (
    <div className="library">
      <LibrarySidebar
        source={active}
        playlists={rows}
        onOpenLiked={openLiked}
      />
      <section className="liked">
        <div className="liked-head">
          <div>
            <span className="eyebrow">LIBRARY / LIKED SONGS</span>
            <h1>Liked Songs</h1>
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
            {!available
              ? "Liked Songs needs a Spotify session."
              : loaded
                ? "No liked songs yet."
                : "Loading Liked Songs…"}
          </p>
        ) : (
          <TrackTable
            tracks={tracks}
            cursor={cursor}
            playingUri={playingUri}
            onKeyDown={onKeyDown}
            onPick={setWant}
            onPlay={play}
          />
        )}
        <footer className="keys">
          <kbd>enter play</kbd>
          <kbd>j k move</kbd>
          {liked && liked.total > tracks.length && (
            <span>
              {tracks.length} of {liked.total}
            </span>
          )}
        </footer>
      </section>
      <UpNext tracks={upNext} />
    </div>
  );
}
