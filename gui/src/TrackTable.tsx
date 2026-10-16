import {
  memo,
  useContext,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type MouseEvent,
} from "react";
import type { TrackInfo } from "./bindings/TrackInfo";
import { clock, sourceOf } from "./format";
import { Glyph } from "./Icon";
import "./ListPage.css";
import type { OpenView } from "./nav";
import { rowKeys } from "./sortModel";
import { SourceBadge } from "./SourceBadge";
import { TrackActionsContext, TrackMenu, type TrackActions } from "./TrackMenu";
import { spotifyTrack, trackMenu, type RowContext } from "./trackMenuModel";
import { Cover } from "./ui";

/** The `# / Title / Album / duration` table; the cursor row is `aria-selected`. */
export const TrackTable = memo(function TrackTable({
  label,
  idPrefix,
  tracks,
  cursor,
  playingUri,
  showAlbum = true,
  showCover = true,
  onKeyDown,
  onPick,
  onPlay,
  open,
  rowContext,
}: {
  label: string;
  idPrefix: string;
  tracks: TrackInfo[];
  cursor: number;
  playingUri: string | null;
  showAlbum?: boolean;
  showCover?: boolean;
  onKeyDown: (event: KeyboardEvent<HTMLDivElement>) => void;
  onPick: (index: number) => void;
  onPlay: (index: number) => void;
  open?: OpenView;
  /** What the list adds to a row's menu; `index` is the row's index in `tracks`. */
  rowContext?: (track: TrackInfo, index: number) => RowContext;
}) {
  const list = useRef<HTMLDivElement>(null);
  const actions = useContext(TrackActionsContext);
  // The row is kept as it was when the menu opened: a reorder under it must not retarget the menu.
  const [menu, setMenu] = useState<{
    index: number;
    at: { x: number; y: number };
    track: TrackInfo;
    context: RowContext | undefined;
  } | null>(null);
  const openMenu = (index: number, at: { x: number; y: number }) => {
    const track = tracks[index];
    onPick(index);
    if (track)
      setMenu({ index, at, track, context: rowContext?.(track, index) });
  };
  const keys = useMemo(() => rowKeys(tracks), [tracks]);
  const cursorTrack = tracks[cursor];
  // The cursor row follows a key press, a page that lands under the cursor, and a reorder.
  useLayoutEffect(() => {
    list.current
      ?.querySelector('[aria-selected="true"]')
      ?.scrollIntoView({ block: "nearest" });
  }, [cursor, tracks.length, cursorTrack]);

  return (
    <div className={showAlbum ? "track-table" : "track-table no-album"}>
      <div className="track-row track-head" aria-hidden="true">
        <span className="n">#</span>
        <span>Title</span>
        {showAlbum && <span className="album">Album</span>}
        <span className="time">
          {actions && <span className="time-space" />}
          <Glyph name="clock" />
          {actions && <span className="time-space" />}
        </span>
      </div>
      <div
        ref={list}
        className="track-rows"
        role="listbox"
        aria-label={label}
        aria-activedescendant={
          cursor >= 0 ? `${idPrefix}-${cursor}` : undefined
        }
        tabIndex={0}
        data-focus
        onKeyDown={(event) => {
          const row = list.current?.querySelector(`#${idPrefix}-${cursor}`);
          if (
            actions &&
            row &&
            (event.key === "ContextMenu" ||
              (event.shiftKey && event.key === "F10"))
          ) {
            const box = row.getBoundingClientRect();
            openMenu(cursor, { x: box.right - 220, y: box.bottom });
            event.preventDefault();
            return;
          }
          onKeyDown(event);
        }}
      >
        {tracks.map((track, index) => {
          const now = playingUri !== null && track.uri === playingUri;
          const source = sourceOf(track.uri);
          return (
            <div
              key={keys[index]}
              id={`${idPrefix}-${index}`}
              role="option"
              className={now ? "track-row now" : "track-row"}
              aria-selected={index === cursor}
              onClick={() => onPick(index)}
              onDoubleClick={() => onPlay(index)}
              onContextMenu={(event) => {
                if (!actions) return;
                event.preventDefault();
                openMenu(index, { x: event.clientX, y: event.clientY });
              }}
            >
              <span className="n">
                <span className="index">{index + 1}</span>
                <button
                  type="button"
                  className="row-play"
                  tabIndex={-1}
                  aria-label={`Play ${track.name}`}
                  onClick={inRow(() => onPlay(index))}
                  onDoubleClick={stop}
                >
                  <Glyph name="play" size={14} />
                </button>
              </span>
              <span className="title-cell">
                {showCover && <Cover src={track.image_url} size={40} />}
                <span className="title-text">
                  <span className="title">{track.name}</span>
                  <span className="artists">
                    {source && source !== "Spotify" && (
                      <SourceBadge source={source} />
                    )}
                    {track.explicit && (
                      <span className="explicit" title="Explicit">
                        <span aria-hidden="true">E</span>
                        <span className="sr-only">Explicit</span>
                      </span>
                    )}
                    <Artists track={track} open={open} />
                  </span>
                </span>
              </span>
              {showAlbum && (
                <span className="album">
                  {open &&
                  sourceOf(track.uri) === "Spotify" &&
                  track.album_id ? (
                    <LinkButton
                      onClick={() =>
                        open({ page: "album", id: track.album_id ?? "" })
                      }
                    >
                      {track.album}
                    </LinkButton>
                  ) : (
                    track.album
                  )}
                </span>
              )}
              <span className="time">
                {actions && <RowHeart track={track} actions={actions} />}
                <span>{track.duration_ms > 0 && clock(track.duration_ms)}</span>
                {actions && (
                  <button
                    type="button"
                    className="row-more"
                    tabIndex={-1}
                    aria-label={`More options for ${track.name}`}
                    aria-haspopup="menu"
                    aria-expanded={menu?.index === index}
                    onClick={inRow((event) => {
                      const box = event.currentTarget.getBoundingClientRect();
                      openMenu(index, { x: box.right - 220, y: box.bottom });
                    })}
                    onDoubleClick={stop}
                  >
                    <Glyph name="more" size={16} />
                  </button>
                )}
              </span>
            </div>
          );
        })}
      </div>
      {actions && menu && (
        <TrackMenu
          label={`Options for ${menu.track.name}`}
          groups={trackMenu(
            menu.track,
            menu.track.id !== null && actions.liked.has(menu.track.id),
            actions.playlists,
            menu.context,
          )}
          at={menu.at}
          actions={actions}
          onClose={() => {
            setMenu(null);
            // The menu leaves focus on <body>; the list keeps j/k/Enter.
            list.current?.focus({ preventScroll: true });
          }}
        />
      )}
    </div>
  );
});

/** The artist names; on a Spotify row each one with an id is a link when `open` is given. */
function Artists({ track, open }: { track: TrackInfo; open?: OpenView }) {
  if (
    !open ||
    sourceOf(track.uri) !== "Spotify" ||
    !track.artist_refs.some((artist) => artist.id)
  )
    return <span>{track.artists.join(", ")}</span>;
  return (
    <span>
      {track.artist_refs.map((artist, index) => (
        <span key={`${index}-${artist.name}`}>
          {index > 0 && ", "}
          {artist.id ? (
            <LinkButton
              onClick={() =>
                open({ page: "artist", id: artist.id ?? "", name: artist.name })
              }
            >
              {artist.name}
            </LinkButton>
          ) : (
            artist.name
          )}
        </span>
      ))}
    </span>
  );
}

/** A link inside a row: its click neither picks nor plays the row. */
function LinkButton({
  onClick,
  children,
}: {
  onClick: () => void;
  children: string;
}) {
  return (
    <button
      type="button"
      className="link"
      tabIndex={-1}
      onClick={inRow(onClick)}
      onDoubleClick={stop}
    >
      {children}
    </button>
  );
}

function inRow(action: (event: MouseEvent<HTMLButtonElement>) => void) {
  return (event: MouseEvent<HTMLButtonElement>) => {
    event.stopPropagation();
    action(event);
  };
}

function stop(event: MouseEvent) {
  event.stopPropagation();
}

/** The heart of a Spotify row: filled when liked; a click saves or removes the track. */
function RowHeart({
  track,
  actions,
}: {
  track: TrackInfo;
  actions: TrackActions;
}) {
  if (!spotifyTrack(track) || !track.uri)
    return <span className="time-space" />;
  const uri = track.uri;
  const liked = track.id !== null && actions.liked.has(track.id);
  return (
    <button
      type="button"
      className={liked ? "row-heart liked" : "row-heart"}
      tabIndex={-1}
      aria-label={liked ? "Remove from Liked Songs" : "Save to Liked Songs"}
      aria-pressed={liked}
      onClick={inRow(() => actions.send({ ToggleSaveTrack: uri }))}
      onDoubleClick={stop}
    >
      <Glyph name="heart" size={16} filled={liked} />
    </button>
  );
}
