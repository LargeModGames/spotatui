import { useLayoutEffect, useRef, useState } from "react";
import type { Action } from "./bindings/Action";
import type { NowPlaying } from "./bindings/NowPlaying";
import type { QueuePayload } from "./bindings/QueuePayload";
import type { TrackInfo } from "./bindings/TrackInfo";
import { clock, sourceOf } from "./format";
import { KeyHints } from "./KeyHints";
import { type QueueCursor, queueKey, queueRow } from "./queueModel";
import { SourceBadge } from "./SourceBadge";

/** The queue over the current screen: the native queue can be played, moved and removed; Spotify's is read-only. */
export function QueueDrawer({
  queue,
  item,
  send,
  onClose,
}: {
  queue: QueuePayload | null;
  item: NowPlaying | null;
  send: (action: Action) => void;
  onClose: () => void;
}) {
  const native = queue?.native ?? [];
  // The Web API queue mirrors Spotify playback only; under another source it is stale.
  const spotify = sourceOf(item?.uri ?? null) === "Spotify";
  const mirrored = spotify
    ? (queue?.spotify.items ?? []).flatMap((entry) =>
        entry.track ? [entry.track] : [],
      )
    : [];
  const [want, setWant] = useState<QueueCursor>({ index: 0, uri: null });
  const cursor = queueRow(native, want);

  const list = useRef<HTMLOListElement>(null);
  const panel = useRef<HTMLElement>(null);
  // The drawer holds the keyboard even before the queue arrives, and hands it to the list once it does.
  const hasList = native.length > 0;
  useLayoutEffect(() => {
    (list.current ?? panel.current)?.focus();
  }, [hasList]);
  useLayoutEffect(() => {
    list.current
      ?.querySelector('[aria-selected="true"]')
      ?.scrollIntoView({ block: "nearest" });
  }, [cursor]);

  const now = queue?.now ?? null;
  return (
    <aside
      ref={panel}
      className="drawer"
      role="dialog"
      aria-label="Queue"
      tabIndex={-1}
      onKeyDown={(event) => {
        if (event.ctrlKey || event.metaKey || event.altKey) return;
        if (event.key === "Escape" || event.key === "Q") {
          event.preventDefault();
          onClose();
        }
      }}
    >
      <header>
        <h2>Queue</h2>
        <button type="button" aria-label="Close" onClick={onClose}>
          ×
        </button>
      </header>
      <div className="drawer-body">
        <span className="eyebrow">NOW PLAYING</span>
        {now ? (
          <Row track={now} />
        ) : item ? (
          <div className="row">
            <span className="name">
              <b>{item.title}</b> <span>{item.artists.join(", ")}</span>
            </span>
          </div>
        ) : (
          <p className="empty">Nothing is playing.</p>
        )}
        <span className="eyebrow">UP NEXT</span>
        {native.length === 0 ? (
          <p className="empty">Nothing is queued.</p>
        ) : (
          <ol
            ref={list}
            role="listbox"
            aria-label="Up next"
            aria-activedescendant={`queue-${cursor}`}
            tabIndex={0}
            onKeyDown={(event) => {
              if (event.ctrlKey || event.metaKey || event.altKey) return;
              const press = queueKey(event.key, cursor, native);
              if (!press) return;
              event.preventDefault();
              setWant(press.cursor);
              if (press.action) send(press.action);
            }}
          >
            {native.map((track, index) => (
              <li
                key={`${index}-${track.uri ?? track.name}`}
                id={`queue-${index}`}
                role="option"
                aria-selected={index === cursor}
                onClick={() => setWant({ index, uri: track.uri })}
                onDoubleClick={() => {
                  if (track.uri)
                    send({
                      PlayQueueItem: { uri: track.uri, position: index },
                    });
                }}
              >
                <Row track={track} />
              </li>
            ))}
          </ol>
        )}
        {mirrored.length > 0 && (
          <>
            <span className="eyebrow">SPOTIFY QUEUE</span>
            <ol className="mirror">
              {mirrored.map((track, index) => (
                <li key={`${index}-${track.uri ?? track.name}`}>
                  <Row track={track} />
                </li>
              ))}
            </ol>
          </>
        )}
      </div>
      <KeyHints hints={["enter play", "x remove", "J K move", "esc close"]} />
    </aside>
  );
}

function Row({ track }: { track: TrackInfo }) {
  const source = sourceOf(track.uri);
  return (
    <div className="row">
      <span className="name">
        <b>{track.name}</b> <span>{track.artists.join(", ")}</span>
      </span>
      {source && <SourceBadge source={source} />}
      <span className="time">
        {track.duration_ms > 0 ? clock(track.duration_ms) : "live"}
      </span>
    </div>
  );
}
