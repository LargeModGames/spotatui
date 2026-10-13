import { useState, type KeyboardEvent } from "react";
import type { Action } from "./bindings/Action";
import type { TrackInfo } from "./bindings/TrackInfo";
import type { TrackTablePayload } from "./bindings/TrackTablePayload";
import { clampCursor, playRequest, songCount } from "./libraryModel";
import "./ListPage.css";
import type { OpenView } from "./nav";
import "./Pages.css";
import { radioRows, trackKey } from "./pagesModel";
import { TrackTable } from "./TrackTable";
import { PageHeader, PlayButton } from "./ui";

const NO_TRACKS: TrackInfo[] = [];

export interface RadioPageProps {
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
  const [shownId, setShownId] = useState(id);
  const [openedAt, setOpenedAt] = useState(statusRev);
  if (shownId !== id) {
    setShownId(id);
    setWant(0);
    setOpenedAt(statusRev);
  }

  const rows = radioRows(table, id);
  const failed = !rows && statusError && statusRev !== openedAt;
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
  const title = `${name} Radio`;
  const first = tracks.findIndex((track) => track.uri);

  return (
    <section className="radio-page" aria-label={title}>
      <PageHeader
        kind="Song radio"
        title={title}
        image={tracks[0]?.image_url}
        tone="playlist"
        meta={
          <>
            <span>Based on {name}</span>
            {rows && <span className="meta-dot">{songCount(rows.length)}</span>}
          </>
        }
      />
      <div className="page-actions">
        <PlayButton
          label={`Play ${title}`}
          large
          disabled={first < 0}
          onClick={() => play(first)}
        />
      </div>
      {tracks.length === 0 ? (
        <p className="list-empty">
          {rows
            ? `No songs like ${name} yet.`
            : !available
              ? "Song radio needs a Spotify session."
              : failed
                ? `Could not load ${title}. Choose it again to retry.`
                : `Loading ${title}…`}
        </p>
      ) : (
        <TrackTable
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
