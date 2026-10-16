import type { HelloPayload } from "./bindings/HelloPayload";
import type { ClientMessage } from "./bindings/ClientMessage";
import type { OnboardingView } from "./bindings/OnboardingView";
import type { ServerMessage } from "./bindings/ServerMessage";
import { applyChannel, type Channels } from "./store";
import type { Frame } from "./visualizerModel";

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
  localization: Pick<HelloPayload, "language" | "strings"> | null;
  connected: boolean;
  expired: boolean;
  channels: Channels;
  position: Position | null;
  /** Set by the first route message, which the server sends only once boot is done. */
  booted: boolean;
  /** The first-launch questions; the page shows them until the app has booted. */
  onboarding: OnboardingView | null;
  /** The running spotatui version, from the hello. */
  version: string | null;
  /** A desktop shell can shrink its window to the mini player, from the hello. */
  window: boolean;
}

/** A tick this far behind the shown position is a jump (a seek, a new track), not jitter. */
const JUMP_MS = 1000;

/** The item that plays and whether it runs, as the playback channel says. */
export interface Playing {
  uri: string | null;
  playing: boolean;
}

function playingOf(channels: Channels): Playing {
  const item = channels.playback?.payload.item;
  return { uri: item?.uri ?? null, playing: item?.is_playing ?? false };
}

function shownAt(
  position: Position | null,
  playing: boolean,
  now: number,
): number | null {
  if (!position || position.ms === null) return null;
  return playing ? position.ms + Math.max(0, now - position.at) : position.ms;
}

/** The anchor for a tick: while playing, one a little behind the shown position holds it until the ticks catch up, never a step back. */
export function positionAfterTick(
  previous: Position | null,
  ms: number | null,
  playing: boolean,
  now: number,
): Position {
  const shown = shownAt(previous, playing, now);
  if (
    !playing ||
    ms === null ||
    shown === null ||
    shown <= ms ||
    shown - ms >= JUMP_MS
  )
    return { ms, at: now };
  return { ms: shown, at: now + (shown - ms) };
}

/** The anchor after a playback update: a new item starts at 0 until its first tick, and a start or stop keeps the shown position. */
export function positionAfterChannel(
  previous: Position | null,
  before: Playing,
  after: Playing,
  now: number,
): Position | null {
  if (!previous) return previous;
  if (before.uri !== null && after.uri !== null && after.uri !== before.uri)
    return { ms: 0, at: now };
  if (after.playing === before.playing) return previous;
  return { ms: shownAt(previous, before.playing, now), at: now };
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
    (state.onboarding.pending !== null || !state.booted)
  );
}

export class Connection {
  private state: State = {
    localization: null,
    connected: false,
    expired: false,
    channels: {},
    position: null,
    booted: false,
    onboarding: null,
    version: null,
    window: false,
  };
  private socket: Socket | null = null;
  /** The last item the playback channel named, kept across a blank update. */
  private lastUri: string | null = null;
  private readonly listeners = new Set<() => void>();
  /** Visualizer frames skip the state: dozens a second must not re-render the page. */
  private readonly viewers = new Set<(frame: Frame) => void>();
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
    // A socket that is still connecting throws on send; the server hello marks it open.
    if (!this.state.connected) return;
    this.socket?.send(JSON.stringify(message));
  }

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  getState = (): State => this.state;

  /** Hands visualizer frames to `listener`; the server sends them while one listens. */
  watchVisualizer = (listener: (frame: Frame) => void): (() => void) => {
    this.viewers.add(listener);
    if (this.viewers.size === 1) this.send({ type: "visualizer", on: true });
    return () => {
      this.viewers.delete(listener);
      if (this.viewers.size === 0) this.send({ type: "visualizer", on: false });
    };
  };

  private receive(text: string): void {
    const message = JSON.parse(text) as ServerMessage;
    switch (message.kind) {
      case "hello":
        if (message.payload.token)
          this.storage.setItem(TOKEN_KEY, message.payload.token);
        // A token resumes the same process, whose revisions only rise, so the resync overwrites the store.
        this.update({
          connected: true,
          version: message.payload.version,
          window: message.payload.window,
          localization: {
            language: message.payload.language,
            strings: message.payload.strings,
          },
        });
        // The server keeps the ask per socket, so a new socket asks again.
        if (this.viewers.size > 0) this.send({ type: "visualizer", on: true });
        return;
      case "visualizer": {
        const frame = { payload: message.payload, at: this.now() };
        this.viewers.forEach((viewer) => viewer(frame));
        return;
      }
      case "onboarding":
        return this.update({ onboarding: message.payload });
      case "tick":
        return this.update({
          position: positionAfterTick(
            this.state.position,
            message.payload,
            playingOf(this.state.channels).playing,
            this.now(),
          ),
        });
      case "settings": {
        const channels = applyChannel(this.state.channels, message);
        const strings = message.payload.strings;
        return this.update({
          channels,
          ...(channels !== this.state.channels && strings
            ? { localization: { language: message.payload.language, strings } }
            : {}),
        });
      }
      default: {
        const channels = applyChannel(this.state.channels, message);
        const before = {
          uri: this.lastUri,
          playing: playingOf(this.state.channels).playing,
        };
        const after = playingOf(channels);
        if (after.uri !== null) this.lastUri = after.uri;
        return this.update({
          channels,
          booted: this.state.booted || message.kind === "route",
          position: positionAfterChannel(
            this.state.position,
            before,
            after,
            this.now(),
          ),
        });
      }
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
