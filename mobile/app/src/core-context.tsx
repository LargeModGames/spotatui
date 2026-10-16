import { createContext, useCallback, useContext, useSyncExternalStore, type ReactNode } from 'react';

import type { Action } from '@gui/bindings/Action';
import type { Connection, State } from '@gui/connection';
import type { ChannelMessage, Kind } from '@gui/store';

const CoreContext = createContext<Connection | null>(null);

type ChannelPayload<K extends Kind> = Extract<ChannelMessage, { kind: K }>['payload'];

export function CoreProvider({ connection, children }: { connection: Connection; children: ReactNode }) {
  return <CoreContext.Provider value={connection}>{children}</CoreContext.Provider>;
}

export function useConnection(): Connection {
  const connection = useContext(CoreContext);
  if (!connection) throw new Error('useConnection outside CoreProvider');
  return connection;
}

/** One slice of the pushed state; `select` returns a primitive or an object already in the state, never a new one. */
export function useCoreSelect<T>(select: (state: State) => T): T {
  const connection = useConnection();
  return useSyncExternalStore(connection.subscribe, () => select(connection.getState()));
}

/** One channel's payload, undefined until the core pushed it. */
export function useChannel<K extends Kind>(kind: K): ChannelPayload<K> | undefined {
  return useCoreSelect((state) => state.channels[kind]?.payload as ChannelPayload<K> | undefined);
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
