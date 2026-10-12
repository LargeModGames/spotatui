import { useState, type ReactNode } from "react";
import type { Action } from "./bindings/Action";
import type { ArtistInfo } from "./bindings/ArtistInfo";
import type { PlaylistSyncPayload } from "./bindings/PlaylistSyncPayload";
import type { Source } from "./bindings/Source";
import type { SourcePayload } from "./bindings/SourcePayload";
import type { SourcePlaylists } from "./bindings/SourcePlaylists";
import { unmatchedTotal } from "./healthModel";
import { Glyph } from "./Icon";
import {
  openRow,
  playlistsFor,
  songCount,
  type SidebarRow,
} from "./libraryModel";
import "./LibraryPanel.css";
import type { OpenView, View } from "./nav";
import { Cover } from "./ui";

type Filter = "Playlists" | "Albums" | "Artists";

/** The left "Your Library" panel. */
export function LibraryPanel({
  playlists,
  artists,
  source,
  sync,
  view,
  open,
  send,
}: {
  playlists: SourcePlaylists | null;
  /** The followed artists; null before the library channel arrived. */
  artists: ArtistInfo[] | null;
  source: SourcePayload | null;
  sync: PlaylistSyncPayload | null;
  /** The view the main panel shows, to mark the open row. */
  view: View;
  open: OpenView;
  send: (action: Action) => void;
}) {
  const active = source?.active ?? null;
  const spotify = active === "Spotify";
  const [chosen, setChosen] = useState<Filter>("Playlists");
  // Albums and Artists exist for Spotify only; another source shows its playlists.
  const filter = spotify ? chosen : "Playlists";
  // Each pick refetches the first page, so new saves and follows show.
  const choose = (next: Filter) => {
    setChosen(next);
    if (next !== "Playlists") send({ OpenLibrary: next });
  };
  const filters: Filter[] = spotify
    ? ["Playlists", "Albums", "Artists"]
    : ["Playlists"];
  const compiled = source?.compiled ?? [];

  return (
    <nav className="library-panel" aria-label="Your Library">
      <header className="library-head">
        <h2>
          <Glyph name="library" size={22} />
          Your Library
        </h2>
        {active && compiled.length > 1 && (
          <select
            className="library-source"
            aria-label="Source"
            value={active}
            onChange={(event) =>
              send({ SelectSource: event.target.value as Source })
            }
          >
            {compiled.map((choice) => (
              <option key={choice} value={choice}>
                {choice}
              </option>
            ))}
          </select>
        )}
      </header>
      <div className="library-filters" role="group" aria-label="Show">
        {filters.map((name) => (
          <button
            key={name}
            type="button"
            aria-pressed={name === filter}
            onClick={() => choose(name)}
          >
            {name}
          </button>
        ))}
      </div>
      <div className="library-rows">
        {filter === "Playlists" && (
          <PlaylistRows
            playlists={playlists}
            source={active}
            unmatched={unmatchedTotal(sync?.links ?? [])}
            view={view}
            open={open}
            send={send}
          />
        )}
        {filter === "Albums" &&
          (playlists?.saved_albums.length ? (
            playlists.saved_albums.map((album, index) => (
              <Row
                key={`${index}-${album.id ?? album.name}`}
                cover={<Cover src={album.image_url} size={48} />}
                title={album.name}
                subline={
                  album.artists.length
                    ? `Album · ${album.artists.map((a) => a.name).join(", ")}`
                    : "Album"
                }
                current={view.page === "album" && view.id === album.id}
                onClick={
                  album.id
                    ? () => open({ page: "album", id: album.id ?? "" })
                    : undefined
                }
              />
            ))
          ) : (
            <p className="library-empty">No saved albums.</p>
          ))}
        {filter === "Artists" &&
          (artists === null ? (
            <p className="library-empty">Loading artists…</p>
          ) : artists.length === 0 ? (
            <p className="library-empty">No followed artists.</p>
          ) : (
            artists.map((artist, index) => (
              <Row
                key={`${index}-${artist.id ?? artist.name}`}
                cover={
                  <Cover
                    src={artist.image_url}
                    size={48}
                    round
                    fallback="user"
                  />
                }
                title={artist.name}
                subline="Artist"
                current={view.page === "artist" && view.id === artist.id}
                onClick={
                  artist.id
                    ? () =>
                        open({
                          page: "artist",
                          id: artist.id ?? "",
                          name: artist.name,
                        })
                    : undefined
                }
              />
            ))
          ))}
      </div>
    </nav>
  );
}

/** Liked Songs, the active source's playlists or stations, and Library health. */
function PlaylistRows({
  playlists,
  source,
  unmatched,
  view,
  open,
  send,
}: {
  playlists: SourcePlaylists | null;
  source: Source | null;
  unmatched: number;
  view: View;
  open: OpenView;
  send: (action: Action) => void;
}) {
  const rows = playlists && source ? playlistsFor(playlists, source) : [];
  const pick = (row: SidebarRow) => {
    if (!source) return;
    if (source === "Radio") {
      const action = openRow(source, row);
      if (action) send(action);
      return;
    }
    open({ page: "playlist", uri: row.uri, name: row.name, source });
  };
  return (
    <>
      <Row
        cover={
          <span className="liked-tile" aria-hidden="true">
            <Glyph name="heart" size={20} filled />
          </span>
        }
        title="Liked Songs"
        subline="Playlist"
        current={view.page === "liked"}
        onClick={() => open({ page: "liked" })}
      />
      {rows.map((row) => (
        <Row
          key={row.uri}
          cover={<Cover src={row.image} size={48} />}
          title={row.name}
          subline={
            source === "Radio"
              ? "Radio station"
              : row.count === null
                ? "Playlist"
                : `Playlist · ${songCount(row.count)}`
          }
          current={view.page === "playlist" && view.uri === row.uri}
          onClick={() => pick(row)}
        />
      ))}
      <Row
        cover={<Cover src={null} size={48} fallback="link" />}
        title="Library health"
        subline={
          unmatched > 0 ? (
            <span className="warn">{unmatched} unmatched</span>
          ) : (
            "Playlist sync"
          )
        }
        current={view.page === "health"}
        onClick={() => open({ page: "health" })}
      />
    </>
  );
}

/** One compact row: the cover, the name and a gray subline. */
function Row({
  cover,
  title,
  subline,
  current,
  onClick,
}: {
  cover: ReactNode;
  title: string;
  subline: ReactNode;
  current: boolean;
  /** Absent, the row cannot open anything. */
  onClick?: () => void;
}) {
  return (
    <button
      type="button"
      className="library-row"
      title={title}
      aria-current={current ? "page" : undefined}
      disabled={!onClick}
      onClick={onClick}
    >
      {cover}
      <span className="library-row-text">
        <span className="library-row-title">{title}</span>
        <span className="library-row-sub">{subline}</span>
      </span>
    </button>
  );
}
