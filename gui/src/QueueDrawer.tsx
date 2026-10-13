import { useLayoutEffect, useRef, useState } from "react";
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
    ? (queue?.spotify.items ?? []).flatMap((entry) => {
        const line = itemLine(entry);
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
  const opened = useRef(false);
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
      <header className="panel-head">
        <h2>Queue</h2>
        <button
          type="button"
          className="panel-close"
          aria-label="Close queue"
          onClick={onClose}
        >
          <Glyph name="close" />
        </button>
      </header>
      <div className="panel-body">
        <h3>Now playing</h3>
        {now ? (
          <Row line={trackLine(now)} />
        ) : item ? (
          <div className="queue-row">
            <Cover src={item.image_url} size={40} />
            <span className="queue-text">
              <strong>{item.title}</strong>
              <span>{item.artists.join(", ")}</span>
            </span>
          </div>
        ) : (
          <p className="panel-empty">Nothing is playing.</p>
        )}
        <h3>Next in queue</h3>
        {native.length === 0 ? (
          <p className="panel-empty">Nothing is queued.</p>
        ) : (
          <ol
            ref={list}
            role="listbox"
            aria-label="Next in queue"
            aria-activedescendant={`queue-${cursor}`}
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
                <Row line={trackLine(track)} />
              </li>
            ))}
          </ol>
        )}
        {mirrored.length > 0 && (
          <>
            <h3>Next from: Spotify</h3>
            <ol className="mirror" aria-label="Next from Spotify">
              {mirrored.map((line, index) => (
                <li key={`${index}-${line.uri ?? line.title}`}>
                  <Row line={line} />
                </li>
              ))}
            </ol>
          </>
        )}
      </div>
      <KeyHints hints={["enter play", "x remove", "J K move", "esc close"]} />
    </section>
  );
}

function Row({ line }: { line: QueueLine }) {
  const source = sourceOf(line.uri);
  return (
    <div className="queue-row">
      <Cover src={line.image} size={40} />
      <span className="queue-text">
        <strong>{line.title}</strong>
        <span>{line.subtitle}</span>
      </span>
      {source && source !== "Spotify" && <SourceBadge source={source} />}
      <span className="time">
        {line.duration_ms > 0 ? clock(line.duration_ms) : "live"}
      </span>
    </div>
  );
}
