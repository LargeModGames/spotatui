import { EN, type GuiStrings, fmt, sourceLabel, sourceBadge } from "./i18n";
import { useEffect, useLayoutEffect, useRef } from "react";
import type { Action } from "./bindings/Action";
import type { AlbumInfo } from "./bindings/AlbumInfo";
import type { LyricsPayload } from "./bindings/LyricsPayload";
import { scrollBehavior } from "./motion";
import type { NowPlaying } from "./bindings/NowPlaying";
import type { TrackInfo } from "./bindings/TrackInfo";
import type { Position } from "./connection";
import { clock, sourceOf } from "./format";
import { Glyph } from "./Icon";
import { LyricText } from "./LyricText";
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
import { SourceIcon } from "./SourceBadge";
import { year } from "./searchModel";
import { usePosition } from "./usePosition";

/** The Listening Room: the playing album on paper, its sides or the lyrics, and the albums queued next. */
export function Room({
  s = EN,
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
  onClose,
}: {
  s?: GuiStrings;
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
  onClose: () => void;
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
    tracks.length > 0 && fmt(s.songs_count, { count: tracks.length }),
    tracks.length > 0 &&
      fmt(s.minutes_count, {
        count: minutes(
          tracks.reduce((sum, track) => sum + track.duration_ms, 0),
        ),
      }),
    source && fmt(s.plays_from, { source: sourceLabel(source, s) }),
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    <div className="room" data-focus tabIndex={-1}>
      <button
        type="button"
        className="room-close"
        aria-label={s.exit_full_screen}
        onClick={onClose}
      >
        <Glyph name="close" size={20} />
      </button>
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
      <section className="room-album" aria-label={s.album}>
        <span className="room-state">
          {!item
            ? s.full_screen
            : inOrder
              ? fmt(s.now_playing_index, {
                  index: index + 1,
                  count: tracks.length,
                })
              : s.now_playing}
        </span>
        <div className="room-title">
          <h1>{title || item?.title || s.nothing_playing}</h1>
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
              <span>{fmt(s.elapsed_in, { time: clock(whole.elapsed) })}</span>
              <span>
                {fmt(s.time_left, { time: clock(whole.left) })}
                {next[0] && fmt(s.then_album, { name: next[0].title })}
              </span>
            </div>
          </>
        )}
        <div className="room-transport">
          <button
            type="button"
            aria-label={s.previous_song}
            disabled={!connected}
            onClick={() => send("PreviousTrack")}
          >
            <Glyph name="previous" size={20} />
          </button>
          <button
            type="button"
            className="play"
            aria-label={playing ? s.pause : s.play}
            disabled={!connected}
            onClick={() => send("TogglePlayback")}
          >
            <Glyph name={playing ? "pause" : "play"} size={24} />
          </button>
          <button
            type="button"
            aria-label={s.next_song}
            disabled={!connected}
            onClick={() => send("NextTrack")}
          >
            <Glyph name="next" size={20} />
          </button>
          {item && (
            <span className="room-time">
              {clock(duration ? Math.min(ms, duration) : ms)} /{" "}
              {item.is_live ? s.live : clock(duration)}
            </span>
          )}
          <span className="room-keys">
            <kbd>l</kbd> {showLyrics ? s.tracklist : s.lyrics_hint} ·{" "}
            <kbd>r</kbd> {s.leave_hint}
          </span>
        </div>
        {showLyrics ? (
          <Lyrics
            s={s}
            lyrics={lyrics}
            ms={ms}
            connected={connected}
            send={send}
          />
        ) : tracks.length > 0 ? (
          <Sides
            s={s}
            tracks={tracks}
            index={index}
            inOrder={inOrder}
            albumUri={shown?.uri ?? null}
            send={send}
          />
        ) : (
          <p className="room-note">
            {!item
              ? s.play_album_here
              : trackId
                ? s.playing_album_not_loaded
                : s.spotify_tracklist_only}
          </p>
        )}
      </section>
      {next.length > 0 && (
        <section className="room-platter" aria-label={s.on_platter_next}>
          <div className="room-head">
            <span>{s.on_platter_next}</span>
            <kbd>{s.from_your_queue}</kbd>
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
                        <SourceIcon source={from} /> {sourceBadge(from, s)}
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
  s = EN,
  tracks,
  index,
  inOrder,
  albumUri,
  send,
}: {
  s?: GuiStrings;
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
                {at === index ? (
                  <Glyph name="play" size={12} />
                ) : (
                  track.track_number || at + 1
                )}
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
      {side(s.side_a, sideA, 0)}
      {side(s.side_b, sideB, sideA.length)}
    </div>
  );
}

function Lyrics({
  s = EN,
  lyrics,
  ms,
  connected,
  send,
}: {
  s?: GuiStrings;
  lyrics: LyricsPayload | null;
  ms: number;
  connected: boolean;
  send: (action: Action) => void;
}) {
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
      behavior: scrollBehavior(),
    });
  }, [current]);

  if (!lyrics || lyrics.status === "loading" || lyrics.status === "not_started")
    return <p className="room-note">{s.loading_lyrics}</p>;
  if (lyrics.status === "not_found" || lines.length === 0)
    return <p className="room-note">{s.no_lyrics_sentence}</p>;
  return (
    <div className="lyrics">
      {!synced && <span className="label">{s.timing_estimated}</span>}
      <ol ref={panel}>
        {lines.map((line, row) => (
          <li
            key={`${row}-${line.at_ms}`}
            aria-current={row === current ? "true" : undefined}
            className={row < current ? "sung" : undefined}
          >
            <LyricText
              line={line}
              synced={synced}
              tabStop={row === Math.max(current, 0)}
              connected={connected}
              send={send}
            />
          </li>
        ))}
      </ol>
    </div>
  );
}
