import { Connection } from '@gui/connection';

import SpotatuiCore from '../modules/spotatui-core/src/SpotatuiCoreModule';

const USED_CODE_KEY = 'spotatui.used-code';

/** Starts the in-process core and connects to it the way the browser page does. */
export async function connect(): Promise<Connection> {
  const url = await SpotatuiCore.start();
  const [, port, code] = /:(\d+)\/#code=(.+)$/.exec(url) ?? [];
  // The core returns its first URL again after a JavaScript reload; its code works
  // only once, so a later connect resumes with the session token.
  const fresh = code && SpotatuiCore.getItem(USED_CODE_KEY) !== code ? code : null;
  if (fresh) SpotatuiCore.setItem(USED_CODE_KEY, fresh);
  const connection = new Connection(
    fresh,
    (protocol, handlers) => {
      // Exactly one protocol: the server reads the whole header as the code or the token.
      const socket = new WebSocket(`ws://127.0.0.1:${port}/ws`, protocol);
      socket.onmessage = (event) => handlers.message(String(event.data));
      socket.onclose = () => handlers.close();
      return socket;
    },
    {
      getItem: (key) => SpotatuiCore.getItem(key),
      setItem: (key, value) => SpotatuiCore.setItem(key, value),
    },
    (retry, ms) => setTimeout(retry, ms),
  );
  connection.start();
  return connection;
}
