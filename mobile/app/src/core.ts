import { Connection } from '@gui/connection';

import SpotatuiCore from '../modules/spotatui-core/src/SpotatuiCoreModule';

// The session token dies with the core process, so memory is enough.
const memory = new Map<string, string>();
const storage = {
  getItem: (key: string) => memory.get(key) ?? null,
  setItem: (key: string, value: string) => void memory.set(key, value),
};

/** Starts the in-process core and connects to it the way the browser page does. */
export async function connect(): Promise<Connection> {
  const url = await SpotatuiCore.start();
  const [, port, code] = /:(\d+)\/#code=(.+)$/.exec(url) ?? [];
  const connection = new Connection(
    code ?? null,
    (protocol, handlers) => {
      // Exactly one protocol: the server reads the whole header as the code or the token.
      const socket = new WebSocket(`ws://127.0.0.1:${port}/ws`, protocol);
      socket.onmessage = (event) => handlers.message(String(event.data));
      socket.onclose = () => handlers.close();
      return socket;
    },
    storage,
    (retry, ms) => setTimeout(retry, ms),
  );
  connection.start();
  return connection;
}
