import { describe, expect, it, vi } from "vitest";
import { Connection, takeLaunchCode, type Handlers } from "./connection";

function harness(code: string | null, stored: Record<string, string> = {}) {
  const sockets: { protocol: string; handlers: Handlers; sent: string[] }[] =
    [];
  const storage = new Map(Object.entries(stored));
  const retries: (() => void)[] = [];
  const connection = new Connection(
    code,
    (protocol, handlers) => {
      const socket = { protocol, handlers, sent: [] as string[] };
      sockets.push(socket);
      return { send: (text: string) => socket.sent.push(text) };
    },
    {
      getItem: (key) => storage.get(key) ?? null,
      setItem: (key, value) => {
        storage.set(key, value);
      },
    },
    (retry) => {
      retries.push(retry);
    },
    () => 42,
  );
  connection.start();
  return { connection, sockets, storage, retries };
}

const hello = (token: string | null) =>
  JSON.stringify({
    kind: "hello",
    payload: { version: "0", token, revisions: {} },
  });

describe("Connection", () => {
  it("connects with the launch code and keeps the token from hello", () => {
    const { connection, sockets, storage } = harness("abc");
    sockets[0].handlers.message(hello("t1"));
    expect(sockets[0].protocol).toBe("spotatui.code.abc");
    expect(storage.get("spotatui.token")).toBe("t1");
    expect(connection.getState().connected).toBe(true);
  });

  it("reconnects with the stored token and never reuses the code", () => {
    const { sockets, retries } = harness("abc");
    sockets[0].handlers.message(hello("t1"));
    sockets[0].handlers.close();
    retries[0]();
    expect(sockets[1].protocol).toBe("spotatui.token.t1");
  });

  it("a reload without a code connects with the stored token", () => {
    const { sockets } = harness(null, { "spotatui.token": "t0" });
    expect(sockets[0].protocol).toBe("spotatui.token.t0");
  });

  it("a close before hello leaves the page expired without a retry", () => {
    const { connection, sockets, retries } = harness("abc");
    sockets[0].handlers.close();
    expect(retries).toHaveLength(0);
    expect(connection.getState().expired).toBe(true);
  });

  it("a new connection starts from an empty store", () => {
    const { connection, sockets, retries } = harness("abc");
    sockets[0].handlers.message(hello("t1"));
    sockets[0].handlers.message(
      JSON.stringify({ kind: "route", rev: 5, payload: "queue" }),
    );
    sockets[0].handlers.close();
    retries[0]();
    sockets[1].handlers.message(hello(null));
    sockets[1].handlers.message(
      JSON.stringify({ kind: "route", rev: 1, payload: "home" }),
    );
    expect(connection.getState().channels.route?.payload).toBe("home");
  });

  it("a tick records the position and when it arrived", () => {
    const { connection, sockets } = harness("abc");
    sockets[0].handlers.message(hello("t1"));
    sockets[0].handlers.message(
      JSON.stringify({ kind: "tick", payload: 1234 }),
    );
    expect(connection.getState().position).toEqual({ ms: 1234, at: 42 });
  });

  it("an action goes out as a typed client message", () => {
    const { connection, sockets } = harness("abc");
    connection.send({ type: "action", action: "TogglePlayback" });
    expect(sockets[0].sent).toEqual([
      '{"type":"action","action":"TogglePlayback"}',
    ]);
  });
});

describe("takeLaunchCode", () => {
  it("reads the code from the fragment and removes it from the address bar", () => {
    const replaceState = vi.fn();
    expect(
      takeLaunchCode({ hash: "#code=xyz", pathname: "/" }, { replaceState }),
    ).toBe("xyz");
    expect(replaceState).toHaveBeenCalledWith(null, "", "/");
  });
});
