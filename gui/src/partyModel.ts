import type { Action } from "./bindings/Action";
import type { PartyPayload } from "./bindings/PartyPayload";
import type { PartyRoom } from "./bindings/PartyRoom";

/** The relay's room codes are six characters of A-Z and 0-9. */
export const CODE_LENGTH = 6;
const NAME_LIMIT = 32;

export function normalizeCode(input: string): string {
  return input
    .toUpperCase()
    .replace(/[^A-Z0-9]/g, "")
    .slice(0, CODE_LENGTH);
}

export function normalizeName(input: string): string {
  return input.slice(0, NAME_LIMIT);
}

export function joinRequest(code: string, name: string): Action | null {
  const trimmed = name.trim();
  return code.length === CODE_LENGTH && trimmed
    ? { JoinParty: { code, name: trimmed } }
    : null;
}

/** "You host · 3 listening" counts the host; a guest names the room it follows. */
export function headline(party: PartyPayload): string {
  const room = party.room;
  if (!room) return party.phase === "connecting" ? "Connecting…" : "";
  if (room.host) return `You host · ${room.guests.length + 1} listening`;
  return `You follow ${room.host_name || "the host"}${room.code ? ` · room ${room.code}` : ""}`;
}

export interface Listener {
  name: string;
  role: string;
  initial: string;
  hue: number;
}

/** You first, then the others; a guest learns only who joined after it. */
export function listeners(room: PartyRoom): Listener[] {
  const others = room.host
    ? room.guests.map((name) => ({ name, role: "guest" }))
    : [
        { name: room.host_name || "Host", role: "host" },
        ...room.guests.map((name) => ({ name, role: "guest" })),
      ];
  return [{ name: "You", role: room.host ? "host" : "guest" }, ...others].map(
    (listener) => ({
      ...listener,
      initial: listener.name.charAt(0).toUpperCase() || "?",
      hue: hue(listener.name),
    }),
  );
}

function hue(name: string): number {
  let hash = 0;
  for (const char of name) hash = (hash * 31 + char.charCodeAt(0)) % 360;
  return hash;
}

/**
 * Guests follow the host only while a Spotify track plays outside the native
 * queue: a queue slot or a decoded source owns the sink and the relay stops.
 */
export function relaying(
  room: PartyRoom,
  queueNow: boolean,
  playingUri: string | null,
): boolean {
  return (
    room.host && !queueNow && /^spotify:(track|episode):/.test(playingUri ?? "")
  );
}
