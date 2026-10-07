import { useEffect, useLayoutEffect, useRef } from "react";
import type { Action } from "./bindings/Action";
import type { AlbumInfo } from "./bindings/AlbumInfo";
import type { LyricsPayload } from "./bindings/LyricsPayload";
import type { NowPlaying } from "./bindings/NowPlaying";
import type { TrackInfo } from "./bindings/TrackInfo";
import type { Position } from "./connection";
import { clock, sourceOf } from "./format";
import { Icon } from "./Icon";
import "./Room.css";
import {
  activeLyric,
  albumClock,
  albumContext,
  minutes,
  platter,
  playingIndex,
  sides,
  spotifyTrackId,
} from "./roomModel";
import { Swatch } from "./SourceBadge";
import { year } from "./searchModel";
import { usePosition } from "./usePosition";

/** The Listening Room: the playing album on paper, its sides or the lyrics, and the albums queued next. */
export function Room({
  item,
  position,
  connected,
  album,
  lyrics,
  upNext,
  queueAhead,
  showLyrics,
  active,
  send,
}: {
  item: NowPlaying | null;
  position: Position | null;
  connected: boolean;
  album: AlbumInfo | null;
  lyrics: LyricsPayload | null;
  upNext: TrackInfo[];
  queueAhead: boolean;
  showLyrics: boolean;
  active: boolean;
  send: (action: Action) => void;
}) {
  const playing = item?.is_playing ?? false;
  // A hidden Room keeps no animation frame running.
  const ms = usePosition(position, playing && active);
  const onAlbum =
    album !== null && playingIndex(album.tracks, item, album.name) >= 0;
  const shown = onAlbum ? album : null;
  const tracks = shown?.tracks ?? [];
  const index = playingIndex(tracks, item, shown?.name ?? null);
  const inOrder = albumContext(shown, item, queueAhead);

  // The album is fetched by the playing track's id, once per track, while the Room is open and
  // the socket is up (a send without a socket is dropped).
  const fetched = useRef<string | null>(null);
  const trackId = spotifyTrackId(item?.uri ?? null);
  useEffect(() => {
    if (!active || !connected || !trackId || onAlbum) return;
    if (fetched.current === trackId) return;
    fetched.current = trackId;
    send({ Open: { TrackAlbum: trackId } });
  }, [active, connected, trackId, onAlbum, send]);

  const title = shown?.name ?? item?.album ?? "";
  const artist =
    shown?.artists.map((entry) => entry.name).join(", ") ||
    item?.artists.join(", ") ||
    "";
  const duration = item?.duration_ms ?? 0;
  const next = platter(upNext, item?.album ?? "");
  const whole = albumClock(tracks, index, ms);
  const source = sourceOf(item?.uri ?? null);
  const caption = [
    year(shown?.release_date ?? null),
    tracks.length > 0 && `${tracks.length} songs`,
    tracks.length > 0 &&
      `${minutes(tracks.reduce((sum, track) => sum + track.duration_ms, 0))} minutes`,
    source && `plays from ${source}`,
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    <div className="room" data-focus tabIndex={-1}>
      <figure>
        {item?.image_url || shown?.image_url ? (
          <img
            className="room-cover"
            src={item?.image_url ?? shown?.image_url ?? undefined}
            alt=""
          />
        ) : (
          <span className="room-cover" />
        )}
        {caption && <figcaption>{caption}</figcaption>}
      </figure>
      <section className="room-album" aria-label="Album">
        <span className="eyebrow">
          {!item
            ? "NOTHING PLAYING"
            : inOrder
              ? `NOW LISTENING · SONG ${index + 1} OF ${tracks.length}`
              : "NOW LISTENING"}
        </span>
        <div className="room-title">
          <h1>{title || item?.title || "—"}</h1>
          <div>
            <span className="room-artist">{artist}</span>
            {item && <em>{item.title}</em>}
          </div>
        </div>
        {inOrder && (
          <>
            <div className="segments" aria-hidden="true">
              {tracks.map((track, row) => (
                <div
                  key={`${row}-${track.uri}`}
                  style={{ flexGrow: Math.max(track.duration_ms, 1) }}
                >
                  <div
                    style={{
                      width:
                        row < index
                          ? "100%"
                          : row === index && track.duration_ms
                            ? `${Math.min(ms / track.duration_ms, 1) * 100}%`
                            : "0%",
                    }}
                  />
                </div>
              ))}
            </div>
            <div className="album-clock">
              <span>{clock(whole.elapsed)} in</span>
              <span>
                {clock(whole.left)} left
                {next[0] && ` · then ${next[0].title}`}
              </span>
            </div>
          </>
        )}
        <div className="room-transport">
          <button
            type="button"
            aria-label="Previous song"
            disabled={!connected}
            onClick={() => send("PreviousTrack")}
          >
            <Icon size={18}>
              <path d="M19 5 9 12l10 7V5z" />
              <path d="M5 5v14" />
            </Icon>
          </button>
          <button
            type="button"
            className="play"
            aria-label={playing ? "Pause" : "Play"}
            disabled={!connected}
            onClick={() => send("TogglePlayback")}
          >
            {playing ? (
              <Icon size={20} stroke={2.5}>
                <path d="M8 5v14M16 5v14" />
              </Icon>
            ) : (
              <Icon size={20} stroke={2}>
                <path d="M7 4.5v15l12-7.5z" fill="currentColor" />
              </Icon>
            )}
          </button>
          <button
            type="button"
            aria-label="Next song"
            disabled={!connected}
            onClick={() => send("NextTrack")}
          >
            <Icon size={18}>
              <path d="m5 5 10 7-10 7V5z" />
              <path d="M19 5v14" />
            </Icon>
          </button>
          {item && (
            <span className="room-time">
              {clock(duration ? Math.min(ms, duration) : ms)} /{" "}
              {item.is_live ? "live" : clock(duration)}
            </span>
          )}
          <span className="room-keys">
            <kbd>l</kbd> {showLyrics ? "tracklist" : "lyrics"} · <kbd>r</kbd>{" "}
            leave
          </span>
        </div>
        {showLyrics ? (
          <Lyrics lyrics={lyrics} ms={ms} />
        ) : tracks.length > 0 ? (
          <Sides
            tracks={tracks}
            index={index}
            inOrder={inOrder}
            albumUri={shown?.uri ?? null}
            send={send}
          />
        ) : (
          <p className="room-note">
            {!item
              ? "Play an album and it opens here."
              : trackId
                ? "The album of this song is not loaded."
                : "The tracklist shows for Spotify albums."}
          </p>
        )}
      </section>
      {next.length > 0 && (
        <section className="room-platter" aria-label="On the platter next">
          <div className="room-head">
            <span>On the platter next</span>
            <kbd>from your queue</kbd>
          </div>
          <div className="platter">
            {next.map((entry, place) => {
              const from = sourceOf(entry.uri);
              return (
                <div key={`${place}-${entry.title}`}>
                  {entry.image ? (
                    <img src={entry.image} alt="" />
                  ) : (
                    <span className="tile" />
                  )}
                  <span>
                    <b>{entry.title}</b>
                    <span>{entry.artist}</span>
                    {from && (
                      <span className="from">
                        <Swatch source={from} /> {from.toUpperCase()}
                      </span>
                    )}
                  </span>
                </div>
              );
            })}
          </div>
        </section>
      )}
    </div>
  );
}

function Sides({
  tracks,
  index,
  inOrder,
  albumUri,
  send,
}: {
  tracks: TrackInfo[];
  index: number;
  /// Rows before the playing one count as played only when the album plays in order.
  inOrder: boolean;
  albumUri: string | null;
  send: (action: Action) => void;
}) {
  const [sideA, sideB] = sides(tracks);
  const side = (label: string, rows: TrackInfo[], offset: number) => (
    <ol>
      <li className="side-label">{label}</li>
      {rows.map((track, row) => {
        const at = offset + row;
        const state =
          at === index ? "now" : inOrder && at < index ? "done" : "";
        return (
          <li key={`${at}-${track.uri}`}>
            <button
              type="button"
              className={state}
              aria-current={at === index ? "true" : undefined}
              disabled={!albumUri}
              onClick={() =>
                albumUri && send({ PlayContext: { uri: albumUri, offset: at } })
              }
            >
              <span className="n">
                {at === index ? "▶" : track.track_number || at + 1}
              </span>
              <span className="name">{track.name}</span>
              <span className="time">{clock(track.duration_ms)}</span>
            </button>
          </li>
        );
      })}
    </ol>
  );
  return (
    <div className="sides">
      {side("SIDE A", sideA, 0)}
      {side("SIDE B", sideB, sideA.length)}
    </div>
  );
}

function Lyrics({ lyrics, ms }: { lyrics: LyricsPayload | null; ms: number }) {
  const synced = lyrics?.synced ?? false;
  const lines = lyrics?.lines ?? [];
  const current = synced ? activeLyric(lines, ms) : -1;
  const panel = useRef<HTMLOListElement>(null);
  // Scrolls the lyrics list alone; scrollIntoView would move the whole Room with it.
  useLayoutEffect(() => {
    const list = panel.current;
    const line = list?.querySelector<HTMLElement>('[aria-current="true"]');
    if (!list || !line) return;
    list.scrollTo({
      top: line.offsetTop - list.offsetTop - list.clientHeight / 2,
      behavior: "smooth",
    });
  }, [current]);

  if (!lyrics || lyrics.status === "loading" || lyrics.status === "not_started")
    return <p className="room-note">Loading lyrics…</p>;
  if (lyrics.status === "not_found" || lines.length === 0)
    return <p className="room-note">No lyrics for this song.</p>;
  return (
    <div className="lyrics">
      {!synced && <span className="eyebrow">TIMING ESTIMATED</span>}
      <ol ref={panel}>
        {lines.map((line, row) => (
          <li
            key={`${row}-${line.at_ms}`}
            aria-current={row === current ? "true" : undefined}
            className={row < current ? "sung" : undefined}
          >
            {line.text || "♪"}
          </li>
        ))}
      </ol>
    </div>
  );
}
