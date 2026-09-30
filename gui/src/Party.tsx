import { useState, type KeyboardEvent } from "react";
import type { Action } from "./bindings/Action";
import type { NowPlaying } from "./bindings/NowPlaying";
import type { PartyPayload } from "./bindings/PartyPayload";
import type { PartyRoom } from "./bindings/PartyRoom";
import type { QueuePayload } from "./bindings/QueuePayload";
import type { Position } from "./connection";
import { clock, sourceOf } from "./format";
import { KeyHints } from "./KeyHints";
import "./Party.css";
import {
  CODE_LENGTH,
  headline,
  joinRequest,
  listeners,
  normalizeCode,
  normalizeName,
  relaying,
} from "./partyModel";
import { type QueueCursor, queueKey, queueRow } from "./queueModel";
import { usePosition } from "./usePosition";

/** The listening party: host or join, then the room with the listeners and the host's queue. */
export function Party({
  party,
  item,
  position,
  queue,
  send,
}: {
  party: PartyPayload | null;
  item: NowPlaying | null;
  position: Position | null;
  queue: QueuePayload | null;
  send: (action: Action) => void;
}) {
  const room = party?.room ?? null;
  if (!party || !room) return <Start party={party} send={send} />;
  return (
    <Room
      party={party}
      room={room}
      item={item}
      position={position}
      queue={queue}
      send={send}
    />
  );
}

function Start({
  party,
  send,
}: {
  party: PartyPayload | null;
  send: (action: Action) => void;
}) {
  const [code, setCode] = useState("");
  const [name, setName] = useState("");
  const available = party?.available ?? false;
  const connecting = party?.phase === "connecting";
  const join = joinRequest(code, name);
  const disabled = !available || connecting;
  return (
    <div className="party" data-focus tabIndex={-1}>
      <div className="party-head">
        <h1>Listening party</h1>
        <span>
          {connecting
            ? "Connecting…"
            : "Listen together: everyone plays the same Spotify song at the same moment."}
        </span>
      </div>
      {!available && (
        <p className="notice-line">
          A listening party needs a Spotify session.
        </p>
      )}
      <div className="party-start">
        <section aria-label="Host a party">
          <span className="eyebrow">HOST</span>
          <h2>Start a party</h2>
          <p>You get a code to share. Guests hear what you play on Spotify.</p>
          <button
            type="button"
            className="primary"
            disabled={disabled}
            onClick={() => send("StartParty")}
          >
            Start a party
          </button>
        </section>
        <section aria-label="Join a party">
          <span className="eyebrow">JOIN</span>
          <h2>Join a party</h2>
          <form
            onSubmit={(event) => {
              event.preventDefault();
              if (join && !disabled) send(join);
            }}
          >
            <label>
              <span>Code</span>
              <input
                type="text"
                value={code}
                maxLength={CODE_LENGTH}
                placeholder="ABC123"
                autoComplete="off"
                disabled={disabled}
                onChange={(event) => setCode(normalizeCode(event.target.value))}
              />
            </label>
            <label>
              <span>Your name</span>
              <input
                type="text"
                value={name}
                placeholder="Sam"
                disabled={disabled}
                onChange={(event) => setName(normalizeName(event.target.value))}
              />
            </label>
            <button type="submit" disabled={disabled || !join}>
              Join
            </button>
          </form>
        </section>
      </div>
    </div>
  );
}

function Room({
  party,
  room,
  item,
  position,
  queue,
  send,
}: {
  party: PartyPayload;
  room: PartyRoom;
  item: NowPlaying | null;
  position: Position | null;
  queue: QueuePayload | null;
  send: (action: Action) => void;
}) {
  const [confirming, setConfirming] = useState(false);
  const [want, setWant] = useState<QueueCursor>({ index: 0, uri: null });
  const playingUri = item?.uri ?? null;
  const live = relaying(room, queue?.now != null, playingUri);
  const native = queue?.native ?? [];
  const mirror =
    sourceOf(playingUri) === "Spotify"
      ? (queue?.spotify.items ?? []).flatMap((entry) =>
          entry.track ? [entry.track] : [],
        )
      : [];
  const cursor = queueRow(native, want);
  const elapsed = usePosition(position, item?.is_playing ?? false);

  const copy = () => {
    if (!room.code) return;
    const failed = () =>
      send({ NotifyError: ["Could not copy the party code.", 4] });
    const write = navigator.clipboard?.writeText(room.code);
    if (!write) return failed();
    write.then(() => send({ Notify: ["Copied the party code.", 3] }), failed);
  };
  const leave = () => {
    if (room.host && !confirming) setConfirming(true);
    else send("LeaveParty");
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (
      event.defaultPrevented ||
      event.ctrlKey ||
      event.metaKey ||
      event.altKey
    )
      return;
    if (event.target instanceof HTMLInputElement) return;
    if (event.key === "y") copy();
    else if (event.key === "L") leave();
    else if (event.key === "c" && room.host) send("TogglePartyControlMode");
    else if (event.key === "Escape" && confirming) setConfirming(false);
    else return;
    event.preventDefault();
  };

  return (
    <div className="party" tabIndex={-1} data-focus onKeyDown={onKeyDown}>
      <div className="party-head">
        <h1>Listening party</h1>
        <span>{headline(party)}</span>
        {room.code ? (
          <button type="button" className="code" onClick={copy}>
            {room.code} <kbd>y copy</kbd>
          </button>
        ) : (
          <span className="code">Getting a code…</span>
        )}
        {confirming ? (
          <span className="confirm" role="group" aria-label="End the party">
            End it for everyone?
            <button type="button" className="danger" onClick={leave}>
              End party
            </button>
            <button type="button" onClick={() => setConfirming(false)}>
              Keep it
            </button>
          </span>
        ) : (
          <button type="button" onClick={leave}>
            {room.host ? "End party" : "Leave party"}
          </button>
        )}
      </div>
      <div className="party-room">
        <section aria-label="Everyone hears">
          <span className="eyebrow">EVERYONE HEARS</span>
          <div className="hears">
            {item?.image_url ? (
              <img className="cover big" src={item.image_url} alt="" />
            ) : (
              <span className="cover big" />
            )}
            <div>
              <span className="song">{item?.title ?? "Nothing playing"}</span>
              {item && (
                <span className="meta">
                  {[item.artists.join(", "), item.album]
                    .filter(Boolean)
                    .join(" · ")}{" "}
                  · {clock(Math.min(elapsed, item.duration_ms || elapsed))} /{" "}
                  {clock(item.duration_ms)}
                </span>
              )}
            </div>
          </div>
          {room.host && !live && (
            <p className="hint">
              Guests follow Spotify songs only. They keep the last one until you
              play Spotify again.
            </p>
          )}
          <span className="eyebrow rule">
            EACH LISTENER PLAYS THEIR OWN COPY
          </span>
          <ul className="listeners">
            {listeners(room).map((listener, index) => (
              <li key={`${index}-${listener.name}`}>
                <span
                  className="avatar-dot"
                  style={{ background: `hsl(${listener.hue} 45% 70%)` }}
                >
                  {listener.initial}
                </span>
                <b>{listener.name}</b>
                <span>{listener.role}</span>
              </li>
            ))}
          </ul>
          {room.host && (
            <label className="control">
              <input
                type="checkbox"
                checked={room.shared_control}
                onChange={() => send("TogglePartyControlMode")}
              />
              Guests can control playback <kbd>c</kbd>
            </label>
          )}
        </section>
        <section aria-label="Up next">
          <div className="section-head">
            <h2>Up next</h2>
            {room.host && <kbd>J K move · x remove</kbd>}
          </div>
          {!room.host ? (
            <p className="empty">The host picks the songs.</p>
          ) : native.length === 0 && mirror.length === 0 ? (
            <p className="empty">Nothing is queued.</p>
          ) : (
            <>
              <div className="qrow head eyebrow" aria-hidden="true">
                <span>#</span>
                <span>SONG</span>
                <span>GUESTS</span>
              </div>
              {native.length > 0 && (
                <ol
                  role="listbox"
                  aria-label="Your queue"
                  aria-activedescendant={`party-q-${cursor}`}
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
                      id={`party-q-${index}`}
                      role="option"
                      aria-selected={index === cursor}
                      className="qrow"
                      onClick={() => setWant({ index, uri: track.uri })}
                    >
                      <span className="n">{index + 1}</span>
                      <span className="name">
                        <b>{track.name}</b>{" "}
                        <span>{track.artists.join(", ")}</span>
                      </span>
                      <span className="only">only you</span>
                    </li>
                  ))}
                </ol>
              )}
              <ol className="mirror">
                {mirror.map((track, index) => (
                  <li
                    key={`${index}-${track.uri ?? track.name}`}
                    className="qrow"
                  >
                    <span className="n">{native.length + index + 1}</span>
                    <span className="name">
                      <b>{track.name}</b>{" "}
                      <span>{track.artists.join(", ")}</span>
                    </span>
                    <span>guests follow</span>
                  </li>
                ))}
              </ol>
            </>
          )}
        </section>
      </div>
      <KeyHints
        hints={
          room.host
            ? ["y copy code", "c guest control", "L end party"]
            : ["y copy code", "L leave"]
        }
      />
    </div>
  );
}
