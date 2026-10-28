import { EN, type GuiStrings, fmt } from "./i18n";
import { useState, type KeyboardEvent } from "react";
import type { Action } from "./bindings/Action";
import type { TrackInfo } from "./bindings/TrackInfo";
import type { TrackTablePayload } from "./bindings/TrackTablePayload";
import { clampCursor, playRequest, songCount } from "./libraryModel";
import "./ListPage.css";
import type { OpenView } from "./nav";
import "./Pages.css";
import { loadLatch, radioRows, trackKey, type LoadLatch } from "./pagesModel";
import { TrackTable } from "./TrackTable";
import { PageHeader, PlayButton } from "./ui";

const NO_TRACKS: TrackInfo[] = [];

export interface RadioPageProps {
  s?: GuiStrings;
  /** The open view; a new one is a fresh open, even of the same seed. */
  view: object;
  /** The seed track id. */
  id: string;
  name: string;
  /** The track table; its rows are this radio's only when `radio` is `id`. */
  table: TrackTablePayload | null;
  /** A Spotify session exists; without one the radio cannot load. */
  available: boolean;
  /** The status revision and whether it is an error: a later error means the fetch failed. */
  statusRev: number | null;
  statusError: boolean;
  playingUri: string | null;
  active: boolean;
  open: OpenView;
  send: (action: Action) => void;
}

/** The tracks Spotify recommends from one song. */
export function RadioPage({
  s = EN,
  view,
  id,
  name,
  table,
  available,
  statusRev,
  statusError,
  playingUri,
  open,
  send,
}: RadioPageProps) {
  const [want, setWant] = useState(0);
  const [shownFor, setShownFor] = useState(view);
  const [latch, setLatch] = useState<LoadLatch>({
    openedAt: statusRev,
    failedId: null,
  });
  const fresh = shownFor !== view;
  if (fresh) {
    setShownFor(view);
    setWant(0);
  }

  const rows = radioRows(table, id);
  const next = loadLatch(latch, {
    id,
    fresh,
    landed: rows !== null,
    statusRev,
    statusError,
  });
  if (next.openedAt !== latch.openedAt || next.failedId !== latch.failedId)
    setLatch(next);
  const failed = !rows && next.failedId === id;
  const tracks = rows ?? NO_TRACKS;
  const cursor = clampCursor(want, tracks.length);
  const play = (index: number) => {
    const action = playRequest(tracks, index);
    if (action) send(action);
  };
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.ctrlKey || event.metaKey || event.altKey) return;
    const key = trackKey(event.key, cursor, tracks.length);
    if (!key) return;
    event.preventDefault();
    if (key.kind === "play") play(cursor);
    else if (key.kind === "move") setWant(key.next);
    else if (tracks[cursor] && !event.repeat)
      send({ QueueTrack: tracks[cursor] });
  };
  const title = fmt(s.song_radio_named, { name });
  const first = tracks.findIndex((track) => track.uri);

  return (
    <section className="radio-page" aria-label={title}>
      <PageHeader
        kind={s.song_radio}
        title={title}
        image={tracks[0]?.image_url}
        tone="playlist"
        meta={
          <>
            <span>{fmt(s.based_on, { name })}</span>
            {rows && (
              <span className="meta-dot">{songCount(rows.length, s)}</span>
            )}
          </>
        }
      />
      <div className="page-actions">
        <PlayButton
          label={fmt(s.play_named, { name: title })}
          large
          disabled={first < 0}
          onClick={() => play(first)}
        />
      </div>
      {tracks.length === 0 ? (
        <p className="list-empty">
          {rows
            ? fmt(s.no_similar_songs, { name })
            : !available
              ? s.song_radio_needs_spotify
              : failed
                ? fmt(s.load_playlist_failed, { name: title })
                : fmt(s.loading_playlist, { name: title })}
        </p>
      ) : (
        <TrackTable
          s={s}
          label={title}
          idPrefix="radio"
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
