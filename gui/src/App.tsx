import { useMemo, useSyncExternalStore } from "react";
import type { Action } from "./bindings/Action";
import { showsOnboarding, type Connection } from "./connection";
import { Onboarding } from "./Onboarding";
import { Shell } from "./Shell";

/** The page: the first-launch questions until boot, then the shell. */
export function App({ connection }: { connection: Connection }) {
  const state = useSyncExternalStore(connection.subscribe, connection.getState);
  // One function for the whole session, so a tick does not re-render the memoized lists.
  const send = useMemo(
    () => (action: Action) => connection.send({ type: "action", action }),
    [connection],
  );

  if (state.expired)
    return (
      <>
        {state.onboarding && !state.booted && (
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

  return <Shell state={state} send={send} />;
}
