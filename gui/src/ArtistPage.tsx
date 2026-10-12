import { useState, type KeyboardEvent } from "react";
import type { Action } from "./bindings/Action";
import type { ArtistPayload } from "./bindings/ArtistPayload";
import { clampCursor, playRequest } from "./libraryModel";
import type { OpenView } from "./nav";
import "./Pages.css";
import { albumKind, trackKey, yearOf } from "./pagesModel";
import { TrackTable } from "./TrackTable";
import { Card, PageHeader, PlayButton, Shelf } from "./ui";

const POPULAR = 5;
const POPULAR_MORE = 10;

/** An artist page; `artist.page` is the last fetched artist, which can still be another one. */
export function ArtistPage({
  id,
  name,
  artist,
  playingUri,
  open,
  send,
}: {
  id: string;
  name: string;
  artist: ArtistPayload | null;
  playingUri: string | null;
  open: OpenView;
  send: (action: Action) => void;
}) {
  const [want, setWant] = useState(0);
  const [more, setMore] = useState(false);
  const [shownId, setShownId] = useState(id);
  if (shownId !== id) {
    setShownId(id);
    setWant(0);
    setMore(false);
  }

  const page = artist?.page;
  if (!page || page.id !== id)
    return (
      <section className="artist-page">
        <PageHeader
          kind="Artist"
          title={name}
          image={null}
          round
          tone="artist"
        />
      </section>
    );

  const top = page.top_tracks;
  const shown = top.slice(0, more ? POPULAR_MORE : POPULAR);
  const cursor = clampCursor(want, shown.length);
  const play = (index: number) => {
    const action = playRequest(top, index);
    if (action) send(action);
  };
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.ctrlKey || event.metaKey || event.altKey) return;
    const key = trackKey(event.key, cursor, shown.length);
    if (!key) return;
    event.preventDefault();
    if (key.kind === "play") play(cursor);
    else if (key.kind === "move") setWant(key.next);
    else if (shown[cursor] && !event.repeat)
      send({ QueueTrack: shown[cursor] });
  };
  const related = page.related_artists.flatMap((entry) =>
    entry.id ? [{ ...entry, id: entry.id }] : [],
  );

  return (
    <section className="artist-page">
      <PageHeader
        kind="Artist"
        title={page.name}
        image={page.image_url}
        round
        tone="artist"
      />
      <div className="page-actions">
        <PlayButton
          label={`Play ${page.name}`}
          large
          onClick={() =>
            send({ PlayContext: { uri: `spotify:artist:${id}`, offset: null } })
          }
        />
        <button
          type="button"
          className="outline-action"
          aria-pressed={page.following}
          onClick={() =>
            send(page.following ? { UnfollowArtist: id } : { FollowArtist: id })
          }
        >
          {page.following ? "Following" : "Follow"}
        </button>
      </div>
      {shown.length > 0 && (
        <section className="page-section" aria-label="Popular">
          <h2>Popular</h2>
          <TrackTable
            label="Popular"
            idPrefix="artist"
            tracks={shown}
            cursor={cursor}
            playingUri={playingUri}
            showAlbum={false}
            showCover
            onKeyDown={onKeyDown}
            onPick={setWant}
            onPlay={play}
            open={open}
          />
          {top.length > POPULAR && (
            <button
              type="button"
              className="text-action"
              onClick={() => setMore((shows) => !shows)}
            >
              {more ? "Show less" : "See more"}
            </button>
          )}
        </section>
      )}
      {page.albums.length > 0 && (
        <Shelf oneRow title="Discography">
          {page.albums.map((album, index) => {
            const year = yearOf(album.release_date);
            const kind = albumKind(album.album_type);
            return (
              <Card
                key={album.id ?? `${index}-${album.name}`}
                image={album.image_url}
                title={album.name}
                subtitle={year ? `${year} · ${kind}` : kind}
                onOpen={() => album.id && open({ page: "album", id: album.id })}
                onPlay={() =>
                  album.uri &&
                  send({ PlayContext: { uri: album.uri, offset: null } })
                }
              />
            );
          })}
        </Shelf>
      )}
      {related.length > 0 && (
        <Shelf oneRow title="Fans also like">
          {related.map((entry) => (
            <Card
              key={entry.id}
              image={entry.image_url}
              title={entry.name}
              subtitle="Artist"
              round
              onOpen={() =>
                open({ page: "artist", id: entry.id, name: entry.name })
              }
            />
          ))}
        </Shelf>
      )}
    </section>
  );
}
