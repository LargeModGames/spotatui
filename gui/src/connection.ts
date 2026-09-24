import type { ClientMessage } from "./bindings/ClientMessage";
import type { OnboardingView } from "./bindings/OnboardingView";
import type { ServerMessage } from "./bindings/ServerMessage";
import { applyChannel, type Channels } from "./store";

const TOKEN_KEY = "spotatui.token";
const RECONNECT_MS = 1000;

export interface Handlers {
  message(text: string): void;
  close(): void;
}

export interface Socket {
  send(text: string): void;
}

export type Open = (protocol: string, handlers: Handlers) => Socket;

/** The last position the server reported, and when it arrived (a `performance.now()` value). */
export interface Position {
  ms: number | null;
  at: number;
}

export interface State {
  connected: boolean;
  expired: boolean;
  channels: Channels;
  position: Position | null;
  /** The first-launch questions; the page shows them until the app has booted. */
  onboarding: OnboardingView | null;
}

/** Reads the one-time launch code from the fragment and removes it from the address bar. */
export function takeLaunchCode(
  location: Pick<Location, "hash" | "pathname">,
  history: Pick<History, "replaceState">,
): string | null {
  const code = new URLSearchParams(location.hash.slice(1)).get("code");
  if (code) history.replaceState(null, "", location.pathname);
  return code;
}

/** Whether the page shows the first-launch questions: a question is open, or the app has not booted. */
export function showsOnboarding(state: State): boolean {
  return (
    state.onboarding !== null &&
    (state.onboarding.pending !== null || state.channels.route === undefined)
  );
}

export class Connection {
  private state: State = {
    connected: false,
    expired: false,
    channels: {},
    position: null,
    onboarding: null,
  };
  private socket: Socket | null = null;
  private readonly listeners = new Set<() => void>();
  private code: string | null;
  private readonly open: Open;
  private readonly storage: Pick<Storage, "getItem" | "setItem">;
  private readonly schedule: (retry: () => void, ms: number) => void;
  private readonly now: () => number;

  constructor(
    code: string | null,
    open: Open,
    storage: Pick<Storage, "getItem" | "setItem">,
    schedule: (retry: () => void, ms: number) => void,
    now: () => number = () => performance.now(),
  ) {
    this.code = code;
    this.open = open;
    this.storage = storage;
    this.schedule = schedule;
    this.now = now;
  }

  start(): void {
    const token = this.storage.getItem(TOKEN_KEY);
    const protocol = this.code
      ? `spotatui.code.${this.code}`
      : token
        ? `spotatui.token.${token}`
        : null;
    // The launch code opens one socket only; every later connect uses the session token.
    this.code = null;
    if (!protocol) return this.update({ expired: true });
    this.socket = this.open(protocol, {
      message: (text) => this.receive(text),
      close: () => this.closed(),
    });
  }

  send(message: ClientMessage): void {
    this.socket?.send(JSON.stringify(message));
  }

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  getState = (): State => this.state;

  private receive(text: string): void {
    const message = JSON.parse(text) as ServerMessage;
    switch (message.kind) {
      case "hello":
        if (message.payload.token)
          this.storage.setItem(TOKEN_KEY, message.payload.token);
        // Every channel follows the hello, so the store starts over.
        return this.update({ connected: true, channels: {} });
      case "onboarding":
        return this.update({ onboarding: message.payload });
      case "tick":
        return this.update({
          position: { ms: message.payload, at: this.now() },
        });
      default:
        return this.update({
          channels: applyChannel(this.state.channels, message),
        });
    }
  }

  /** A drop after hello retries; a close before hello (refused, or spotatui has exited) ends the page. */
  private closed(): void {
    this.socket = null;
    if (!this.state.connected) return this.update({ expired: true });
    this.update({ connected: false });
    this.schedule(() => this.start(), RECONNECT_MS);
  }

  private update(patch: Partial<State>): void {
    this.state = { ...this.state, ...patch };
    this.listeners.forEach((listener) => listener());
  }
}
