import { useSyncExternalStore } from "react";
import type { Action } from "./bindings/Action";
import type { TrackInfo } from "./bindings/TrackInfo";
import { showsOnboarding, type Connection } from "./connection";
import { clock, sourceOf } from "./format";
import { Onboarding } from "./Onboarding";
import { PlayerBar } from "./PlayerBar";
import { SourceBadge } from "./SourceBadge";
import { TopBar } from "./TopBar";

/** The page: the top bar, the content area and the player bar. */
export function App({ connection }: { connection: Connection }) {
  const state = useSyncExternalStore(connection.subscribe, connection.getState);

  // The channels arrive once boot is done; before that the transcript is the page.
  const booted = state.channels.route !== undefined;
  if (state.expired)
    return (
      <>
        {state.onboarding && !booted && (
          <pre className="transcript">{state.onboarding.transcript}</pre>
        )}
        <p className="notice">This page is no longer connected to spotatui.</p>
      </>
    );

  if (state.onboarding && showsOnboarding(state))
    return (
      <Onboarding
        view={state.onboarding}
        connected={state.connected}
        onReply={(reply) => connection.send({ type: "onboarding", reply })}
      />
    );

  const send = (action: Action) => connection.send({ type: "action", action });
  const queue = state.channels.queue?.payload;
  const playback = state.channels.playback?.payload ?? null;
  const upNext = [
    ...(queue?.native ?? []),
    ...(queue?.spotify.items ?? []).flatMap((item) =>
      item.track ? [item.track] : [],
    ),
  ];
  return (
    <main className="shell">
      <TopBar
        device={playback?.device ?? null}
        connected={state.connected}
        queued={upNext.length}
      />
      <UpNext tracks={upNext} />
      <PlayerBar
        playback={playback}
        position={state.position}
        connected={state.connected}
        send={send}
      />
    </main>
  );
}

/** The queue after the playing track; the content area until the Library screen exists. */
function UpNext({ tracks }: { tracks: TrackInfo[] }) {
  return (
    <section className="upnext" aria-label="Up next">
      <h1>Up next</h1>
      {tracks.length === 0 ? (
        <p className="empty">Nothing is queued.</p>
      ) : (
        <ol>
          {tracks.map((track, index) => {
            const source = sourceOf(track.uri);
            return (
              <li key={`${index}-${track.uri ?? track.name}`}>
                <span className="n">{index + 1}</span>
                <span className="name">
                  <b>{track.name}</b> <span>{track.artists.join(", ")}</span>
                </span>
                <span>{source && <SourceBadge source={source} />}</span>
                <span className="time">
                  {track.duration_ms > 0 && clock(track.duration_ms)}
                </span>
              </li>
            );
          })}
        </ol>
      )}
    </section>
  );
}
