import { createContext, useCallback, useContext, useSyncExternalStore, type ReactNode } from 'react';

import type { Action } from '@gui/bindings/Action';
import type { Connection, State } from '@gui/connection';

const CoreContext = createContext<Connection | null>(null);

export function CoreProvider({ connection, children }: { connection: Connection; children: ReactNode }) {
  return <CoreContext.Provider value={connection}>{children}</CoreContext.Provider>;
}

function useConnection(): Connection {
  const connection = useContext(CoreContext);
  if (!connection) throw new Error('useCore outside CoreProvider');
  return connection;
}

/** Everything the core pushed so far. */
export function useCore(): State {
  const connection = useConnection();
  return useSyncExternalStore(connection.subscribe, connection.getState);
}

/** Sends one action to the core. */
export function useSend(): (action: Action) => void {
  const connection = useConnection();
  return useCallback((action: Action) => connection.send({ type: 'action', action }), [connection]);
}

export function useConnectionSend(): Connection['send'] {
  const connection = useConnection();
  return useCallback<Connection['send']>((message) => connection.send(message), [connection]);
}
