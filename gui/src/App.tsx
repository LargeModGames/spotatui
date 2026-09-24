import { useEffect, useState, useSyncExternalStore } from "react";
import type { Action } from "./bindings/Action";
import type { NowPlaying as Item } from "./bindings/NowPlaying";
import type { TrackInfo } from "./bindings/TrackInfo";
import type { Connection, Position } from "./connection";

export function App({ connection }: { connection: Connection }) {
  const state = useSyncExternalStore(connection.subscribe, connection.getState);
  const theme = state.channels.theme?.payload;

  useEffect(() => {
    const style = document.documentElement.style;
    const fields = Object.entries(theme ?? {});
    for (const [field, rgb] of fields) {
      if (rgb) style.setProperty(`--${field}`, `rgb(${rgb.join(" ")})`);
    }
    // A field back at Reset falls back to the page default.
    return () => {
      for (const [field] of fields) style.removeProperty(`--${field}`);
    };
  }, [theme]);

  if (state.expired)
    return (
      <p className="notice">This page is no longer connected to spotatui.</p>
    );

  const send = (action: Action) => connection.send({ type: "action", action });
  const queue = state.channels.queue?.payload;
  const item = state.channels.playback?.payload.item ?? null;
  return (
    <main>
      <Queue
        tracks={[...(queue?.now ? [queue.now] : []), ...(queue?.native ?? [])]}
      />
      <NowPlaying
        item={item}
        position={state.position}
        connected={state.connected}
        send={send}
      />
    </main>
  );
}

function Queue({ tracks }: { tracks: TrackInfo[] }) {
  return (
    <ol className="queue">
      {tracks.map((track, index) => (
        <li key={`${index}-${track.uri ?? track.name}`}>
          <span>{track.name}</span>{" "}
          <span className="hint">{track.artists.join(", ")}</span>
        </li>
      ))}
    </ol>
  );
}

function NowPlaying({
  item,
  position,
  connected,
  send,
}: {
  item: Item | null;
  position: Position | null;
  connected: boolean;
  send: (action: Action) => void;
}) {
  const playing = item?.is_playing ?? false;
  const ms = usePosition(position, playing);
  return (
    <footer className="bar">
      {item?.image_url && (
        <img src={item.image_url} alt="" width={56} height={56} />
      )}
      <div className="meta">
        <strong>{item?.title ?? "Nothing playing"}</strong>
        <span className="hint">{item?.artists.join(", ")}</span>
        <progress
          max={item?.duration_ms || 1}
          value={Math.min(ms, item?.duration_ms ?? 0)}
        />
      </div>
      <button disabled={!connected} onClick={() => send("PreviousTrack")}>
        Previous
      </button>
      <button disabled={!connected} onClick={() => send("TogglePlayback")}>
        {playing ? "Pause" : "Play"}
      </button>
      <button disabled={!connected} onClick={() => send("NextTrack")}>
        Next
      </button>
    </footer>
  );
}

/** The position interpolated per animation frame from the last tick while playing. */
function usePosition(position: Position | null, playing: boolean): number {
  const [now, setNow] = useState(() => performance.now());
  useEffect(() => {
    if (!playing) return;
    let frame = requestAnimationFrame(function step(time) {
      setNow(time);
      frame = requestAnimationFrame(step);
    });
    return () => cancelAnimationFrame(frame);
  }, [playing]);
  if (!position || position.ms === null) return 0;
  return playing ? position.ms + Math.max(0, now - position.at) : position.ms;
}
