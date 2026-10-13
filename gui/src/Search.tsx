import {
  memo,
  useCallback,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";
import type { Action } from "./bindings/Action";
import type { AlbumInfo } from "./bindings/AlbumInfo";
import type { ArtistInfo } from "./bindings/ArtistInfo";
import type { PlaylistInfo } from "./bindings/PlaylistInfo";
import type { SearchPayload } from "./bindings/SearchPayload";
import type { ShowInfo } from "./bindings/ShowInfo";
import type { SourcePayload } from "./bindings/SourcePayload";
import type { TrackInfo } from "./bindings/TrackInfo";
import { clock, sourceOf } from "./format";
import { Glyph } from "./Icon";
import { KeyHints } from "./KeyHints";
import { clampCursor, playRequest, step } from "./libraryModel";
import type { OpenView } from "./nav";
import "./Search.css";
import {
  otherSources,
  scopeLabel,
  showView,
  TABS,
  tabHasResults,
  year,
  type Tab,
} from "./searchModel";
import { SourceBadge } from "./SourceBadge";
import { Card, Cover, PlayButton, Shelf } from "./ui";

/** The songs the All view lists next to the top result. */
const TOP_SONGS = 4;
/** The cards of each shelf in the All view. */
const SHELF_CARDS = 6;

type Send = (action: Action) => void;

/** The Search results: the filter chips, the top result and songs, then shelves per kind. */
export const Search = memo(function Search({
  search,
  waiting,
  source,
  playingUri,
  query,
  run,
  open,
  send,
}: {
  search: SearchPayload | null;
  /** A search the shell ran has no answer yet. */
  waiting: boolean;
  source: SourcePayload | null;
  playingUri: string | null;
  query: string;
  /** Runs a search through the shell, which shows the wait. */
  run: (query: string) => void;
  open: OpenView;
  send: Send;
}) {
  const [chosenTab, setTab] = useState<Tab>("All");
  // A tab the new results leave empty falls back to All instead of hiding them.
  const tab: Tab =
    search && tabHasResults(search, chosenTab) ? chosenTab : "All";
  // The cursor belongs to one result set; new results start it at the top.
  const [wantFor, setWantFor] = useState<{ query: string | null; row: number }>(
    { query: null, row: 0 },
  );
  const resultQuery = search?.query ?? null;
  // Store the reset, so results for an earlier query start at the top when they come back.
  if (wantFor.query !== resultQuery) setWantFor({ query: resultQuery, row: 0 });
  const want = wantFor.query === resultQuery ? wantFor.row : 0;
  const setWant = (row: number) => setWantFor({ query: resultQuery, row });

  const active = source?.active ?? null;
  const compiled = source?.compiled ?? [];
  const tracks = search?.tracks ?? [];
  const artists = search?.artists ?? [];
  const albums = search?.albums ?? [];
  const playlists = search?.playlists ?? [];
  const shows = (search?.shows ?? []).filter((show) => show.id !== null);
  const asked = query.trim();
  const others = active && asked ? otherSources(compiled, active) : [];
  const rerun = () => {
    if (asked) run(asked);
  };
  const nothing =
    tracks.length +
      artists.length +
      albums.length +
      playlists.length +
      shows.length ===
    0;
  const top: Top | null = artists[0]
    ? { kind: "artist", artist: artists[0] }
    : albums[0]
      ? { kind: "album", album: albums[0] }
      : null;
  const shown = tab === "Songs" ? tracks : tracks.slice(0, TOP_SONGS);
  const songs = (full: boolean) =>
    search && (
      <ResultTracks
        tracks={shown}
        full={full}
        liked={search.liked}
        cursor={clampCursor(want, shown.length)}
        playingUri={playingUri}
        onCursor={setWant}
        send={send}
      />
    );
  const cap = <T,>(list: T[]) =>
    tab === "All" ? list.slice(0, SHELF_CARDS) : list;
  // The sticky track header sits under the bar, which grows when its chips wrap.
  const page = useRef<HTMLDivElement>(null);
  const bar = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const root = page.current;
    const sticky = bar.current;
    if (!root || !sticky) return;
    const observer = new ResizeObserver(() =>
      root.style.setProperty("--search-bar-height", `${sticky.offsetHeight}px`),
    );
    observer.observe(sticky);
    return () => observer.disconnect();
  }, []);

  return (
    <div ref={page} className="search-page">
      <div ref={bar} className="search-bar">
        {search?.ran && (
          <div className="search-chips" role="group" aria-label="Result type">
            {TABS.map((name) => (
              <button
                key={name}
                type="button"
                aria-pressed={tab === name}
                disabled={!tabHasResults(search, name)}
                onClick={() => setTab(name)}
              >
                {name}
              </button>
            ))}
          </div>
        )}
        {active && compiled.length > 1 && (
          <select
            className="search-scope"
            aria-label="Search in"
            value={active}
            onChange={(event) => {
              const picked = compiled.find((s) => s === event.target.value);
              if (!picked) return;
              send({ SelectSource: picked });
              rerun();
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
      {waiting && (
        <p className="search-note" role="status">
          Searching…
        </p>
      )}
      {!search?.ran ? (
        !waiting && (
          <div className="search-empty">
            <Glyph name="search" size={48} />
            <h1>Search for songs, artists, albums and playlists</h1>
            <p>Type in the search field at the top and press Enter.</p>
            {active && compiled.length <= 1 && <p>{scopeLabel(active)}</p>}
          </div>
        )
      ) : nothing ? (
        <p className="search-note">
          No results found for “{search.query ?? asked}”.
        </p>
      ) : (
        <>
          {tab === "All" && (top || tracks.length > 0) && (
            <div className={top ? "search-lead wide" : "search-lead"}>
              {top && (
                <section aria-label="Top result">
                  <h2>Top result</h2>
                  <TopResult top={top} open={open} send={send} />
                </section>
              )}
              {tracks.length > 0 && (
                <section aria-label="Songs">
                  <h2>Songs</h2>
                  {songs(false)}
                </section>
              )}
            </div>
          )}
          {tab === "Songs" && (
            <section className="search-songs" aria-label="Songs">
              {songs(true)}
            </section>
          )}
          {(tab === "All" || tab === "Artists") && artists.length > 0 && (
            <Shelf title="Artists">
              {cap(artists).map((artist, index) => (
                <ArtistCard
                  key={`${index}-${artist.uri ?? artist.name}`}
                  artist={artist}
                  open={open}
                  send={send}
                />
              ))}
            </Shelf>
          )}
          {(tab === "All" || tab === "Albums") && albums.length > 0 && (
            <Shelf title="Albums">
              {cap(albums).map((album, index) => (
                <AlbumCard
                  key={`${index}-${album.uri ?? album.name}`}
                  album={album}
                  open={open}
                  send={send}
                />
              ))}
            </Shelf>
          )}
          {(tab === "All" || tab === "Playlists") && playlists.length > 0 && (
            <Shelf title="Playlists">
              {cap(playlists).map((playlist, index) => (
                <PlaylistCard
                  key={`${index}-${playlist.uri}`}
                  playlist={playlist}
                  open={open}
                  send={send}
                />
              ))}
            </Shelf>
          )}
          {(tab === "All" || tab === "Podcasts") && shows.length > 0 && (
            <Shelf title="Podcasts">
              {cap(shows).map((show, index) => (
                <ShowCard key={`${index}-${show.id}`} show={show} open={open} />
              ))}
            </Shelf>
          )}
        </>
      )}
      {search?.ran && others.length > 0 && (
        <section
          className="search-others"
          aria-label="Search in another source"
        >
          <h2>Search in another source</h2>
          {others.map((other) => (
            <button
              key={other}
              type="button"
              onClick={() => {
                send({ SelectSource: other });
                rerun();
              }}
            >
              <SourceBadge source={other} />
              <span>
                Search “{asked}” in {other}
              </span>
            </button>
          ))}
        </section>
      )}
      <KeyHints hints={["j k move", "enter play", "q queue", "/ search"]} />
    </div>
  );
});

type Top =
  { kind: "artist"; artist: ArtistInfo } | { kind: "album"; album: AlbumInfo };

function play(uri: string | null, send: Send) {
  if (uri) send({ PlayContext: { uri, offset: null } });
}

/** The page id of a Spotify hit; other sources have no artist or album page, so their hits play. */
function spotifyId(id: string | null, uri: string | null): string | null {
  return id !== null && sourceOf(uri) === "Spotify" ? id : null;
}

function openArtist(artist: ArtistInfo, open: OpenView, send: Send) {
  const id = spotifyId(artist.id, artist.uri);
  if (id) open({ page: "artist", id, name: artist.name });
  else play(artist.uri, send);
}

function openAlbum(album: AlbumInfo, open: OpenView, send: Send) {
  const id = spotifyId(album.id, album.uri);
  if (id) open({ page: "album", id });
  else play(album.uri, send);
}

function albumLine(album: AlbumInfo): string {
  return [
    year(album.release_date),
    album.artists.map((artist) => artist.name).join(", "),
  ]
    .filter(Boolean)
    .join(" · ");
}

function TopResult({
  top,
  open,
  send,
}: {
  top: Top;
  open: OpenView;
  send: Send;
}) {
  const artist = top.kind === "artist";
  const name = artist ? top.artist.name : top.album.name;
  const uri = artist ? top.artist.uri : top.album.uri;
  return (
    <div className="search-top">
      <button
        type="button"
        className="search-top-open"
        onClick={() =>
          artist
            ? openArtist(top.artist, open, send)
            : openAlbum(top.album, open, send)
        }
      >
        <Cover
          src={artist ? top.artist.image_url : top.album.image_url}
          size={92}
          round={artist}
          fallback={artist ? "user" : "music"}
        />
        <span className="search-top-name">{name}</span>
        <span className="search-top-kind">
          <span className="search-kind">{artist ? "Artist" : "Album"}</span>
          {!artist && albumLine(top.album)}
        </span>
      </button>
      {uri && (
        <span className="search-top-play">
          <PlayButton label={`Play ${name}`} onClick={() => play(uri, send)} />
        </span>
      )}
    </div>
  );
}

function ArtistCard({
  artist,
  open,
  send,
}: {
  artist: ArtistInfo;
  open: OpenView;
  send: Send;
}) {
  return (
    <Card
      image={artist.image_url}
      title={artist.name}
      subtitle="Artist"
      round
      onOpen={() => openArtist(artist, open, send)}
      onPlay={artist.uri ? () => play(artist.uri, send) : undefined}
    />
  );
}

function AlbumCard({
  album,
  open,
  send,
}: {
  album: AlbumInfo;
  open: OpenView;
  send: Send;
}) {
  return (
    <Card
      image={album.image_url}
      title={album.name}
      subtitle={albumLine(album)}
      onOpen={() => openAlbum(album, open, send)}
      onPlay={album.uri ? () => play(album.uri, send) : undefined}
    />
  );
}

function PlaylistCard({
  playlist,
  open,
  send,
}: {
  playlist: PlaylistInfo;
  open: OpenView;
  send: Send;
}) {
  const spotify = sourceOf(playlist.uri) === "Spotify";
  return (
    <Card
      image={playlist.image_url}
      title={playlist.name}
      subtitle={playlist.owner ? `By ${playlist.owner}` : "Playlist"}
      onOpen={() =>
        spotify
          ? open({
              page: "playlist",
              uri: playlist.uri,
              name: playlist.name,
              source: "Spotify",
            })
          : play(playlist.uri, send)
      }
      onPlay={() => play(playlist.uri, send)}
    />
  );
}

function ShowCard({ show, open }: { show: ShowInfo; open: OpenView }) {
  return (
    <Card
      image={show.image_url}
      title={show.name}
      subtitle={show.publisher || "Podcast"}
      onOpen={() => {
        const view = showView(show);
        if (view) open(view);
      }}
    />
  );
}

const ResultTracks = memo(function ResultTracks({
  tracks,
  full,
  liked,
  cursor,
  playingUri,
  onCursor,
  send,
}: {
  tracks: TrackInfo[];
  /// The Songs view: a numbered table with the album column.
  full: boolean;
  liked: string[];
  cursor: number;
  playingUri: string | null;
  onCursor: (index: number) => void;
  send: Send;
}) {
  const list = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    list.current
      ?.querySelector('[aria-selected="true"]')
      ?.scrollIntoView({ block: "nearest" });
  }, [cursor]);

  const playAt = useCallback(
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
      playAt(cursor);
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
    <div className={full ? "search-tracks full" : "search-tracks"}>
      {full && (
        <div className="search-row head" aria-hidden="true">
          <span className="n">#</span>
          <span>Title</span>
          <span className="album">Album</span>
          <span />
          <span className="time">
            <Glyph name="clock" />
          </span>
        </div>
      )}
      <div
        ref={list}
        role="listbox"
        aria-label="Songs"
        aria-activedescendant={cursor >= 0 ? `search-${cursor}` : undefined}
        tabIndex={0}
        data-focus
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
              className={now ? "search-row now" : "search-row"}
              onClick={() => onCursor(index)}
              onDoubleClick={() => playAt(index)}
            >
              {full && (
                <span className="n">
                  {now ? <Glyph name="play" size={12} /> : index + 1}
                </span>
              )}
              <span className="title">
                <Cover src={track.image_url} size={40} />
                <span className="words">
                  <b>
                    {now && <span className="sr-only">Playing: </span>}
                    {track.name}
                  </b>
                  <span>{track.artists.join(", ")}</span>
                </span>
              </span>
              {full && <span className="album">{track.album}</span>}
              <span className="liked">
                {mine && (
                  <>
                    <Glyph name="heart" size={16} filled />
                    <span className="sr-only">Liked</span>
                  </>
                )}
              </span>
              <span className="time">
                {track.duration_ms > 0 ? clock(track.duration_ms) : "live"}
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
});
