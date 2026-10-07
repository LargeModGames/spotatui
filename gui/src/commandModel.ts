import type { Action } from "./bindings/Action";
import type { DeviceInfo } from "./bindings/DeviceInfo";
import type { SearchPayload } from "./bindings/SearchPayload";
import type { Source } from "./bindings/Source";
import type { TrackInfo } from "./bindings/TrackInfo";
import { playRequest } from "./libraryModel";
import { year } from "./searchModel";

export type Verb = "search" | "play" | "queue" | "source" | "device" | "party";
export type Kind = "song" | "album" | "artist";

export const COMMANDS: Record<Verb, { usage: string; what: string }> = {
  search: { usage: "search <words>", what: "open Search with the words" },
  play: {
    usage: "play [song|album|artist] <name>",
    what: "find it and play it",
  },
  queue: { usage: "queue <song>", what: "add a song to the end of the queue" },
  source: { usage: "source <name>", what: "browse another source" },
  device: { usage: "device <name>", what: "play on another device" },
  party: {
    usage: "party start | join <code> <name> | leave",
    what: "listening party",
  },
};

const VERBS = Object.keys(COMMANDS) as Verb[];
const KINDS: Kind[] = ["song", "album", "artist"];

export interface Parsed {
  verb: Verb | null;
  kind: Kind | null;
  arg: string;
}

/** A leading colon is dropped; an unknown first word makes the whole input a search. */
export function parse(input: string): Parsed {
  const text = input.replace(/^:/, "").trimStart();
  const [first = "", ...rest] = text.split(/\s+/);
  const verb = VERBS.find((name) => name === first.toLowerCase()) ?? null;
  if (!verb) return { verb: null, kind: null, arg: text.trim() };
  const kindWord = rest[0]?.toLowerCase();
  const kind =
    verb === "play" ? (KINDS.find((name) => name === kindWord) ?? null) : null;
  const words = kind ? rest.slice(1) : rest;
  return { verb, kind, arg: words.join(" ").trim() };
}

/** Tab: a unique verb prefix, or a compiled source after `source `, or a device after `device `. */
export function complete(
  input: string,
  sources: Source[],
  devices: DeviceInfo[],
): string {
  const text = input.replace(/^:/, "");
  const words = text.split(" ");
  if (words.length === 1) {
    const matches = VERBS.filter((verb) =>
      verb.startsWith(words[0].toLowerCase()),
    );
    return matches.length === 1 ? `${matches[0]} ` : input;
  }
  const verb = words[0].toLowerCase();
  const rest = words.slice(1).join(" ").toLowerCase();
  const names =
    verb === "source"
      ? sources.map(String)
      : verb === "device"
        ? devices.map((device) => device.name)
        : [];
  const matches = names.filter((name) => name.toLowerCase().startsWith(rest));
  return matches.length === 1 ? `${words[0]} ${matches[0]}` : input;
}

/** A pasted Spotify URI or open.spotify.com link plays at once instead of costing a search. */
export function spotifyLink(text: string): Action | null {
  const trimmed = text.trim();
  const match =
    /^spotify:(track|album|artist|playlist):([A-Za-z0-9]+)$/.exec(trimmed) ??
    /^https?:\/\/open\.spotify\.com\/(?:intl-[a-z-]+\/)?(track|album|artist|playlist)\/([A-Za-z0-9]+)/.exec(
      trimmed,
    );
  if (!match) return null;
  const uri = `spotify:${match[1]}:${match[2]}`;
  return match[1] === "track"
    ? { PlayUris: { uris: [uri], offset: null } }
    : { PlayContext: { uri, offset: null } };
}

/** A result row of the palette. */
export interface Row {
  kind: Kind | "device";
  title: string;
  meta: string;
  uri: string | null;
  track?: TrackInfo;
  deviceId?: string;
}

const MAX_ROWS = 9;

/** The rows a parsed command offers from the search results and the device list. */
export function rowsFor(
  parsed: Parsed,
  search: SearchPayload | null,
  devices: DeviceInfo[],
): Row[] {
  if (parsed.verb === "device") {
    const want = parsed.arg.toLowerCase();
    return devices
      .filter((device) => device.id && device.name.toLowerCase().includes(want))
      .slice(0, MAX_ROWS)
      .map((device) => ({
        kind: "device",
        title: device.name,
        meta: device.is_active ? "playing here now" : device.kind,
        uri: null,
        deviceId: device.id ?? undefined,
      }));
  }
  if (!search || (parsed.verb !== "play" && parsed.verb !== "queue")) return [];
  const songs: Row[] = search.tracks.map((track) => ({
    kind: "song",
    title: track.name,
    meta: [track.artists.join(", "), track.album && `from ${track.album}`]
      .filter(Boolean)
      .join(" · "),
    uri: track.uri,
    track,
  }));
  if (parsed.verb === "queue" || parsed.kind === "song")
    return songs.slice(0, MAX_ROWS);
  const albums: Row[] = search.albums.map((album) => ({
    kind: "album",
    title: album.name,
    meta: [
      album.artists.map((artist) => artist.name).join(", "),
      year(album.release_date),
    ]
      .filter(Boolean)
      .join(" · "),
    uri: album.uri,
  }));
  const artists: Row[] = search.artists.map((artist) => ({
    kind: "artist",
    title: artist.name,
    meta: "artist",
    uri: artist.uri,
  }));
  if (parsed.kind === "album") return albums.slice(0, MAX_ROWS);
  if (parsed.kind === "artist") return artists.slice(0, MAX_ROWS);
  return [...albums.slice(0, 3), ...songs].slice(0, MAX_ROWS);
}

/** The Action a row runs. Songs play on through the results, as the terminal does. */
export function actionFor(
  parsed: Parsed,
  row: Row,
  search: SearchPayload | null,
  playNow: boolean,
): Action | null {
  if (row.kind === "device")
    return row.deviceId
      ? { TransferPlayback: { device_id: row.deviceId, persist: true } }
      : null;
  if (!row.uri) return null;
  if (row.kind !== "song")
    return { PlayContext: { uri: row.uri, offset: null } };
  if (parsed.verb === "queue" && !playNow && row.track)
    return { QueueTrack: row.track };
  const tracks = search?.tracks ?? [];
  const index = tracks.findIndex((track) => track.uri === row.uri);
  return index >= 0
    ? playRequest(tracks, index)
    : { PlayUris: { uris: [row.uri], offset: null } };
}

/** The Action a command runs with no row: party, and a source by name. */
export function directAction(parsed: Parsed, sources: Source[]): Action | null {
  if (parsed.verb === "source") {
    const want = parsed.arg.toLowerCase();
    const source = sources.find((name) => name.toLowerCase() === want);
    return source ? { SelectSource: source } : null;
  }
  if (parsed.verb !== "party") return null;
  const [sub = "", code = "", ...name] = parsed.arg.split(/\s+/);
  switch (sub.toLowerCase()) {
    case "start":
      return "StartParty";
    case "leave":
      return "LeaveParty";
    case "join":
      return code && name.length > 0
        ? { JoinParty: { code: code.toUpperCase(), name: name.join(" ") } }
        : null;
    default:
      return null;
  }
}

const RECENT_KEY = "spotatui.commands";
const RECENT_KEEP = 20;

/** The newest entry first, without duplicates. */
export function pushRecent(list: string[], entry: string): string[] {
  const trimmed = entry.trim();
  if (!trimmed) return list;
  return [trimmed, ...list.filter((item) => item !== trimmed)].slice(
    0,
    RECENT_KEEP,
  );
}

/** This browser's storage, or one that keeps nothing when the browser blocks access. */
export function pageStorage(): Pick<Storage, "getItem" | "setItem"> {
  try {
    return window.localStorage;
  } catch {
    return { getItem: () => null, setItem: () => {} };
  }
}

/** Recent commands from this browser; empty when storage is blocked. */
export function loadRecent(storage: Pick<Storage, "getItem">): string[] {
  try {
    const value: unknown = JSON.parse(storage.getItem(RECENT_KEY) ?? "[]");
    return Array.isArray(value)
      ? value.filter((item): item is string => typeof item === "string")
      : [];
  } catch {
    return [];
  }
}

export function saveRecent(
  storage: Pick<Storage, "setItem">,
  list: string[],
): void {
  try {
    storage.setItem(RECENT_KEY, JSON.stringify(list));
  } catch {
    // Blocked storage only loses the history.
  }
}
