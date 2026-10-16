import strings from "./bindings/strings.json";
import { describe, expect, it, vi } from "vitest";
import {
  Connection,
  positionAfterChannel,
  positionAfterTick,
  showsOnboarding,
  takeLaunchCode,
  type Handlers,
  type State,
} from "./connection";

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
    payload: {
      version: "0",
      token,
      revisions: {},
      window: false,
      language: "en",
      strings: strings.EN,
    },
  });

describe("Connection", () => {
  it("drops an action sent before the hello", () => {
    const { connection, sockets } = harness("abc");
    connection.send({ type: "quit" });
    expect(sockets[0].sent).toEqual([]);
    sockets[0].handlers.message(hello("t1"));
    connection.send({ type: "quit" });
    expect(sockets[0].sent).toEqual([JSON.stringify({ type: "quit" })]);
  });

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

  it("a launch with neither a code nor a token is expired at once", () => {
    const { connection, sockets } = harness(null);
    expect(sockets).toHaveLength(0);
    expect(connection.getState().expired).toBe(true);
  });

  it("a refused token after a drop leaves the page expired", () => {
    const { connection, sockets, retries } = harness("abc");
    sockets[0].handlers.message(hello("t1"));
    sockets[0].handlers.close();
    retries[0]();
    sockets[1].handlers.close();
    expect(retries).toHaveLength(1);
    expect(connection.getState().expired).toBe(true);
  });

  it("a resumed connection keeps the store and stays booted", () => {
    const { connection, sockets, retries } = harness("abc");
    sockets[0].handlers.message(hello("t1"));
    sockets[0].handlers.message(
      JSON.stringify({ kind: "route", rev: 5, payload: "queue" }),
    );
    sockets[0].handlers.close();
    retries[0]();
    sockets[1].handlers.message(hello(null));
    const state = connection.getState();
    expect(state.booted).toBe(true);
    expect(state.channels.route?.payload).toBe("queue");
  });

  it("keeps the running version from hello", () => {
    const { connection, sockets } = harness("abc");
    expect(connection.getState().version).toBeNull();
    sockets[0].handlers.message(hello("t1"));
    expect(connection.getState().version).toBe("0");
  });

  it("receives the preboot table in Hello and ignores stale language revisions", () => {
    const { connection, sockets } = harness("abc");
    const receive = (message: object) =>
      sockets[0].handlers.message(JSON.stringify(message));
    receive({
      kind: "hello",
      payload: {
        ...JSON.parse(hello("t1")).payload,
        window: true,
        language: "zh-Hant",
        strings: strings.ZH_HANT,
      },
    });
    expect(connection.getState().window).toBe(true);
    expect(connection.getState().booted).toBe(false);
    expect(
      connection.getState().localization?.strings.gui.continue_button,
    ).toBe(strings.ZH_HANT.gui.continue_button);
    receive({
      kind: "settings",
      rev: 3,
      payload: { language: "zh-Hans", strings: strings.ZH_HANS, rows: [] },
    });
    receive({
      kind: "settings",
      rev: 2,
      payload: { language: "en", strings: strings.EN, rows: [] },
    });
    const active = connection.getState().localization;
    expect(active?.language).toBe("zh-Hans");
    expect(active?.strings.gui.settings).toBe(strings.ZH_HANS.gui.settings);
    receive({
      kind: "settings",
      rev: 4,
      payload: { language: "zh-Hans", strings: null, rows: [] },
    });
    expect(connection.getState().localization).toBe(active);
  });

  it("the first route message marks the app booted", () => {
    const { connection, sockets } = harness("abc");
    sockets[0].handlers.message(hello("t1"));
    expect(connection.getState().booted).toBe(false);
    sockets[0].handlers.message(
      JSON.stringify({ kind: "route", rev: 1, payload: "home" }),
    );
    expect(connection.getState().booted).toBe(true);
  });

  it("an onboarding message replaces the questions on show", () => {
    const { connection, sockets } = harness("abc");
    const view = { transcript: "Hi\n", pending: null };
    sockets[0].handlers.message(
      JSON.stringify({ kind: "onboarding", payload: view }),
    );
    expect(connection.getState().onboarding).toEqual(view);
  });

  it("a tick records the position and when it arrived", () => {
    const { connection, sockets } = harness("abc");
    sockets[0].handlers.message(hello("t1"));
    sockets[0].handlers.message(
      JSON.stringify({ kind: "tick", payload: 1234 }),
    );
    expect(connection.getState().position).toEqual({ ms: 1234, at: 42 });
  });

  it("a new item after a blank playback update starts at zero", () => {
    const { connection, sockets } = harness("abc");
    const playback = (rev: number, uri: string | null) =>
      JSON.stringify({
        kind: "playback",
        rev,
        payload: {
          item: uri === null ? null : { uri, is_playing: false },
          volume: 50,
          device: null,
          liked: false,
        },
      });
    sockets[0].handlers.message(hello("t1"));
    sockets[0].handlers.message(playback(1, "spotify:track:a"));
    sockets[0].handlers.message(
      JSON.stringify({ kind: "tick", payload: 103_000 }),
    );
    sockets[0].handlers.message(playback(2, null));
    expect(connection.getState().position).toEqual({ ms: 103_000, at: 42 });
    sockets[0].handlers.message(playback(3, "spotify:track:b"));
    expect(connection.getState().position).toEqual({ ms: 0, at: 42 });
  });

  it("an action goes out as a typed client message", () => {
    const { connection, sockets } = harness("abc");
    sockets[0].handlers.message(hello("t1"));
    connection.send({ type: "action", action: "TogglePlayback" });
    expect(sockets[0].sent).toEqual([
      '{"type":"action","action":"TogglePlayback"}',
    ]);
  });
});

describe("positionAfterTick", () => {
  const anchor = { ms: 10_000, at: 0 };

  it("holds the shown position when a tick lands a little behind it", () => {
    expect(positionAfterTick(anchor, 10_800, true, 1_000)).toEqual({
      ms: 11_000,
      at: 1_200,
    });
  });

  it("takes a tick ahead of the shown position or a jump back as it is", () => {
    expect(positionAfterTick(anchor, 11_300, true, 1_000)).toEqual({
      ms: 11_300,
      at: 1_000,
    });
    expect(positionAfterTick(anchor, 5_000, true, 1_000)).toEqual({
      ms: 5_000,
      at: 1_000,
    });
  });

  it("takes a paused tick as it is, so a small seek while paused shows", () => {
    expect(positionAfterTick(anchor, 9_900, false, 1_000)).toEqual({
      ms: 9_900,
      at: 1_000,
    });
  });
});

describe("positionAfterChannel", () => {
  const song = (playing: boolean) => ({ uri: "spotify:track:a", playing });

  it("a pause freezes the position where it shows", () => {
    expect(
      positionAfterChannel({ ms: 10_000, at: 0 }, song(true), song(false), 200),
    ).toEqual({ ms: 10_200, at: 200 });
  });

  it("a resume counts on from the paused position from now", () => {
    expect(
      positionAfterChannel(
        { ms: 10_200, at: 200 },
        song(false),
        song(true),
        5_000,
      ),
    ).toEqual({ ms: 10_200, at: 5_000 });
  });

  it("a new item starts at zero instead of the last item's position", () => {
    expect(
      positionAfterChannel(
        { ms: 103_000, at: 0 },
        song(true),
        { uri: "spotify:track:b", playing: true },
        200,
      ),
    ).toEqual({ ms: 0, at: 200 });
  });

  it("an item that comes back after a blank update keeps its position", () => {
    const anchor = { ms: 10_000, at: 0 };
    expect(
      positionAfterChannel(
        anchor,
        { uri: null, playing: true },
        song(true),
        200,
      ),
    ).toBe(anchor);
  });

  it("an update that changes neither keeps the anchor", () => {
    const anchor = { ms: 10_000, at: 0 };
    expect(positionAfterChannel(anchor, song(true), song(true), 200)).toBe(
      anchor,
    );
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

describe("showsOnboarding", () => {
  const base: State = {
    localization: null,
    connected: true,
    expired: false,
    channels: {},
    position: null,
    booted: false,
    onboarding: null,
    version: null,
    window: false,
  };
  const view = { transcript: "", pending: null };

  it("shows the questions until the app has booted", () => {
    expect(showsOnboarding({ ...base, onboarding: view })).toBe(true);
  });

  it("hands over to the player once the app has booted", () => {
    expect(showsOnboarding({ ...base, onboarding: view, booted: true })).toBe(
      false,
    );
  });

  it("shows a question asked after boot", () => {
    const pending = {
      seq: 3,
      ask: { kind: "Line" as const, prompt: "?", masked: false },
    };
    expect(
      showsOnboarding({
        ...base,
        onboarding: { transcript: "", pending },
        booted: true,
      }),
    ).toBe(true);
  });

  it("shows nothing before the first onboarding message", () => {
    expect(showsOnboarding(base)).toBe(false);
  });
});
