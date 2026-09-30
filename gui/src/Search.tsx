import {
  memo,
  useCallback,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type RefObject,
} from "react";
import type { Action } from "./bindings/Action";
import type { SearchPayload } from "./bindings/SearchPayload";
import type { SourcePayload } from "./bindings/SourcePayload";
import type { TrackInfo } from "./bindings/TrackInfo";
import { clock } from "./format";
import { KeyHints } from "./KeyHints";
import { clampCursor, playRequest, step } from "./libraryModel";
import "./Search.css";
import {
  otherSources,
  scopeLabel,
  TABS,
  tabHasResults,
  year,
  type Tab,
} from "./searchModel";
import { SourceBadge } from "./SourceBadge";

/** The Search screen: the query, the result tabs, the top artist and albums, and the tracks. */
export const Search = memo(function Search({
  search,
  waiting,
  source,
  playingUri,
  query,
  onQuery,
  onRun,
  send,
}: {
  search: SearchPayload | null;
  /// A search this page ran has no answer yet.
  waiting: boolean;
  source: SourcePayload | null;
  playingUri: string | null;
  query: string;
  onQuery: (query: string) => void;
  /// Runs a search; the shell owns the waiting state, so the palette's searches show it too.
  onRun: (query: string) => void;
  send: (action: Action) => void;
}) {
  const [chosenTab, setTab] = useState<Tab>("Everything");
  // A tab the new results leave empty falls back to Everything instead of hiding them.
  const tab: Tab =
    search && tabHasResults(search, chosenTab) ? chosenTab : "Everything";
  // The cursor belongs to one result set; new results start it at the top.
  const [wantFor, setWantFor] = useState<{ query: string | null; row: number }>(
    {
      query: null,
      row: 0,
    },
  );
  const resultQuery = search?.query ?? null;
  // Store the reset, so results for an earlier query start at the top when they come back.
  if (wantFor.query !== resultQuery) setWantFor({ query: resultQuery, row: 0 });
  const want = wantFor.query === resultQuery ? wantFor.row : 0;
  const setWant = (row: number) => setWantFor({ query: resultQuery, row });

  const list = useRef<HTMLDivElement>(null);
  const run = onRun;

  const active = source?.active ?? null;
  const compiled = source?.compiled ?? [];
  const tracks = search?.tracks ?? [];
  const artists = search?.artists ?? [];
  const albums = search?.albums ?? [];
  const playlists = search?.playlists ?? [];
  const top = artists[0] ?? null;
  const wide = tab === "Everything" && (top !== null || albums.length > 0);
  const others = active && query.trim() ? otherSources(compiled, active) : [];

  return (
    <div className="search-screen">
      <div className="search-head">
        <label className="query">
          <span aria-hidden="true">/</span>
          <input
            type="text"
            aria-label="Search"
            placeholder="Search your music"
            value={query}
            data-focus
            onChange={(event) => onQuery(event.target.value)}
            onKeyDown={(event) => {
              if (event.nativeEvent.isComposing) return;
              if (event.key === "Enter") {
                event.preventDefault();
                run(query);
              } else if (event.key === "ArrowDown" && tracks.length > 0) {
                event.preventDefault();
                list.current?.focus();
              }
            }}
          />
        </label>
        <div className="tabs" role="group" aria-label="Result type">
          {TABS.map((name) => (
            <button
              key={name}
              type="button"
              aria-pressed={tab === name}
              disabled={!search || !tabHasResults(search, name)}
              onClick={() => setTab(name)}
            >
              {name}
            </button>
          ))}
        </div>
        {active && compiled.length > 1 && (
          <select
            className="scope"
            aria-label="Search in"
            value={active}
            onChange={(event) => {
              const picked = compiled.find((s) => s === event.target.value);
              if (!picked) return;
              send({ SelectSource: picked });
              if (query.trim()) run(query);
            }}
          >
            {compiled.map((choice) => (
              <option key={choice} value={choice}>
                {scopeLabel(choice)}
              </option>
            ))}
          </select>
        )}
      </div>
      {waiting && <p className="searching">Searching…</p>}
      {!search?.ran ? (
        !waiting && <p className="empty">Type a search and press Enter.</p>
      ) : (
        <div className={wide ? "search-body wide" : "search-body"}>
          {wide && (
            <section className="top" aria-label="Top result">
              {top && (
                <>
                  <span className="eyebrow">TOP RESULT · ARTIST</span>
                  <div className="artist">
                    {top.image_url ? (
                      <img className="avatar" src={top.image_url} alt="" />
                    ) : (
                      <span className="avatar" />
                    )}
                    <span className="big-name">{top.name}</span>
                  </div>
                  <div className="actions">
                    <button
                      type="button"
                      className="primary"
                      disabled={!top.uri}
                      onClick={() =>
                        top.uri &&
                        send({ PlayContext: { uri: top.uri, offset: null } })
                      }
                    >
                      Play {top.name}
                    </button>
                    <button type="button" disabled>
                      Open artist
                    </button>
                  </div>
                </>
              )}
              {albums.length > 0 && (
                <>
                  <span className="eyebrow rule">ALBUMS</span>
                  {albums.map((album, index) => (
                    <Hit
                      key={`${index}-${album.uri ?? album.name}`}
                      cover={album.image_url}
                      title={album.name}
                      sub={[
                        album.artists.map((artist) => artist.name).join(", "),
                        year(album.release_date),
                      ]
                        .filter(Boolean)
                        .join(" · ")}
                      uri={album.uri}
                      send={send}
                    />
                  ))}
                </>
              )}
            </section>
          )}
          <section className="results" aria-label="Results">
            {(tab === "Everything" || tab === "Tracks") &&
              (tracks.length === 0 ? (
                <p className="empty">No songs for that search.</p>
              ) : (
                <ResultTracks
                  listRef={list}
                  tracks={tracks}
                  liked={search.liked}
                  cursor={clampCursor(want, tracks.length)}
                  playingUri={playingUri}
                  onCursor={setWant}
                  send={send}
                />
              ))}
            {tab === "Albums" &&
              albums.map((album, index) => (
                <Hit
                  key={`${index}-${album.uri ?? album.name}`}
                  cover={album.image_url}
                  title={album.name}
                  sub={[
                    album.artists.map((artist) => artist.name).join(", "),
                    year(album.release_date),
                  ]
                    .filter(Boolean)
                    .join(" · ")}
                  uri={album.uri}
                  send={send}
                />
              ))}
            {tab === "Artists" &&
              artists.map((artist, index) => (
                <Hit
                  key={`${index}-${artist.uri ?? artist.name}`}
                  cover={artist.image_url}
                  round
                  title={artist.name}
                  sub=""
                  uri={artist.uri}
                  send={send}
                />
              ))}
            {tab === "Playlists" &&
              playlists.map((playlist, index) => (
                <Hit
                  key={`${index}-${playlist.uri}`}
                  cover={playlist.image_url}
                  title={playlist.name}
                  sub={[playlist.owner, `${playlist.track_count} songs`]
                    .filter(Boolean)
                    .join(" · ")}
                  uri={playlist.uri}
                  send={send}
                />
              ))}
            {others.length > 0 && (
              <div className="others">
                <span className="eyebrow rule">SEARCH IN ANOTHER SOURCE</span>
                {others.map((other) => (
                  <button
                    key={other}
                    type="button"
                    onClick={() => {
                      send({ SelectSource: other });
                      run(query);
                    }}
                  >
                    <SourceBadge source={other} />
                    <span>
                      Search “{query.trim()}” in {other}
                    </span>
                  </button>
                ))}
              </div>
            )}
            <KeyHints
              hints={["↓ results", "enter play", "q queue", "/ search"]}
            />
          </section>
        </div>
      )}
    </div>
  );
});

/** An album, artist or playlist hit; a click plays it, since the page has no album or artist screen yet. */
function Hit({
  cover,
  round = false,
  title,
  sub,
  uri,
  send,
}: {
  cover: string | null;
  round?: boolean;
  title: string;
  sub: string;
  uri: string | null;
  send: (action: Action) => void;
}) {
  return (
    <button
      type="button"
      className="hit"
      disabled={!uri}
      onClick={() => uri && send({ PlayContext: { uri, offset: null } })}
    >
      {cover ? (
        <img className={round ? "cover round" : "cover"} src={cover} alt="" />
      ) : (
        <span className={round ? "cover round" : "cover"} />
      )}
      <span className="hit-text">
        <b>{title}</b>
        {sub && <span>{sub}</span>}
      </span>
    </button>
  );
}

const ResultTracks = memo(function ResultTracks({
  listRef,
  tracks,
  liked,
  cursor,
  playingUri,
  onCursor,
  send,
}: {
  listRef: RefObject<HTMLDivElement | null>;
  tracks: TrackInfo[];
  liked: string[];
  cursor: number;
  playingUri: string | null;
  onCursor: (index: number) => void;
  send: (action: Action) => void;
}) {
  useLayoutEffect(() => {
    listRef.current
      ?.querySelector('[aria-selected="true"]')
      ?.scrollIntoView({ block: "nearest" });
  }, [cursor, listRef]);

  const play = useCallback(
    (index: number) => {
      const action = playRequest(tracks, index);
      if (action) send(action);
    },
    [tracks, send],
  );

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.ctrlKey || event.metaKey || event.altKey) return;
    if (event.key === "Enter") {
      event.preventDefault();
      play(cursor);
      return;
    }
    if (event.key === "q") {
      event.preventDefault();
      const track = tracks[cursor];
      // The app refuses a station itself and says why.
      if (track) send({ QueueTrack: track });
      return;
    }
    const move = step(event.key, cursor, tracks.length, false);
    if (!move) return;
    event.preventDefault();
    onCursor(clampCursor(move.next, tracks.length));
  };

  const likedIds = new Set(liked);
  return (
    <>
      <div className="track-row head eyebrow" aria-hidden="true">
        <span>TRACK</span>
        <span className="album">ALBUM</span>
        <span>YOURS</span>
        <span className="time">TIME</span>
      </div>
      <div
        ref={listRef}
        className="result-tracks"
        role="listbox"
        aria-label="Songs"
        aria-activedescendant={cursor >= 0 ? `search-${cursor}` : undefined}
        tabIndex={0}
        onKeyDown={onKeyDown}
      >
        {tracks.map((track, index) => {
          const now = playingUri !== null && track.uri === playingUri;
          const mine =
            track.id !== null &&
            (track.uri ?? "").startsWith("spotify:track:") &&
            likedIds.has(track.id);
          return (
            <div
              key={`${index}-${track.uri ?? track.name}`}
              id={`search-${index}`}
              role="option"
              aria-selected={index === cursor}
              className={now ? "track-row now" : "track-row"}
              onClick={() => onCursor(index)}
              onDoubleClick={() => play(index)}
            >
              <span className="title">
                <b>
                  {now && "▶ "}
                  {track.name}
                </b>
                <span>{track.artists.join(", ")}</span>
              </span>
              <span className="album">{track.album}</span>
              <span className="yours">{mine && "♥ liked"}</span>
              <span className="time">
                {track.duration_ms > 0 ? clock(track.duration_ms) : "live"}
              </span>
            </div>
          );
        })}
      </div>
    </>
  );
});
