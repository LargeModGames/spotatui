import { EN, type GuiStrings } from "./i18n";
import type { Action } from "./bindings/Action";
import type { PlaybackPayload } from "./bindings/PlaybackPayload";
import type { TrackInfo } from "./bindings/TrackInfo";
import { sourceOf } from "./format";
import { Glyph } from "./Icon";
import type { OpenView } from "./nav";
import { Cover } from "./ui";

/** The Now Playing pane: the playing track large, and the next track in the queue. */
export function NowPlayingPanel({
  s = EN,
  playback,
  upNext,
  open,
  send,
  onClose,
  onCover,
}: {
  s?: GuiStrings;
  playback: PlaybackPayload | null;
  upNext: TrackInfo[];
  open: OpenView;
  send: (action: Action) => void;
  onClose: () => void;
  /** Opens the cover art in a pane of its own; absent where it cannot. */
  onCover?: () => void;
}) {
  const item = playback?.item ?? null;
  const next = upNext[0] ?? null;
  const liked = playback?.liked ?? false;
  // The album is the only play context the panel can name.
  const heading =
    item?.album && item.context_uri?.startsWith("spotify:album:")
      ? item.album
      : s.now_playing;
  return (
    <section className="now-panel" aria-label={s.now_playing_view}>
      <header className="panel-head">
        <h2>{heading}</h2>
        <span className="panel-actions">
          {onCover && (
            <button
              type="button"
              className="panel-close"
              aria-label={s.cover_art}
              title={s.detach_pane}
              onClick={onCover}
            >
              <Glyph name="panel" />
            </button>
          )}
          <button
            type="button"
            className="panel-close"
            aria-label={s.close_now_playing}
            onClick={onClose}
          >
            <Glyph name="close" />
          </button>
        </span>
      </header>
      {item ? (
        <div className="now-body">
          <Cover src={item.image_url} />
          <div className="now-title">
            <div className="now-text">
              <h3 title={item.title}>{item.title}</h3>
              <p className="artist-line">
                <span>{item.artists.join(", ")}</span>
                {item.quality && (
                  <span className="quality">{` • ${item.quality}`}</span>
                )}
              </p>
            </div>
            {sourceOf(item.uri) === "Spotify" && (
              <button
                type="button"
                className="like"
                aria-label={s.like}
                aria-pressed={liked}
                onClick={() => send("ToggleSaveCurrentItem")}
              >
                <Glyph name="heart" size={20} filled={liked} />
              </button>
            )}
          </div>
          {next && (
            <section className="next-card" aria-label={s.next_in_queue}>
              <h3>{s.next_in_queue}</h3>
              <div className="next-track">
                <Cover src={next.image_url} size={48} />
                <span className="next-text">
                  <strong>{next.name}</strong>
                  <Artists track={next} open={open} />
                </span>
              </div>
            </section>
          )}
        </div>
      ) : (
        <p className="panel-empty">{s.nothing_is_playing}</p>
      )}
    </section>
  );
}

/** The artist names; one with a Spotify id opens its page. */
function Artists({ track, open }: { track: TrackInfo; open: OpenView }) {
  // Other sources carry their own artist ids, which no Spotify page can open.
  if (track.artist_refs.length === 0 || sourceOf(track.uri) !== "Spotify")
    return <span>{track.artists.join(", ")}</span>;
  return (
    <span>
      {track.artist_refs.map(({ id, name }, index) => (
        <span key={`${index}-${name}`}>
          {index > 0 && ", "}
          {id ? (
            <button
              type="button"
              className="artist-link"
              onClick={() => open({ page: "artist", id, name })}
            >
              {name}
            </button>
          ) : (
            name
          )}
        </span>
      ))}
    </span>
  );
}
