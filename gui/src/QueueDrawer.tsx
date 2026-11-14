import { EN, type GuiStrings } from "./i18n";
import { useId, useLayoutEffect, useRef, useState } from "react";
import type { Action } from "./bindings/Action";
import type { NowPlaying } from "./bindings/NowPlaying";
import type { QueuePayload } from "./bindings/QueuePayload";
import { clock, sourceOf } from "./format";
import { Glyph } from "./Icon";
import { KeyHints } from "./KeyHints";
import {
  itemLine,
  type QueueCursor,
  queueKey,
  type QueueLine,
  queueRow,
  trackLine,
} from "./queueModel";
import { SourceBadge } from "./SourceBadge";
import { Cover } from "./ui";

/** The docked queue panel: the native queue can be played, moved and removed; Spotify's is read-only. */
export function QueueDrawer({
  s = EN,
  queue,
  item,
  send,
  onClose,
  grab = true,
}: {
  s?: GuiStrings;
  queue: QueuePayload | null;
  item: NowPlaying | null;
  send: (action: Action) => void;
  onClose: () => void;
  /** Take the keyboard on mount; false for a queue that was open at launch. */
  grab?: boolean;
}) {
  const ids = useId();
  const native = queue?.native ?? [];
  // The Web API queue mirrors Spotify playback only; under another source it is stale.
  const spotify = sourceOf(item?.uri ?? null) === "Spotify";
  const mirrored = spotify
    ? (queue?.spotify.items ?? []).flatMap((entry) => {
        const line = itemLine(entry, s);
        return line ? [line] : [];
      })
    : [];
  const [want, setWant] = useState<QueueCursor>({ index: 0, uri: null });
  const cursor = queueRow(native, want);

  const list = useRef<HTMLOListElement>(null);
  const panel = useRef<HTMLElement>(null);
  // The panel takes the keyboard when it opens, and hands it to the list once the queue arrives,
  // but never pulls it away from another part of the page.
  const hasList = native.length > 0;
  const opened = useRef(!grab);
  useLayoutEffect(() => {
    if (!opened.current || document.activeElement === panel.current)
      (list.current ?? panel.current)?.focus();
    opened.current = true;
  }, [hasList]);
  useLayoutEffect(() => {
    list.current
      ?.querySelector('[aria-selected="true"]')
      ?.scrollIntoView({ block: "nearest" });
  }, [cursor]);

  const now = queue?.now ?? null;
  return (
    <section
      ref={panel}
      className="queue-panel"
      aria-label={s.queue}
      tabIndex={-1}
      onKeyDown={(event) => {
        if (event.ctrlKey || event.metaKey || event.altKey) return;
        if (event.key === "Escape" || event.key === "Q") {
          event.preventDefault();
          onClose();
        }
      }}
    >
      <header className="panel-head">
        <h2>{s.queue}</h2>
        <button
          type="button"
          className="panel-close"
          aria-label={s.close_queue}
          onClick={onClose}
        >
          <Glyph name="close" />
        </button>
      </header>
      <div className="panel-body">
        <h3>{s.now_playing}</h3>
        {now ? (
          <Row s={s} line={trackLine(now)} />
        ) : item ? (
          <div className="queue-row">
            <Cover src={item.image_url} size={40} />
            <span className="queue-text">
              <strong>{item.title}</strong>
              <span>{item.artists.join(", ")}</span>
            </span>
          </div>
        ) : (
          <p className="panel-empty">{s.nothing_is_playing}</p>
        )}
        <h3>{s.next_in_queue}</h3>
        {native.length === 0 ? (
          <p className="panel-empty">{s.nothing_queued}</p>
        ) : (
          <ol
            ref={list}
            role="listbox"
            aria-label={s.next_in_queue}
            aria-activedescendant={`${ids}queue-${cursor}`}
            tabIndex={0}
            data-focus
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
                id={`${ids}queue-${index}`}
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
                <Row s={s} line={trackLine(track)} />
              </li>
            ))}
          </ol>
        )}
        {mirrored.length > 0 && (
          <>
            <h3>{s.next_from_spotify}</h3>
            <ol className="mirror" aria-label={s.next_from_spotify_label}>
              {mirrored.map((line, index) => (
                <li key={`${index}-${line.uri ?? line.title}`}>
                  <Row s={s} line={line} />
                </li>
              ))}
            </ol>
          </>
        )}
      </div>
      <KeyHints
        hints={[s.hint_play, s.hint_remove, s.hint_reorder_move, s.hint_close]}
      />
    </section>
  );
}

function Row({ s = EN, line }: { s?: GuiStrings; line: QueueLine }) {
  const source = sourceOf(line.uri);
  return (
    <div className="queue-row">
      <Cover src={line.image} size={40} />
      <span className="queue-text">
        <strong>{line.title}</strong>
        <span>{line.subtitle}</span>
      </span>
      {source && source !== "Spotify" && <SourceBadge s={s} source={source} />}
      <span className="time">
        {line.duration_ms > 0 ? clock(line.duration_ms) : s.live}
      </span>
    </div>
  );
}
