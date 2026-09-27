import { useMemo, useSyncExternalStore } from "react";
import type { Action } from "./bindings/Action";
import { showsOnboarding, type Connection } from "./connection";
import { upNext } from "./libraryModel";
import { Library } from "./Library";
import { Onboarding } from "./Onboarding";
import { PlayerBar } from "./PlayerBar";
import { TopBar } from "./TopBar";

/** The page: the top bar, the Library screen and the player bar. */
export function App({ connection }: { connection: Connection }) {
  const state = useSyncExternalStore(connection.subscribe, connection.getState);
  // One function for the whole session, so a tick does not re-render the memoized lists.
  const send = useMemo(
    () => (action: Action) => connection.send({ type: "action", action }),
    [connection],
  );

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

  const playback = state.channels.playback?.payload ?? null;
  const queued = upNext(state.channels.queue?.payload);
  return (
    <main className="shell">
      <TopBar
        device={playback?.device ?? null}
        connected={state.connected}
        queued={queued.length}
      />
      <Library
        playlists={state.channels.library?.payload ?? null}
        liked={state.channels.liked?.payload ?? null}
        source={state.channels.source?.payload ?? null}
        playingUri={playback?.item?.uri ?? null}
        upNext={queued}
        send={send}
      />
      <PlayerBar
        playback={playback}
        position={state.position}
        connected={state.connected}
        send={send}
      />
    </main>
  );
}
