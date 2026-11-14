import type { HelloPayload } from "./bindings/HelloPayload";
import type { ClientMessage } from "./bindings/ClientMessage";
import type { OnboardingView } from "./bindings/OnboardingView";
import type { ServerMessage } from "./bindings/ServerMessage";
import type { WelcomePayload } from "./bindings/WelcomePayload";
import { applyChannel, type ChannelMessage, type Channels } from "./store";
import { sourceOf } from "./format";
import type { ScreenFrame } from "./bindings/ScreenFrame";
import type { ScreenFeed } from "./screenModel";
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
  /** The Home page's lines under the banner. */
  welcome: WelcomePayload | null;
  /** The running app's changelog; null until the Home page asked and the answer came. */
  changelog: string | null;
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

/** The position at `now`; null before the first tick. */
export function shownAt(
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
    welcome: null,
    changelog: null,
  };
  private socket: Socket | null = null;
  private unsentLayout: ClientMessage | null = null;
  /** The last item the playback channel named, kept across a blank update. */
  private lastUri: string | null = null;
  private readonly listeners = new Set<() => void>();
  /** Visualizer frames skip the state: dozens a second must not re-render the page. */
  private readonly viewers = new Set<(frame: Frame) => void>();
  private readonly screenViewers = new Set<
    (frame: Frame<ScreenFrame>) => void
  >();
  private screenBars = 0;
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
    // A layout is state, not a command: the last one waits for the next hello.
    if (!this.state.connected) {
      if (message.type === "pane_layout") this.unsentLayout = message;
      return;
    }
    this.socket?.send(JSON.stringify(message));
  }

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  getState = (): State => this.state;

  /** The same-origin URL of a cover reference or playing item; null before hello. */
  coverSrc = (uri: string): string | null => {
    const token = this.storage.getItem(TOKEN_KEY);
    return token
      ? `/cover?uri=${encodeURIComponent(uri)}&t=${encodeURIComponent(token)}`
      : null;
  };

  /** Hands visualizer frames to `listener`; the server sends them while one listens. */
  watchVisualizer = (listener: (frame: Frame) => void): (() => void) => {
    this.viewers.add(listener);
    if (this.viewers.size === 1) this.send({ type: "visualizer", on: true });
    return () => {
      this.viewers.delete(listener);
      if (this.viewers.size === 0) this.send({ type: "visualizer", on: false });
    };
  };

  /** The visualizer screen's frames; the server sends them while one listens and a count is known. */
  readonly screen: ScreenFeed = {
    watch: (listener) => {
      this.screenViewers.add(listener);
      if (this.screenViewers.size === 1) this.askScreen();
      return () => {
        this.screenViewers.delete(listener);
        if (this.screenViewers.size === 0)
          this.send({ type: "visualizer_screen", bars: null });
      };
    },
    bars: (count) => {
      if (count === this.screenBars) return;
      this.screenBars = count;
      if (this.screenViewers.size > 0) this.askScreen();
    },
  };

  private askScreen(): void {
    if (this.screenBars > 0)
      this.send({ type: "visualizer_screen", bars: this.screenBars });
  }

  /** Asks the server for the changelog; the answer lands in `changelog`. */
  askChangelog = (): void => this.send({ type: "changelog" });

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
        if (this.screenViewers.size > 0) this.askScreen();
        if (this.unsentLayout) {
          this.send(this.unsentLayout);
          this.unsentLayout = null;
        }
        return;
      case "visualizer": {
        const frame = { payload: message.payload, at: this.now() };
        this.viewers.forEach((viewer) => viewer(frame));
        return;
      }
      case "visualizer_screen": {
        const frame = { payload: message.payload, at: this.now() };
        this.screenViewers.forEach((viewer) => viewer(frame));
        return;
      }
      case "welcome":
        return this.update({ welcome: message.payload });
      case "changelog":
        return this.update({ changelog: message.payload });
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
        const channels = applyChannel(
          this.state.channels,
          this.withServerCover(message),
        );
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

  /** Attach the session token to private cover references, including nested list rows. */
  private withServerCover(message: ChannelMessage): ChannelMessage {
    const replace = (value: unknown): void => {
      if (Array.isArray(value)) {
        value.forEach(replace);
      } else if (value !== null && typeof value === "object") {
        const fields = value as Record<string, unknown>;
        if (
          typeof fields.image_url === "string" &&
          /^subsonic-cover:[0-9]+$/.test(fields.image_url)
        )
          fields.image_url = this.coverSrc(fields.image_url);
        Object.values(fields).forEach(replace);
      }
    };
    replace(message);
    if (message.kind !== "playback") return message;
    const item = message.payload.item;
    if (!item?.uri || item.image_url || sourceOf(item.uri) !== "Subsonic")
      return message;
    const image_url = this.coverSrc(item.uri);
    return {
      ...message,
      payload: { ...message.payload, item: { ...item, image_url } },
    };
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
